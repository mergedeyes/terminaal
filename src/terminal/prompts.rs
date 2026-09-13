//! Prompts in the grid, found by their marks (`terminal::integration`):
//! jumping from one to the next, and how the command typed at each one
//! ended.
//!
//! A prompt's row is the first one its mark is on: the mark is a
//! hyperlink, and a row whose row above has the same one is its
//! continuation (a wrapped prompt).

use alacritty_terminal::event::EventListener;
use alacritty_terminal::grid::{Dimensions, Scroll};
use alacritty_terminal::index::{Column, Line};
use alacritty_terminal::term::Term;
use alacritty_terminal::term::cell::Hyperlink;

use crate::terminal::integration::{PROMPT_SCHEME, mark_exit};

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
pub fn visible<T>(term: &Term<T>) -> Vec<(usize, Option<i32>)> {
    let offset = term.grid().display_offset() as i32;
    let (top, bottom) = (-offset, term.screen_lines() as i32 - 1 - offset);
    let mut prompts: Vec<(usize, Option<i32>)> = Vec::new();
    let set_previous = |prompts: &mut Vec<(usize, Option<i32>)>, link: &Hyperlink| {
        if let Some(last) = prompts.last_mut() {
            last.1 = mark_exit(link.uri());
        }
    };
    for line in top..=bottom {
        if let Some(link) = prompt_at(term, Line(line)) {
            set_previous(&mut prompts, &link);
            prompts.push(((line - top) as usize, None));
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
        assert_eq!(visible(&term), [(3, None)]);

        let mut term = term;
        term.scroll_display(Scroll::Top);
        // "$ true", "$ false", "$ ls", "a".
        assert_eq!(visible(&term), [(0, Some(0)), (1, Some(1)), (2, Some(0))]);
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
        assert_eq!(visible(&term), [(0, None), (2, None)]);
    }
}
