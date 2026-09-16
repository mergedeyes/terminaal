//! Prompts in the grid, found by their marks (`terminal::integration`):
//! jumping from one to the next, how the command typed at each one ended,
//! and each command as a block -- its prompt, what was typed and its
//! output, up to the next prompt.
//!
//! A prompt's row is the first one its mark is on: the mark is a
//! hyperlink, and a row whose row above has the same one is its
//! continuation (a wrapped prompt). The output starts at the cell with the
//! output mark.

use alacritty_terminal::event::EventListener;
use alacritty_terminal::grid::{Dimensions, Scroll};
use alacritty_terminal::index::{Column, Line, Point, Side};
use alacritty_terminal::selection::{Selection, SelectionType};
use alacritty_terminal::term::Term;
use alacritty_terminal::term::cell::Hyperlink;

use crate::terminal::integration::{Finished, OUTPUT_SCHEME, PROMPT_SCHEME, mark_finished};

/// Rows looked at below the screen for the prompt after the last one on
/// it, which knows how its command ended.
const MAX_LOOKAHEAD: i32 = 500;

/// The prompt mark on `line`, if there's one.
fn mark<T>(term: &Term<T>, line: Line) -> Option<Hyperlink> {
    let row = &term.grid()[line];
    (0..term.columns())
        .filter_map(|col| row[Column(col)].hyperlink())
        .find(|link| link.uri().starts_with(PROMPT_SCHEME))
}

/// `point` is on a prompt mark.
pub fn is_prompt_cell<T>(term: &Term<T>, point: Point) -> bool {
    term.grid()[point].hyperlink().is_some_and(|link| link.uri().starts_with(PROMPT_SCHEME))
}

/// The mark on `line` if a prompt starts there.
fn prompt_at<T>(term: &Term<T>, line: Line) -> Option<Hyperlink> {
    let link = mark(term, line)?;
    let above = line.0 > term.topmost_line().0;
    let continued = above && mark(term, line - 1).is_some_and(|prev| prev.id() == link.id());
    (!continued).then_some(link)
}

/// Scroll to the prompt above the top of the screen (`older`) or below
/// it, putting it at the top. Going down past the last prompt scrolls to
/// the bottom. `false` if there's nowhere to go.
pub fn jump<T: EventListener>(term: &mut Term<T>, older: bool) -> bool {
    let offset = term.grid().display_offset() as i32;
    let top = -offset;
    let target = if older {
        (term.topmost_line().0..top).rev().find(|&line| prompt_at(term, Line(line)).is_some())
    } else {
        // A prompt further down than fits at the top counts as reached at
        // the bottom of the scrollback.
        (top + 1..=term.bottommost_line().0).find(|&line| prompt_at(term, Line(line)).is_some())
    };
    let wanted = match target {
        Some(line) => (-line).max(0),
        None if older => return false,
        None => 0,
    };
    if wanted == offset {
        return false;
    }
    term.scroll_display(Scroll::Delta(wanted - offset));
    true
}

/// The prompts on screen: their row and, if the next prompt says so, how
/// the command typed at it ended.
pub fn visible<T>(term: &Term<T>) -> Vec<(usize, Finished)> {
    let offset = term.grid().display_offset() as i32;
    let (top, bottom) = (-offset, term.screen_lines() as i32 - 1 - offset);
    let mut prompts: Vec<(usize, Finished)> = Vec::new();
    let set_previous = |prompts: &mut Vec<(usize, Finished)>, link: &Hyperlink| {
        if let Some(last) = prompts.last_mut() {
            last.1 = mark_finished(link.uri());
        }
    };
    for line in top..=bottom {
        if let Some(link) = prompt_at(term, Line(line)) {
            set_previous(&mut prompts, &link);
            prompts.push(((line - top) as usize, Finished::default()));
        }
    }
    if !prompts.is_empty() {
        let end = term.bottommost_line().0.min(bottom + MAX_LOOKAHEAD);
        if let Some(link) = (bottom + 1..=end).find_map(|line| prompt_at(term, Line(line))) {
            set_previous(&mut prompts, &link);
        }
    }
    prompts
}

/// Commands that ran shorter show no run time at their prompt.
pub const MIN_SHOWN_DURATION: std::time::Duration = std::time::Duration::from_secs(1);

/// A run time as shown at a prompt: `4.2 s`, `12 s`, `3 min 5 s`,
/// `1 h 20 min` (decimal comma in German).
pub fn duration_label(duration: std::time::Duration) -> String {
    let secs = duration.as_secs();
    if secs < 10 {
        let tenths = duration.as_millis() / 100;
        let separator = if crate::i18n::current() == crate::i18n::Language::German { "," } else { "." };
        let value = format!("{}{separator}{}", tenths / 10, tenths % 10);
        crate::i18n::t!("prompt-duration-seconds", value = value)
    } else if secs < 60 {
        crate::i18n::t!("prompt-duration-seconds", value = secs.to_string())
    } else if secs < 3600 {
        crate::i18n::t!("prompt-duration-minutes", minutes = secs / 60, seconds = secs % 60)
    } else {
        crate::i18n::t!("prompt-duration-hours", hours = secs / 3600, minutes = secs / 60 % 60)
    }
}

/// A command in the grid: from its prompt to the next one.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Command {
    /// The prompt's first row.
    pub prompt: Line,
    /// The next prompt's first row.
    pub next: Line,
    /// The first cell of its output, if it printed anything.
    pub output: Option<Point>,
    /// A command ran (not just an empty line); the next prompt knows.
    pub ran: bool,
}

/// The command whose block `line` is in: at or below its prompt, above the
/// next one. `None` above the first prompt and at the last one, whose
/// command hasn't finished.
pub fn command_at<T>(term: &Term<T>, line: Line) -> Option<Command> {
    let prompt = (term.topmost_line().0..=line.0).rev().find(|&l| prompt_at(term, Line(l)).is_some())?;
    let (next, link) =
        (line.0 + 1..=term.bottommost_line().0).find_map(|l| prompt_at(term, Line(l)).map(|link| (l, link)))?;
    let output = (prompt..next).find_map(|l| {
        let row = &term.grid()[Line(l)];
        (0..term.columns())
            .find(|&col| row[Column(col)].hyperlink().is_some_and(|link| link.uri().starts_with(OUTPUT_SCHEME)))
            .map(|col| Point::new(Line(l), Column(col)))
    });
    Some(Command { prompt: Line(prompt), next: Line(next), output, ran: mark_finished(link.uri()).duration.is_some() })
}

/// The last command that ran -- empty lines typed at a prompt since don't
/// count.
pub fn last_command<T>(term: &Term<T>) -> Option<Command> {
    let mut line = term.bottommost_line();
    loop {
        let prompt = (term.topmost_line().0..line.0).rev().find(|&l| prompt_at(term, Line(l)).is_some())?;
        let command = command_at(term, Line(prompt))?;
        if command.ran {
            return Some(command);
        }
        line = Line(prompt);
    }
}

/// The last row of `command`'s block with anything on it.
fn last_row<T>(term: &Term<T>, command: &Command) -> Line {
    let blank = |l: i32| {
        let row = &term.grid()[Line(l)];
        (0..term.columns()).all(|col| matches!(row[Column(col)].c, ' ' | '\t' | '\0'))
    };
    Line((command.prompt.0..command.next.0).rev().find(|&l| !blank(l)).unwrap_or(command.prompt.0))
}

/// `command`'s output as text, without trailing blank lines; empty if it
/// printed nothing.
pub fn output_text<T>(term: &Term<T>, command: &Command) -> String {
    let Some(start) = command.output else { return String::new() };
    let end = last_row(term, command);
    if end < start.line {
        return String::new();
    }
    let text = term.bounds_to_string(start, Point::new(end, term.last_column()));
    text.trim_end().to_string()
}

/// Select `command`'s whole block, prompt to last output line.
pub fn select<T>(term: &mut Term<T>, command: &Command) {
    let end = last_row(term, command);
    let mut selection = Selection::new(SelectionType::Lines, Point::new(command.prompt, Column(0)), Side::Left);
    selection.update(Point::new(end, term.last_column()), Side::Right);
    term.selection = Some(selection);
}

#[cfg(test)]
mod tests {
    use alacritty_terminal::event::VoidListener;
    use alacritty_terminal::term::Config;
    use alacritty_terminal::vte::ansi::Processor;

    use super::*;
    use crate::terminal::GridSize;
    use crate::terminal::integration::Filter;

    /// A 20×4 terminal fed `output` through the integration filter.
    fn term(output: &str) -> Term<VoidListener> {
        let size = GridSize { columns: 20, screen_lines: 4 };
        let mut term = Term::new(Config { scrolling_history: 100, ..Config::default() }, &size, VoidListener);
        let (mut filtered, mut events) = (Vec::new(), Vec::new());
        Filter::default().feed(output.replace('\n', "\r\n").as_bytes(), &mut filtered, &mut events);
        Processor::<alacritty_terminal::vte::ansi::StdSyncHandler>::new().advance(&mut term, &filtered);
        term
    }

    const A: &str = "\x1b]133;A\x07";
    const B: &str = "\x1b]133;B\x07";

    /// Three commands: `true` (0), `false` (1), `ls` with three lines of output.
    fn session() -> String {
        let d = |code: u8| format!("\x1b]133;C\x07\x1b]133;D;{code}\x07");
        format!("{A}$ {B}true\n{}{A}$ {B}false\n{}{A}$ {B}ls\na\nb\nc\n{}{A}$ {B}", d(0), d(1), d(0))
    }

    #[test]
    fn visible_prompts_know_how_their_command_ended() {
        let term = term(&session());
        // 7 lines: "$ true", "$ false", "$ ls", "a", "b", "c", "$ ".
        assert_eq!(term.grid().history_size(), 3);
        // On screen: "a", "b", "c", "$ ".
        let exits = |term: &Term<VoidListener>| -> Vec<(usize, Option<i32>)> {
            visible(term).into_iter().map(|(row, finished)| (row, finished.exit)).collect()
        };
        assert_eq!(exits(&term), [(3, None)]);

        let mut term = term;
        term.scroll_display(Scroll::Top);
        // "$ true", "$ false", "$ ls", "a".
        assert_eq!(exits(&term), [(0, Some(0)), (1, Some(1)), (2, Some(0))]);
        assert!(visible(&term).iter().all(|(_, finished)| finished.duration.is_some()), "each of them ran");
    }

    #[test]
    fn jumping_goes_from_prompt_to_prompt() {
        let mut term = term(&session());
        assert!(jump(&mut term, true));
        assert_eq!(term.grid().display_offset(), 1); // "$ ls" at the top
        assert!(jump(&mut term, true));
        assert_eq!(term.grid().display_offset(), 2);
        assert!(jump(&mut term, true));
        assert_eq!(term.grid().display_offset(), 3);
        assert!(!jump(&mut term, true), "no prompt above the first");
        assert!(jump(&mut term, false));
        assert_eq!(term.grid().display_offset(), 2);
        assert!(jump(&mut term, false));
        assert!(jump(&mut term, false));
        // The last prompt is on the bottom screen: back at the bottom.
        assert_eq!(term.grid().display_offset(), 0);
        assert!(!jump(&mut term, false));
    }

    #[test]
    fn a_wrapped_prompt_is_one_prompt() {
        let long = "x".repeat(30);
        let term = term(&format!("{A}{long}{B}\n{A}$ "));
        let rows: Vec<usize> = visible(&term).into_iter().map(|(row, _)| row).collect();
        assert_eq!(rows, [0, 2]);
    }

    #[test]
    fn run_times_read_well() {
        use std::time::Duration;
        assert_eq!(duration_label(Duration::from_millis(4230)), "4,2 s");
        assert_eq!(duration_label(Duration::from_secs(12)), "12 s");
        assert_eq!(duration_label(Duration::from_secs(185)), "3 min 5 s");
        assert_eq!(duration_label(Duration::from_secs(4800)), "1 h 20 min");
    }

    #[test]
    fn commands_as_blocks_with_their_output() {
        let c = "\x1b]133;C\x07";
        let d = |code: u8| format!("\x1b]133;D;{code}\x07");
        // A command line wrapped over two rows, output of two lines; one
        // without output; an empty line at the last prompt.
        let long = "echo ".to_string() + &"y".repeat(20);
        let output = format!(
            "{A}$ {B}{long}\n{c}first\nsecond\n\n{}{A}$ {B}cd /\n{c}{}{A}$ {B}\n\x1b]133;D;0\x07{A}$ {B}",
            d(0),
            d(0)
        );
        let mut term = term(&output);
        let first = command_at(&term, term.topmost_line()).expect("the first command");
        assert_eq!(output_text(&term, &first), "first\nsecond");
        assert!(first.ran);
        // Its block ends before the blank line.
        select(&mut term, &first);
        // (The wrapped command line comes out as one line.)
        assert_eq!(term.selection_to_string().unwrap(), format!("$ {long}\nfirst\nsecond\n"));

        // The last one that ran printed nothing; the empty line doesn't count.
        let last = last_command(&term).expect("a command ran");
        assert_eq!((last.output, output_text(&term, &last).as_str(), last.ran), (None, "", true));
        assert_eq!(last.prompt, first.next);
        // At the last prompt nothing has finished.
        assert!(command_at(&term, term.bottommost_line()).is_none());

        let without = self::term(&format!("{A}$ ls\nfile\n{A}$ "));
        assert!(last_command(&without).is_none(), "a shell without C and D");
    }
}
