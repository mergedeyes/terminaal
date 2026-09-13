//! Search in the scrollback: the query, the match in focus and the
//! matches on screen for highlighting. The searching itself is
//! alacritty_terminal's (`Term::search_next`, `RegexIter`); the query is
//! taken literally, ignoring case unless it has an uppercase letter.
//!
//! Directions follow the scrollback: "older" is up, towards the start of
//! the history, where searching begins -- the latest output is at the
//! bottom.

use alacritty_terminal::event::EventListener;
use alacritty_terminal::grid::{Dimensions, Scroll};
use alacritty_terminal::index::{Boundary, Column, Direction, Line, Point, Side};
use alacritty_terminal::term::Term;
use alacritty_terminal::term::search::{Match, RegexIter, RegexSearch};

/// Lines searched per keystroke while typing, like Alacritty; Enter
/// searches all of them.
const MAX_LINES_WHILE_TYPING: usize = 1000;
/// Wrapped lines followed past the top and bottom of the screen for
/// matches that are only partly on it.
const MAX_WRAPPED_LINES: i32 = 100;

pub struct Search {
    query: String,
    /// `None` while the query is empty.
    regex: Option<RegexSearch>,
    focus: Option<Match>,
    /// Where typing searches from: at first the bottom right of the
    /// screen, so each keystroke finds the nearest match above it; after
    /// a jump the match jumped to.
    origin: Point,
    /// How far the view was scrolled back when the search opened; an
    /// emptied query goes back there.
    start_offset: usize,
    /// Keys edit the query; otherwise they jump (n/N).
    editing: bool,
}

impl Search {
    pub fn new<T>(term: &Term<T>) -> Self {
        let offset = term.grid().display_offset();
        let origin = Point::new(Line(term.screen_lines() as i32 - 1 - offset as i32), term.last_column());
        Self { query: String::new(), regex: None, focus: None, origin, start_offset: offset, editing: true }
    }

    pub fn query(&self) -> &str {
        &self.query
    }

    pub fn editing(&self) -> bool {
        self.editing
    }

    pub fn set_editing(&mut self, editing: bool) {
        self.editing = editing;
    }

    pub fn focus(&self) -> Option<&Match> {
        self.focus.as_ref()
    }

    /// There's a query, but nothing matches it.
    pub fn no_match(&self) -> bool {
        !self.query.is_empty() && self.focus.is_none()
    }

    /// Add typed or pasted text to the query. Line breaks and other
    /// control characters are dropped: the grid has no line breaks to
    /// match.
    pub fn push_str<T: EventListener>(&mut self, term: &mut Term<T>, text: &str) {
        let before = self.query.len();
        self.query.extend(text.chars().filter(|c| !c.is_control()));
        if self.query.len() != before {
            self.update(term);
        }
    }

    /// Backspace: the query's last character goes.
    pub fn pop<T: EventListener>(&mut self, term: &mut Term<T>) {
        if self.query.pop().is_some() {
            self.update(term);
        }
    }

    /// Search anew for the changed query from `origin`.
    fn update<T: EventListener>(&mut self, term: &mut Term<T>) {
        self.focus = None;
        self.regex = None;
        if self.query.is_empty() {
            let delta = self.start_offset as i32 - term.grid().display_offset() as i32;
            term.scroll_display(Scroll::Delta(delta));
            return;
        }
        match RegexSearch::new(&escape(&self.query)) {
            Ok(regex) => self.regex = Some(regex),
            Err(err) => return log::warn!("failed to build search for {:?}: {err}", self.query),
        }
        let regex = self.regex.as_mut().expect("just set");
        self.focus =
            term.search_next(regex, self.origin, Direction::Left, Side::Left, Some(MAX_LINES_WHILE_TYPING));
        self.reveal(term);
    }

    /// Move the focus to the next match up (`older`) or down, around the
    /// ends of the scrollback, and scroll it into view.
    pub fn jump<T: EventListener>(&mut self, term: &mut Term<T>, older: bool) {
        let Some(regex) = self.regex.as_mut() else { return };
        let direction = if older { Direction::Left } else { Direction::Right };
        // One cell past the focus, so it doesn't find itself again.
        let origin = match &self.focus {
            Some(focus) if older => focus.start().sub(term, Boundary::None, 1),
            Some(focus) => focus.start().add(term, Boundary::None, 1),
            None => self.origin,
        };
        self.focus = term.search_next(regex, origin, direction, Side::Left, None);
        if let Some(focus) = &self.focus {
            self.origin = *focus.start();
        }
        self.reveal(term);
    }

    /// Scroll so the focus is on screen: left alone if it already is,
    /// otherwise in the middle, with lines above and below it to read.
    fn reveal<T: EventListener>(&self, term: &mut Term<T>) {
        let Some(focus) = &self.focus else { return };
        let offset = term.grid().display_offset() as i32;
        let screen_lines = term.screen_lines() as i32;
        let row = focus.start().line.0 + offset;
        if (0..screen_lines).contains(&row) {
            return;
        }
        let wanted_offset = screen_lines / 2 - focus.start().line.0;
        term.scroll_display(Scroll::Delta(wanted_offset - offset));
    }

    /// The matches touching the screen, first to last -- also those
    /// only partly on it, through a line wrapped from above or below.
    pub fn visible_matches<T>(&mut self, term: &Term<T>) -> Vec<Match> {
        let Some(regex) = self.regex.as_mut() else { return Vec::new() };
        let offset = term.grid().display_offset() as i32;
        let top = Line(-offset);
        let bottom = Line(term.screen_lines() as i32 - 1 - offset);
        let start = term.line_search_left(Point::new(top, Column(0)));
        let start = start.max(Point::new(top - MAX_WRAPPED_LINES, Column(0)));
        let end = term.line_search_right(Point::new(bottom, term.last_column()));
        let end = end.min(Point::new(bottom + MAX_WRAPPED_LINES, term.last_column()));
        RegexIter::new(start, end, Direction::Right, term, regex).collect()
    }
}

/// `text` as a regex matching just that text.
fn escape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        if "\\.+*?()|[]{}^$#&-~".contains(c) {
            out.push('\\');
        }
        out.push(c);
    }
    out
}

#[cfg(test)]
mod tests {
    use alacritty_terminal::event::VoidListener;
    use alacritty_terminal::term::Config;
    use alacritty_terminal::vte::ansi::Processor;

    use super::*;
    use crate::terminal::GridSize;

    /// A 20×5 terminal that got `output`.
    fn term(output: &str) -> Term<VoidListener> {
        let size = GridSize { columns: 20, screen_lines: 5 };
        let mut term = Term::new(Config { scrolling_history: 100, ..Config::default() }, &size, VoidListener);
        let mut parser: Processor = Processor::new();
        parser.advance(&mut term, output.replace('\n', "\r\n").as_bytes());
        term
    }

    fn at(line: i32, column: usize) -> Point {
        Point::new(Line(line), Column(column))
    }

    /// Lines "line 0" to "line 11" and a prompt: the last 5 rows are on
    /// screen (lines 0..=4), "line 0" is the oldest of 8 in the history.
    fn numbered() -> Term<VoidListener> {
        let lines: String = (0..12).map(|i| format!("line {i}\n")).collect();
        term(&format!("{lines}$ "))
    }

    #[test]
    fn typing_finds_the_nearest_match_above() {
        let mut term = numbered();
        let mut search = Search::new(&term);
        search.push_str(&mut term, "line 1");
        // "line 11" is the last line starting with it, right above the prompt.
        assert_eq!(search.focus(), Some(&(at(3, 0)..=at(3, 5))));
        search.push_str(&mut term, "0");
        assert_eq!(search.focus(), Some(&(at(2, 0)..=at(2, 6))));
        search.pop(&mut term);
        assert_eq!(search.focus(), Some(&(at(3, 0)..=at(3, 5))));
    }

    #[test]
    fn jumps_go_up_and_down_and_around() {
        let mut term = numbered();
        let mut search = Search::new(&term);
        search.push_str(&mut term, "line 1");
        search.jump(&mut term, true);
        assert_eq!(search.focus().map(|m| m.start().line), Some(Line(2))); // line 10
        search.jump(&mut term, true);
        assert_eq!(search.focus().map(|m| m.start().line), Some(Line(-7))); // line 1
        // Scrolled back towards putting it in the middle, as far as the history goes.
        assert_eq!(term.grid().display_offset(), 8);
        // Past the oldest match it wraps around to the newest.
        search.jump(&mut term, true);
        assert_eq!(search.focus().map(|m| m.start().line), Some(Line(3)));
        search.jump(&mut term, false);
        assert_eq!(search.focus().map(|m| m.start().line), Some(Line(-7)));
    }

    #[test]
    fn query_is_literal_and_smart_case() {
        let mut term = term("a.b axb (x)\nFoo foo");
        let mut search = Search::new(&term);
        search.push_str(&mut term, "a.b");
        assert_eq!(search.visible_matches(&term), vec![at(0, 0)..=at(0, 2)]);
        search.pop(&mut term);
        search.pop(&mut term);
        search.pop(&mut term);
        search.push_str(&mut term, "(x)");
        assert_eq!(search.visible_matches(&term), vec![at(0, 8)..=at(0, 10)]);

        let mut search = Search::new(&term);
        search.push_str(&mut term, "foo");
        assert_eq!(search.visible_matches(&term).len(), 2);
        let mut search = Search::new(&term);
        search.push_str(&mut term, "Foo");
        assert_eq!(search.visible_matches(&term), vec![at(1, 0)..=at(1, 2)]);
    }

    #[test]
    fn nothing_found_and_emptied_query() {
        let mut term = numbered();
        term.scroll_display(Scroll::Delta(3));
        let mut search = Search::new(&term);
        search.push_str(&mut term, "nope\n");
        assert_eq!(search.query(), "nope");
        assert!(search.no_match());
        assert!(search.visible_matches(&term).is_empty());

        let mut search = Search::new(&term);
        search.push_str(&mut term, "line 0");
        assert_eq!(term.grid().display_offset(), 8);
        // Emptying the query goes back to where the search started.
        for _ in 0..6 {
            search.pop(&mut term);
        }
        assert!(!search.no_match() && search.focus().is_none());
        assert_eq!(term.grid().display_offset(), 3);
    }

    #[test]
    fn matches_across_a_wrapped_line() {
        // 25 characters wrap after 20 columns; "tuvwxy" spans both rows.
        let mut term = term("abcdefghijklmnopqrstuvwxy");
        let mut search = Search::new(&term);
        search.push_str(&mut term, "tuvwxy");
        assert_eq!(search.visible_matches(&term), vec![at(0, 19)..=at(1, 4)]);
    }
}
