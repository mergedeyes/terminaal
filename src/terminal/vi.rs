//! Vi mode: a cursor of its own that moves through the scrollback with
//! vi's keys, selects and copies -- no mouse needed (`Action::ViMode`).
//!
//! The state lives in `alacritty_terminal`'s `Term` (`TermMode::VI`, the
//! vi cursor, the selection following it); here is which key does what
//! ([`action`]) and the moves themselves ([`apply`]). What needs the rest
//! of the app -- the clipboard, the search bar, opening a link -- comes
//! back as an [`Action`] for `app.rs`.

use alacritty_terminal::event::EventListener;
use alacritty_terminal::grid::{Dimensions, Scroll};
use alacritty_terminal::index::{Column, Side};
use alacritty_terminal::selection::{Selection, SelectionType};
use alacritty_terminal::term::{Term, TermMode};
use alacritty_terminal::vi_mode::ViMotion;
use winit::keyboard::{Key, ModifiersState, NamedKey};

use crate::input::KeyInput;

/// What a key does in vi mode.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
    Motion(ViMotion),
    /// Scroll by this many screen lines (positive: up), the cursor along.
    Scroll(Lines),
    /// To the top of the scrollback / the bottom of the screen.
    Top,
    Bottom,
    /// Start a selection of this kind at the cursor; the same again ends
    /// it, another kind changes it.
    Select(SelectionType),
    /// Copy the selection and leave vi mode.
    Copy,
    /// Drop the selection, or without one leave vi mode.
    Escape,
    Exit,
    /// Open the search bar; its matches move the cursor.
    Search,
    /// Open the link under the cursor.
    Open,
}

/// How far a scroll goes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Lines {
    One(i32),
    HalfPage(i32),
    Page(i32),
}

/// The vi mode meaning of a key press, if it has one. Everything else is
/// swallowed while vi mode is on: nothing goes to the program.
pub fn action(event: &KeyInput, mods: ModifiersState) -> Option<Action> {
    use Action::{Motion, Scroll, Select};
    use ViMotion as M;
    if mods.control_key() {
        let Key::Character(key) = &event.key_without_modifiers else { return None };
        return Some(match key.to_lowercase().as_str() {
            "u" => Scroll(Lines::HalfPage(1)),
            "d" => Scroll(Lines::HalfPage(-1)),
            "b" => Scroll(Lines::Page(1)),
            "f" => Scroll(Lines::Page(-1)),
            "y" => Scroll(Lines::One(1)),
            "e" => Scroll(Lines::One(-1)),
            "v" => Select(SelectionType::Block),
            "c" => Action::Exit,
            _ => return None,
        });
    }
    if let Key::Named(named) = &event.logical_key {
        return Some(match named {
            NamedKey::ArrowLeft => Motion(M::Left),
            NamedKey::ArrowRight => Motion(M::Right),
            NamedKey::ArrowUp => Motion(M::Up),
            NamedKey::ArrowDown => Motion(M::Down),
            NamedKey::Home => Motion(M::First),
            NamedKey::End => Motion(M::Last),
            NamedKey::PageUp => Scroll(Lines::Page(1)),
            NamedKey::PageDown => Scroll(Lines::Page(-1)),
            NamedKey::Escape => Action::Escape,
            NamedKey::Enter => Action::Open,
            _ => return None,
        });
    }
    let Key::Character(key) = &event.logical_key else { return None };
    // Alt+V: the word under the cursor and on, a word at a time.
    if mods.alt_key() {
        return (event.key_without_modifiers == Key::Character("v".into())).then_some(Select(SelectionType::Semantic));
    }
    Some(match key.as_str() {
        "h" => Motion(M::Left),
        "j" => Motion(M::Down),
        "k" => Motion(M::Up),
        "l" => Motion(M::Right),
        "w" => Motion(M::SemanticRight),
        "b" => Motion(M::SemanticLeft),
        "e" => Motion(M::SemanticRightEnd),
        "W" => Motion(M::WordRight),
        "B" => Motion(M::WordLeft),
        "E" => Motion(M::WordRightEnd),
        "0" => Motion(M::First),
        "$" => Motion(M::Last),
        "^" => Motion(M::FirstOccupied),
        "H" => Motion(M::High),
        "M" => Motion(M::Middle),
        "L" => Motion(M::Low),
        "%" => Motion(M::Bracket),
        "{" => Motion(M::ParagraphUp),
        "}" => Motion(M::ParagraphDown),
        // One g is enough; gg goes there just the same.
        "g" => Action::Top,
        "G" => Action::Bottom,
        "v" => Select(SelectionType::Simple),
        "V" => Select(SelectionType::Lines),
        "y" => Action::Copy,
        "i" | "q" => Action::Exit,
        "/" | "?" => Action::Search,
        _ => return None,
    })
}

/// Carry out what concerns only the terminal: moving, scrolling,
/// selecting. The view follows the cursor. `false` for the actions left
/// to the app.
pub fn apply<T: EventListener>(term: &mut Term<T>, action: Action) -> bool {
    match action {
        Action::Motion(motion) => term.vi_motion(motion),
        Action::Scroll(lines) => scroll(term, lines),
        Action::Top | Action::Bottom => {
            let (scroll, line) = match action {
                Action::Top => (Scroll::Top, term.topmost_line()),
                _ => (Scroll::Bottom, term.bottommost_line()),
            };
            term.scroll_display(scroll);
            term.vi_goto_point(alacritty_terminal::index::Point::new(line, Column(0)));
            // Twice: across a wrapped line to its very start.
            term.vi_motion(ViMotion::FirstOccupied);
            term.vi_motion(ViMotion::FirstOccupied);
        }
        Action::Select(ty) => select(term, ty),
        Action::Escape if term.selection.as_ref().is_some_and(|selection| !selection.is_empty()) => term.selection = None,
        Action::Escape | Action::Exit | Action::Copy | Action::Search | Action::Open => return false,
    }
    let point = term.vi_mode_cursor.point;
    term.scroll_to_point(point);
    true
}

/// Scroll the view, the cursor keeping its row on screen -- or, with the
/// view at an end already, moving by as much on its own (Ctrl+U at the
/// top still goes up).
fn scroll<T: EventListener>(term: &mut Term<T>, lines: Lines) {
    let screen = term.screen_lines() as i32;
    let by = match lines {
        Lines::One(by) => by,
        Lines::HalfPage(by) => by * (screen / 2).max(1),
        Lines::Page(by) => by * screen,
    };
    let before = term.grid().display_offset() as i32;
    let line = term.vi_mode_cursor.point.line;
    term.scroll_display(Scroll::Delta(by));
    let scrolled = term.grid().display_offset() as i32 - before;
    let moved = if scrolled == 0 { by } else { scrolled };
    let top = -(term.grid().display_offset() as i32);
    let target = (line.0 - moved).clamp(top.max(term.topmost_line().0), top + screen - 1);
    term.vi_mode_cursor.point.line = alacritty_terminal::index::Line(target);
    term.vi_goto_point(term.vi_mode_cursor.point);
}

/// Like vim's `v`, `V`, Ctrl+V: a selection from the cursor that follows
/// it; the same kind again ends it, another changes it.
fn select<T: EventListener>(term: &mut Term<T>, ty: SelectionType) {
    match &mut term.selection {
        Some(selection) if selection.ty == ty && !selection.is_empty() => term.selection = None,
        Some(selection) if !selection.is_empty() => selection.ty = ty,
        _ => {
            let mut selection = Selection::new(ty, term.vi_mode_cursor.point, Side::Left);
            selection.include_all();
            term.selection = Some(selection);
        }
    }
}

/// Turn vi mode on or off; on, it starts where the terminal's cursor is
/// (or at the top of the view, when that's scrolled away from it).
pub fn toggle<T: EventListener>(term: &mut Term<T>) {
    term.toggle_vi_mode();
    if !term.mode().contains(TermMode::VI) {
        term.selection = None;
    }
}

#[cfg(test)]
mod tests {
    use alacritty_terminal::event::VoidListener;
    use alacritty_terminal::index::Point;
    use alacritty_terminal::term::Config;
    use alacritty_terminal::vte::ansi::{Processor, StdSyncHandler};
    use winit::event::ElementState;
    use winit::keyboard::{KeyLocation, SmolStr};

    use super::*;
    use crate::terminal::GridSize;

    fn key(typed: &str) -> KeyInput {
        let plain = typed.to_lowercase();
        KeyInput {
            state: ElementState::Pressed,
            logical_key: Key::Character(SmolStr::new(typed)),
            key_without_modifiers: Key::Character(SmolStr::new(plain)),
            text: Some(SmolStr::new(typed)),
            location: KeyLocation::Standard,
            repeat: false,
        }
    }

    fn named(named: NamedKey) -> KeyInput {
        KeyInput {
            state: ElementState::Pressed,
            logical_key: Key::Named(named),
            key_without_modifiers: Key::Named(named),
            text: None,
            location: KeyLocation::Standard,
            repeat: false,
        }
    }

    const NONE: ModifiersState = ModifiersState::empty();

    #[test]
    fn keys() {
        assert_eq!(action(&key("j"), NONE), Some(Action::Motion(ViMotion::Down)));
        assert_eq!(action(&key("W"), ModifiersState::SHIFT), Some(Action::Motion(ViMotion::WordRight)));
        assert_eq!(action(&key("$"), ModifiersState::SHIFT), Some(Action::Motion(ViMotion::Last)));
        assert_eq!(action(&key("u"), ModifiersState::CONTROL), Some(Action::Scroll(Lines::HalfPage(1))));
        assert_eq!(action(&key("v"), ModifiersState::CONTROL), Some(Action::Select(SelectionType::Block)));
        assert_eq!(action(&key("v"), ModifiersState::ALT), Some(Action::Select(SelectionType::Semantic)));
        assert_eq!(action(&key("V"), ModifiersState::SHIFT), Some(Action::Select(SelectionType::Lines)));
        assert_eq!(action(&named(NamedKey::Escape), NONE), Some(Action::Escape));
        assert_eq!(action(&named(NamedKey::PageUp), NONE), Some(Action::Scroll(Lines::Page(1))));
        assert_eq!(action(&key("x"), NONE), None);
        assert_eq!(action(&key("x"), ModifiersState::CONTROL), None);
    }

    /// 20 numbered lines in a 5-line terminal: 15 in the scrollback.
    fn term() -> Term<VoidListener> {
        let size = GridSize { columns: 20, screen_lines: 5 };
        let mut term = Term::new(Config::default(), &size, VoidListener);
        let text: String = (1..=20).map(|n| format!("line {n} word\r\n")).collect();
        Processor::<StdSyncHandler>::new().advance(&mut term, text.trim_end().as_bytes());
        term
    }

    fn line_at_cursor(term: &Term<VoidListener>) -> String {
        let point = term.vi_mode_cursor.point;
        term.bounds_to_string(Point::new(point.line, Column(0)), Point::new(point.line, term.last_column()))
            .trim()
            .to_string()
    }

    #[test]
    fn moving_scrolls_the_view_along() {
        let mut term = term();
        toggle(&mut term);
        assert!(term.mode().contains(TermMode::VI));
        assert_eq!(line_at_cursor(&term), "line 20 word", "starts at the terminal's cursor");
        for _ in 0..6 {
            apply(&mut term, Action::Motion(ViMotion::Up));
        }
        assert_eq!(line_at_cursor(&term), "line 14 word");
        assert_eq!(term.grid().display_offset(), 2, "scrolled to keep it in view");
        apply(&mut term, Action::Top);
        assert_eq!(line_at_cursor(&term), "line 1 word");
        assert_eq!(term.grid().display_offset(), 15);
        apply(&mut term, Action::Scroll(Lines::HalfPage(-1)));
        assert_eq!(term.grid().display_offset(), 13);
        assert_eq!(line_at_cursor(&term), "line 3 word", "the cursor keeps its row");
        apply(&mut term, Action::Bottom);
        assert_eq!((term.grid().display_offset(), line_at_cursor(&term).as_str()), (0, "line 20 word"));
        // At the bottom already, Ctrl+D still moves the cursor.
        apply(&mut term, Action::Motion(ViMotion::High));
        apply(&mut term, Action::Scroll(Lines::HalfPage(-1)));
        assert_eq!(line_at_cursor(&term), "line 18 word");
        toggle(&mut term);
        assert!(!term.mode().contains(TermMode::VI));
    }

    #[test]
    fn selecting_follows_the_cursor() {
        let mut term = term();
        toggle(&mut term);
        apply(&mut term, Action::Motion(ViMotion::Up));
        apply(&mut term, Action::Motion(ViMotion::First));
        apply(&mut term, Action::Select(SelectionType::Simple));
        assert_eq!(term.selection_to_string().as_deref(), Some("l"), "the cell under the cursor at once");
        apply(&mut term, Action::Motion(ViMotion::SemanticRight));
        assert_eq!(term.selection_to_string().as_deref(), Some("line 1"));
        apply(&mut term, Action::Select(SelectionType::Lines));
        assert_eq!(term.selection_to_string().as_deref(), Some("line 19 word\n"));
        apply(&mut term, Action::Motion(ViMotion::Down));
        assert_eq!(term.selection_to_string().as_deref(), Some("line 19 word\nline 20 word\n"));
        // Escape drops the selection first, and only then leaves.
        assert!(apply(&mut term, Action::Escape));
        assert_eq!(term.selection, None);
        assert!(!apply(&mut term, Action::Escape));
        // The same kind again ends it.
        apply(&mut term, Action::Select(SelectionType::Simple));
        apply(&mut term, Action::Select(SelectionType::Simple));
        assert_eq!(term.selection, None);
    }
}
