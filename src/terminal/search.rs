//! Search in the scrollback: the query, the match in focus, how many
//! there are and the matches on screen for highlighting. The searching
//! itself is alacritty_terminal's (`Term::search_next`, `RegexIter`); the
//! query is taken literally -- unless regex mode is on ([`Search::toggle_regex`])
//! -- and ignores case unless it has an uppercase letter.
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
/// How many matches the bar counts before it says "and more".
const MAX_COUNTED: usize = 1000;

/// How many matches there are in the scrollback and which one has the
/// focus ([`Search::counted`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Counted {
    pub total: usize,
    /// Counting stopped early -- at [`MAX_COUNTED`] matches, or at the
    /// lines a keystroke may look through: there are at least `total`.
    pub capped: bool,
    /// The focused match, counted from 1; `None` if it's past the cap.
    pub index: Option<usize>,
}

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
    /// The query is a regular expression as typed, not text to find.
    regex_mode: bool,
    /// The query is regex mode's and doesn't compile.
    invalid: bool,
    /// How many matches there are, for the bar; `None` without a query.
    counted: Option<Counted>,
}

impl Search {
    pub fn new<T>(term: &Term<T>) -> Self {
        let offset = term.grid().display_offset();
        let origin = Point::new(Line(term.screen_lines() as i32 - 1 - offset as i32), term.last_column());
        Self {
            query: String::new(),
            regex: None,
            focus: None,
            origin,
            start_offset: offset,
            editing: true,
            regex_mode: false,
            invalid: false,
            counted: None,
        }
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

    /// The query is read as a regular expression.
    pub fn regex_mode(&self) -> bool {
        self.regex_mode
    }

    /// Regex mode, and the query isn't a regular expression (yet).
    pub fn invalid(&self) -> bool {
        self.invalid
    }

    /// How many matches there are, for the bar.
    pub fn counted(&self) -> Option<Counted> {
        self.counted
    }

    /// Read the query as a regular expression from now on, or as plain
    /// text again, and search anew.
    pub fn toggle_regex<T: EventListener>(&mut self, term: &mut Term<T>) {
        self.regex_mode = !self.regex_mode;
        self.update(term);
    }

    /// What is handed to the regex engine: the query itself in regex
    /// mode, else every character of it taken literally.
    fn pattern(&self) -> String {
        if self.regex_mode { self.query.clone() } else { escape(&self.query) }
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
        self.invalid = false;
        self.counted = None;
        if self.query.is_empty() {
            let delta = self.start_offset as i32 - term.grid().display_offset() as i32;
            term.scroll_display(Scroll::Delta(delta));
            return;
        }
        match RegexSearch::new(&self.pattern()) {
            Ok(regex) => self.regex = Some(regex),
            Err(err) => {
                // Halfway typed, `a|` and the like don't compile; in
                // plain-text mode nothing a user can type does.
                self.invalid = true;
                return log::debug!("search for {:?} isn't a regex (yet): {err}", self.query);
            }
        }
        let regex = self.regex.as_mut().expect("just set");
        self.focus =
            term.search_next(regex, self.origin, Direction::Left, Side::Left, Some(MAX_LINES_WHILE_TYPING));
        self.reveal(term);
        // Every keystroke counts anew, so it looks through no more of
        // the scrollback than the search itself does; the exact number
        // comes with the first jump.
        self.recount(term, Some(MAX_LINES_WHILE_TYPING));
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
        self.recount(term, None);
    }

    /// Count the matches and find the focused one among them, in one
    /// pass. `lines` bounds how far up the scrollback that looks, and
    /// [`MAX_COUNTED`] how many matches are counted; either bound makes
    /// the count a lower bound, which the bar shows as "and more".
    fn recount<T>(&mut self, term: &Term<T>, lines: Option<usize>) {
        self.counted = None;
        let focus = self.focus.as_ref().map(|focus| *focus.start());
        let Some(regex) = self.regex.as_mut() else { return };
        let bottom = term.bottommost_line();
        let top = match lines {
            Some(lines) => term.topmost_line().max(bottom - lines as i32),
            None => term.topmost_line(),
        };
        let start = Point::new(top, Column(0));
        let end = Point::new(bottom, term.last_column());
        let mut capped = top > term.topmost_line();
        let (mut total, mut index) = (0, None);
        for found in RegexIter::new(start, end, Direction::Right, term, regex) {
            total += 1;
            if Some(*found.start()) == focus {
                index = Some(total);
            }
            if total >= MAX_COUNTED {
                capped = true;
                break;
            }
        }
        self.counted = Some(Counted { total, capped, index });
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
        term_with_history(output, 100)
    }

    fn term_with_history(output: &str, scrolling_history: usize) -> Term<VoidListener> {
        let size = GridSize { columns: 20, screen_lines: 5 };
        let mut term = Term::new(Config { scrolling_history, ..Config::default() }, &size, VoidListener);
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
    fn counts_the_matches_and_which_one_has_the_focus() {
        let mut term = numbered();
        let mut search = Search::new(&term);
        search.push_str(&mut term, "line 1");
        // "line 1" and "line 10".."line 11": three in all, the last of
        // them found first (typing searches upwards).
        assert_eq!(search.counted(), Some(Counted { total: 3, capped: false, index: Some(3) }));
        search.jump(&mut term, true);
        assert_eq!(search.counted(), Some(Counted { total: 3, capped: false, index: Some(2) }));
        // A query nothing matches counts zero, which is what the bar
        // turns into "no matches".
        search.push_str(&mut term, "zzz");
        assert_eq!(search.counted(), Some(Counted { total: 0, capped: false, index: None }));
        assert!(search.no_match());
    }

    #[test]
    fn counting_stops_at_the_cap() {
        let lines: String = (0..MAX_COUNTED + 100).map(|i| format!("hit {i}\n")).collect();
        let mut term = term_with_history(&lines, MAX_COUNTED + 200);
        let mut search = Search::new(&term);
        search.push_str(&mut term, "hit");
        let counted = search.counted().expect("a query");
        assert_eq!((counted.total, counted.capped), (MAX_COUNTED, true));
    }

    #[test]
    fn regex_mode_reads_the_query_as_a_pattern() {
        let mut term = numbered();
        let mut search = Search::new(&term);
        // Taken literally, the brackets are just characters.
        search.push_str(&mut term, "line 1[01]");
        assert!(search.no_match());
        search.toggle_regex(&mut term);
        assert!(search.regex_mode() && !search.invalid());
        assert_eq!(search.counted().map(|c| c.total), Some(2));
        // Back to plain text, and the same query finds nothing again.
        search.toggle_regex(&mut term);
        assert!(search.no_match());
    }

    #[test]
    fn a_half_typed_pattern_is_not_an_error_message() {
        let mut term = numbered();
        let mut search = Search::new(&term);
        search.toggle_regex(&mut term);
        search.push_str(&mut term, "line (1");
        assert!(search.invalid() && search.focus().is_none() && search.counted().is_none());
        search.push_str(&mut term, "0)");
        assert!(!search.invalid());
        assert_eq!(search.counted().map(|c| c.total), Some(1));
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
