//! Shell integration on the byte stream from the shell: picks out the
//! sequences shells send about themselves, which `alacritty_terminal`
//! would drop, before the parser sees them.
//!
//! - OSC 7 (`file://host/path`): the working directory, as
//!   [`ShellEvent::Cwd`]. Passed on unchanged.
//! - OSC 133 (FinalTerm prompt marks): `A` prompt starts, `B` prompt ends,
//!   `C` command starts, `D;code` command finished. `C` and `D` become
//!   events; only their timing matters.
//!
//! Where a prompt is does matter, and has to keep mattering as the
//! screen scrolls and lines rewrap. So `A` is turned into an OSC 8
//! hyperlink with the [`PROMPT_SCHEME`] (`alacritty_terminal` stores
//! hyperlinks on the cells printed while one is open) and closed again
//! at `B`, `C`, `D`, the next `A`, or the end of the prompt's first line
//! with anything on it -- a shell that never sends `B` doesn't mark its
//! whole session. The prompt's cells then carry the mark wherever they
//! move; selection and search don't see hyperlinks. The mark's URI also
//! says how the command before it ended and how long it ran, if one ran:
//! `terminaal-prompt:exit=1&ms=5230` (`;` would end the URI in OSC 8).
//!
//! `C` opens another hyperlink, [`OUTPUT_SCHEME`], closed right after the
//! first character printed: where the command's output starts, however
//! long or wrapped the command line was. A command without output gets
//! none; one that sets a hyperlink of its own before printing anything
//! neither (closing ours would close its link).
//!
//! Everything else goes through byte for byte. The filter keeps no more
//! than an unfinished OSC sequence between calls; `feed` never holds on
//! to anything that isn't part of one.

use std::path::{Path, PathBuf};

/// URI scheme of prompt marks; never opened as a link.
pub const PROMPT_SCHEME: &str = "terminaal-prompt:";
/// URI scheme of the mark on a command's first output character.
pub const OUTPUT_SCHEME: &str = "terminaal-output:";

/// Longest OSC sequence looked at; longer ones pass through unexamined.
const MAX_OSC: usize = 4096;

const ESC: u8 = 0x1b;
const BEL: u8 = 0x07;
/// Cancel and substitute abort an escape sequence.
const CAN: u8 = 0x18;
const SUB: u8 = 0x1a;

/// What the shell told about itself.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ShellEvent {
    /// OSC 7. `host` is empty when the shell left it out.
    Cwd { host: String, path: PathBuf },
    /// OSC 133;A: a prompt is coming -- the shell waits for input.
    Prompt,
    /// OSC 133;C: a command started running.
    CommandStarted,
    /// OSC 133;D: it finished, with this exit status if the shell said.
    CommandFinished { exit: Option<i32> },
}

/// What a prompt mark says about the command typed at the prompt before.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Finished {
    /// Its exit status, if the shell said.
    pub exit: Option<i32>,
    /// How long it ran; `None` if no command ran (an empty line, the first
    /// prompt, a shell without `C`).
    pub duration: Option<std::time::Duration>,
}

/// The command before a prompt, from the prompt's mark URI.
pub fn mark_finished(uri: &str) -> Finished {
    let mut finished = Finished::default();
    let Some(fields) = uri.strip_prefix(PROMPT_SCHEME) else { return finished };
    for field in fields.split('&') {
        match field.split_once('=') {
            Some(("exit", code)) => finished.exit = code.parse().ok(),
            Some(("ms", ms)) => finished.duration = ms.parse().ok().map(std::time::Duration::from_millis),
            _ => {}
        }
    }
    finished
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum State {
    Ground,
    /// After ESC, not yet known what follows. The ESC is held back.
    Escape,
    /// ESC [ ... up to the final byte.
    Csi,
    /// Collecting an OSC sequence into `osc`.
    Osc,
    /// ESC inside an OSC: the start of ST (ESC \) or not.
    OscEscape,
    /// An OSC too long to look at, passed through up to its end.
    OscPassthrough,
    OscPassthroughEscape,
}

pub struct Filter {
    state: State,
    /// The OSC sequence being collected, from its ESC ].
    osc: Vec<u8>,
    /// A prompt mark is open.
    mark_open: bool,
    /// Something visible was printed since the mark opened.
    printed: bool,
    /// When the command that started (C) and hasn't finished (D) yet began.
    running: Option<std::time::Instant>,
    /// Exit status and run time of the last finished command, for the
    /// next mark.
    last_exit: Option<i32>,
    last_duration: Option<std::time::Duration>,
    /// The output mark is open: the first character after C isn't out yet.
    output_open: bool,
    /// Continuation bytes still to come of the UTF-8 character being printed
    /// while the output mark is open.
    utf8_rest: u8,
}

impl Default for Filter {
    fn default() -> Self {
        Self {
            state: State::Ground,
            osc: Vec::new(),
            mark_open: false,
            printed: false,
            running: None,
            last_exit: None,
            last_duration: None,
            output_open: false,
            utf8_rest: 0,
        }
    }
}

impl Filter {
    /// Filter `input` into `out`; what the shell said goes to `events`.
    pub fn feed(&mut self, input: &[u8], out: &mut Vec<u8>, events: &mut Vec<ShellEvent>) {
        for &byte in input {
            match self.state {
                State::Ground => match byte {
                    ESC => self.state = State::Escape,
                    b'\n' => {
                        if self.mark_open && self.printed {
                            self.close_mark(out);
                        }
                        out.push(byte);
                    }
                    _ => {
                        if byte >= 0x20 && byte != 0x7f && self.mark_open {
                            self.printed = true;
                        }
                        out.push(byte);
                        if self.output_open {
                            self.after_output_byte(byte, out);
                        }
                    }
                },
                State::Escape => match byte {
                    b']' => {
                        self.osc.clear();
                        self.osc.extend_from_slice(&[ESC, b']']);
                        self.state = State::Osc;
                    }
                    ESC => out.push(ESC),
                    _ => {
                        out.extend_from_slice(&[ESC, byte]);
                        self.state = if byte == b'[' { State::Csi } else { State::Ground };
                    }
                },
                State::Csi => {
                    out.push(byte);
                    if (0x40..=0x7e).contains(&byte) || byte == CAN || byte == SUB {
                        self.state = State::Ground;
                    }
                }
                State::Osc => match byte {
                    BEL => {
                        self.finish_osc(2, out, events);
                        out.extend_from_slice(&self.osc_tail(&[BEL]));
                    }
                    ESC => self.state = State::OscEscape,
                    CAN | SUB => self.abort_osc(byte, out),
                    _ if self.osc.len() >= MAX_OSC => {
                        out.append(&mut self.osc);
                        out.push(byte);
                        self.state = State::OscPassthrough;
                    }
                    _ => self.osc.push(byte),
                },
                State::OscEscape => {
                    if byte == b'\\' {
                        self.finish_osc(2, out, events);
                        out.extend_from_slice(&self.osc_tail(&[ESC, b'\\']));
                    } else {
                        // ESC ends the OSC like vte does, and starts
                        // whatever comes next.
                        self.finish_osc(2, out, events);
                        out.extend_from_slice(&self.osc_tail(&[]));
                        self.state = State::Escape;
                        self.feed(&[byte], out, events);
                    }
                }
                State::OscPassthrough => {
                    out.push(byte);
                    match byte {
                        BEL | CAN | SUB => self.state = State::Ground,
                        ESC => self.state = State::OscPassthroughEscape,
                        _ => {}
                    }
                }
                State::OscPassthroughEscape => {
                    if byte == b'\\' {
                        out.push(byte);
                        self.state = State::Ground;
                    } else {
                        // The ESC went out already; `Escape` sends it again.
                        out.pop();
                        self.state = State::Escape;
                        self.feed(&[byte], out, events);
                    }
                }
            }
        }
    }

    /// A complete OSC sequence is in `osc` (from `skip` on its body). Handle
    /// it; if it's to be passed on, it stays in `osc` for [`Filter::osc_tail`],
    /// otherwise `osc` is emptied.
    fn finish_osc(&mut self, skip: usize, out: &mut Vec<u8>, events: &mut Vec<ShellEvent>) {
        self.state = State::Ground;
        let osc = std::mem::take(&mut self.osc);
        let body = &osc[skip..];
        if let Some(uri) = body.strip_prefix(b"7;") {
            if let Some(event) = parse_cwd(uri) {
                events.push(event);
            }
        } else if let Some(mark) = body.strip_prefix(b"133;") {
            let mut parts = mark.split(|&b| b == b';');
            match parts.next() {
                Some(b"A") => {
                    self.close_mark(out);
                    self.close_output(out);
                    let fields: Vec<String> = [
                        self.last_exit.map(|code| format!("exit={code}")),
                        self.last_duration.map(|duration| format!("ms={}", duration.as_millis())),
                    ]
                    .into_iter()
                    .flatten()
                    .collect();
                    out.extend_from_slice(format!("\x1b]8;;{PROMPT_SCHEME}{}\x07", fields.join("&")).as_bytes());
                    self.mark_open = true;
                    self.printed = false;
                    self.last_exit = None;
                    self.last_duration = None;
                    events.push(ShellEvent::Prompt);
                }
                Some(b"B") => self.close_mark(out),
                Some(b"C") => {
                    self.close_mark(out);
                    self.close_output(out);
                    // Several lines typed at once: bash starts each (C) and
                    // finishes once (D). They ran from the first on, and
                    // the output starts at the first one's.
                    if self.running.is_none() {
                        self.running = Some(std::time::Instant::now());
                        out.extend_from_slice(format!("\x1b]8;;{OUTPUT_SCHEME}\x07").as_bytes());
                        self.output_open = true;
                    }
                    self.utf8_rest = 0;
                    events.push(ShellEvent::CommandStarted);
                }
                // Only after C: shells send D at every prompt, also after
                // an empty line, whose status is still the command before.
                Some(b"D") if self.running.is_some() => {
                    self.close_mark(out);
                    self.close_output(out);
                    let started = self.running.take().expect("checked");
                    let exit = parts.next().and_then(|code| std::str::from_utf8(code).ok()?.trim().parse().ok());
                    self.last_exit = exit;
                    self.last_duration = Some(started.elapsed());
                    events.push(ShellEvent::CommandFinished { exit });
                }
                Some(b"D") => self.close_mark(out),
                _ => {}
            }
            // Handled (or unknown); alacritty would drop it anyway.
            return;
        } else if body.starts_with(b"8;") && self.output_open {
            // The program's own hyperlink replaces ours; closing ours later
            // would end its link.
            self.output_open = false;
        }
        self.osc = osc;
    }

    /// The collected OSC sequence with `terminator`, if [`Filter::finish_osc`]
    /// left it to pass on; empties `osc`.
    fn osc_tail(&mut self, terminator: &[u8]) -> Vec<u8> {
        if self.osc.is_empty() {
            return Vec::new();
        }
        let mut tail = std::mem::take(&mut self.osc);
        tail.extend_from_slice(terminator);
        tail
    }

    fn abort_osc(&mut self, byte: u8, out: &mut Vec<u8>) {
        out.append(&mut self.osc);
        out.push(byte);
        self.state = State::Ground;
    }

    /// `byte` of the output went out while the output mark is open: close
    /// it once the first character is complete -- never inside a UTF-8
    /// sequence, which the OSC would break.
    fn after_output_byte(&mut self, byte: u8, out: &mut Vec<u8>) {
        match byte {
            0x80..=0xbf if self.utf8_rest > 0 => self.utf8_rest -= 1,
            0xc0..=0xdf => self.utf8_rest = 1,
            0xe0..=0xef => self.utf8_rest = 2,
            0xf0..=0xf7 => self.utf8_rest = 3,
            0x21..=0x7e => {}
            // Blanks and control characters don't start the output.
            _ => return,
        }
        if self.utf8_rest == 0 {
            self.close_output(out);
        }
    }

    fn close_output(&mut self, out: &mut Vec<u8>) {
        if self.output_open {
            out.extend_from_slice(b"\x1b]8;;\x07");
            self.output_open = false;
        }
    }

    fn close_mark(&mut self, out: &mut Vec<u8>) {
        if self.mark_open {
            out.extend_from_slice(b"\x1b]8;;\x07");
            self.mark_open = false;
        }
    }
}

/// A working directory as short as tab titles want it: the home folder
/// as `~`, every folder but the last cut to its first letter (like fish),
/// `host:` in front when it's another machine's.
pub fn short_path(host: &str, path: &Path, home: Option<&Path>, local_host: &str) -> String {
    let foreign = !host.is_empty() && host != local_host && host != "localhost";
    let (mut parts, rooted): (Vec<String>, bool) = match home.filter(|_| !foreign).and_then(|home| path.strip_prefix(home).ok()) {
        Some(rest) => (std::iter::once("~".to_string()).chain(names(rest)).collect(), false),
        None => (names(path).collect(), true),
    };
    let last = parts.len().saturating_sub(1);
    for part in &mut parts[..last] {
        if part != "~" {
            let keep = if part.starts_with('.') { 2 } else { 1 };
            *part = part.chars().take(keep).collect();
        }
    }
    let joined = parts.join("/");
    let shown = if rooted { format!("/{joined}") } else { joined };
    if foreign { format!("{host}:{shown}") } else { shown }
}

fn names(path: &Path) -> impl Iterator<Item = String> + '_ {
    path.components().filter_map(|part| match part {
        std::path::Component::Normal(name) => Some(name.to_string_lossy().into_owned()),
        _ => None,
    })
}

/// OSC 7's `file://host/path`, percent-decoded. Other schemes and
/// relative paths are ignored.
fn parse_cwd(uri: &[u8]) -> Option<ShellEvent> {
    use std::os::unix::ffi::OsStringExt;

    let rest = uri.strip_prefix(b"file://")?;
    let slash = rest.iter().position(|&b| b == b'/')?;
    let host = String::from_utf8_lossy(&rest[..slash]).into_owned();
    let path = percent_decode(&rest[slash..]);
    Some(ShellEvent::Cwd { host, path: PathBuf::from(std::ffi::OsString::from_vec(path)) })
}

fn percent_decode(bytes: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        let hex = |b: u8| (b as char).to_digit(16);
        match (bytes[i], bytes.get(i + 1).copied().and_then(hex), bytes.get(i + 2).copied().and_then(hex)) {
            (b'%', Some(high), Some(low)) => {
                out.push((high * 16 + low) as u8);
                i += 3;
            }
            (byte, ..) => {
                out.push(byte);
                i += 1;
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(chunks: &[&[u8]]) -> (Vec<u8>, Vec<ShellEvent>) {
        let mut filter = Filter::default();
        let (mut out, mut events) = (Vec::new(), Vec::new());
        for chunk in chunks {
            filter.feed(chunk, &mut out, &mut events);
        }
        (out, events)
    }

    fn text(bytes: &[u8]) -> String {
        String::from_utf8_lossy(bytes).replace('\x1b', "⎋").replace('\x07', "🔔")
    }

    #[test]
    fn plain_output_and_other_sequences_pass_unchanged() {
        let input: &[u8] = b"ls\r\n\x1b[1;31mred\x1b[0m \x1b]0;title\x07 \x1b]8;;https://x.org\x1b\\link\x1b]8;;\x1b\\\x1bM\n";
        let (out, events) = run(&[input]);
        assert_eq!(text(&out), text(input));
        assert!(events.is_empty());
    }

    #[test]
    fn a_whole_prompt_cycle() {
        let (out, events) = run(&[b"\x1b]133;A\x07$ \x1b]133;B\x07ls\r\n\x1b]133;C\x07file\r\n\x1b]133;D;2\x07\x1b]133;A\x1b\\$ "]);
        let out = text(&out);
        let (out, ms) = out.split_once("&ms=").expect("run time recorded");
        assert_eq!(
            out,
            text(b"\x1b]8;;terminaal-prompt:\x07$ \x1b]8;;\x07ls\r\n\x1b]8;;terminaal-output:\x07f\x1b]8;;\x07ile\r\n\x1b]8;;terminaal-prompt:exit=2")
        );
        assert!(ms.starts_with('0'), "{ms}");
        assert_eq!(
            events,
            [ShellEvent::Prompt, ShellEvent::CommandStarted, ShellEvent::CommandFinished { exit: Some(2) }, ShellEvent::Prompt]
        );
    }

    #[test]
    fn sequences_split_across_reads() {
        let input: &[u8] = b"\x1b]133;A\x07$ \x1b]7;file://box/home/me\x1b\\x";
        let whole = run(&[input]);
        for split in 1..input.len() {
            assert_eq!(run(&[&input[..split], &input[split..]]), whole, "split at {split}");
        }
        let bytewise: Vec<&[u8]> = input.chunks(1).collect();
        assert_eq!(run(&bytewise), whole);
    }

    #[test]
    fn a_mark_without_b_ends_with_its_first_line() {
        // An empty first line doesn't count: the mark goes on the text.
        let (out, _) = run(&[b"\x1b]133;A\x07\r\n~/src\r\n> "]);
        assert_eq!(text(&out), text(b"\x1b]8;;terminaal-prompt:\x07\r\n~/src\r\x1b]8;;\x07\n> "));
        // Colors inside the prompt aren't something printed.
        let (out, _) = run(&[b"\x1b]133;A\x07\x1b[32m\n"]);
        assert_eq!(text(&out), text(b"\x1b]8;;terminaal-prompt:\x07\x1b[32m\n"));
    }

    #[test]
    fn working_directory_is_reported_and_passed_on() {
        let input: &[u8] = b"\x1b]7;file://my-box/home/me/with%20space/%C3%A4\x07";
        let (out, events) = run(&[input]);
        assert_eq!(out, input);
        assert_eq!(events, [ShellEvent::Cwd { host: "my-box".into(), path: PathBuf::from("/home/me/with space/ä") }]);
        assert_eq!(run(&[b"\x1b]7;file:///tmp\x07"]).1, [ShellEvent::Cwd { host: String::new(), path: "/tmp".into() }]);
        assert!(run(&[b"\x1b]7;kitty-shell-cwd://x/y\x07"]).1.is_empty());
    }

    #[test]
    fn exit_codes_and_marks() {
        let (_, events) = run(&[b"\x1b]133;C\x07\x1b]133;D\x07\x1b]133;C\x07\x1b]133;D;130\x07\x1b]133;C\x07\x1b]133;D;x\x07"]);
        let finished = [None, Some(130), None].map(|exit| ShellEvent::CommandFinished { exit });
        assert_eq!(events.iter().filter(|e| **e != ShellEvent::CommandStarted).cloned().collect::<Vec<_>>(), finished);
        // An empty line: D without C neither reports nor repeats the status.
        let (out, events) = run(&[b"\x1b]133;C\x07\x1b]133;D;1\x07\x1b]133;A\x07\x1b]133;D;1\x07\x1b]133;A\x07"]);
        let finished: Vec<_> = events.iter().filter(|e| matches!(e, ShellEvent::CommandFinished { .. })).collect();
        assert_eq!(finished, [&ShellEvent::CommandFinished { exit: Some(1) }]);
        assert_eq!(events.iter().filter(|e| **e == ShellEvent::Prompt).count(), 2);
        assert!(text(&out).contains("terminaal-prompt:exit=1&ms="));
        assert!(text(&out).ends_with(&text(b"\x1b]8;;\x07\x1b]8;;terminaal-prompt:\x07")), "{}", text(&out));
        let finished = mark_finished("terminaal-prompt:exit=130&ms=2500");
        assert_eq!((finished.exit, finished.duration), (Some(130), Some(std::time::Duration::from_millis(2500))));
        assert_eq!(mark_finished("terminaal-prompt:ms=7").exit, None);
        assert_eq!(mark_finished("terminaal-prompt:exit=1"), Finished { exit: Some(1), duration: None }, "older marks");
        assert_eq!(mark_finished("terminaal-prompt:"), Finished::default());
        assert_eq!(mark_finished("https://x.org"), Finished::default());
        // fish 4 sends extra fields after A; still a prompt.
        let (out, _) = run(&[b"\x1b]133;A;click_events=1\x07"]);
        assert_eq!(text(&out), text(b"\x1b]8;;terminaal-prompt:\x07"));
    }

    #[test]
    fn output_mark_covers_the_first_character_only() {
        let c = b"\x1b]133;C\x07";
        // Blank lines and colors come before; a multi-byte character stays whole.
        let (out, _) = run(&[c, b"\r\n \x1b[31m\xc3\xa4bc"]);
        assert_eq!(text(&out), text(b"\x1b]8;;terminaal-output:\x07\r\n \x1b[31m\xc3\xa4\x1b]8;;\x07bc"));
        // Split between the bytes of it, too.
        let (split, _) = run(&[c, b"\xe2\x82", b"\xac!"]);
        assert_eq!(text(&split), text(b"\x1b]8;;terminaal-output:\x07\xe2\x82\xac\x1b]8;;\x07!"));
        // No output: D closes it.
        let (out, _) = run(&[c, b"\x1b]133;D;0\x07"]);
        assert_eq!(text(&out), text(b"\x1b]8;;terminaal-output:\x07\x1b]8;;\x07"));
        // C again before D (bash, several lines at once): one command, one mark.
        let (out, _) = run(&[c, b"x", c, b"y"]);
        assert_eq!(text(&out), text(b"\x1b]8;;terminaal-output:\x07x\x1b]8;;\x07y"));
        // A program's own link first: ours isn't closed over it.
        let (out, _) = run(&[c, b"\x1b]8;;https://x.org\x07x\x1b]8;;\x07"]);
        assert_eq!(text(&out), text(b"\x1b]8;;terminaal-output:\x07\x1b]8;;https://x.org\x07x\x1b]8;;\x07"));
    }

    #[test]
    fn short_paths_for_titles() {
        let home = Some(Path::new("/home/me"));
        let short = |host, path: &str| short_path(host, Path::new(path), home, "box");
        assert_eq!(short("box", "/home/me"), "~");
        assert_eq!(short("", "/home/me/Projekte/Terminal"), "~/P/Terminal");
        assert_eq!(short("localhost", "/home/me/.config/fish"), "~/.c/fish");
        assert_eq!(short("box", "/"), "/");
        assert_eq!(short("box", "/usr/share/doc"), "/u/s/doc");
        assert_eq!(short("server", "/home/me/src"), "server:/h/m/src");
    }

    #[test]
    fn broken_and_huge_sequences_go_through() {
        let aborted: &[u8] = b"\x1b]133;A\x18rest";
        assert_eq!(run(&[aborted]).0, aborted);
        let mut huge = b"\x1b]52;c;".to_vec();
        huge.extend(std::iter::repeat_n(b'A', MAX_OSC * 2));
        let mut ended = huge.clone();
        huge.push(BEL);
        huge.extend_from_slice(b"after");
        assert_eq!(run(&[&huge]).0, huge);
        ended.extend_from_slice(b"\x1b[0m");
        assert_eq!(run(&[&ended]).0, ended);
        // ESC ending an OSC starts the next sequence.
        let (out, _) = run(&[b"\x1b]0;t\x1b[31m"]);
        assert_eq!(text(&out), text(b"\x1b]0;t\x1b[31m"));
    }
}
