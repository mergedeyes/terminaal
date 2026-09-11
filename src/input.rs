//! Winit key event -> PTY bytes.
//!
//! This is intentionally minimal for the MVP: enough control-sequence
//! coverage (arrows, Home/End, PageUp/Down, Delete, Ctrl+letter) to use a
//! shell comfortably, plumbed straight through `KeyEvent::text` for
//! everything else (regular characters, IME output, etc).

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
