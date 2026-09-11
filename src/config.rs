//! App configuration.
//!
//! Loaded from `$HOME/.config/terminaal/config.toml` with
//! `#[serde(default)]` so a missing file, an empty file, or a file that
//! only sets a couple of fields all work the same way: whatever isn't
//! specified falls back to `Config::default()`. A missing or unparseable
//! file is never fatal -- we log why and start with defaults, since a
//! typo in the config shouldn't stop the terminal from opening.
//!
//! The only things ever written back are the default shell, the UI
//! language and the scroll speed (all picked in the sidebar), via
//! `toml_edit` so the rest of
//! the file -- comments, ordering, formatting -- stays exactly as the user
//! wrote it.

use serde::Deserialize;
use std::path::{Path, PathBuf};

use crate::i18n::{Language, t};

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
        Self::edit(|doc| doc["shell"] = toml_edit::value(shell.to_string_lossy().as_ref()))?;
        self.shell = Some(shell.to_path_buf());
        Ok(())
    }

    /// Set the UI language (`None`: follow the locale) and persist it.
    pub fn save_language(&mut self, language: Option<Language>) -> Result<(), String> {
        Self::edit(|doc| match language {
            Some(language) => doc["language"] = toml_edit::value(language.code()),
            None => drop(doc.remove("language")),
        })?;
        self.language = language.map(|language| language.code().to_string());
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

    /// Set the lines per mouse-wheel notch and persist them.
    pub fn save_scroll_lines(&mut self, lines: f32) -> Result<(), String> {
        Self::edit(|doc| doc["scroll_lines"] = toml_edit::value(f64::from(lines)))?;
        self.scroll_lines = lines;
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

#[cfg(test)]
mod tests {
    use super::Config;

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
