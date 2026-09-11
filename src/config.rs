//! App configuration.
//!
//! Loaded from `$HOME/.config/terminaal/config.toml` with
//! `#[serde(default)]` so a missing file, an empty file, or a file that
//! only sets a couple of fields all work the same way: whatever isn't
//! specified falls back to `Config::default()`. A missing or unparseable
//! file is never fatal -- we log why and start with defaults, since a
//! typo in the config shouldn't stop the terminal from opening.
//!
//! Every option can also be changed in the settings tab (⚙). Such a
//! change is written back via `toml_edit`, one key at a time, so the rest
//! of the file -- comments, ordering, formatting -- stays exactly as the
//! user wrote it.

use serde::Deserialize;
use std::collections::BTreeMap;
use std::ops::RangeInclusive;
use std::path::{Path, PathBuf};

use crate::i18n::{Language, t};
use crate::shortcuts::{Action, Bindings, KeyCombo};

/// Font sizes the settings and zooming by shortcut allow.
pub const FONT_SIZES: RangeInclusive<f32> = 6.0..=36.0;

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct Config {
    /// Base font size in logical pixels.
    pub font_size: f32,
    /// Line height as a multiple of the font size.
    pub line_height_factor: f32,
    /// Padding around the terminal grid, in logical pixels.
    pub padding: f32,
    /// Scrollback size, in lines.
    pub scrollback_lines: usize,
    /// Lines scrolled per mouse-wheel notch; touchpads scroll by their
    /// pixels instead. Read through [`Config::scroll_lines`].
    pub scroll_lines: f32,
    /// Default window size (logical pixels).
    pub default_width: f64,
    pub default_height: f64,
    /// Whether the cursor should blink at all.
    pub cursor_blink: bool,
    /// Blink half-period in milliseconds (time visible == time hidden).
    pub cursor_blink_interval_ms: u64,
    /// Show the clickable tab bar along the top of the window.
    pub tab_bar: bool,
    /// Shell new tabs start with, e.g. `/usr/bin/fish`. Unset: `$SHELL`.
    pub shell: Option<PathBuf>,
    /// Show the sidebar on startup (toggle with Ctrl+Shift+B).
    pub sidebar: bool,
    /// Sidebar width, in logical pixels.
    pub sidebar_width: f32,
    /// Play the Terminaal animation over the window at startup.
    pub splash: bool,
    /// UI language, `de` or `en`. Unset (or `auto`): German for a German
    /// locale, English otherwise. See [`Config::language`].
    pub language: Option<String>,
    /// Keyboard shortcuts that differ from the defaults, by action name
    /// (`[shortcuts]`); see `shortcuts`.
    pub shortcuts: BTreeMap<String, Bindings>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            font_size: 15.0,
            line_height_factor: 1.25,
            padding: 8.0,
            scrollback_lines: 10_000,
            // Like Alacritty and most desktops.
            scroll_lines: 3.0,
            default_width: 1000.0,
            default_height: 650.0,
            cursor_blink: true,
            cursor_blink_interval_ms: 600,
            tab_bar: true,
            shell: None,
            sidebar: true,
            sidebar_width: 300.0,
            splash: true,
            language: None,
            shortcuts: BTreeMap::new(),
        }
    }
}

impl Config {
    /// `~/.config/terminaal`: the config, and the saved SSH hosts next to it.
    pub fn dir() -> Option<PathBuf> {
        let home = std::env::var_os("HOME")?;
        Some(PathBuf::from(home).join(".config/terminaal"))
    }

    fn path() -> Option<PathBuf> {
        Some(Self::dir()?.join("config.toml"))
    }

    /// Load from disk, falling back to (partial or full) defaults on any
    /// problem -- missing file, unreadable file, or invalid TOML.
    pub fn load() -> Self {
        let Some(path) = Self::path() else {
            log::warn!("$HOME not set, using default config");
            return Self::default();
        };

        let text = match std::fs::read_to_string(&path) {
            Ok(text) => text,
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Self::default(),
            Err(err) => {
                log::warn!("failed to read config at {}: {err}", path.display());
                return Self::default();
            }
        };

        match toml::from_str(&text) {
            Ok(config) => config,
            Err(err) => {
                log::warn!("failed to parse config at {}: {err}", path.display());
                Self::default()
            }
        }
    }

    /// The language chosen in the config, if any. An unknown value counts
    /// as unset rather than failing the whole config.
    pub fn chosen_language(&self) -> Option<Language> {
        self.language.as_deref().and_then(Language::parse)
    }

    /// The UI language: the configured one, else the locale's.
    pub fn language(&self) -> Language {
        if let Some(value) = self.language.as_deref()
            && Language::parse(value).is_none()
            && !value.trim().eq_ignore_ascii_case("auto")
        {
            log::warn!("unknown language {value:?} in config, expected \"de\" or \"en\"");
        }
        self.chosen_language().unwrap_or_else(Language::from_locale)
    }

    /// Set the default shell and persist it to the config file, creating
    /// the file if needed. The error is meant for display in the UI.
    pub fn save_shell(&mut self, shell: &Path) -> Result<(), String> {
        Self::edit(|doc| set_value(doc, "shell", shell.to_string_lossy().as_ref()))?;
        self.shell = Some(shell.to_path_buf());
        Ok(())
    }

    /// Set the UI language (`None`: follow the locale) and persist it.
    pub fn save_language(&mut self, language: Option<Language>) -> Result<(), String> {
        Self::edit(|doc| match language {
            Some(language) => set_value(doc, "language", language.code()),
            None => drop(doc.remove("language")),
        })?;
        self.language = language.map(|language| language.code().to_string());
        Ok(())
    }

    /// Bind `action` to `combos` and persist that under `[shortcuts]`. Its
    /// defaults remove the entry instead.
    pub fn save_shortcut(&mut self, action: Action, combos: &[KeyCombo]) -> Result<(), String> {
        let name = action.name();
        let custom = (combos != action.defaults().as_slice()).then(|| Bindings::of(combos));
        Self::edit(|doc| write_shortcut(doc, &name, custom.as_ref()))?;
        match custom {
            Some(bindings) => self.shortcuts.insert(name.into_owned(), bindings),
            None => self.shortcuts.remove(name.as_ref()),
        };
        Ok(())
    }

    /// Lines per mouse-wheel notch. Zero, negative or not a number counts
    /// as the default rather than breaking scrolling.
    pub fn scroll_lines(&self) -> f32 {
        if self.scroll_lines.is_finite() && self.scroll_lines > 0.0 {
            self.scroll_lines
        } else {
            Self::default().scroll_lines
        }
    }

    /// Take over `setting` in memory only.
    pub fn set(&mut self, setting: Setting) {
        match setting {
            Setting::FontSize(size) => self.font_size = size,
            Setting::LineHeight(factor) => self.line_height_factor = factor,
            Setting::Padding(padding) => self.padding = padding,
            Setting::ScrollbackLines(lines) => self.scrollback_lines = lines,
            Setting::ScrollLines(lines) => self.scroll_lines = lines,
            Setting::WindowSize { width, height } => (self.default_width, self.default_height) = (width, height),
            Setting::CursorBlink(on) => self.cursor_blink = on,
            Setting::CursorBlinkInterval(ms) => self.cursor_blink_interval_ms = ms,
            Setting::TabBar(on) => self.tab_bar = on,
            Setting::Sidebar(on) => self.sidebar = on,
            Setting::SidebarWidth(width) => self.sidebar_width = width,
            Setting::Splash(on) => self.splash = on,
        }
    }

    /// Take over `setting` and persist it.
    pub fn save(&mut self, setting: Setting) -> Result<(), String> {
        Self::edit(|doc| setting.write(doc))?;
        self.set(setting);
        Ok(())
    }

    /// Change the config file in place, creating it if needed.
    fn edit(change: impl FnOnce(&mut toml_edit::DocumentMut)) -> Result<(), String> {
        let path = Self::path().ok_or_else(|| t!("common-home-unset"))?;
        let shown = path.display().to_string();
        let text = match std::fs::read_to_string(&path) {
            Ok(text) => text,
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => String::new(),
            Err(err) => return Err(t!("common-file-unreadable", path = &shown, err = err.to_string())),
        };
        let mut doc: toml_edit::DocumentMut =
            text.parse().map_err(|err: toml_edit::TomlError| t!("config-invalid-toml", path = &shown, err = err.to_string()))?;
        change(&mut doc);

        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir).map_err(|err| format!("{}: {err}", dir.display()))?;
        }
        std::fs::write(&path, doc.to_string()).map_err(|err| format!("{}: {err}", path.display()))
    }
}

/// One option changed in the sidebar, with its new value. Shell and
/// language have their own savers ([`Config::save_shell`],
/// [`Config::save_language`]).
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Setting {
    FontSize(f32),
    LineHeight(f32),
    Padding(f32),
    ScrollbackLines(usize),
    ScrollLines(f32),
    /// Window size at start; both keys at once.
    WindowSize { width: f64, height: f64 },
    CursorBlink(bool),
    CursorBlinkInterval(u64),
    TabBar(bool),
    /// Show the sidebar at start.
    Sidebar(bool),
    SidebarWidth(f32),
    Splash(bool),
}

impl Setting {
    /// Write the value to its key(s) in `doc`, leaving everything else be.
    fn write(self, doc: &mut toml_edit::DocumentMut) {
        // Sliders hand over f32s; 1.3 would come out as 1.2999999523162842.
        let float = |value: f64| (value * 1000.0).round() / 1000.0;
        let int = |value: u64| i64::try_from(value).unwrap_or(i64::MAX);
        match self {
            Self::FontSize(size) => set_value(doc, "font_size", float(size.into())),
            Self::LineHeight(factor) => set_value(doc, "line_height_factor", float(factor.into())),
            Self::Padding(padding) => set_value(doc, "padding", float(padding.into())),
            Self::ScrollbackLines(lines) => set_value(doc, "scrollback_lines", int(lines as u64)),
            Self::ScrollLines(lines) => set_value(doc, "scroll_lines", float(lines.into())),
            Self::WindowSize { width, height } => {
                set_value(doc, "default_width", float(width.round()));
                set_value(doc, "default_height", float(height.round()));
            }
            Self::CursorBlink(on) => set_value(doc, "cursor_blink", on),
            Self::CursorBlinkInterval(ms) => set_value(doc, "cursor_blink_interval_ms", int(ms)),
            Self::TabBar(on) => set_value(doc, "tab_bar", on),
            Self::Sidebar(on) => set_value(doc, "sidebar", on),
            Self::SidebarWidth(width) => set_value(doc, "sidebar_width", float(width.into())),
            Self::Splash(on) => set_value(doc, "splash", on),
        }
    }
}

/// Set `key` to `value`. A comment after the old value stays, spacing
/// included -- assigning a fresh item would drop it.
fn set_value(doc: &mut toml_edit::DocumentMut, key: &str, value: impl Into<toml_edit::Value>) {
    let mut value = value.into();
    if let Some(old) = doc.get(key).and_then(toml_edit::Item::as_value) {
        *value.decor_mut() = old.decor().clone();
    }
    doc[key] = toml_edit::Item::Value(value);
}

/// Set `name` under `[shortcuts]` -- or with `None` remove it, and the
/// table with its last entry. A comment after the old value stays.
fn write_shortcut(doc: &mut toml_edit::DocumentMut, name: &str, bindings: Option<&Bindings>) {
    let item = doc.entry("shortcuts").or_insert(toml_edit::table());
    if item.as_table_like().is_none() {
        *item = toml_edit::table();
    }
    let Some(table) = item.as_table_like_mut() else { return };
    match bindings {
        Some(bindings) => {
            let mut value = match bindings {
                Bindings::One(text) => toml_edit::Value::from(text.as_str()),
                Bindings::Many(texts) => toml_edit::Value::Array(texts.iter().map(String::as_str).collect()),
                Bindings::Other(_) => return,
            };
            if let Some(old) = table.get(name).and_then(toml_edit::Item::as_value) {
                *value.decor_mut() = old.decor().clone();
            }
            table.insert(name, toml_edit::Item::Value(value));
        }
        None => drop(table.remove(name)),
    }
    if table.is_empty() {
        doc.remove("shortcuts");
    }
}

#[cfg(test)]
mod tests {
    use super::{Config, Setting, write_shortcut};
    use crate::shortcuts::Bindings;

    #[test]
    fn shortcuts_are_written_to_their_table_and_removed_again() {
        let mut doc: toml_edit::DocumentMut = "font_size = 12\n".parse().unwrap();
        write_shortcut(&mut doc, "new_tab", Some(&Bindings::One("Ctrl+Alt+N".into())));
        write_shortcut(&mut doc, "copy", Some(&Bindings::Many(vec!["Ctrl+Insert".into(), "Ctrl+Shift+C".into()])));
        write_shortcut(&mut doc, "paste", Some(&Bindings::Many(Vec::new())));
        let text = doc.to_string();
        assert!(text.starts_with("font_size = 12\n"), "{text}");
        assert!(text.contains("[shortcuts]\n"), "{text}");

        let config: Config = toml::from_str(&text).unwrap();
        assert_eq!(config.font_size, 12.0);
        assert_eq!(config.shortcuts["new_tab"], Bindings::One("Ctrl+Alt+N".into()));
        assert_eq!(config.shortcuts["copy"], Bindings::Many(vec!["Ctrl+Insert".into(), "Ctrl+Shift+C".into()]));
        assert_eq!(config.shortcuts["paste"], Bindings::Many(Vec::new()));

        for name in ["new_tab", "copy", "paste"] {
            write_shortcut(&mut doc, name, None);
        }
        assert_eq!(doc.to_string().trim(), "font_size = 12");
    }

    #[test]
    fn settings_are_written_in_place_and_read_back() {
        let mut doc: toml_edit::DocumentMut = "# mine\nfont_size = 12 # small\ntab_bar = true\n".parse().unwrap();
        for setting in [
            Setting::FontSize(14.0),
            Setting::LineHeight(1.3),
            Setting::ScrollbackLines(20_000),
            Setting::WindowSize { width: 1200.4, height: 700.0 },
            Setting::TabBar(false),
            Setting::CursorBlinkInterval(450),
        ] {
            setting.write(&mut doc);
        }
        let text = doc.to_string();
        assert!(text.starts_with("# mine\nfont_size = 14.0 # small\ntab_bar = false\n"), "{text}");
        assert!(text.contains("line_height_factor = 1.3\n"), "{text}");

        let config: Config = toml::from_str(&text).unwrap();
        assert_eq!(config.font_size, 14.0);
        assert_eq!(config.line_height_factor, 1.3);
        assert_eq!(config.scrollback_lines, 20_000);
        assert_eq!((config.default_width, config.default_height), (1200.0, 700.0));
        assert!(!config.tab_bar);
        assert_eq!(config.cursor_blink_interval_ms, 450);
    }

    #[test]
    fn scroll_lines_accepts_integers_and_rejects_nonsense() {
        let parse = |text| toml::from_str::<Config>(text).unwrap().scroll_lines();
        assert_eq!(parse(""), 3.0);
        assert_eq!(parse("scroll_lines = 5"), 5.0);
        assert_eq!(parse("scroll_lines = 1.5"), 1.5);
        assert_eq!(parse("scroll_lines = 0"), 3.0);
        assert_eq!(parse("scroll_lines = -2.0"), 3.0);
        assert_eq!(parse("scroll_lines = nan"), 3.0);
    }
}
