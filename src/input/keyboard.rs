//! Keys -> the bytes the program in the terminal gets.
//!
//! Two encodings:
//! - xterm's, which every program understands: text as typed, Alt as an
//!   ESC in front, Ctrl+letter (and Ctrl+Space, `[`, `\`, ...) as C0
//!   control codes, cursor and function keys as `CSI`/`SS3` sequences
//!   with the modifiers as a parameter (`ESC [1;5C` for Ctrl+→). Cursor
//!   keys follow the program's cursor-key mode (`ESC O A` for vim and
//!   less, `ESC [A` otherwise).
//! - The kitty keyboard protocol
//!   (<https://sw.kovidgoyal.net/kitty/keyboard-protocol/>), for programs
//!   that turn it on (fish 4, Neovim, Helix, ...): keys that the old
//!   encoding mixes up -- Esc and Alt+[, Ctrl+I and Tab, Ctrl+Shift+A and
//!   Ctrl+A -- become `CSI <code>;<modifiers> u`, and as the program asks,
//!   with repeat and release events, the shifted key, every key including
//!   the modifiers themselves, and the text a key types. `alacritty_terminal`
//!   keeps the program's flags (`TermMode::KITTY_KEYBOARD_PROTOCOL`); the
//!   encoding is ours.

use alacritty_terminal::term::TermMode;
use winit::event::ElementState;
use winit::keyboard::{Key, KeyLocation, ModifiersState, NamedKey};

use super::KeyInput;

const ESC: u8 = 0x1b;

/// What `event` sends to a program in `mode`, with `mods` held. `None`
/// for keys that send nothing: modifiers and releases unless the kitty
/// protocol wants them, dead keys, keys without text.
pub fn key_bytes(event: &KeyInput, mods: ModifiersState, mode: TermMode) -> Option<Vec<u8>> {
    if event.state == ElementState::Released {
        if !mode.contains(TermMode::REPORT_EVENT_TYPES) {
            return None;
        }
        // These keep their old bytes for the press, so a release would be
        // the only sign of them -- except when every key is reported.
        if !mode.contains(TermMode::REPORT_ALL_KEYS_AS_ESC) && is_named(event, &[NamedKey::Enter, NamedKey::Tab, NamedKey::Backspace]) {
            return None;
        }
        return kitty(event, mods, mode);
    }
    if mode.intersects(TermMode::KITTY_KEYBOARD_PROTOCOL) && wants_kitty(event, mods, mode) {
        return kitty(event, mods, mode);
    }
    legacy(event, mods, mode)
}

/// Shift, Ctrl, Alt, Super and their kind: pressing one alone types
/// nothing.
pub fn is_modifier(key: &Key) -> bool {
    matches!(
        key,
        Key::Named(
            NamedKey::Shift
                | NamedKey::Control
                | NamedKey::Alt
                | NamedKey::Super
                | NamedKey::Meta
                | NamedKey::Hyper
                | NamedKey::AltGraph
                | NamedKey::CapsLock
                | NamedKey::NumLock
                | NamedKey::ScrollLock
        )
    )
}

fn is_named(event: &KeyInput, names: &[NamedKey]) -> bool {
    matches!(&event.logical_key, Key::Named(named) if names.contains(named))
}

/// The modifiers as both encodings put them in a parameter: 1 plus
/// Shift 1, Alt 2, Ctrl 4, Super 8.
fn modifier_param(mods: ModifiersState) -> u8 {
    1 + u8::from(mods.shift_key()) + 2 * u8::from(mods.alt_key()) + 4 * u8::from(mods.control_key()) + 8 * u8::from(mods.super_key())
}

/// A cursor, editing or function key and how xterm sends it.
#[derive(Clone, Copy)]
enum Functional {
    /// `CSI 1;<mods> <letter>`, unmodified `CSI <letter>` -- or `SS3
    /// <letter>`: always for F1-F4, in cursor-key mode for the cursor keys.
    Letter { letter: u8, ss3: Ss3 },
    /// `CSI <number>;<mods> ~`.
    Tilde(u16),
}

#[derive(Clone, Copy, PartialEq)]
enum Ss3 {
    /// In cursor-key mode (DECCKM).
    AppCursor,
    Always,
}

fn functional(named: NamedKey) -> Option<Functional> {
    use Functional::{Letter, Tilde};
    let cursor = |letter| Letter { letter, ss3: Ss3::AppCursor };
    let function = |letter| Letter { letter, ss3: Ss3::Always };
    Some(match named {
        NamedKey::ArrowUp => cursor(b'A'),
        NamedKey::ArrowDown => cursor(b'B'),
        NamedKey::ArrowRight => cursor(b'C'),
        NamedKey::ArrowLeft => cursor(b'D'),
        NamedKey::Home => cursor(b'H'),
        NamedKey::End => cursor(b'F'),
        NamedKey::F1 => function(b'P'),
        NamedKey::F2 => function(b'Q'),
        NamedKey::F3 => function(b'R'),
        NamedKey::F4 => function(b'S'),
        NamedKey::Insert => Tilde(2),
        NamedKey::Delete => Tilde(3),
        NamedKey::PageUp => Tilde(5),
        NamedKey::PageDown => Tilde(6),
        NamedKey::F5 => Tilde(15),
        NamedKey::F6 => Tilde(17),
        NamedKey::F7 => Tilde(18),
        NamedKey::F8 => Tilde(19),
        NamedKey::F9 => Tilde(20),
        NamedKey::F10 => Tilde(21),
        NamedKey::F11 => Tilde(23),
        NamedKey::F12 => Tilde(24),
        NamedKey::F13 => Tilde(25),
        NamedKey::F14 => Tilde(26),
        NamedKey::F15 => Tilde(28),
        NamedKey::F16 => Tilde(29),
        NamedKey::F17 => Tilde(31),
        NamedKey::F18 => Tilde(32),
        NamedKey::F19 => Tilde(33),
        NamedKey::F20 => Tilde(34),
        _ => return None,
    })
}

/// xterm's encoding.
fn legacy(event: &KeyInput, mods: ModifiersState, mode: TermMode) -> Option<Vec<u8>> {
    let alt = mods.alt_key();
    // Alt goes in front as an ESC, for everything but the sequences that
    // carry it in their parameter.
    let with_alt = |bytes: &[u8]| {
        let mut out = Vec::with_capacity(bytes.len() + 1);
        if alt {
            out.push(ESC);
        }
        out.extend_from_slice(bytes);
        Some(out)
    };
    let text = || event.text.as_deref().filter(|text| !text.is_empty());

    match &event.logical_key {
        Key::Named(named) => {
            if let Some(key) = functional(*named) {
                let param = modifier_param(mods);
                return Some(match key {
                    Functional::Letter { letter, ss3 } if param == 1 => {
                        let ss3 = ss3 == Ss3::Always || (ss3 == Ss3::AppCursor && mode.contains(TermMode::APP_CURSOR));
                        vec![ESC, if ss3 { b'O' } else { b'[' }, letter]
                    }
                    Functional::Letter { letter, .. } => format!("\x1b[1;{param}{}", letter as char).into_bytes(),
                    Functional::Tilde(number) if param == 1 => format!("\x1b[{number}~").into_bytes(),
                    Functional::Tilde(number) => format!("\x1b[{number};{param}~").into_bytes(),
                });
            }
            match named {
                NamedKey::Enter => with_alt(b"\r"),
                NamedKey::Tab if mods.shift_key() => with_alt(b"\x1b[Z"),
                NamedKey::Tab => with_alt(b"\t"),
                // Ctrl+Backspace is ^H, which shells take for deleting a word.
                NamedKey::Backspace if mods.control_key() => with_alt(b"\x08"),
                NamedKey::Backspace => with_alt(b"\x7f"),
                NamedKey::Escape => with_alt(b"\x1b"),
                NamedKey::Space if mods.control_key() => with_alt(b"\0"),
                NamedKey::Space => with_alt(b" "),
                _ => with_alt(text()?.as_bytes()),
            }
        }
        Key::Character(_) if mods.control_key() => match control_code(event) {
            Some(code) => with_alt(&[code]),
            None => with_alt(text()?.as_bytes()),
        },
        _ => with_alt(text()?.as_bytes()),
    }
}

/// The C0 control code Ctrl turns a key into: Ctrl+A is 1, Ctrl+[ is ESC,
/// Ctrl+Space and Ctrl+@ NUL and so on, with the digits 2-8 standing in
/// for the punctuation as on a VT220. The key as typed counts first
/// (Ctrl+Shift+2 is `@` on a US layout), then the key without modifiers.
fn control_code(event: &KeyInput) -> Option<u8> {
    let code = |key: &Key| {
        let Key::Character(text) = key else { return None };
        let mut chars = text.chars();
        let (Some(c), None) = (chars.next(), chars.next()) else { return None };
        Some(match c.to_ascii_lowercase() {
            c @ 'a'..='z' => c as u8 - b'a' + 1,
            ' ' | '@' | '2' => 0,
            '[' | '3' => 0x1b,
            '\\' | '4' => 0x1c,
            ']' | '5' => 0x1d,
            '^' | '~' | '6' => 0x1e,
            '_' | '/' | '-' | '7' => 0x1f,
            '?' | '8' => 0x7f,
            _ => return None,
        })
    };
    code(&event.logical_key).or_else(|| code(&event.key_without_modifiers))
}

/// Whether the kitty protocol's flags make `event` (a press) an escape
/// sequence rather than the old bytes.
fn wants_kitty(event: &KeyInput, mods: ModifiersState, mode: TermMode) -> bool {
    if mode.contains(TermMode::REPORT_ALL_KEYS_AS_ESC) {
        return true;
    }
    if mode.contains(TermMode::DISAMBIGUATE_ESC_CODES) {
        // Esc, keypad keys that type nothing, and every key with a
        // modifier but Shift alone -- Shift+Enter/Tab/Backspace included.
        let keypad = event.location == KeyLocation::Numpad && matches!(event.logical_key, Key::Named(_));
        let modified = !mods.is_empty()
            && (mods != ModifiersState::SHIFT || is_named(event, &[NamedKey::Enter, NamedKey::Tab, NamedKey::Backspace]));
        if is_named(event, &[NamedKey::Escape]) || keypad || modified {
            return true;
        }
    }
    match &event.logical_key {
        // Those with a text of their own keep it.
        Key::Named(_) => {
            !is_named(event, &[NamedKey::Enter, NamedKey::Tab, NamedKey::Backspace, NamedKey::Space, NamedKey::Escape])
        }
        _ => event.text.as_deref().is_none_or(str::is_empty),
    }
}

/// `CSI <key>[:<shifted>] ; <mods>[:<event>] [; <text>] <terminator>`
fn kitty(event: &KeyInput, mods: ModifiersState, mode: TermMode) -> Option<Vec<u8>> {
    let release = event.state == ElementState::Released;
    let event_type = match () {
        _ if !mode.contains(TermMode::REPORT_EVENT_TYPES) => None,
        _ if release => Some(3),
        _ if event.repeat => Some(2),
        // A press is the default.
        _ => None,
    };
    let mut mods = mods;
    // A modifier's own event carries the state after it: Shift pressed
    // is shifted, Shift released isn't.
    if let Key::Named(named) = &event.logical_key {
        let own = match named {
            NamedKey::Shift => ModifiersState::SHIFT,
            NamedKey::Control => ModifiersState::CONTROL,
            NamedKey::Alt => ModifiersState::ALT,
            NamedKey::Super => ModifiersState::SUPER,
            _ => ModifiersState::empty(),
        };
        mods.set(own, !release);
    }
    let text = event
        .text
        .as_deref()
        .filter(|text| mode.contains(TermMode::REPORT_ASSOCIATED_TEXT) && !release && !text.is_empty())
        .filter(|text| !text.chars().any(char::is_control))
        // What Ctrl makes a control code types no text.
        .filter(|_| !(mods.control_key() && control_code(event).is_some()));
    let param = modifier_param(mods);
    let params = param != 1 || event_type.is_some() || text.is_some();

    let (key, terminator) = keypad_key(event)
        .or_else(|| functional_key(event, params))
        .or_else(|| control_key(event))
        .or_else(|| lone_key(event, mode))
        .or_else(|| text_key(event, mods, mode, text.is_some()))?;

    let mut out = format!("\x1b[{key}");
    if params {
        out.push_str(&format!(";{param}"));
        if let Some(event_type) = event_type {
            out.push_str(&format!(":{event_type}"));
        }
    }
    if let Some(text) = text {
        let codes: Vec<String> = text.chars().map(|c| u32::from(c).to_string()).collect();
        out.push_str(&format!(";{}", codes.join(":")));
    }
    out.push(terminator);
    Some(out.into_bytes())
}

/// Keys of the numeric keypad, numbers of their own.
fn keypad_key(event: &KeyInput) -> Option<(String, char)> {
    if event.location != KeyLocation::Numpad {
        return None;
    }
    let code = match &event.logical_key {
        Key::Character(text) => match text.as_str() {
            digit @ ("0" | "1" | "2" | "3" | "4" | "5" | "6" | "7" | "8" | "9") => 57399 + digit.parse::<u32>().ok()?,
            "." => 57409,
            "/" => 57410,
            "*" => 57411,
            "-" => 57412,
            "+" => 57413,
            "=" => 57415,
            "," => 57416,
            _ => return None,
        },
        Key::Named(named) => match named {
            NamedKey::Enter => 57414,
            NamedKey::ArrowLeft => 57417,
            NamedKey::ArrowRight => 57418,
            NamedKey::ArrowUp => 57419,
            NamedKey::ArrowDown => 57420,
            NamedKey::PageUp => 57421,
            NamedKey::PageDown => 57422,
            NamedKey::Home => 57423,
            NamedKey::End => 57424,
            NamedKey::Insert => 57425,
            NamedKey::Delete => 57426,
            NamedKey::Clear => 57427,
            _ => return None,
        },
        _ => return None,
    };
    Some((code.to_string(), 'u'))
}

/// Cursor, editing and function keys: xterm's form, the `1` left out
/// when nothing follows -- except F3, whose `CSI R` would read as a
/// cursor position report, and F13 on, which have numbers of their own.
fn functional_key(event: &KeyInput, params: bool) -> Option<(String, char)> {
    let Key::Named(named) = &event.logical_key else { return None };
    let numbered = match named {
        NamedKey::F3 => return Some(("13".into(), '~')),
        NamedKey::F13 => 57376,
        NamedKey::F14 => 57377,
        NamedKey::F15 => 57378,
        NamedKey::F16 => 57379,
        NamedKey::F17 => 57380,
        NamedKey::F18 => 57381,
        NamedKey::F19 => 57382,
        NamedKey::F20 => 57383,
        NamedKey::F21 => 57384,
        NamedKey::F22 => 57385,
        NamedKey::F23 => 57386,
        NamedKey::F24 => 57387,
        NamedKey::F25 => 57388,
        NamedKey::F26 => 57389,
        NamedKey::F27 => 57390,
        NamedKey::F28 => 57391,
        NamedKey::F29 => 57392,
        NamedKey::F30 => 57393,
        NamedKey::F31 => 57394,
        NamedKey::F32 => 57395,
        NamedKey::F33 => 57396,
        NamedKey::F34 => 57397,
        NamedKey::F35 => 57398,
        NamedKey::PrintScreen => 57361,
        NamedKey::Pause => 57362,
        NamedKey::ContextMenu => 57363,
        NamedKey::MediaPlay => 57428,
        NamedKey::MediaPause => 57429,
        NamedKey::MediaPlayPause => 57430,
        NamedKey::MediaStop => 57432,
        NamedKey::MediaFastForward => 57433,
        NamedKey::MediaRewind => 57434,
        NamedKey::MediaTrackNext => 57435,
        NamedKey::MediaTrackPrevious => 57436,
        NamedKey::MediaRecord => 57437,
        NamedKey::AudioVolumeDown => 57438,
        NamedKey::AudioVolumeUp => 57439,
        NamedKey::AudioVolumeMute => 57440,
        _ => {
            return Some(match functional(*named)? {
                Functional::Letter { letter, .. } => ((if params { "1" } else { "" }).into(), letter as char),
                Functional::Tilde(number) => (number.to_string(), '~'),
            });
        }
    };
    Some((numbered.to_string(), 'u'))
}

/// Keys with a control character of their own.
fn control_key(event: &KeyInput) -> Option<(String, char)> {
    let Key::Named(named) = &event.logical_key else { return None };
    let code = match named {
        NamedKey::Escape => 27,
        NamedKey::Enter => 13,
        NamedKey::Tab => 9,
        NamedKey::Backspace => 127,
        NamedKey::Space => 32,
        _ => return None,
    };
    Some((code.to_string(), 'u'))
}

/// Modifiers and lock keys: only when every key is reported.
fn lone_key(event: &KeyInput, mode: TermMode) -> Option<(String, char)> {
    if !mode.contains(TermMode::REPORT_ALL_KEYS_AS_ESC) {
        return None;
    }
    let Key::Named(named) = &event.logical_key else { return None };
    let right = event.location == KeyLocation::Right;
    let side = |left: u32| if right { left + 6 } else { left };
    let code = match named {
        NamedKey::Shift => side(57441),
        NamedKey::Control => side(57442),
        NamedKey::Alt => side(57443),
        NamedKey::Super => side(57444),
        NamedKey::Hyper => side(57445),
        NamedKey::Meta => side(57446),
        NamedKey::AltGraph => 57453,
        NamedKey::CapsLock => 57358,
        NamedKey::ScrollLock => 57359,
        NamedKey::NumLock => 57360,
        _ => return None,
    };
    Some((code.to_string(), 'u'))
}

/// A key that types a character: its code without modifiers (lowercase),
/// with the alternate-keys flag also the shifted one (`97:65` for
/// Shift+A, `49:33` for Shift+1 on a US layout).
fn text_key(event: &KeyInput, mods: ModifiersState, mode: TermMode, has_text: bool) -> Option<(String, char)> {
    let single = |key: &Key| match key {
        Key::Character(text) => {
            let mut chars = text.chars();
            match (chars.next(), chars.next()) {
                (Some(c), None) => Some(c),
                _ => None,
            }
        }
        _ => None,
    };
    let Some(typed) = single(&event.logical_key) else {
        // Text without a key of its own (composed, from an input method).
        return (mode.contains(TermMode::REPORT_ALL_KEYS_AS_ESC) && has_text).then(|| ("0".into(), 'u'));
    };
    let base = single(&event.key_without_modifiers).unwrap_or(typed);
    let base = base.to_lowercase().next().unwrap_or(base);
    let key = if mode.contains(TermMode::REPORT_ALTERNATE_KEYS) && mods.shift_key() && typed != base {
        format!("{}:{}", u32::from(base), u32::from(typed))
    } else {
        u32::from(base).to_string()
    };
    Some((key, 'u'))
}

#[cfg(test)]
mod tests {
    use winit::keyboard::SmolStr;

    use super::*;

    const NONE: ModifiersState = ModifiersState::empty();
    const SHIFT: ModifiersState = ModifiersState::SHIFT;
    const CTRL: ModifiersState = ModifiersState::CONTROL;
    const ALT: ModifiersState = ModifiersState::ALT;

    /// A press of a character key: `typed` as the layout makes it, `plain`
    /// without modifiers.
    fn char_key(typed: &str, plain: &str) -> KeyInput {
        KeyInput {
            state: ElementState::Pressed,
            logical_key: Key::Character(SmolStr::new(typed)),
            key_without_modifiers: Key::Character(SmolStr::new(plain)),
            text: Some(SmolStr::new(typed)),
            location: KeyLocation::Standard,
            repeat: false,
        }
    }

    fn named(key: NamedKey) -> KeyInput {
        let text = match key {
            NamedKey::Enter => Some("\r"),
            NamedKey::Tab => Some("\t"),
            NamedKey::Space => Some(" "),
            NamedKey::Backspace => Some("\x08"),
            NamedKey::Escape => Some("\x1b"),
            _ => None,
        };
        KeyInput {
            state: ElementState::Pressed,
            logical_key: Key::Named(key),
            key_without_modifiers: Key::Named(key),
            text: text.map(SmolStr::new),
            location: KeyLocation::Standard,
            repeat: false,
        }
    }

    fn released(mut key: KeyInput) -> KeyInput {
        key.state = ElementState::Released;
        key.text = None;
        key
    }

    fn send(key: &KeyInput, mods: ModifiersState, mode: TermMode) -> String {
        key_bytes(key, mods, mode).map(|bytes| String::from_utf8(bytes).unwrap()).unwrap_or_default()
    }

    fn legacy(key: &KeyInput, mods: ModifiersState) -> String {
        send(key, mods, TermMode::default())
    }

    #[test]
    fn text_and_control_codes() {
        assert_eq!(legacy(&char_key("a", "a"), NONE), "a");
        assert_eq!(legacy(&char_key("A", "a"), SHIFT), "A");
        assert_eq!(legacy(&char_key("ä", "ä"), NONE), "ä");
        assert_eq!(legacy(&char_key("a", "a"), CTRL), "\x01");
        assert_eq!(legacy(&char_key("A", "a"), CTRL | SHIFT), "\x01");
        assert_eq!(legacy(&char_key("c", "c"), CTRL), "\x03");
        assert_eq!(legacy(&char_key("[", "["), CTRL), "\x1b");
        assert_eq!(legacy(&char_key("@", "2"), CTRL | SHIFT), "\0");
        assert_eq!(legacy(&char_key("_", "-"), CTRL | SHIFT), "\x1f");
        // No control code for it: the character itself.
        assert_eq!(legacy(&char_key("ö", "ö"), CTRL), "ö");
        assert_eq!(legacy(&named(NamedKey::Space), CTRL), "\0");
    }

    #[test]
    fn alt_puts_an_escape_in_front() {
        assert_eq!(legacy(&char_key("b", "b"), ALT), "\x1bb");
        assert_eq!(legacy(&char_key("B", "b"), ALT | SHIFT), "\x1bB");
        assert_eq!(legacy(&char_key("x", "x"), ALT | CTRL), "\x1b\x18");
        assert_eq!(legacy(&named(NamedKey::Backspace), ALT), "\x1b\x7f");
        assert_eq!(legacy(&named(NamedKey::Enter), ALT), "\x1b\r");
    }

    #[test]
    fn keys_with_bytes_of_their_own() {
        assert_eq!(legacy(&named(NamedKey::Enter), NONE), "\r");
        assert_eq!(legacy(&named(NamedKey::Tab), NONE), "\t");
        assert_eq!(legacy(&named(NamedKey::Tab), SHIFT), "\x1b[Z");
        assert_eq!(legacy(&named(NamedKey::Backspace), NONE), "\x7f");
        assert_eq!(legacy(&named(NamedKey::Backspace), CTRL), "\x08");
        assert_eq!(legacy(&named(NamedKey::Escape), NONE), "\x1b");
        assert_eq!(legacy(&named(NamedKey::Space), NONE), " ");
    }

    #[test]
    fn cursor_and_function_keys() {
        assert_eq!(legacy(&named(NamedKey::ArrowUp), NONE), "\x1b[A");
        assert_eq!(send(&named(NamedKey::ArrowUp), NONE, TermMode::APP_CURSOR), "\x1bOA");
        assert_eq!(send(&named(NamedKey::Home), NONE, TermMode::APP_CURSOR), "\x1bOH");
        assert_eq!(legacy(&named(NamedKey::ArrowRight), CTRL), "\x1b[1;5C");
        assert_eq!(send(&named(NamedKey::ArrowRight), CTRL, TermMode::APP_CURSOR), "\x1b[1;5C");
        assert_eq!(legacy(&named(NamedKey::ArrowLeft), ALT), "\x1b[1;3D");
        assert_eq!(legacy(&named(NamedKey::End), SHIFT), "\x1b[1;2F");
        assert_eq!(legacy(&named(NamedKey::F1), NONE), "\x1bOP");
        assert_eq!(legacy(&named(NamedKey::F4), SHIFT), "\x1b[1;2S");
        assert_eq!(legacy(&named(NamedKey::F5), NONE), "\x1b[15~");
        assert_eq!(legacy(&named(NamedKey::F12), CTRL | SHIFT), "\x1b[24;6~");
        assert_eq!(legacy(&named(NamedKey::Delete), NONE), "\x1b[3~");
        assert_eq!(legacy(&named(NamedKey::PageDown), ALT), "\x1b[6;3~");
    }

    #[test]
    fn nothing_for_modifiers_releases_and_dead_keys() {
        assert_eq!(legacy(&named(NamedKey::Shift), SHIFT), "");
        assert_eq!(legacy(&released(char_key("a", "a")), NONE), "");
        let mut dead = char_key("", "");
        dead.logical_key = Key::Dead(Some('^'));
        dead.text = None;
        assert_eq!(legacy(&dead, NONE), "");
    }

    const DISAMBIGUATE: TermMode = TermMode::DISAMBIGUATE_ESC_CODES;

    /// What fish 4 turns on (`CSI = 5 u`): disambiguate plus alternates.
    #[test]
    fn disambiguated_keys() {
        let mode = DISAMBIGUATE;
        // Plain typing stays text, Enter/Tab/Backspace their old bytes.
        assert_eq!(send(&char_key("a", "a"), NONE, mode), "a");
        assert_eq!(send(&char_key("A", "a"), SHIFT, mode), "A");
        assert_eq!(send(&named(NamedKey::Enter), NONE, mode), "\r");
        assert_eq!(send(&named(NamedKey::Tab), NONE, mode), "\t");
        assert_eq!(send(&named(NamedKey::Backspace), NONE, mode), "\x7f");
        assert_eq!(send(&named(NamedKey::Space), NONE, mode), " ");
        // What the old bytes mix up.
        assert_eq!(send(&named(NamedKey::Escape), NONE, mode), "\x1b[27u");
        assert_eq!(send(&char_key("c", "c"), CTRL, mode), "\x1b[99;5u");
        assert_eq!(send(&char_key("i", "i"), CTRL, mode), "\x1b[105;5u");
        assert_eq!(send(&char_key("[", "["), ALT, mode), "\x1b[91;3u");
        assert_eq!(send(&char_key("A", "a"), CTRL | SHIFT, mode), "\x1b[97;6u");
        assert_eq!(send(&named(NamedKey::Enter), SHIFT, mode), "\x1b[13;2u");
        assert_eq!(send(&named(NamedKey::Tab), SHIFT, mode), "\x1b[9;2u");
        assert_eq!(send(&named(NamedKey::Backspace), CTRL, mode), "\x1b[127;5u");
        assert_eq!(send(&named(NamedKey::Space), CTRL, mode), "\x1b[32;5u");
        // Cursor and function keys in xterm's form, never SS3.
        assert_eq!(send(&named(NamedKey::ArrowUp), NONE, mode | TermMode::APP_CURSOR), "\x1b[A");
        assert_eq!(send(&named(NamedKey::ArrowUp), CTRL, mode), "\x1b[1;5A");
        assert_eq!(send(&named(NamedKey::F1), NONE, mode), "\x1b[P");
        assert_eq!(send(&named(NamedKey::F3), NONE, mode), "\x1b[13~");
        assert_eq!(send(&named(NamedKey::F5), ALT, mode), "\x1b[15;3~");
        assert_eq!(send(&named(NamedKey::F13), NONE, mode), "\x1b[57376u");
        // Shifted keys only with the alternates flag.
        let alternates = mode | TermMode::REPORT_ALTERNATE_KEYS;
        assert_eq!(send(&char_key("A", "a"), CTRL | SHIFT, alternates), "\x1b[97:65;6u");
        assert_eq!(send(&char_key("!", "1"), CTRL | SHIFT, alternates), "\x1b[49:33;6u");
        assert_eq!(send(&char_key("a", "a"), CTRL, alternates), "\x1b[97;5u");
    }

    #[test]
    fn keypad_keys() {
        let mut enter = named(NamedKey::Enter);
        enter.location = KeyLocation::Numpad;
        let mut one = char_key("1", "1");
        one.location = KeyLocation::Numpad;
        assert_eq!(legacy(&enter, NONE), "\r");
        assert_eq!(legacy(&one, NONE), "1");
        assert_eq!(send(&enter, NONE, DISAMBIGUATE), "\x1b[57414u");
        assert_eq!(send(&one, NONE, DISAMBIGUATE), "1", "a digit is text");
        assert_eq!(send(&one, NONE, TermMode::REPORT_ALL_KEYS_AS_ESC), "\x1b[57400u");
    }

    #[test]
    fn repeats_and_releases() {
        let mode = DISAMBIGUATE | TermMode::REPORT_EVENT_TYPES;
        let mut a = char_key("a", "a");
        assert_eq!(send(&a, NONE, mode), "a");
        a.repeat = true;
        assert_eq!(send(&a, NONE, mode), "a", "a repeat of text is text");
        assert_eq!(send(&released(char_key("a", "a")), NONE, mode), "\x1b[97;1:3u");
        let mut up = named(NamedKey::ArrowUp);
        up.repeat = true;
        assert_eq!(send(&up, NONE, mode), "\x1b[1;1:2A");
        assert_eq!(send(&released(named(NamedKey::ArrowUp)), CTRL, mode), "\x1b[1;5:3A");
        assert_eq!(send(&released(char_key("c", "c")), CTRL, mode), "\x1b[99;5:3u");
        // Enter, Tab and Backspace keep quiet when let go.
        assert_eq!(send(&released(named(NamedKey::Enter)), NONE, mode), "");
        // Without the flag nothing is let go.
        assert_eq!(send(&released(char_key("a", "a")), NONE, DISAMBIGUATE), "");
    }

    #[test]
    fn all_keys_with_text() {
        let mode = TermMode::REPORT_ALL_KEYS_AS_ESC | TermMode::REPORT_ASSOCIATED_TEXT | TermMode::REPORT_EVENT_TYPES;
        assert_eq!(send(&char_key("a", "a"), NONE, mode), "\x1b[97;1;97u");
        assert_eq!(send(&char_key("A", "a"), SHIFT, mode), "\x1b[97;2;65u");
        assert_eq!(send(&char_key("c", "c"), CTRL, mode), "\x1b[99;5u");
        assert_eq!(send(&char_key("ö", "ö"), CTRL, mode), "\x1b[246;5;246u");
        assert_eq!(send(&named(NamedKey::Enter), NONE, mode), "\x1b[13u");
        assert_eq!(send(&released(named(NamedKey::Enter)), NONE, mode), "\x1b[13;1:3u");
        // Modifiers themselves, with the state after the event.
        let mut shift = named(NamedKey::Shift);
        shift.location = KeyLocation::Left;
        assert_eq!(send(&shift, NONE, mode), "\x1b[57441;2u");
        assert_eq!(send(&released(shift.clone()), SHIFT, mode), "\x1b[57441;1:3u");
        shift.location = KeyLocation::Right;
        assert_eq!(send(&shift, NONE, mode), "\x1b[57447;2u");
        // Text that isn't one key's.
        assert_eq!(send(&char_key("ê", "e"), NONE, mode), "\x1b[101;1;234u");
        let mut composed = char_key("ab", "");
        composed.key_without_modifiers = Key::Dead(None);
        assert_eq!(send(&composed, NONE, mode), "\x1b[0;1;97:98u");
    }

    #[test]
    fn modifier_keys_alone_need_all_keys() {
        assert_eq!(send(&named(NamedKey::Control), CTRL, DISAMBIGUATE), "");
        assert_eq!(send(&named(NamedKey::Shift), SHIFT, DISAMBIGUATE | TermMode::REPORT_EVENT_TYPES), "");
    }
}
