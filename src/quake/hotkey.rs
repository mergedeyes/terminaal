//! A global shortcut for the drop-down Terminaal on Windows
//! (`quake_hotkey`): unlike a Wayland client, a Windows program may
//! register one. A thread of its own registers it and waits for its
//! `WM_HOTKEY` messages -- they go to the queue of the thread that
//! registered it.

use std::sync::mpsc;

use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
    MOD_ALT, MOD_CONTROL, MOD_NOREPEAT, MOD_SHIFT, MOD_WIN, RegisterHotKey, VK_BACK, VK_DELETE, VK_DOWN, VK_END,
    VK_ESCAPE, VK_F1, VK_HOME, VK_INSERT, VK_LEFT, VK_NEXT, VK_PRIOR, VK_RETURN, VK_RIGHT, VK_SPACE, VK_TAB, VK_UP,
    VkKeyScanW,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{GetMessageW, MSG, WM_HOTKEY};
use winit::keyboard::NamedKey;

use crate::shortcuts::{KeyCombo, KeyName};

/// Call `toggled` whenever `combo` is pressed, wherever the focus is.
/// `Err` (for the log) if Windows has no key for it or another program
/// has it.
pub fn listen(combo: KeyCombo, toggled: impl Fn() + Send + 'static) -> Result<(), String> {
    let vk = virtual_key(combo.key).ok_or_else(|| format!("no key for {combo}"))?;
    let mut modifiers = MOD_NOREPEAT;
    for (on, modifier) in [(combo.ctrl, MOD_CONTROL), (combo.alt, MOD_ALT), (combo.shift, MOD_SHIFT), (combo.logo, MOD_WIN)] {
        if on {
            modifiers |= modifier;
        }
    }
    let (registered, result) = mpsc::channel();
    std::thread::Builder::new()
        .name("quake hotkey".into())
        .spawn(move || {
            // SAFETY: no window: the messages come to this thread.
            let ok = unsafe { RegisterHotKey(std::ptr::null_mut(), 1, modifiers, u32::from(vk)) } != 0;
            let _ = registered.send(if ok { Ok(()) } else { Err(std::io::Error::last_os_error().to_string()) });
            if !ok {
                return;
            }
            // SAFETY: a plain C struct, zeroed is valid; GetMessageW fills it.
            let mut msg: MSG = unsafe { std::mem::zeroed() };
            while unsafe { GetMessageW(&mut msg, std::ptr::null_mut(), 0, 0) } > 0 {
                if msg.message == WM_HOTKEY {
                    toggled();
                }
            }
        })
        .map_err(|err| err.to_string())?;
    result.recv().map_err(|err| err.to_string())?
}

/// The virtual key: a character through the keyboard layout (as
/// shortcuts go by the layout's keys), named keys by their code.
fn virtual_key(key: KeyName) -> Option<u16> {
    let named = match key {
        KeyName::Char(c) => {
            let mut utf16 = [0; 2];
            let [unit] = c.encode_utf16(&mut utf16) else { return None };
            // SAFETY: a plain lookup; -1 when the layout has no key for it.
            let scan = unsafe { VkKeyScanW(*unit) };
            return (scan != -1).then_some(scan as u16 & 0xff);
        }
        KeyName::Named(named) => named,
    };
    let f = |n: u16| VK_F1 + n;
    Some(match named {
        NamedKey::Tab => VK_TAB,
        NamedKey::Enter => VK_RETURN,
        NamedKey::Escape => VK_ESCAPE,
        NamedKey::Space => VK_SPACE,
        NamedKey::Backspace => VK_BACK,
        NamedKey::Delete => VK_DELETE,
        NamedKey::Insert => VK_INSERT,
        NamedKey::Home => VK_HOME,
        NamedKey::End => VK_END,
        NamedKey::PageUp => VK_PRIOR,
        NamedKey::PageDown => VK_NEXT,
        NamedKey::ArrowUp => VK_UP,
        NamedKey::ArrowDown => VK_DOWN,
        NamedKey::ArrowLeft => VK_LEFT,
        NamedKey::ArrowRight => VK_RIGHT,
        NamedKey::F1 => f(0),
        NamedKey::F2 => f(1),
        NamedKey::F3 => f(2),
        NamedKey::F4 => f(3),
        NamedKey::F5 => f(4),
        NamedKey::F6 => f(5),
        NamedKey::F7 => f(6),
        NamedKey::F8 => f(7),
        NamedKey::F9 => f(8),
        NamedKey::F10 => f(9),
        NamedKey::F11 => f(10),
        NamedKey::F12 => f(11),
        _ => return None,
    })
}
