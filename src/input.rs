//! Winit key and wheel events -> PTY bytes.
//!
//! Keys are intentionally minimal for the MVP: enough control-sequence
//! coverage (arrows, Home/End, PageUp/Down, Delete, Ctrl+letter) to use a
//! shell comfortably, plumbed straight through `KeyEvent::text` for
//! everything else (regular characters, IME output, etc).
//!
//! The mouse wheel goes to the program when it asked for that -- see
//! [`wheel_to_bytes`].

use alacritty_terminal::term::TermMode;
use winit::event::{ElementState, KeyEvent};
use winit::keyboard::{Key, ModifiersState, NamedKey};

/// Turn a key event into the bytes that should be written to the PTY, or
/// `None` if this event doesn't produce any input on its own (key
/// releases, bare modifier presses, unmapped keys, ...).
pub fn key_event_to_bytes(event: &KeyEvent, modifiers: ModifiersState) -> Option<Vec<u8>> {
    if event.state != ElementState::Pressed {
        return None;
    }

    // Ctrl+<letter> takes priority over everything else and produces the
    // corresponding C0 control code (Ctrl+A -> 0x01, ... Ctrl+Z -> 0x1a).
    if modifiers.control_key()
        && !modifiers.alt_key()
        && let Key::Character(ch) = &event.logical_key
    {
        let mut chars = ch.chars();
        if let (Some(c), None) = (chars.next(), chars.next()) {
            let upper = c.to_ascii_uppercase();
            if upper.is_ascii_uppercase() {
                return Some(vec![(upper as u8) - b'A' + 1]);
            }
        }
    }

    if let Key::Named(named) = &event.logical_key {
        let bytes: &[u8] = match named {
            NamedKey::Enter => b"\r",
            NamedKey::Backspace => b"\x7f",
            NamedKey::Tab => b"\t",
            NamedKey::Escape => b"\x1b",
            NamedKey::ArrowUp => b"\x1b[A",
            NamedKey::ArrowDown => b"\x1b[B",
            NamedKey::ArrowRight => b"\x1b[C",
            NamedKey::ArrowLeft => b"\x1b[D",
            NamedKey::Home => b"\x1b[H",
            NamedKey::End => b"\x1b[F",
            NamedKey::PageUp => b"\x1b[5~",
            NamedKey::PageDown => b"\x1b[6~",
            NamedKey::Delete => b"\x1b[3~",
            NamedKey::Insert => b"\x1b[2~",
            _ => return event.text.as_ref().map(|text| text.as_bytes().to_vec()),
        };
        return Some(bytes.to_vec());
    }

    event.text.as_ref().map(|text| text.as_bytes().to_vec())
}

/// What the program in the terminal gets for `lines` of wheel scrolling
/// (positive: up) with the mouse over `cell` (column, row on screen,
/// 0-based) -- or `None` if the wheel is ours, to scroll the scrollback.
/// Like Alacritty:
/// - Mouse reporting on (htop, mc, vim with `mouse=a`): one wheel report
///   per line, as buttons 64/65, in the encoding the program chose. An
///   empty result means the position doesn't fit that encoding.
/// - Alternate screen with alternate scroll (less, man; on by default):
///   one arrow key per line. Full-screen programs have no scrollback.
///
/// Shift keeps the wheel for the scrollback either way.
pub fn wheel_to_bytes(
    lines: i32,
    mode: TermMode,
    (col, row): (usize, usize),
    modifiers: ModifiersState,
) -> Option<Vec<u8>> {
    if modifiers.shift_key() || lines == 0 {
        return None;
    }
    let count = lines.unsigned_abs() as usize;
    let up = lines > 0;

    if mode.intersects(TermMode::MOUSE_MODE) {
        let mut button: u32 = if up { 64 } else { 65 };
        if modifiers.alt_key() {
            button += 8;
        }
        if modifiers.control_key() {
            button += 16;
        }
        let report = mouse_report(mode, button, col + 1, row + 1);
        return Some(report.repeat(count));
    }

    if mode.contains(TermMode::ALT_SCREEN | TermMode::ALTERNATE_SCROLL) {
        let arrow: &[u8] = match (up, mode.contains(TermMode::APP_CURSOR)) {
            (true, false) => b"\x1b[A",
            (false, false) => b"\x1b[B",
            (true, true) => b"\x1bOA",
            (false, true) => b"\x1bOB",
        };
        return Some(arrow.repeat(count));
    }
    None
}

/// One mouse-button report at 1-based `col`/`row`. Empty if the position
/// can't be encoded: the X10 default fits values up to 255 in one byte
/// each (so coordinates up to 223), UTF-8 up to 2047 in two.
fn mouse_report(mode: TermMode, button: u32, col: usize, row: usize) -> Vec<u8> {
    if mode.contains(TermMode::SGR_MOUSE) {
        return format!("\x1b[<{button};{col};{row}M").into_bytes();
    }
    let utf8 = mode.contains(TermMode::UTF8_MOUSE);
    let max = if utf8 { 0x7ff } else { 0xff };
    let mut report = b"\x1b[M".to_vec();
    for value in [button as usize, col, row].map(|v| v + 32) {
        if value > max {
            return Vec::new();
        }
        match char::from_u32(value as u32) {
            Some(c) if utf8 => report.extend_from_slice(c.encode_utf8(&mut [0; 4]).as_bytes()),
            _ => report.push(value as u8),
        }
    }
    report
}

#[cfg(test)]
mod tests {
    use alacritty_terminal::term::TermMode;
    use winit::keyboard::ModifiersState;

    use super::wheel_to_bytes;

    const NONE: ModifiersState = ModifiersState::empty();

    #[test]
    fn normal_screen_wheel_scrolls_the_scrollback() {
        assert_eq!(wheel_to_bytes(3, TermMode::default(), (0, 0), NONE), None);
        // Alternate scroll turned off by the program.
        assert_eq!(wheel_to_bytes(3, TermMode::ALT_SCREEN, (0, 0), NONE), None);
    }

    #[test]
    fn alternate_screen_gets_arrow_keys() {
        let mode = TermMode::default() | TermMode::ALT_SCREEN;
        assert_eq!(wheel_to_bytes(3, mode, (0, 0), NONE).unwrap(), b"\x1b[A\x1b[A\x1b[A");
        assert_eq!(wheel_to_bytes(-1, mode, (0, 0), NONE).unwrap(), b"\x1b[B");
        let app_cursor = mode | TermMode::APP_CURSOR;
        assert_eq!(wheel_to_bytes(-2, app_cursor, (0, 0), NONE).unwrap(), b"\x1bOB\x1bOB");
    }

    #[test]
    fn mouse_mode_gets_wheel_reports() {
        let sgr = TermMode::default() | TermMode::ALT_SCREEN | TermMode::MOUSE_REPORT_CLICK | TermMode::SGR_MOUSE;
        assert_eq!(wheel_to_bytes(2, sgr, (4, 9), NONE).unwrap(), b"\x1b[<64;5;10M\x1b[<64;5;10M");
        assert_eq!(wheel_to_bytes(-1, sgr, (4, 9), NONE).unwrap(), b"\x1b[<65;5;10M");
        assert_eq!(wheel_to_bytes(1, sgr, (0, 0), ModifiersState::CONTROL).unwrap(), b"\x1b[<80;1;1M");
        assert_eq!(wheel_to_bytes(1, sgr, (0, 0), ModifiersState::ALT).unwrap(), b"\x1b[<72;1;1M");

        let x10 = TermMode::MOUSE_DRAG;
        assert_eq!(wheel_to_bytes(1, x10, (0, 0), NONE).unwrap(), [0x1b, b'[', b'M', 32 + 64, 33, 33]);
        assert_eq!(wheel_to_bytes(1, x10, (223, 0), NONE).unwrap(), b"", "column 224 doesn't fit a byte");

        let utf8 = TermMode::MOUSE_MOTION | TermMode::UTF8_MOUSE;
        let mut expected = b"\x1b[M".to_vec();
        expected.extend("\u{60}\u{119}\u{21}".as_bytes()); // 96, 32+249, 33
        assert_eq!(wheel_to_bytes(1, utf8, (248, 0), NONE).unwrap(), expected);
    }

    #[test]
    fn shift_keeps_the_wheel_for_the_scrollback() {
        let mode = TermMode::default() | TermMode::ALT_SCREEN | TermMode::MOUSE_REPORT_CLICK;
        assert_eq!(wheel_to_bytes(1, mode, (0, 0), ModifiersState::SHIFT), None);
    }
}
