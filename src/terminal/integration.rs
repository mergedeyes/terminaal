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
//! says how the command before it ended: `terminaal-prompt:exit=1`.
//!
//! Everything else goes through byte for byte. The filter keeps no more
//! than an unfinished OSC sequence between calls; `feed` never holds on
//! to anything that isn't part of one.

use std::path::{Path, PathBuf};

/// URI scheme of prompt marks; never opened as a link.
pub const PROMPT_SCHEME: &str = "terminaal-prompt:";

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

/// The exit status recorded in a prompt mark's URI, `None` if there's
/// none (the first prompt, or a shell that doesn't say).
pub fn mark_exit(uri: &str) -> Option<i32> {
    uri.strip_prefix(PROMPT_SCHEME)?.strip_prefix("exit=")?.parse().ok()
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
    /// A command started (C) and hasn't finished (D) yet.
    running: bool,
    /// Exit status of the last finished command, for the next mark.
    last_exit: Option<i32>,
}

impl Default for Filter {
    fn default() -> Self {
        Self { state: State::Ground, osc: Vec::new(), mark_open: false, printed: false, running: false, last_exit: None }
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
                    let exit = self.last_exit.map(|code| format!("exit={code}")).unwrap_or_default();
                    out.extend_from_slice(format!("\x1b]8;;{PROMPT_SCHEME}{exit}\x07").as_bytes());
                    self.mark_open = true;
                    self.printed = false;
                    self.last_exit = None;
                    events.push(ShellEvent::Prompt);
                }
                Some(b"B") => self.close_mark(out),
                Some(b"C") => {
                    self.close_mark(out);
                    self.running = true;
                    events.push(ShellEvent::CommandStarted);
                }
                // Only after C: shells send D at every prompt, also after
                // an empty line, whose status is still the command before.
                Some(b"D") if self.running => {
                    self.close_mark(out);
                    self.running = false;
                    let exit = parts.next().and_then(|code| std::str::from_utf8(code).ok()?.trim().parse().ok());
                    self.last_exit = exit;
                    events.push(ShellEvent::CommandFinished { exit });
                }
                Some(b"D") => self.close_mark(out),
                _ => {}
            }
            // Handled (or unknown); alacritty would drop it anyway.
            return;
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
        assert_eq!(
            text(&out),
            text(b"\x1b]8;;terminaal-prompt:\x07$ \x1b]8;;\x07ls\r\nfile\r\n\x1b]8;;terminaal-prompt:exit=2\x07$ ")
        );
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
        assert!(text(&out).ends_with(&text(b"\x1b]8;;terminaal-prompt:exit=1\x07\x1b]8;;\x07\x1b]8;;terminaal-prompt:\x07")));
        assert_eq!(mark_exit("terminaal-prompt:exit=130"), Some(130));
        assert_eq!(mark_exit("terminaal-prompt:"), None);
        assert_eq!(mark_exit("https://x.org"), None);
        // fish 4 sends extra fields after A; still a prompt.
        let (out, _) = run(&[b"\x1b]133;A;click_events=1\x07"]);
        assert_eq!(text(&out), text(b"\x1b]8;;terminaal-prompt:\x07"));
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
