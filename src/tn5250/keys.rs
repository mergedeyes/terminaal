//! What the pane sends as typed → 5250 keys.
//!
//! The 5250 pane is a terminal like any other: keys arrive encoded by
//! `input::key_bytes`, pasted text as text, the wheel as mouse reports.
//! The emulator asks its `Term` for the kitty keyboard protocol with every
//! key as an escape code and its text (flags 1, 8 and 16), so Ctrl+Enter,
//! Esc and the keypad come through distinct; with the protocol turned off
//! in the settings, the xterm encoding still gives everything but those.
//!
//! The layout (`docs/guides/ibm-i-5250.md`):
//! - Enter, keypad Enter: Enter. Ctrl+Enter, Shift+Enter, Ctrl+K: Field Exit
//! - keypad +/−: Field+/Field− in numeric fields, else the character;
//!   Ctrl+X: Field+
//! - Tab/Shift+Tab: next/previous field; arrows, Home, End, Insert,
//!   Delete, Backspace as on a PC
//! - Page Up/Down and the wheel: Roll Down/Up
//! - F1–F12, Shift+F1–F12: F13–F24
//! - Esc, Ctrl+A: Attention; Shift+Esc, Ctrl+C: System Request
//! - Ctrl+R: Reset; Ctrl+E, Ctrl+End: Erase to end of field; Ctrl+P,
//!   Print Screen: Print; Ctrl+F1: Help
//! - Ctrl+D: close the tab while disconnected

use super::screen::Key;

/// The kitty keyboard flags the emulator pushes: disambiguate (1), all
/// keys as escape codes (8), their text (16).
pub const KITTY_FLAGS: u8 = 1 | 8 | 16;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Typed {
    Key(Key),
    /// Keypad + or −: Field+/Field− in a numeric field, else typed.
    Keypad(char),
    /// Ctrl+D.
    Close,
}

const SHIFT: u8 = 1;
const ALT: u8 = 2;
const CTRL: u8 = 4;

/// Everything in one write from the pane.
pub fn decode(bytes: &[u8]) -> Vec<Typed> {
    let mut out = Vec::new();
    // A lone CR is the Enter key; inside pasted text it's a line break.
    if bytes == b"\r" {
        return vec![Typed::Key(Key::Enter)];
    }
    let mut wheel = false;
    let mut i = 0;
    while i < bytes.len() {
        let byte = bytes[i];
        i += 1;
        match byte {
            0x1b => match bytes.get(i) {
                Some(b'[') => {
                    let start = i + 1;
                    let Some(len) = bytes[start..].iter().position(|b| (0x40..=0x7e).contains(b)) else { break };
                    let params = std::str::from_utf8(&bytes[start..start + len]).unwrap_or("");
                    let last = bytes[start + len];
                    i = start + len + 1;
                    match csi(params, last) {
                        Some(Csi::Typed(typed)) => out.push(typed),
                        Some(Csi::Text(text)) => out.extend(text.chars().map(|c| Typed::Key(Key::Char(c)))),
                        // A notch of the wheel comes as several reports: one roll.
                        Some(Csi::Wheel(key)) => {
                            if !wheel {
                                out.push(Typed::Key(key));
                            }
                            wheel = true;
                        }
                        None => {}
                    }
                }
                Some(b'O') if i + 1 < bytes.len() => {
                    let key = letter(bytes[i + 1], 0);
                    i += 2;
                    out.extend(key.map(Typed::Key));
                }
                // Alt+key in the xterm encoding: nothing on a 5250.
                Some(_) => i += 1,
                None => out.push(Typed::Key(Key::Attention)),
            },
            b'\r' | b'\n' => out.push(Typed::Key(Key::NewLine)),
            b'\t' => out.push(Typed::Key(Key::Tab)),
            0x7f | 0x08 => out.push(Typed::Key(Key::Backspace)),
            0x00..=0x1f => out.extend(control(byte + b'a' - 1)),
            _ => {
                // UTF-8: as long as the lead byte says.
                let len = match byte {
                    0xC0..=0xDF => 2,
                    0xE0..=0xEF => 3,
                    0xF0..=0xF7 => 4,
                    _ => 1,
                };
                let end = (i - 1 + len).min(bytes.len());
                if let Ok(text) = std::str::from_utf8(&bytes[i - 1..end]) {
                    out.extend(text.chars().filter(|c| !c.is_control()).map(|c| Typed::Key(Key::Char(c))));
                }
                i = end;
            }
        }
    }
    out
}

/// Ctrl+letter.
fn control(letter: u8) -> Option<Typed> {
    Some(match letter {
        b'a' => Typed::Key(Key::Attention),
        b'c' => Typed::Key(Key::SystemRequest),
        b'd' => Typed::Close,
        b'e' => Typed::Key(Key::EraseEof),
        b'k' => Typed::Key(Key::FieldExit),
        b'p' => Typed::Key(Key::Print),
        b'r' => Typed::Key(Key::Reset),
        b'x' => Typed::Key(Key::FieldPlus),
        _ => return None,
    })
}

enum Csi {
    Typed(Typed),
    Text(String),
    Wheel(Key),
}

fn csi(params: &str, last: u8) -> Option<Csi> {
    // SGR mouse: only the wheel matters (64 up, 65 down; +4/8/16 for
    // modifiers).
    if let Some(mouse) = params.strip_prefix('<') {
        let button: u32 = mouse.split(';').next()?.parse().ok()?;
        return match (last, button & !0x1c) {
            (b'M', 64) => Some(Csi::Wheel(Key::RollDown)),
            (b'M', 65) => Some(Csi::Wheel(Key::RollUp)),
            _ => None,
        };
    }
    let fields: Vec<Vec<u32>> =
        params.split(';').map(|field| field.split(':').map(|n| n.parse().unwrap_or(0)).collect()).collect();
    let number = fields.first().and_then(|f| f.first()).copied().unwrap_or(1);
    let mods = fields.get(1).and_then(|f| f.first()).copied().unwrap_or(1).saturating_sub(1) as u8;
    let typed = |key: Option<Key>| key.map(|key| Csi::Typed(Typed::Key(key)));
    match last {
        b'u' => {
            let text: String =
                fields.get(2).map(|codes| codes.iter().filter_map(|&c| char::from_u32(c)).collect()).unwrap_or_default();
            kitty(number, mods, text)
        }
        b'~' => typed(tilde(number, mods)),
        // Shift+Tab in the xterm encoding.
        b'Z' => typed(Some(Key::Backtab)),
        _ => typed(letter(last, mods)),
    }
}

/// `CSI <letter>` and `SS3 <letter>`: cursor keys and F1-F4.
fn letter(last: u8, mods: u8) -> Option<Key> {
    let shifted = |n: u8| if mods & SHIFT != 0 { n + 12 } else { n };
    Some(match last {
        b'A' => Key::Up,
        b'B' => Key::Down,
        b'C' => Key::Right,
        b'D' => Key::Left,
        b'H' => Key::Home,
        b'F' if mods & CTRL != 0 => Key::EraseEof,
        b'F' => Key::End,
        b'P' if mods & CTRL != 0 => Key::Help,
        b'P' => Key::Function(shifted(1)),
        b'Q' => Key::Function(shifted(2)),
        b'R' => Key::Function(shifted(3)),
        b'S' => Key::Function(shifted(4)),
        // The application keypad's Enter.
        b'M' => Key::Enter,
        _ => return None,
    })
}

/// `CSI <number> ~`: editing keys and F5 on (F3 too, under kitty).
fn tilde(number: u32, mods: u8) -> Option<Key> {
    let function = |n: u8| Some(Key::Function(if mods & SHIFT != 0 && n <= 12 { n + 12 } else { n }));
    match number {
        2 => Some(Key::Insert),
        3 => Some(Key::Delete),
        5 => Some(Key::RollDown),
        6 => Some(Key::RollUp),
        13 => function(3),
        15 => function(5),
        17..=21 => function(number as u8 - 11),
        23 | 24 => function(number as u8 - 12),
        // F13-F20 of a keyboard that has them.
        25 | 26 => function(number as u8 - 12),
        28 | 29 => function(number as u8 - 13),
        31..=34 => function(number as u8 - 14),
        _ => None,
    }
}

/// `CSI <code> ; <mods> ; <text> u`.
fn kitty(code: u32, mods: u8, text: String) -> Option<Csi> {
    let typed = |key: Key| Some(Csi::Typed(Typed::Key(key)));
    let ctrl = mods & CTRL != 0;
    match code {
        13 if ctrl || mods & SHIFT != 0 => typed(Key::FieldExit),
        13 | 57414 => typed(Key::Enter),
        9 if mods & SHIFT != 0 => typed(Key::Backtab),
        9 => typed(Key::Tab),
        127 | 8 => typed(Key::Backspace),
        27 if mods & SHIFT != 0 => typed(Key::SystemRequest),
        27 => typed(Key::Attention),
        57412 => Some(Csi::Typed(Typed::Keypad('-'))),
        57413 => Some(Csi::Typed(Typed::Keypad('+'))),
        57417 => typed(Key::Left),
        57418 => typed(Key::Right),
        57419 => typed(Key::Up),
        57420 => typed(Key::Down),
        57421 => typed(Key::RollDown),
        57422 => typed(Key::RollUp),
        57423 => typed(Key::Home),
        57424 => typed(Key::End),
        57425 => typed(Key::Insert),
        57426 => typed(Key::Delete),
        57361 => typed(Key::Print),
        // Ctrl+letter, as the xterm encoding has it.
        97..=122 if ctrl => control(code as u8).map(Csi::Typed),
        _ if ctrl || mods & ALT != 0 => None,
        // Text the key types: the layout's character, AltGr and dead keys
        // done.
        _ if !text.is_empty() => Some(Csi::Text(text)),
        // The keypad's digits and signs without the text flag.
        57399..=57408 => Some(Csi::Text(char::from_digit(code - 57399, 10)?.to_string())),
        57409 => Some(Csi::Text(".".into())),
        57410 => Some(Csi::Text("/".into())),
        57411 => Some(Csi::Text("*".into())),
        // Modifiers, locks, media keys: nothing.
        57344.. => None,
        _ => {
            let c = char::from_u32(code)?;
            if c.is_control() {
                return None;
            }
            let c = if mods & SHIFT != 0 { c.to_uppercase().next().unwrap_or(c) } else { c };
            Some(Csi::Text(c.to_string()))
        }
    }
}

#[cfg(test)]
mod tests {
    use alacritty_terminal::term::TermMode;
    use winit::event::ElementState;
    use winit::keyboard::{Key as WinitKey, KeyLocation, ModifiersState, NamedKey, SmolStr};

    use super::*;
    use crate::input::{KeyInput, key_bytes};

    fn input(key: WinitKey, text: Option<&str>, location: KeyLocation) -> KeyInput {
        KeyInput {
            state: ElementState::Pressed,
            logical_key: key.clone(),
            key_without_modifiers: key,
            text: text.map(SmolStr::new),
            location,
            repeat: false,
        }
    }

    fn named(key: NamedKey) -> KeyInput {
        let text = match key {
            NamedKey::Enter => Some("\r"),
            NamedKey::Tab => Some("\t"),
            NamedKey::Escape => Some("\x1b"),
            NamedKey::Backspace => Some("\x08"),
            _ => None,
        };
        input(WinitKey::Named(key), text, KeyLocation::Standard)
    }

    fn char_key(c: &str) -> KeyInput {
        input(WinitKey::Character(SmolStr::new(c)), Some(c), KeyLocation::Standard)
    }

    fn keypad(key: WinitKey, text: Option<&str>) -> KeyInput {
        input(key, text, KeyLocation::Numpad)
    }

    /// Through the real encoder, in both modes the pane can be in.
    fn typed(event: &KeyInput, mods: ModifiersState) -> [Vec<Typed>; 2] {
        let kitty = TermMode::DISAMBIGUATE_ESC_CODES | TermMode::REPORT_ALL_KEYS_AS_ESC | TermMode::REPORT_ASSOCIATED_TEXT;
        [kitty, TermMode::empty()].map(|mode| decode(&key_bytes(event, mods, mode).unwrap_or_default()))
    }

    fn both(event: &KeyInput, mods: ModifiersState, expected: Typed) {
        assert_eq!(typed(event, mods), [vec![expected], vec![expected]], "{event:?} {mods:?}");
    }

    const NONE: ModifiersState = ModifiersState::empty();

    #[test]
    fn keys_decode_the_same_with_and_without_the_kitty_protocol() {
        both(&named(NamedKey::Enter), NONE, Typed::Key(Key::Enter));
        both(&named(NamedKey::Tab), NONE, Typed::Key(Key::Tab));
        both(&named(NamedKey::Tab), ModifiersState::SHIFT, Typed::Key(Key::Backtab));
        both(&named(NamedKey::Backspace), NONE, Typed::Key(Key::Backspace));
        both(&named(NamedKey::Escape), NONE, Typed::Key(Key::Attention));
        both(&named(NamedKey::ArrowUp), NONE, Typed::Key(Key::Up));
        both(&named(NamedKey::ArrowLeft), NONE, Typed::Key(Key::Left));
        both(&named(NamedKey::Home), NONE, Typed::Key(Key::Home));
        both(&named(NamedKey::End), NONE, Typed::Key(Key::End));
        both(&named(NamedKey::End), ModifiersState::CONTROL, Typed::Key(Key::EraseEof));
        both(&named(NamedKey::Insert), NONE, Typed::Key(Key::Insert));
        both(&named(NamedKey::Delete), NONE, Typed::Key(Key::Delete));
        both(&named(NamedKey::PageUp), NONE, Typed::Key(Key::RollDown));
        both(&named(NamedKey::PageDown), NONE, Typed::Key(Key::RollUp));
        both(&named(NamedKey::F1), NONE, Typed::Key(Key::Function(1)));
        both(&named(NamedKey::F3), NONE, Typed::Key(Key::Function(3)));
        both(&named(NamedKey::F5), NONE, Typed::Key(Key::Function(5)));
        both(&named(NamedKey::F12), NONE, Typed::Key(Key::Function(12)));
        both(&named(NamedKey::F1), ModifiersState::SHIFT, Typed::Key(Key::Function(13)));
        both(&named(NamedKey::F4), ModifiersState::SHIFT, Typed::Key(Key::Function(16)));
        both(&named(NamedKey::F12), ModifiersState::SHIFT, Typed::Key(Key::Function(24)));
        both(&named(NamedKey::F1), ModifiersState::CONTROL, Typed::Key(Key::Help));
        both(&char_key("a"), NONE, Typed::Key(Key::Char('a')));
        both(&char_key("Ä"), ModifiersState::SHIFT, Typed::Key(Key::Char('Ä')));
        both(&char_key("@"), NONE, Typed::Key(Key::Char('@')));
        both(&char_key("r"), ModifiersState::CONTROL, Typed::Key(Key::Reset));
        both(&char_key("a"), ModifiersState::CONTROL, Typed::Key(Key::Attention));
        both(&char_key("c"), ModifiersState::CONTROL, Typed::Key(Key::SystemRequest));
        both(&char_key("k"), ModifiersState::CONTROL, Typed::Key(Key::FieldExit));
        both(&char_key("d"), ModifiersState::CONTROL, Typed::Close);
    }

    #[test]
    fn the_kitty_protocol_tells_more_keys_apart() {
        let kitty = |event: &KeyInput, mods| typed(event, mods)[0].clone();
        assert_eq!(kitty(&named(NamedKey::Enter), ModifiersState::CONTROL), [Typed::Key(Key::FieldExit)]);
        assert_eq!(kitty(&named(NamedKey::Enter), ModifiersState::SHIFT), [Typed::Key(Key::FieldExit)]);
        assert_eq!(kitty(&named(NamedKey::Escape), ModifiersState::SHIFT), [Typed::Key(Key::SystemRequest)]);
        assert_eq!(kitty(&keypad(WinitKey::Named(NamedKey::Enter), Some("\r")), NONE), [Typed::Key(Key::Enter)]);
        assert_eq!(kitty(&keypad(WinitKey::Character("+".into()), Some("+")), NONE), [Typed::Keypad('+')]);
        assert_eq!(kitty(&keypad(WinitKey::Character("-".into()), Some("-")), NONE), [Typed::Keypad('-')]);
        assert_eq!(kitty(&keypad(WinitKey::Character("7".into()), Some("7")), NONE), [Typed::Key(Key::Char('7'))]);
        // Shift alone types, it doesn't count as a key of its own.
        assert!(kitty(&named(NamedKey::Shift), ModifiersState::SHIFT).is_empty());
    }

    #[test]
    fn pasted_text_moves_through_fields() {
        assert_eq!(
            decode(b"ab\tc\r\nd"),
            [
                Typed::Key(Key::Char('a')),
                Typed::Key(Key::Char('b')),
                Typed::Key(Key::Tab),
                Typed::Key(Key::Char('c')),
                Typed::Key(Key::NewLine),
                Typed::Key(Key::NewLine),
                Typed::Key(Key::Char('d')),
            ]
        );
        assert_eq!(decode("ü€".as_bytes()), [Typed::Key(Key::Char('ü')), Typed::Key(Key::Char('€'))]);
    }

    #[test]
    fn one_notch_of_the_wheel_rolls_once() {
        assert_eq!(decode(b"\x1b[<64;10;5M\x1b[<64;10;5M\x1b[<64;10;5M"), [Typed::Key(Key::RollDown)]);
        assert_eq!(decode(b"\x1b[<65;1;1M"), [Typed::Key(Key::RollUp)]);
        assert!(decode(b"\x1b[<0;1;1M").is_empty());
    }
}
