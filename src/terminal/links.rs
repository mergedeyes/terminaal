//! What's clickable under the mouse: a hyperlink the program set (OSC 8),
//! a URL in the text, or a file or folder that exists -- names relative
//! to the shell's working directory (OSC 7) included.
//!
//! Found on demand for one cell, across wrapped lines; the matched cells
//! get underlined while Ctrl is held and Ctrl+click opens the target.

use std::ops::RangeInclusive;
use std::path::{Path, PathBuf};

use alacritty_terminal::index::{Boundary, Direction, Point};
use alacritty_terminal::term::Term;
use alacritty_terminal::term::search::{RegexIter, RegexSearch};


/// URI schemes that are opened, in text and hyperlinks alike; a program
/// can't make a click start just anything a scheme handler does.
const SCHEMES: [&str; 12] =
    ["ipfs:", "ipns:", "magnet:", "mailto:", "gemini://", "gopher://", "https://", "http://", "news:", "file:", "git://", "ftp://"];
/// Like Alacritty's default URL hint.
const URL: &str = "(ipfs:|ipns:|magnet:|mailto:|gemini://|gopher://|https://|http://|news:|file:|git://|ftp://)\
                   [^\u{0000}-\u{001F}\u{007F}-\u{009F}<>\"\\s{-}\\^⟨⟩`\\\\]+";
/// Anything that could be a file name; whether it is one is up to the disk.
const PATH: &str = "[^\\s'\"`<>|:;,()\\[\\]{}]+";

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Target {
    Uri(String),
    Path(PathBuf),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Link {
    /// The cells, first to last.
    pub cells: RangeInclusive<Point>,
    pub target: Target,
}

/// The regexes, built once.
pub struct Finder {
    url: RegexSearch,
    path: RegexSearch,
}

impl Default for Finder {
    fn default() -> Self {
        Self {
            url: RegexSearch::new(URL).expect("URL regex"),
            path: RegexSearch::new(PATH).expect("path regex"),
        }
    }
}

impl Finder {
    /// The link at `point`. File names count if they exist, relative ones
    /// only with the working directory `cwd` -- and none unless `files`:
    /// over SSH they'd be the server's.
    pub fn at<T>(&mut self, term: &Term<T>, point: Point, files: bool, cwd: Option<&Path>) -> Option<Link> {
        if let Some(link) = hyperlink(term, point) {
            return Some(link);
        }
        let start = term.line_search_left(point);
        let end = term.line_search_right(point);
        if let Some(cells) = containing(term, &mut self.url, start, end, point) {
            let (cells, text) = trim_url(term, cells);
            if cells.contains(&point) {
                return Some(Link { cells, target: Target::Uri(text) });
            }
        }
        if !files {
            return None;
        }
        let cells = containing(term, &mut self.path, start, end, point)?;
        let path = resolve(&text_of(term, &cells), cwd)?;
        Some(Link { cells, target: Target::Path(path) })
    }
}

/// The cells around `point` that carry the same OSC 8 hyperlink.
fn hyperlink<T>(term: &Term<T>, point: Point) -> Option<Link> {
    let grid = term.grid();
    let link = grid[point].hyperlink().filter(|link| SCHEMES.iter().any(|scheme| link.uri().starts_with(scheme)))?;
    let same = |p: Point| grid[p].hyperlink().is_some_and(|other| other == link);
    let (mut first, mut last) = (point, point);
    loop {
        let prev = first.sub(term, Boundary::Grid, 1);
        if prev == first || !same(prev) {
            break;
        }
        first = prev;
    }
    loop {
        let next = last.add(term, Boundary::Grid, 1);
        if next == last || !same(next) {
            break;
        }
        last = next;
    }
    Some(Link { cells: first..=last, target: Target::Uri(link.uri().to_string()) })
}

/// The match of `regex` between `start` and `end` that covers `point`.
fn containing<T>(
    term: &Term<T>,
    regex: &mut RegexSearch,
    start: Point,
    end: Point,
    point: Point,
) -> Option<RangeInclusive<Point>> {
    RegexIter::new(start, end, Direction::Right, term, regex)
        .take_while(|found| *found.start() <= point)
        .find(|found| found.contains(&point))
}

fn text_of<T>(term: &Term<T>, cells: &RangeInclusive<Point>) -> String {
    term.bounds_to_string(*cells.start(), *cells.end()).replace('\n', "")
}

/// A URL without what usually follows one in prose: a full stop, a comma,
/// a closing bracket it doesn't open.
fn trim_url<T>(term: &Term<T>, cells: RangeInclusive<Point>) -> (RangeInclusive<Point>, String) {
    let mut text = text_of(term, &cells);
    let mut end = *cells.end();
    while let Some(last) = text.chars().last() {
        let unbalanced = |open, close| last == close && text.matches(open).count() < text.matches(close).count();
        if !(matches!(last, '.' | ',' | ':' | ';' | '!' | '?' | '\'') || unbalanced('(', ')') || unbalanced('[', ']')) {
            break;
        }
        text.pop();
        end = end.sub(term, Boundary::Grid, 1);
    }
    (*cells.start()..=end, text)
}

/// `name` as an existing file or folder: `~/` expanded, relative to `cwd`.
fn resolve(name: &str, cwd: Option<&Path>) -> Option<PathBuf> {
    let path = match name.strip_prefix("~/").or(if name == "~" { Some("") } else { None }) {
        Some(rest) => PathBuf::from(std::env::var_os("HOME")?).join(rest),
        None if name.starts_with('/') => PathBuf::from(name),
        None => cwd?.join(name),
    };
    path.exists().then_some(path)
}

#[cfg(test)]
mod tests {
    use alacritty_terminal::event::VoidListener;
    use alacritty_terminal::index::{Column, Line};
    use alacritty_terminal::term::Config;
    use alacritty_terminal::vte::ansi::{Processor, StdSyncHandler};

    use super::*;
    use crate::terminal::GridSize;

    fn term(output: &str) -> Term<VoidListener> {
        let size = GridSize { columns: 30, screen_lines: 5 };
        let mut term = Term::new(Config::default(), &size, VoidListener);
        Processor::<StdSyncHandler>::new().advance(&mut term, output.replace('\n', "\r\n").as_bytes());
        term
    }

    fn at(line: i32, column: usize) -> Point {
        Point::new(Line(line), Column(column))
    }

    #[test]
    fn finds_urls_without_trailing_punctuation() {
        // The first line wraps after 30 columns, " ok" is row 1.
        let term = term("see https://example.com/a_(b). ok\n(http://x.org/y), done");
        let mut finder = Finder::default();
        let link = finder.at(&term, at(0, 10), true, None).unwrap();
        assert_eq!(link.target, Target::Uri("https://example.com/a_(b)".into()));
        assert_eq!(link.cells, at(0, 4)..=at(0, 28));
        let link = finder.at(&term, at(2, 3), true, None).unwrap();
        assert_eq!(link.target, Target::Uri("http://x.org/y".into()));
        assert_eq!(finder.at(&term, at(0, 1), true, None), None);
    }

    #[test]
    fn a_url_wrapped_onto_the_next_line() {
        // 30 columns: the URL goes on past the end of the first row.
        let term = term("xx https://example.com/a/very/long/path end");
        let link = Finder::default().at(&term, at(1, 2), true, None).unwrap();
        assert_eq!(link.target, Target::Uri("https://example.com/a/very/long/path".into()));
        assert_eq!(link.cells, at(0, 3)..=at(1, 8));
    }

    #[test]
    fn hyperlinks_the_program_set() {
        let term = term("go \x1b]8;;https://docs.rs\x07here\x1b]8;;\x07 now \x1b]8;;terminaal-prompt:\x07$ \x1b]8;;\x07");
        let mut finder = Finder::default();
        let link = finder.at(&term, at(0, 5), true, None).unwrap();
        assert_eq!(link, Link { cells: at(0, 3)..=at(0, 6), target: Target::Uri("https://docs.rs".into()) });
        // Prompt marks aren't links, and neither is an unknown scheme.
        assert_eq!(finder.at(&term, at(0, 12), true, None), None);
        let term = self::term("\x1b]8;;steam://run/1\x07game\x1b]8;;\x07");
        assert_eq!(finder.at(&term, at(0, 1), true, None), None);
    }

    #[test]
    fn existing_files_relative_to_the_working_directory() {
        let dir = std::env::temp_dir().join(format!("terminaal-links-{}", std::process::id()));
        std::fs::create_dir_all(dir.join("src")).unwrap();
        std::fs::write(dir.join("src/main.rs"), "").unwrap();
        let term = term(&format!("src/main.rs:12:5 missing.rs\n{}/src", dir.display()));
        let mut finder = Finder::default();
        let link = finder.at(&term, at(0, 4), true, Some(&dir)).unwrap();
        assert_eq!(link, Link { cells: at(0, 0)..=at(0, 10), target: Target::Path(dir.join("src/main.rs")) });
        assert_eq!(finder.at(&term, at(0, 20), true, Some(&dir)), None, "doesn't exist");
        assert_eq!(finder.at(&term, at(0, 4), true, None), None, "relative without a working directory");
        let absolute = finder.at(&term, at(1, 2), true, None).unwrap();
        assert_eq!(absolute.target, Target::Path(dir.join("src")));
        assert_eq!(finder.at(&term, at(1, 2), false, None), None, "no files over SSH");
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
