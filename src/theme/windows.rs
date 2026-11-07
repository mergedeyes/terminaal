//! The "Windows" theme: following Windows' own light/dark app mode and
//! accent colour, the counterpart of the COSMIC theme ([`super::cosmic`]).
//! Windows keeps both in the registry -- the mode as `AppsUseLightTheme`
//! under `…\Themes\Personalize`, the accent's shades as `AccentPalette`
//! under `…\Explorer\Accent` (and the plain colour as DWM's `AccentColor`)
//! -- and [`watch`] notices when they change.
//!
//! The console gets Windows Terminal's defaults for the mode (Campbell
//! dark, One Half Light light; `themes/windows-*.toml`), the chrome
//! Windows 11's greys; the accent colours cursor, selection and the
//! chrome's highlights, in the shade Windows' own apps use for the mode.

use alacritty_terminal::vte::ansi::Rgb;

use super::{Source, Theme, mix};

/// The theme's name in the list.
pub const NAME: &str = "Windows";

const DARK: &str = include_str!("../../themes/windows-dark.toml");
const LIGHT: &str = include_str!("../../themes/windows-light.toml");

/// Windows' default accent in the shades for a dark and a light
/// background, for when the registry has none.
const DEFAULT_ACCENT: (Rgb, Rgb) = (Rgb { r: 0x60, g: 0xcd, b: 0xff }, Rgb { r: 0x00, g: 0x5f, b: 0xb8 });

#[cfg(windows)]
const PERSONALIZE: &str = r"Software\Microsoft\Windows\CurrentVersion\Themes\Personalize";
#[cfg(windows)]
const ACCENT: &str = r"Software\Microsoft\Windows\CurrentVersion\Explorer\Accent";
#[cfg(windows)]
const DWM: &str = r"Software\Microsoft\Windows\DWM";

/// A change of accent writes several values; [`watch`] waits until none
/// has changed for this long.
#[cfg(windows)]
const SETTLE_MS: u32 = 200;

/// What the theme is made from, as Windows has it set.
#[derive(Clone, Debug, PartialEq)]
pub struct Desktop {
    /// Apps in dark mode.
    pub dark: bool,
    /// `AccentPalette`: eight RGBA colours, lightest to darkest, the
    /// accent itself fourth.
    pub palette: Option<Vec<u8>>,
    /// DWM's `AccentColor`, 0xAABBGGRR.
    pub color: Option<u32>,
}

impl Desktop {
    /// Windows' settings right now; `None` off Windows.
    pub fn current() -> Option<Self> {
        #[cfg(windows)]
        {
            // No value at all: Windows' default, light.
            let light = registry::dword(PERSONALIZE, "AppsUseLightTheme").unwrap_or(1);
            Some(Self {
                dark: light == 0,
                palette: registry::binary(ACCENT, "AccentPalette"),
                color: registry::dword(DWM, "AccentColor"),
            })
        }
        #[cfg(not(windows))]
        None
    }

    /// The accent in the shade Windows' apps use on this mode's
    /// background (WinUI's `AccentFillColorDefault`): "light 2" on dark,
    /// "dark 1" on light. DWM's plain colour moved that way if there's no
    /// palette, Windows' default blue if there's neither.
    pub fn accent(&self) -> Rgb {
        let index = if self.dark { 1 } else { 4 };
        if let Some(rgba) = self.palette.as_deref().and_then(|palette| palette.get(index * 4..index * 4 + 3)) {
            return Rgb { r: rgba[0], g: rgba[1], b: rgba[2] };
        }
        match self.color {
            Some(abgr) => {
                let [r, g, b, _] = abgr.to_le_bytes();
                let plain = Rgb { r, g, b };
                if self.dark {
                    mix(plain, Rgb { r: 255, g: 255, b: 255 }, 0.4)
                } else {
                    mix(plain, Rgb::default(), 0.2)
                }
            }
            None if self.dark => DEFAULT_ACCENT.0,
            None => DEFAULT_ACCENT.1,
        }
    }

    /// The theme for these settings.
    pub fn theme(&self) -> Theme {
        let base = if self.dark { DARK } else { LIGHT };
        let mut theme = Theme::parse(NAME, base, Source::Windows).expect("built-in Windows theme");
        let accent = self.accent();
        let (terminal, ui) = (&mut theme.terminal, &mut theme.ui);
        terminal.cursor = accent;
        terminal.selection = mix(terminal.background, accent, 0.35);
        ui.accent = accent;
        ui.selected = mix(ui.background, accent, 0.2);
        ui.text_selection = mix(ui.background, accent, 0.4);
        theme
    }
}

/// Call `changed` whenever Windows' mode or accent colour changes. Runs a
/// thread blocked on registry notifications, so a quiet system costs
/// nothing; `changed` comes once a change has settled ([`SETTLE_MS`]).
#[cfg(windows)]
pub fn watch(changed: impl Fn() + Send + 'static) {
    let keys: Vec<registry::Watched> = [PERSONALIZE, ACCENT, DWM].into_iter().filter_map(registry::Watched::open).collect();
    if keys.is_empty() {
        log::warn!("can't watch Windows' theme settings");
        return;
    }
    let thread = std::thread::Builder::new().name("windows-theme".into()).spawn(move || {
        if !keys.iter().all(registry::Watched::arm) {
            return;
        }
        while let Some(fired) = registry::wait(&keys, None) {
            if !keys[fired].arm() {
                break;
            }
            loop {
                match registry::wait(&keys, Some(SETTLE_MS)) {
                    Some(fired) if keys[fired].arm() => continue,
                    Some(_) => return,
                    None => break,
                }
            }
            log::debug!("Windows' theme settings changed");
            changed();
        }
        log::warn!("stopped watching Windows' theme settings: {}", std::io::Error::last_os_error());
    });
    if let Err(err) = thread {
        log::warn!("can't watch Windows' theme settings: {err}");
    }
}

/// Windows only.
#[cfg(not(windows))]
pub fn watch(_changed: impl Fn() + Send + 'static) {}

#[cfg(windows)]
mod registry {
    use std::ffi::OsStr;
    use std::os::windows::ffi::OsStrExt;

    use windows_sys::Win32::Foundation::{CloseHandle, HANDLE, WAIT_OBJECT_0, WAIT_TIMEOUT};
    use windows_sys::Win32::System::Registry::{
        HKEY, HKEY_CURRENT_USER, KEY_NOTIFY, REG_NOTIFY_CHANGE_LAST_SET, RRF_RT_REG_BINARY, RRF_RT_REG_DWORD, RegCloseKey,
        RegGetValueW, RegNotifyChangeKeyValue, RegOpenKeyExW,
    };
    use windows_sys::Win32::System::Threading::{CreateEventW, INFINITE, WaitForMultipleObjects};

    fn wide(text: &str) -> Vec<u16> {
        OsStr::new(text).encode_wide().chain(Some(0)).collect()
    }

    pub fn dword(key: &str, value: &str) -> Option<u32> {
        let (key, value) = (wide(key), wide(value));
        let mut data = 0u32;
        let mut size = 4u32;
        // SAFETY: NUL-terminated names, `data` holds the `size` bytes a
        // DWORD takes.
        let status = unsafe {
            RegGetValueW(
                HKEY_CURRENT_USER,
                key.as_ptr(),
                value.as_ptr(),
                RRF_RT_REG_DWORD,
                std::ptr::null_mut(),
                (&mut data as *mut u32).cast(),
                &mut size,
            )
        };
        (status == 0).then_some(data)
    }

    pub fn binary(key: &str, value: &str) -> Option<Vec<u8>> {
        let (key, value) = (wide(key), wide(value));
        let mut data = vec![0u8; 256];
        let mut size = data.len() as u32;
        // SAFETY: NUL-terminated names, `data` holds `size` bytes.
        let status = unsafe {
            RegGetValueW(
                HKEY_CURRENT_USER,
                key.as_ptr(),
                value.as_ptr(),
                RRF_RT_REG_BINARY,
                std::ptr::null_mut(),
                data.as_mut_ptr().cast(),
                &mut size,
            )
        };
        if status != 0 {
            return None;
        }
        data.truncate(size as usize);
        Some(data)
    }

    /// A key of the user's, open for change notifications, and the event
    /// they set.
    pub struct Watched {
        key: HKEY,
        event: HANDLE,
    }

    // SAFETY: registry and event handles may be used from any thread.
    unsafe impl Send for Watched {}

    impl Watched {
        pub fn open(path: &str) -> Option<Self> {
            let path = wide(path);
            let mut key: HKEY = std::ptr::null_mut();
            // SAFETY: NUL-terminated path; the handles are closed on drop.
            unsafe {
                if RegOpenKeyExW(HKEY_CURRENT_USER, path.as_ptr(), 0, KEY_NOTIFY, &mut key) != 0 {
                    return None;
                }
                // Auto-reset: one wait per notification.
                let event = CreateEventW(std::ptr::null(), 0, 0, std::ptr::null());
                if event.is_null() {
                    RegCloseKey(key);
                    return None;
                }
                Some(Self { key, event })
            }
        }

        /// Have the next change of a value set the event. Once per
        /// notification: each fires a single time.
        pub fn arm(&self) -> bool {
            // SAFETY: both handles are ours and open.
            unsafe { RegNotifyChangeKeyValue(self.key, 0, REG_NOTIFY_CHANGE_LAST_SET, self.event, 1) == 0 }
        }
    }

    impl Drop for Watched {
        fn drop(&mut self) {
            // SAFETY: ours, closed once.
            unsafe {
                RegCloseKey(self.key);
                CloseHandle(self.event);
            }
        }
    }

    /// Which key changed first within `ms` (`None`: as long as it takes);
    /// `None` on timeout or failure.
    pub fn wait(keys: &[Watched], ms: Option<u32>) -> Option<usize> {
        let events: Vec<HANDLE> = keys.iter().map(|key| key.event).collect();
        // SAFETY: open event handles, fewer than MAXIMUM_WAIT_OBJECTS.
        let result = unsafe { WaitForMultipleObjects(events.len() as u32, events.as_ptr(), 0, ms.unwrap_or(INFINITE)) };
        if result == WAIT_TIMEOUT {
            return None;
        }
        let index = result.wrapping_sub(WAIT_OBJECT_0) as usize;
        (index < events.len()).then_some(index)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rgb(hex: u32) -> Rgb {
        let [_, r, g, b] = hex.to_be_bytes();
        Rgb { r, g, b }
    }

    /// An `AccentPalette` with the shades of Windows' default blue.
    const BLUE: [u8; 32] = [
        0x99, 0xeb, 0xff, 0x00, 0x4c, 0xc2, 0xff, 0x00, 0x00, 0x91, 0xf8, 0x00, 0x00, 0x78, 0xd4, 0x00, 0x00, 0x5a, 0x9e, 0x00, 0x00,
        0x42, 0x75, 0x00, 0x00, 0x26, 0x42, 0x00, 0xf7, 0x63, 0x0c, 0x00,
    ];

    #[test]
    fn the_accent_takes_the_shade_for_the_mode() {
        let dark = Desktop { dark: true, palette: Some(BLUE.to_vec()), color: None };
        assert_eq!(dark.accent(), rgb(0x4cc2ff));
        let light = Desktop { dark: false, ..dark };
        assert_eq!(light.accent(), rgb(0x005a9e));
    }

    #[test]
    fn without_a_palette_dwm_s_colour_is_moved_towards_the_background_s_opposite() {
        // 0xAABBGGRR: red.
        let dark = Desktop { dark: true, palette: None, color: Some(0xff00_00ff) };
        assert_eq!(dark.accent(), mix(rgb(0xff0000), rgb(0xffffff), 0.4));
        let light = Desktop { dark: false, ..dark };
        assert_eq!(light.accent(), mix(rgb(0xff0000), rgb(0x000000), 0.2));
        // A short palette counts as none.
        let short = Desktop { dark: true, palette: Some(vec![1, 2, 3]), color: None };
        assert_eq!(short.accent(), DEFAULT_ACCENT.0);
        assert_eq!(Desktop { dark: false, ..short }.accent(), DEFAULT_ACCENT.1);
    }

    /// Only reads the user's settings, never writes them.
    #[cfg(windows)]
    #[test]
    fn windows_settings_are_read_from_the_registry() {
        let desktop = Desktop::current().expect("on Windows");
        if let Some(palette) = &desktop.palette {
            assert_eq!(palette.len(), 32, "{palette:?}");
        }
        assert_eq!(desktop.theme().ui.is_dark(), desktop.dark);
    }

    #[test]
    fn the_theme_follows_the_mode_and_wears_the_accent() {
        let dark = Desktop { dark: true, palette: Some(BLUE.to_vec()), color: None }.theme();
        assert_eq!(dark.name, NAME);
        assert_eq!(dark.source, Source::Windows);
        assert!(dark.ui.is_dark());
        assert_eq!(dark.terminal.background, rgb(0x0c0c0c));
        assert_eq!(dark.terminal.cursor, rgb(0x4cc2ff));
        assert_eq!(dark.ui.accent, rgb(0x4cc2ff));

        let light = Desktop { dark: false, palette: Some(BLUE.to_vec()), color: None }.theme();
        assert!(!light.ui.is_dark());
        assert_eq!(light.terminal.background, rgb(0xfafafa));
        assert_eq!(light.ui.accent, rgb(0x005a9e));
        assert_eq!(light.ui.selected, mix(rgb(0xf3f3f3), rgb(0x005a9e), 0.2));
    }
}
