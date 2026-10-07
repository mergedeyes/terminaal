//! Keyboard hints: everything worth picking on the screen -- links,
//! paths, IP addresses, hashes -- gets a label of a letter or two, and
//! typing it copies, opens or inserts that text (`Action::Hints`).
//!
//! Found on the visible screen (wrapped lines included) in this order,
//! later finds skipped where they overlap an earlier one:
//! - hyperlinks the program set (OSC 8) and URLs, trimmed like Ctrl+click
//!   links ([`super::links`]);
//! - paths: absolute, `~/`, `./` or `../`, or any word naming a file or
//!   folder that exists here (relative to the shell's working directory);
//! - IPv4 and IPv6 addresses (valid ones: `12:30:45` isn't);
//! - hashes and IDs: hex of 7 or more with a letter and a digit (git, docker),
//!   UUIDs, MAC addresses.
//!
//! The labels go to the hints from the bottom of the screen up, so the
//! newest output gets the home row's letters.

use std::net::{Ipv4Addr, Ipv6Addr};
use std::ops::RangeInclusive;
use std::path::{Path, PathBuf};

use alacritty_terminal::grid::Dimensions;
use alacritty_terminal::index::{Boundary, Column, Direction, Line, Point};
use alacritty_terminal::term::Term;
use alacritty_terminal::term::search::{RegexIter, RegexSearch};

use super::links::{self, Target};

/// Label letters, the home row first. No y/z: they swap places between
/// German and US layouts.
pub const ALPHABET: &str = "fjdkslaghrueiwoqpcmvnbtx";

const IPV4: &str = "[0-9]{1,3}(\\.[0-9]{1,3}){3}";
const IPV6: &str = "[0-9a-fA-F]{0,4}(:[0-9a-fA-F]{0,4}){2,7}";
const HEX: &str = "[0-9a-f]{7,128}";
const UUID: &str = "[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{12}";
const MAC: &str = "[0-9a-fA-F]{2}(:[0-9a-fA-F]{2}){5}";

/// A find before it's labelled: cells, text, what opening it opens.
type Found = (RangeInclusive<Point>, String, Option<Target>);
/// A regex, whether its match is one (`valid`), and which characters
/// mustn't stand right before or after it.
type Check<'a> = (&'a mut RegexSearch, &'a dyn Fn(&str) -> bool, &'a dyn Fn(char) -> bool);

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Hint {
    /// The cells, first to last.
    pub cells: RangeInclusive<Point>,
    pub text: String,
    /// What opening it opens; `None`: there's nothing to open (an IP, a
    /// hash, a path that isn't here), so it's copied instead.
    pub target: Option<Target>,
    pub label: String,
}

/// The regexes, built once.
pub struct Finder {
    url: RegexSearch,
    path: RegexSearch,
    ipv4: RegexSearch,
    ipv6: RegexSearch,
    hex: RegexSearch,
    uuid: RegexSearch,
    mac: RegexSearch,
}

impl Default for Finder {
    fn default() -> Self {
        let regex = |pattern| RegexSearch::new(pattern).expect("hint regex");
        Self {
            url: regex(links::URL),
            path: regex(links::PATH),
            ipv4: regex(IPV4),
            ipv6: regex(IPV6),
            hex: regex(HEX),
            uuid: regex(UUID),
            mac: regex(MAC),
        }
    }
}

impl Finder {
    /// The hints on `term`'s screen, top to bottom, labelled. Paths that
    /// exist count only with `files` (over SSH they'd be the server's),
    /// relative ones with the working directory `cwd`.
    pub fn find<T>(&mut self, term: &Term<T>, files: bool, cwd: Option<&Path>) -> Vec<Hint> {
        let offset = term.grid().display_offset() as i32;
        let start = Point::new(Line(-offset), Column(0));
        let end = Point::new(Line(term.screen_lines() as i32 - 1 - offset), term.last_column());
        let mut found: Vec<Found> = Vec::new();
        let free = |found: &[Found], cells: &RangeInclusive<Point>| {
            !found.iter().any(|(other, ..)| other.start() <= cells.end() && cells.start() <= other.end())
        };

        // Hyperlinks: the runs of cells carrying one.
        let mut point = start;
        while point <= end {
            if let Some(link) = links::hyperlink(term, point) {
                let next = link.cells.end().add(term, Boundary::Grid, 1);
                if let Target::Uri(uri) = &link.target {
                    found.push((link.cells.clone(), uri.clone(), Some(link.target.clone())));
                }
                if next <= point {
                    break;
                }
                point = next;
            } else {
                let next = point.add(term, Boundary::Grid, 1);
                if next == point {
                    break;
                }
                point = next;
            }
        }

        for cells in RegexIter::new(start, end, Direction::Right, term, &mut self.url).collect::<Vec<_>>() {
            let (cells, text) = links::trim_url(term, cells);
            if free(&found, &cells) {
                found.push((cells, text.clone(), Some(Target::Uri(text))));
            }
        }

        for cells in RegexIter::new(start, end, Direction::Right, term, &mut self.path).collect::<Vec<_>>() {
            let (cells, text) = trim_end(term, cells, &['.', ',']);
            if text.is_empty() || !free(&found, &cells) {
                continue;
            }
            let here = if files { links::resolve(&text, cwd) } else { None };
            let looks_like_one = ["/", "~/", "./", "../"].iter().any(|prefix| text.starts_with(prefix)) && text.len() > 1;
            if here.is_some() || looks_like_one {
                found.push((cells, text, here.map(Target::Path)));
            }
        }

        let word = |c: char| c.is_alphanumeric() || c == '_';
        let checks: [Check; 5] = [
            // A version number like v1.2.3.4 isn't one.
            (&mut self.ipv4, &|text| text.parse::<Ipv4Addr>().is_ok(), &|c| word(c) || c == '.'),
            (
                &mut self.ipv6,
                &|text| text.contains(|c: char| c.is_ascii_hexdigit()) && text.parse::<Ipv6Addr>().is_ok(),
                &|c| word(c) || c == ':',
            ),
            (&mut self.uuid, &|_| true, &|c| word(c) || c == '-'),
            (&mut self.mac, &|_| true, &|c| c.is_ascii_hexdigit() || c == ':'),
            (
                &mut self.hex,
                &|text| text.contains(|c: char| c.is_ascii_digit()) && text.contains(|c: char| c.is_ascii_lowercase()),
                &word,
            ),
        ];
        for (regex, valid, joins) in checks {
            for cells in RegexIter::new(start, end, Direction::Right, term, regex).collect::<Vec<_>>() {
                let text = links::text_of(term, &cells);
                let alone = !neighbour(term, *cells.start(), -1).is_some_and(joins)
                    && !neighbour(term, *cells.end(), 1).is_some_and(joins);
                if alone && valid(&text) && free(&found, &cells) {
                    found.push((cells, text, None));
                }
            }
        }

        found.sort_by_key(|(cells, ..)| *cells.start());
        let labels = labels(found.len());
        // The newest output, at the bottom, gets the first labels.
        found
            .into_iter()
            .rev()
            .zip(labels)
            .map(|((cells, text, target), label)| Hint { cells, text, target, label })
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .collect()
    }
}

/// The character `by` cells before (-1) or after (1) `point`, if there's
/// a cell there.
fn neighbour<T>(term: &Term<T>, point: Point, by: i32) -> Option<char> {
    let other = if by < 0 { point.sub(term, Boundary::Grid, 1) } else { point.add(term, Boundary::Grid, 1) };
    (other != point).then(|| term.grid()[other].c)
}

/// `cells` without the characters in `chars` at their end.
fn trim_end<T>(term: &Term<T>, cells: RangeInclusive<Point>, chars: &[char]) -> (RangeInclusive<Point>, String) {
    let mut text = links::text_of(term, &cells);
    let mut end = *cells.end();
    while text.len() > 1 && text.ends_with(chars) && end > *cells.start() {
        text.pop();
        end = end.sub(term, Boundary::Grid, 1);
    }
    (*cells.start()..=end, text)
}

/// `count` labels, none the beginning of another: one letter each while
/// the alphabet lasts, then all of two (or three) letters.
pub fn labels(count: usize) -> Vec<String> {
    let letters: Vec<char> = ALPHABET.chars().collect();
    let mut length = 1;
    while letters.len().pow(length) < count {
        length += 1;
    }
    (0..count)
        .map(|mut n| {
            let mut label = vec![letters[0]; length as usize];
            for slot in label.iter_mut().rev() {
                *slot = letters[n % letters.len()];
                n /= letters.len();
            }
            label.into_iter().collect()
        })
        .collect()
}

/// What picking a hint does.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Pick {
    Copy,
    /// Open the link or file; copy what can't be opened.
    Open,
    /// Type it into the terminal.
    Insert,
}

/// Hint mode over one pane: its hints and the label typed so far.
pub struct Hints {
    pub pane: usize,
    /// Paths may be its files (a local terminal), relative ones to `cwd`.
    pub files: bool,
    pub cwd: Option<PathBuf>,
    pub hints: Vec<Hint>,
    pub typed: String,
}

impl Hints {
    pub fn new(pane: usize, files: bool, cwd: Option<PathBuf>) -> Self {
        Self { pane, files, cwd, hints: Vec::new(), typed: String::new() }
    }

    /// Take over what's on screen now. The same hints keep their labels
    /// and what's typed; others start over.
    pub fn update(&mut self, hints: Vec<Hint>) {
        let same = hints.len() == self.hints.len()
            && hints.iter().zip(&self.hints).all(|(new, old)| new.cells == old.cells && new.text == old.text);
        if !same {
            self.hints = hints;
            self.typed.clear();
        }
    }

    /// The hints whose label starts with what's typed.
    pub fn shown(&self) -> impl Iterator<Item = &Hint> {
        self.hints.iter().filter(|hint| hint.label.starts_with(&self.typed))
    }

    /// One more letter of a label. The hint it completes, if it does; a
    /// letter no label goes on with is ignored.
    pub fn push(&mut self, letter: char) -> Option<&Hint> {
        self.typed.push(letter);
        if self.shown().next().is_none() {
            self.typed.pop();
            return None;
        }
        self.hints.iter().find(|hint| hint.label == self.typed)
    }

    pub fn pop(&mut self) {
        self.typed.pop();
    }
}

#[cfg(test)]
mod tests {
    use alacritty_terminal::event::VoidListener;
    use alacritty_terminal::term::Config;
    use alacritty_terminal::vte::ansi::{Processor, StdSyncHandler};

    use super::*;
    use crate::terminal::GridSize;

    fn term(output: &str, columns: usize) -> Term<VoidListener> {
        let size = GridSize { columns, screen_lines: 6 };
        let mut term = Term::new(Config::default(), &size, VoidListener);
        Processor::<StdSyncHandler>::new().advance(&mut term, output.replace('\n', "\r\n").as_bytes());
        term
    }

    fn texts(hints: &[Hint]) -> Vec<&str> {
        hints.iter().map(|hint| hint.text.as_str()).collect()
    }

    #[test]
    fn finds_what_is_worth_picking() {
        let term = term(
            "see https://example.com/x. and /etc/hosts.\n\
             inet 192.168.178.20/24 brd 192.168.178.255 fe80::1c2:3d4\n\
             commit 3f2a9c1e at 12:30:45 by deadbeef 1234567\n\
             id 123e4567-e89b-12d3-a456-426614174000 mac 0a:1b:2c:3d:4e:5f\n\
             999.1.1.1 v1.2.3.4x \x1b]8;;https://docs.rs\x07docs\x1b]8;;\x07 ~/x ./run a/b",
            80,
        );
        let hints = Finder::default().find(&term, false, None);
        assert_eq!(
            texts(&hints),
            [
                "https://example.com/x",
                "/etc/hosts",
                "192.168.178.20",
                "192.168.178.255",
                "fe80::1c2:3d4",
                "3f2a9c1e",
                "123e4567-e89b-12d3-a456-426614174000",
                "0a:1b:2c:3d:4e:5f",
                "https://docs.rs",
                "~/x",
                "./run",
            ]
        );
        assert_eq!(hints[0].target, Some(Target::Uri("https://example.com/x".into())));
        assert_eq!(hints[8].target, Some(Target::Uri("https://docs.rs".into())));
        assert_eq!(hints[1].target, None, "files only where they're ours");
        // The bottom gets the first letters.
        assert_eq!(hints.last().unwrap().label, "f");
        assert_eq!(hints[hints.len() - 2].label, "j");
    }

    #[test]
    fn existing_files_open_and_wrapped_urls_count() {
        let dir = std::env::temp_dir().join(format!("terminaal-hints-{}", std::process::id()));
        std::fs::create_dir_all(dir.join("src")).unwrap();
        std::fs::write(dir.join("src/main.rs"), "").unwrap();
        // 30 columns: the URL wraps onto the second row.
        let term = term("xx https://example.com/a/very/long/path\nsrc/main.rs:12 gone.rs", 30);
        let hints = Finder::default().find(&term, true, Some(&dir));
        assert_eq!(texts(&hints), ["https://example.com/a/very/long/path", "src/main.rs"]);
        assert_eq!(hints[1].target, Some(Target::Path(dir.join("src/main.rs"))));
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn labels_are_never_the_start_of_another() {
        assert_eq!(labels(3), ["f", "j", "d"]);
        let many = labels(30);
        assert!(many.iter().all(|label| label.len() == 2));
        assert_eq!(&many[..3], ["ff", "fj", "fd"]);
        let unique: std::collections::HashSet<_> = many.iter().collect();
        assert_eq!(unique.len(), 30);
        assert!(labels(ALPHABET.len() * ALPHABET.len() + 1).iter().all(|label| label.len() == 3));
    }

    #[test]
    fn typing_a_label() {
        let hint = |label: &str| Hint {
            cells: Point::new(Line(0), Column(0))..=Point::new(Line(0), Column(1)),
            text: label.to_uppercase(),
            target: None,
            label: label.into(),
        };
        let mut hints = Hints::new(1, false, None);
        hints.update(vec![hint("ff"), hint("fj"), hint("jf")]);
        assert_eq!(hints.push('x'), None, "no label goes on with it");
        assert_eq!(hints.typed, "");
        assert_eq!(hints.push('f'), None);
        assert_eq!(hints.shown().count(), 2);
        assert_eq!(hints.push('j').map(|hint| hint.text.as_str()), Some("FJ"));
        hints.pop();
        hints.pop();
        assert_eq!(hints.shown().count(), 3);
        // The same hints keep what's typed; others start over.
        hints.push('j');
        hints.update(vec![hint("ff"), hint("fj"), hint("jf")]);
        assert_eq!(hints.typed, "j");
        hints.update(vec![hint("ff")]);
        assert_eq!(hints.typed, "");
    }
}
