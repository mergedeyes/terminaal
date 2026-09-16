//! Asking before a paste that could do more than it seems: several lines
//! that each run the moment they arrive, `sudo`, a download piped into a
//! shell, a command that wipes files -- and anything of that sent to
//! several terminals at once by the broadcast.
//!
//! [`check`] decides from the text alone plus what the terminals do with
//! it; [`PasteWarning`] is the dialog. Like the command palette, `app.rs`
//! keeps the keyboard (Enter pastes, Esc cancels) and egui only draws it
//! and reports a click on its buttons. `paste_warning = false` in the
//! config turns it off.

use egui::{Align2, Area, Button, Frame, Id, Order, Rect, RichText, TextStyle, UiKind};

use crate::i18n::t;
use crate::ui::theme;

/// Widest the dialog gets, in points.
const WIDTH: f32 = 520.0;
/// Lines of the text shown before "… N more".
const PREVIEW_LINES: usize = 10;
/// Characters shown of a long line.
const PREVIEW_COLUMNS: usize = 160;

/// Why a paste is worth a second look.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Reason {
    /// No bracketed paste: every line break is an Enter, each of these
    /// lines runs as soon as it arrives.
    RunsLines(usize),
    /// Several lines, inserted as one paste but still going to several
    /// terminals or run right away.
    Lines(usize),
    /// `sudo`, `doas` or `pkexec`.
    Sudo,
    /// A download or anything else piped into a shell.
    PipeToShell,
    /// `rm -rf`, `mkfs`, `dd of=/dev/…`, a fork bomb.
    Destructive,
}

impl Reason {
    fn label(self) -> String {
        match self {
            Reason::RunsLines(lines) => t!("paste-warn-runs-lines", lines = lines),
            Reason::Lines(lines) => t!("paste-warn-lines", lines = lines),
            Reason::Sudo => t!("paste-warn-sudo"),
            Reason::PipeToShell => t!("paste-warn-pipe"),
            Reason::Destructive => t!("paste-warn-destructive"),
        }
    }
}

/// What to look out for in `text`, going to `terminals` terminals. `bracketed`:
/// all of them take a bracketed paste; `run`: an Enter follows (paste and run).
pub fn check(text: &str, bracketed: bool, run: bool, terminals: usize) -> Vec<Reason> {
    let mut reasons = Vec::new();
    // "Paste and run" drops trailing line breaks and adds one Enter itself.
    let body = if run { text.trim_end_matches(['\r', '\n']) } else { text };
    let lines = body.trim_end_matches(['\r', '\n']).lines().count();
    if !bracketed && body.contains(['\n', '\r']) {
        reasons.push(Reason::RunsLines(lines.max(1)));
    } else if lines > 1 && (run || terminals > 1) {
        reasons.push(Reason::Lines(lines));
    }
    let words: Vec<&str> = text.split(|c: char| c.is_whitespace() || ";&|(){}`".contains(c)).filter(|w| !w.is_empty()).collect();
    if words.iter().any(|word| matches!(*word, "sudo" | "doas" | "pkexec")) {
        reasons.push(Reason::Sudo);
    }
    if pipes_into_shell(text) {
        reasons.push(Reason::PipeToShell);
    }
    if destructive(&words) || text.contains(":(){") {
        reasons.push(Reason::Destructive);
    }
    reasons
}

/// `… | sh`, `… | sudo bash`, `sh -c "$(curl …)"`, `bash <(wget …)`.
fn pipes_into_shell(text: &str) -> bool {
    const SHELLS: [&str; 6] = ["sh", "bash", "zsh", "fish", "dash", "ksh"];
    let downloads = ["$(curl", "$(wget", "<(curl", "<(wget", "`curl", "`wget"];
    if downloads.iter().any(|download| text.contains(download)) {
        return true;
    }
    text.match_indices('|').any(|(i, _)| {
        let rest = &text[i + 1..];
        // `||` is "or", not a pipe.
        if rest.starts_with('|') || text[..i].ends_with('|') {
            return false;
        }
        let mut words = rest.split_whitespace().skip_while(|word| matches!(*word, "sudo" | "doas" | "-E" | "env"));
        words.next().is_some_and(|word| SHELLS.contains(&word.rsplit('/').next().unwrap_or(word)))
    })
}

/// `rm` with both recursive and force flags, `mkfs…`, `dd of=/dev/…`.
fn destructive(words: &[&str]) -> bool {
    words.iter().enumerate().any(|(i, word)| match *word {
        "rm" => {
            let flags: Vec<&str> = words[i + 1..].iter().copied().take_while(|w| w.starts_with('-')).collect();
            let has = |short: char, long: &str| {
                flags.iter().any(|flag| *flag == long || (!flag.starts_with("--") && flag.contains(short)))
            };
            (has('r', "--recursive") || has('R', "--recursive")) && has('f', "--force")
        }
        "dd" => words[i + 1..].iter().any(|w| w.starts_with("of=/dev/")),
        word => word.starts_with("mkfs"),
    })
}

/// What the user chose in the dialog.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Choice {
    Paste,
    Cancel,
}

/// A paste waiting for the go-ahead.
pub struct PasteWarning {
    pub text: String,
    pub run: bool,
    reasons: Vec<Reason>,
    terminals: usize,
    /// Where the dialog ended up in the last pass, in points.
    rect: Option<Rect>,
}

impl PasteWarning {
    pub fn new(text: String, run: bool, reasons: Vec<Reason>, terminals: usize) -> Self {
        Self { text, run, reasons, terminals, rect: None }
    }

    /// `pos` (points) lies on the dialog as last shown.
    pub fn contains(&self, pos: egui::Pos2) -> bool {
        self.rect.is_some_and(|rect| rect.contains(pos))
    }

    /// Show the dialog at the top of `area` (points). Returns a button
    /// clicked in this pass.
    pub fn show(&mut self, ctx: &egui::Context, area: Rect) -> Option<Choice> {
        let colors = theme::colors();
        let width = WIDTH.min(area.width() - 32.0).max(220.0);
        let mut choice = None;
        let response = Area::new(Id::new("paste_warning"))
            .kind(UiKind::Popup)
            .order(Order::Foreground)
            .pivot(Align2::CENTER_TOP)
            .fixed_pos(egui::pos2(area.center().x, area.top() + 24.0))
            .constrain(true)
            .show(ctx, |ui| {
                Frame::popup(ui.style()).inner_margin(12).show(ui, |ui| {
                    ui.set_width(width);
                    ui.label(RichText::new(t!("paste-warn-title")).strong().size(15.0));
                    ui.add_space(6.0);
                    if self.terminals > 1 {
                        let broadcast = t!("paste-warn-broadcast", terminals = self.terminals);
                        ui.label(RichText::new(broadcast).color(colors.error).strong());
                    }
                    for reason in &self.reasons {
                        ui.label(format!("• {}", reason.label()));
                    }
                    ui.add_space(6.0);
                    Frame::new().fill(colors.input).corner_radius(4).inner_margin(8).show(ui, |ui| {
                        ui.set_width(ui.available_width());
                        ui.label(RichText::new(preview(&self.text)).text_style(TextStyle::Monospace).color(colors.text));
                    });
                    ui.add_space(8.0);
                    ui.horizontal(|ui| {
                        let paste = if self.run { t!("paste-warn-paste-run") } else { t!("paste-warn-paste") };
                        if ui.add(Button::new(RichText::new(paste).strong())).clicked() {
                            choice = Some(Choice::Paste);
                        }
                        if ui.button(t!("common-cancel")).clicked() {
                            choice = Some(Choice::Cancel);
                        }
                    });
                    ui.add_space(4.0);
                    ui.label(RichText::new(t!("paste-warn-hint")).size(11.0).color(colors.text_weak));
                });
            })
            .response;
        self.rect = Some(response.rect);
        choice
    }
}

/// The first lines of `text`, long ones cut, with how many more there are.
fn preview(text: &str) -> String {
    let text = text.trim_end_matches(['\r', '\n']);
    let lines: Vec<&str> = text.lines().collect();
    let mut shown: Vec<String> = lines
        .iter()
        .take(PREVIEW_LINES)
        .map(|line| {
            let line: String = line.chars().filter(|c| !c.is_control() || *c == '\t').collect();
            if line.chars().count() > PREVIEW_COLUMNS {
                line.chars().take(PREVIEW_COLUMNS).chain(std::iter::once('…')).collect()
            } else {
                line
            }
        })
        .collect();
    if lines.len() > PREVIEW_LINES {
        shown.push(t!("paste-warn-more-lines", lines = lines.len() - PREVIEW_LINES));
    }
    shown.join("\n")
}

#[cfg(test)]
mod tests {
    use egui::vec2;

    use super::*;

    #[test]
    fn plain_pastes_pass() {
        assert!(check("ls -la", false, false, 1).is_empty());
        assert!(check("ls -la", false, true, 1).is_empty(), "paste and run of one line was asked for");
        assert!(check("line one\nline two\n", true, false, 1).is_empty(), "the shell shows bracketed lines first");
        assert!(check("echo a || echo b", false, false, 1).is_empty());
        assert!(check("rm -r build", false, false, 1).is_empty());
        assert!(check("pseudo sudoku", false, false, 1).is_empty());
    }

    #[test]
    fn lines_that_run_at_once() {
        assert_eq!(check("cd /\nls\n", false, false, 1), [Reason::RunsLines(2)]);
        assert_eq!(check("reboot\n", false, false, 1), [Reason::RunsLines(1)], "one line with its Enter");
        // Paste and run drops the trailing break: one line is fine.
        assert!(check("reboot\n", false, true, 1).is_empty());
        assert_eq!(check("a\nb", true, true, 1), [Reason::Lines(2)]);
        assert_eq!(check("a\nb", true, false, 3), [Reason::Lines(2)], "several terminals");
    }

    #[test]
    fn risky_commands() {
        assert_eq!(check("sudo pacman -Syu", true, false, 1), [Reason::Sudo]);
        assert_eq!(check("curl -fsSL https://x.sh | sh", true, false, 1), [Reason::PipeToShell]);
        assert_eq!(check("wget -qO- x | sudo -E bash", true, false, 1), [Reason::Sudo, Reason::PipeToShell]);
        assert_eq!(check("curl x|/bin/bash -s", true, false, 1), [Reason::PipeToShell]);
        assert_eq!(check(r#"sh -c "$(curl -fsSL https://x)""#, true, false, 1), [Reason::PipeToShell]);
        for text in ["rm -rf ~/x", "rm -f -r x", "rm --recursive --force x", "rm -Rf /", "mkfs.ext4 /dev/sdb1", "dd if=x of=/dev/sda", ":(){ :|:& };:"] {
            assert!(check(text, true, false, 1).contains(&Reason::Destructive), "{text}");
        }
    }

    #[test]
    fn preview_cuts_long_texts() {
        let text: String = (1..=14).map(|n| format!("line {n}\n")).collect();
        let shown = preview(&text);
        assert_eq!(shown.lines().count(), PREVIEW_LINES + 1);
        assert!(shown.ends_with("4 weitere Zeilen"), "{shown}");
        assert!(preview(&"x".repeat(500)).ends_with('…'));
        assert_eq!(preview("a\x1b[31m\n"), "a[31m", "no escape characters shown");
    }

    #[test]
    fn renders_and_reports_a_click() {
        let ctx = egui::Context::default();
        let mut dialog = PasteWarning::new("sudo rm -rf /\nls".into(), false, check("sudo rm -rf /\nls", false, false, 3), 3);
        let screen = Rect::from_min_size(egui::Pos2::ZERO, vec2(800.0, 600.0));
        let pass = |dialog: &mut PasteWarning, events: Vec<egui::Event>| {
            let input = egui::RawInput { screen_rect: Some(screen), events, ..Default::default() };
            let mut choice = None;
            ctx.run_ui(input, |ui| choice = dialog.show(ui.ctx(), screen).or(choice)).drop_without_applying_deltas();
            choice
        };
        for _ in 0..3 {
            assert_eq!(pass(&mut dialog, Vec::new()), None);
        }
        assert!(dialog.contains(egui::pos2(400.0, 60.0)));
        assert!(!dialog.contains(egui::pos2(5.0, 590.0)));
    }
}
