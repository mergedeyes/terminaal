//! The 5250 screen as VT output for the pane's `Term`: everything else in
//! Terminaal (drawing, selection, copy, search, themes) then works on it
//! as on any terminal.
//!
//! Each row is built as text with SGR runs; only rows that differ from
//! what was last written go out. Colors are the terminal palette's
//! (green, red, cyan for turquoise, ...), "white" is the theme's own
//! foreground, bold -- readable on light themes too. Below the screen,
//! if the pane has room, the status line (IBM's operator information
//! area): X SYSTEM while the server works, X II and the reason when input
//! is inhibited, insert mode, message waiting, the cursor position.

use crate::i18n::t;

use super::screen::{
    ATTR_NORMAL, ERR_ALPHA_ONLY, ERR_DIGITS_ONLY, ERR_EXIT_REQUIRED, ERR_FIELD_MINUS, ERR_NO_ROOM, ERR_NUMERIC_ONLY,
    ERR_PROTECTED, ERR_SIGN_POSITION, ERR_UNDEFINED_CHARACTER, Inhibit, Keyboard, Screen,
};

/// The screen starts empty and the keyboard locked, before the server's
/// first record.
pub struct Renderer {
    /// The pane, in cells.
    pane: (usize, usize),
    /// What each pane row shows now; `None`: unknown, write it.
    shown: Vec<Option<String>>,
    /// Shown instead of the status line: the connection is gone.
    pub notice: Option<String>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Color {
    Green,
    White,
    Red,
    Turquoise,
    Yellow,
    Pink,
    Blue,
}

/// How an attribute byte (0x20..=0x3F) shows what follows it.
#[derive(Clone, Copy, PartialEq, Eq)]
struct Look {
    color: Color,
    reverse: bool,
    underline: bool,
    hidden: bool,
}

fn look(attr: u8) -> Look {
    use Color::*;
    let low = attr & 0x07;
    // Every eighth is nondisplay: password fields.
    let hidden = low == 0x07;
    let (color, reverse, underline) = match attr & 0x1F {
        0x00..=0x07 => {
            let color = if low & 0x02 != 0 { White } else { Green };
            (color, low & 0x01 != 0, low & 0x04 != 0)
        }
        0x08..=0x0F => (Red, low & 0x01 != 0, low & 0x04 != 0),
        0x10..=0x17 => {
            let color = if low & 0x02 != 0 { Yellow } else { Turquoise };
            (color, low & 0x01 != 0, low & 0x04 != 0)
        }
        _ => {
            let color = if low & 0x02 != 0 { Blue } else { Pink };
            (color, low & 0x01 != 0, low & 0x04 != 0)
        }
    };
    // Nothing of a nondisplay field shows, not even its highlight.
    Look { color, reverse: reverse && !hidden, underline: underline && !hidden, hidden }
}

fn sgr(look: Look) -> String {
    let color = match look.color {
        Color::Green => "32",
        Color::White => "1",
        Color::Red => "31",
        Color::Turquoise => "36",
        Color::Yellow => "33",
        Color::Pink => "35",
        Color::Blue => "34",
    };
    let mut out = format!("\x1b[0;{color}");
    if look.reverse {
        out.push_str(";7");
    }
    if look.underline {
        out.push_str(";4");
    }
    out.push('m');
    out
}

impl Renderer {
    pub fn new(columns: usize, lines: usize) -> Self {
        Self { pane: (columns, lines), shown: vec![None; lines], notice: None }
    }

    /// The pane has a new size: everything is written again.
    pub fn resize(&mut self, columns: usize, lines: usize) {
        self.pane = (columns, lines);
        self.shown = vec![None; lines];
    }

    /// Forget what's shown (the `Term` was written to otherwise).
    pub fn invalidate(&mut self) {
        self.shown.iter_mut().for_each(|row| *row = None);
    }

    /// What to write to bring the pane up to date with `screen`.
    pub fn frame(&mut self, screen: &Screen) -> Vec<u8> {
        let (columns, lines) = self.pane;
        let mut rows: Vec<String> = Vec::with_capacity(lines);
        let mut attr = ATTR_NORMAL;
        for row in 0..screen.rows.min(lines) {
            let mut text = String::new();
            let mut current = None;
            for col in 0..screen.cols {
                let byte = screen.cell(row * screen.cols + col);
                if (0x20..=0x3F).contains(&byte) {
                    attr = byte;
                }
                if col >= columns {
                    continue;
                }
                // The attribute's own position is a plain blank.
                let (style, c) = if (0x20..=0x3F).contains(&byte) {
                    (None, ' ')
                } else {
                    let look = look(attr);
                    let c = match byte {
                        _ if look.hidden => ' ',
                        // DUP shows as an overstruck asterisk.
                        0x1C => '*',
                        _ => screen.codepage.to_char(byte),
                    };
                    (Some(look), c)
                };
                if style != current {
                    text.push_str(&style.map_or_else(|| "\x1b[0m".to_string(), sgr));
                    current = style;
                }
                text.push(c);
            }
            rows.push(text);
        }
        // The status line under the screen, if there's room; a notice
        // (connection gone) takes the last row if there isn't.
        let status_row = if lines > screen.rows { Some(screen.rows) } else { self.notice.as_ref().map(|_| lines - 1) };
        if let Some(status_row) = status_row {
            rows.truncate(status_row);
            rows.resize(status_row, String::new());
            rows.push(self.status(screen, columns));
        }
        rows.resize(lines, String::new());

        let mut out = Vec::new();
        for (index, row) in rows.into_iter().enumerate() {
            if self.shown[index].as_ref() == Some(&row) {
                continue;
            }
            out.extend(format!("\x1b[{};1H{row}\x1b[0m\x1b[K", index + 1).into_bytes());
            self.shown[index] = Some(row);
        }
        // The cursor: a bar in insert mode, as editors do.
        let (row, col) = screen.row_col(screen.cursor);
        let shape = if screen.insert { 5 } else { 0 };
        out.extend(format!("\x1b[{shape} q\x1b[{};{}H", row + 1, col.min(columns.saturating_sub(1)) + 1).into_bytes());
        out
    }

    fn status(&self, screen: &Screen, columns: usize) -> String {
        if let Some(notice) = &self.notice {
            return format!("\x1b[0;31m{}", clip(notice, columns));
        }
        let mut left = match screen.keyboard {
            Keyboard::Unlocked => String::new(),
            Keyboard::Locked => t!("tn5250-system"),
            Keyboard::Inhibited(Inhibit::Host) => format!("{}  {}", t!("tn5250-inhibited"), t!("tn5250-reset-hint")),
            Keyboard::Inhibited(Inhibit::Operator(code)) => {
                format!("{} {code:04X}  {}  {}", t!("tn5250-inhibited"), operator_error(code), t!("tn5250-reset-hint"))
            }
        };
        if screen.insert {
            left.push_str(&format!("  {}", t!("tn5250-insert")));
        }
        if screen.message_waiting {
            left.push_str(&format!("  {}", t!("tn5250-message-waiting")));
        }
        let (row, col) = screen.row_col(screen.cursor);
        let right = format!("{:02}/{:03}", row + 1, col + 1);
        let width = screen.cols.min(columns);
        let left = clip(&left, width.saturating_sub(right.len() + 1));
        let gap = width.saturating_sub(left.chars().count() + right.len());
        let color = if matches!(screen.keyboard, Keyboard::Inhibited(_)) { "31" } else { "2" };
        format!("\x1b[0;{color}m{left}\x1b[0;2m{}{right}", " ".repeat(gap))
    }
}

fn clip(text: &str, width: usize) -> String {
    text.chars().take(width).collect()
}

/// The text for an operator error code.
fn operator_error(code: u16) -> String {
    match code {
        ERR_PROTECTED => t!("tn5250-error-protected"),
        ERR_ALPHA_ONLY => t!("tn5250-error-alpha-only"),
        ERR_NUMERIC_ONLY => t!("tn5250-error-numeric-only"),
        ERR_DIGITS_ONLY => t!("tn5250-error-digits-only"),
        ERR_SIGN_POSITION => t!("tn5250-error-sign-position"),
        ERR_NO_ROOM => t!("tn5250-error-no-room"),
        ERR_FIELD_MINUS => t!("tn5250-error-field-minus"),
        ERR_EXIT_REQUIRED => t!("tn5250-error-exit-required"),
        ERR_UNDEFINED_CHARACTER => t!("tn5250-error-undefined-character"),
        _ => String::new(),
    }
}

#[cfg(test)]
mod tests {
    use alacritty_terminal::event::VoidListener;
    use alacritty_terminal::grid::Dimensions;
    use alacritty_terminal::index::{Column, Line, Point};
    use alacritty_terminal::term::cell::Flags;
    use alacritty_terminal::term::{Config, Term};
    use alacritty_terminal::vte::ansi::{Color as TermColor, NamedColor, Processor};

    use super::super::screen::Key;
    use super::super::screen::tests::{screen, sign_on};
    use super::*;
    use crate::terminal::session::GridSize;

    fn term(columns: usize, lines: usize) -> Term<VoidListener> {
        Term::new(Config::default(), &GridSize { columns, screen_lines: lines }, VoidListener)
    }

    fn row_text(term: &Term<VoidListener>, line: usize) -> String {
        let row = &term.grid()[Line(line as i32)];
        (0..term.columns()).map(|col| row[Column(col)].c).collect::<String>().trim_end().to_string()
    }

    #[test]
    fn draws_the_screen_with_its_attributes_into_a_term() {
        let mut s = screen();
        s.record(&sign_on());
        for c in "secret".chars() {
            s.key(Key::Char(c));
        }
        let mut term = term(100, 30);
        let mut renderer = Renderer::new(100, 30);
        let mut parser: Processor = Processor::new();
        parser.advance(&mut term, &renderer.frame(&s));

        assert_eq!(row_text(&term, 0), format!("{}Sign On", " ".repeat(30)));
        // White: bold in the theme's own color.
        let cell = &term.grid()[Line(0)][Column(30)];
        assert!(cell.flags.contains(Flags::BOLD));
        assert_eq!(cell.fg, TermColor::Named(NamedColor::Foreground));
        // The user field: green, underlined, typed text visible.
        let cell = &term.grid()[Line(5)][Column(52)];
        assert_eq!(cell.c, 'S');
        assert_eq!(cell.fg, TermColor::Named(NamedColor::Green));
        assert!(cell.flags.contains(Flags::UNDERLINE));
        // The status line under the screen.
        assert_eq!(row_text(&term, 24), format!("{}06/059", " ".repeat(74)));
        let cursor = term.grid().cursor.point;
        assert_eq!(cursor, Point::new(Line(5), Column(58)));

        // Unchanged: nothing but the cursor goes out again.
        let again = String::from_utf8(renderer.frame(&s)).unwrap();
        assert!(!again.contains("Sign On"), "{again:?}");
    }

    #[test]
    fn hides_nondisplay_fields() {
        let mut s = screen();
        s.record(&sign_on());
        s.key(Key::Tab);
        for c in "secret".chars() {
            s.key(Key::Char(c));
        }
        let mut term = term(80, 24);
        let mut parser: Processor = Processor::new();
        parser.advance(&mut term, &Renderer::new(80, 24).frame(&s));
        assert_eq!(row_text(&term, 6), "");
    }

    #[test]
    fn a_small_pane_shows_what_fits_and_the_notice_on_its_last_row() {
        let mut s = screen();
        s.record(&sign_on());
        let mut term = term(40, 10);
        let mut renderer = Renderer::new(40, 10);
        renderer.notice = Some("Connection closed".into());
        let mut parser: Processor = Processor::new();
        parser.advance(&mut term, &renderer.frame(&s));
        assert_eq!(row_text(&term, 0), format!("{}Sign On", " ".repeat(30)).chars().take(40).collect::<String>().trim_end());
        assert_eq!(row_text(&term, 9), "Connection closed");
    }

    #[test]
    fn the_status_line_names_an_operator_error() {
        let mut s = screen();
        s.record(&sign_on());
        s.key(Key::Up);
        s.key(Key::Char('x'));
        let mut term = term(80, 25);
        let mut parser: Processor = Processor::new();
        parser.advance(&mut term, &Renderer::new(80, 25).frame(&s));
        let status = row_text(&term, 24);
        assert!(status.starts_with("X II 0005  Cursor"), "{status}");
    }

    #[test]
    fn attributes_map_to_colors_and_highlights() {
        assert!(look(0x27).hidden && look(0x2F).hidden && look(0x37).hidden && look(0x3F).hidden);
        assert!(!look(0x27).reverse && !look(0x27).underline);
        assert!(!look(0x20).hidden);
        assert!(look(0x21).reverse && !look(0x21).underline);
        assert!(look(0x24).underline);
        assert!(look(0x22).color == Color::White);
        assert!(look(0x28).color == Color::Red);
        assert!(look(0x30).color == Color::Turquoise && look(0x32).color == Color::Yellow);
        assert!(look(0x38).color == Color::Pink && look(0x3A).color == Color::Blue);
    }
}
