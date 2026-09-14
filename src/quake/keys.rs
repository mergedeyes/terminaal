//! The drop-down window's keyboard: `wl_keyboard` events through
//! xkbcommon into [`KeyInput`] -- the same the rest of the app gets from
//! winit, whose own xkb handling isn't public.

use std::os::fd::OwnedFd;
use std::time::Duration;

use winit::event::ElementState;
use winit::keyboard::{Key, ModifiersState, NamedKey, NativeKey, SmolStr};
use xkbcommon::xkb::{self, compose, keysyms};

use crate::input::KeyInput;

/// Keymap, modifier state and dead-key composition of the seat's keyboard.
pub struct Keyboard {
    context: xkb::Context,
    keymap: Option<(xkb::Keymap, xkb::State)>,
    compose: Option<compose::State>,
    pub modifiers: ModifiersState,
    /// Characters per second and the wait before the first repeat;
    /// `None`: the compositor wants no repeating.
    pub repeat: Option<(u32, Duration)>,
}

impl Keyboard {
    pub fn new() -> Self {
        let context = xkb::Context::new(xkb::CONTEXT_NO_FLAGS);
        let compose = compose_locale().and_then(|locale| {
            compose::Table::new_from_locale(&context, locale.as_ref(), compose::COMPILE_NO_FLAGS)
                .inspect_err(|()| log::info!("no compose table for {locale:?}"))
                .ok()
        });
        let compose = compose.map(|table| compose::State::new(&table, compose::STATE_NO_FLAGS));
        Self { context, keymap: None, compose, modifiers: ModifiersState::empty(), repeat: Some((25, Duration::from_millis(600))) }
    }

    /// The compositor's keymap, as `wl_keyboard.keymap` hands it over.
    pub fn set_keymap(&mut self, fd: OwnedFd, size: u32) {
        // SAFETY: the fd and size come from the compositor for this purpose.
        let keymap = unsafe {
            xkb::Keymap::new_from_fd(&self.context, fd, size as usize, xkb::KEYMAP_FORMAT_TEXT_V1, xkb::KEYMAP_COMPILE_NO_FLAGS)
        };
        match keymap {
            Ok(Some(keymap)) => {
                let state = xkb::State::new(&keymap);
                self.keymap = Some((keymap, state));
            }
            Ok(None) => log::warn!("the compositor's keymap didn't compile"),
            Err(err) => log::warn!("failed to read the compositor's keymap: {err}"),
        }
    }

    pub fn set_modifiers(&mut self, depressed: u32, latched: u32, locked: u32, group: u32) {
        let Some((_, state)) = &mut self.keymap else { return };
        state.update_mask(depressed, latched, locked, 0, 0, group);
        let active = |name| state.mod_name_is_active(name, xkb::STATE_MODS_EFFECTIVE);
        let mut modifiers = ModifiersState::empty();
        modifiers.set(ModifiersState::SHIFT, active(xkb::MOD_NAME_SHIFT));
        modifiers.set(ModifiersState::CONTROL, active(xkb::MOD_NAME_CTRL));
        modifiers.set(ModifiersState::ALT, active(xkb::MOD_NAME_ALT));
        modifiers.set(ModifiersState::SUPER, active(xkb::MOD_NAME_LOGO));
        self.modifiers = modifiers;
    }

    /// Key `code` (evdev) went down or up; `None` without a keymap.
    pub fn key(&mut self, code: u32, state: ElementState) -> Option<KeyInput> {
        let (keymap, xkb_state) = self.keymap.as_ref()?;
        let key = xkb::Keycode::new(code + 8);
        let sym = xkb_state.key_get_one_sym(key);
        let layout = xkb_state.key_get_layout(key);
        let plain = keymap.key_get_syms_by_level(key, layout, 0).first().copied().unwrap_or(sym);
        let mut text = Some(xkb_state.key_get_utf8(key)).filter(|text| !text.is_empty());
        let mut logical_key = key_of(sym, text.as_deref());

        if state == ElementState::Pressed
            && let Some(compose) = &mut self.compose
        {
            compose.feed(sym);
            match compose.status() {
                compose::Status::Composing => {
                    // The dead key itself types nothing yet.
                    text = None;
                    if !matches!(logical_key, Key::Dead(_)) {
                        logical_key = Key::Dead(None);
                    }
                }
                compose::Status::Composed => {
                    text = compose.utf8().filter(|text| !text.is_empty());
                    if let Some(composed) = &text {
                        logical_key = Key::Character(SmolStr::new(composed));
                    }
                    compose.reset();
                }
                compose::Status::Cancelled => {
                    text = None;
                    compose.reset();
                }
                compose::Status::Nothing => {}
            }
        }
        Some(KeyInput { state, key_without_modifiers: key_of(plain, None), logical_key, text: text.map(SmolStr::from) })
    }

    /// Whether holding key `code` repeats it.
    pub fn repeats(&self, code: u32) -> bool {
        self.keymap.as_ref().is_some_and(|(keymap, _)| keymap.key_repeats(xkb::Keycode::new(code + 8)))
    }
}

/// The locale for dead keys and compose sequences, like libc picks it.
fn compose_locale() -> Option<String> {
    ["LC_ALL", "LC_CTYPE", "LANG"]
        .into_iter()
        .filter_map(|name| std::env::var(name).ok())
        .find(|value| !value.is_empty())
        .or_else(|| Some("C".into()))
}

/// The key a keysym stands for, as winit names it. `text` is what it types,
/// for keys that aren't named.
pub fn key_of(sym: xkb::Keysym, text: Option<&str>) -> Key {
    if let Some(named) = named_key(sym.raw()) {
        return Key::Named(named);
    }
    let raw = sym.raw();
    if (keysyms::KEY_dead_grave..=keysyms::KEY_dead_greek).contains(&raw) {
        return Key::Dead(None);
    }
    match sym.key_char().filter(|c| !c.is_control()) {
        Some(c) => Key::Character(SmolStr::new(c.encode_utf8(&mut [0; 4]))),
        None => match text.filter(|text| !text.chars().any(char::is_control)) {
            Some(text) => Key::Character(SmolStr::new(text)),
            None => Key::Unidentified(NativeKey::Xkb(raw)),
        },
    }
}

fn named_key(raw: u32) -> Option<NamedKey> {
    use NamedKey as N;
    use keysyms as k;
    Some(match raw {
        k::KEY_Return | k::KEY_KP_Enter | k::KEY_ISO_Enter => N::Enter,
        k::KEY_Tab | k::KEY_KP_Tab | k::KEY_ISO_Left_Tab => N::Tab,
        k::KEY_space | k::KEY_KP_Space => N::Space,
        k::KEY_BackSpace => N::Backspace,
        k::KEY_Escape => N::Escape,
        k::KEY_Delete | k::KEY_KP_Delete => N::Delete,
        k::KEY_Insert | k::KEY_KP_Insert => N::Insert,
        k::KEY_Home | k::KEY_KP_Home => N::Home,
        k::KEY_End | k::KEY_KP_End => N::End,
        k::KEY_Page_Up | k::KEY_KP_Page_Up => N::PageUp,
        k::KEY_Page_Down | k::KEY_KP_Page_Down => N::PageDown,
        k::KEY_Left | k::KEY_KP_Left => N::ArrowLeft,
        k::KEY_Right | k::KEY_KP_Right => N::ArrowRight,
        k::KEY_Up | k::KEY_KP_Up => N::ArrowUp,
        k::KEY_Down | k::KEY_KP_Down => N::ArrowDown,
        k::KEY_Shift_L | k::KEY_Shift_R => N::Shift,
        k::KEY_Control_L | k::KEY_Control_R => N::Control,
        k::KEY_Alt_L | k::KEY_Alt_R => N::Alt,
        k::KEY_ISO_Level3_Shift => N::AltGraph,
        k::KEY_Super_L | k::KEY_Super_R => N::Super,
        k::KEY_Meta_L | k::KEY_Meta_R => N::Meta,
        k::KEY_Caps_Lock => N::CapsLock,
        k::KEY_Num_Lock => N::NumLock,
        k::KEY_Scroll_Lock => N::ScrollLock,
        k::KEY_Pause => N::Pause,
        k::KEY_Print => N::PrintScreen,
        k::KEY_Menu => N::ContextMenu,
        k::KEY_F1 => N::F1,
        k::KEY_F2 => N::F2,
        k::KEY_F3 => N::F3,
        k::KEY_F4 => N::F4,
        k::KEY_F5 => N::F5,
        k::KEY_F6 => N::F6,
        k::KEY_F7 => N::F7,
        k::KEY_F8 => N::F8,
        k::KEY_F9 => N::F9,
        k::KEY_F10 => N::F10,
        k::KEY_F11 => N::F11,
        k::KEY_F12 => N::F12,
        k::KEY_F13 => N::F13,
        k::KEY_F14 => N::F14,
        k::KEY_F15 => N::F15,
        k::KEY_F16 => N::F16,
        k::KEY_F17 => N::F17,
        k::KEY_F18 => N::F18,
        k::KEY_F19 => N::F19,
        k::KEY_F20 => N::F20,
        k::KEY_F21 => N::F21,
        k::KEY_F22 => N::F22,
        k::KEY_F23 => N::F23,
        k::KEY_F24 => N::F24,
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sym(raw: u32) -> xkb::Keysym {
        xkb::Keysym::new(raw)
    }

    #[test]
    fn keysyms_become_winit_keys() {
        assert_eq!(key_of(sym(keysyms::KEY_Return), Some("\r")), Key::Named(NamedKey::Enter));
        assert_eq!(key_of(sym(keysyms::KEY_KP_Page_Up), None), Key::Named(NamedKey::PageUp));
        assert_eq!(key_of(sym(keysyms::KEY_ISO_Left_Tab), None), Key::Named(NamedKey::Tab));
        assert_eq!(key_of(sym(keysyms::KEY_a), Some("a")), Key::Character("a".into()));
        assert_eq!(key_of(sym(keysyms::KEY_A), Some("A")), Key::Character("A".into()));
        assert_eq!(key_of(sym(keysyms::KEY_adiaeresis), Some("ä")), Key::Character("ä".into()));
        assert_eq!(key_of(sym(keysyms::KEY_plus), Some("+")), Key::Character("+".into()));
        assert_eq!(key_of(sym(keysyms::KEY_dead_circumflex), None), Key::Dead(None));
        assert_eq!(key_of(sym(keysyms::KEY_F12), None), Key::Named(NamedKey::F12));
    }

    /// Against a real keymap: German layout, Shift, Ctrl and a dead key.
    #[test]
    fn a_german_keymap_types_and_composes() {
        let mut keyboard = Keyboard::new();
        let context = xkb::Context::new(xkb::CONTEXT_NO_FLAGS);
        let Some(keymap) = xkb::Keymap::new_from_names(&context, "", "pc105", "de", "", None, xkb::KEYMAP_COMPILE_NO_FLAGS)
        else {
            eprintln!("no xkb data for a German layout; skipped");
            return;
        };
        let state = xkb::State::new(&keymap);
        keyboard.keymap = Some((keymap, state));
        // evdev codes
        const KEY_Z: u32 = 44; // "y" on a German layout
        const KEY_E: u32 = 18;
        const KEY_GRAVE: u32 = 41; // dead circumflex
        let shift = 1; // Shift's bit in the keymap's modifier mask

        let y = keyboard.key(KEY_Z, ElementState::Pressed).unwrap();
        assert_eq!((y.logical_key, y.text.as_deref()), (Key::Character("y".into()), Some("y")));

        keyboard.set_modifiers(shift, 0, 0, 0);
        assert_eq!(keyboard.modifiers, ModifiersState::SHIFT);
        let big = keyboard.key(KEY_Z, ElementState::Pressed).unwrap();
        assert_eq!(big.logical_key, Key::Character("Y".into()));
        assert_eq!(big.key_without_modifiers, Key::Character("y".into()), "shortcuts see the plain key");
        keyboard.set_modifiers(0, 0, 0, 0);

        if keyboard.compose.is_some() {
            let dead = keyboard.key(KEY_GRAVE, ElementState::Pressed).unwrap();
            assert_eq!((dead.logical_key, dead.text), (Key::Dead(None), None));
            let e = keyboard.key(KEY_E, ElementState::Pressed).unwrap();
            assert_eq!(e.text.as_deref(), Some("ê"));
        }
        assert!(keyboard.repeats(KEY_E));
    }
}
