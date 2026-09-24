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

use crate::commands::Family;
use crate::i18n::{Language, t};
use crate::shortcuts::{Action, Bindings, KeyCombo};

/// Font sizes the settings and zooming by shortcut allow.
pub const FONT_SIZES: RangeInclusive<f32> = 6.0..=36.0;
/// Window opacities the settings allow; less and the text on top would be
/// all that's left.
pub const OPACITIES: RangeInclusive<f32> = 0.2..=1.0;
/// How much faster the wheel may scroll while marking text; 1 is the
/// usual speed.
pub const SELECT_FACTORS: RangeInclusive<f32> = 1.0..=10.0;
/// Heights of the drop-down window the settings allow, in percent.
pub const QUAKE_HEIGHTS: RangeInclusive<f32> = 20.0..=100.0;

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
    /// How much faster the wheel scrolls while a selection is being
    /// dragged. 1 keeps it at [`Config::scroll_lines`]. Read through
    /// [`Config::scroll_select_factor`].
    pub scroll_select_factor: f32,
    /// Default window size (logical pixels).
    pub default_width: f64,
    pub default_height: f64,
    /// Whether the cursor should blink at all.
    pub cursor_blink: bool,
    /// Blink half-period in milliseconds (time visible == time hidden).
    pub cursor_blink_interval_ms: u64,
    /// Notify when a command that ran at least this many seconds finishes
    /// while its tab isn't in view (needs shell integration); 0: never.
    pub notify_after_secs: u64,
    /// A terminal watched for silence ("Watch for silence") counts as
    /// quiet after this many seconds without output.
    pub silence_secs: u64,
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
    /// Open the tabs of the last session again at startup
    /// (`crate::session`).
    pub restore_session: bool,
    /// Height of the drop-down window (`--quake`), in percent of the
    /// screen's. Read through [`Config::quake_height`].
    pub quake_height: f32,
    /// Hide the drop-down window when another window gets the keyboard.
    pub quake_hide_on_unfocus: bool,
    /// UI language, `de` or `en`. Unset (or `auto`): German for a German
    /// locale, English otherwise. See [`Config::language`].
    pub language: Option<String>,
    /// Color theme by name, built in or from the themes folder
    /// (`crate::theme`). Unset: `theme::DEFAULT`.
    pub theme: Option<String>,
    /// Console font family. Unset: cosmic-text's monospace default.
    pub font_family: Option<String>,
    /// Font family of the menus and panels. Unset: egui's own.
    pub ui_font_family: Option<String>,
    /// How opaque the window's backgrounds are, 1 = not see-through. Read
    /// through [`Config::opacity`].
    pub opacity: f32,
    /// Blur what shines through a see-through window, where the compositor
    /// can.
    pub blur: bool,
    /// Send a built-in command (`crate::commands`) to the shell right
    /// away, Enter included; off types it into the prompt instead.
    pub commands_run: bool,
    /// Let the built-in commands skip their confirmation prompts (`-y`,
    /// `--noconfirm`). Off by default: a click shouldn't be able to remove
    /// packages unasked.
    pub commands_assume_yes: bool,
    /// Whether the warning about commands running right away has been
    /// acknowledged; set the first time one is used.
    pub commands_warned: bool,
    /// Command groups the sidebar shows collapsed: a built-in group by
    /// its [`crate::commands::Group::key`], one of your own categories as
    /// `custom:<name>`. Not an option anyone edits by hand -- it's where
    /// the sidebar remembers what you folded away.
    pub commands_collapsed: Vec<String>,
    /// Ask before a paste with several lines or risky commands
    /// (`ui::paste_warning`).
    pub paste_warning: bool,
    /// The system the built-in commands are tailored to, e.g. `debian`
    /// (`commands::Family::key`). Unset (or `auto`): from `/etc/os-release`.
    pub system: Option<String>,
    /// Command a file edited from the files tab opens with, the file's path
    /// appended (e.g. `code`, `gedit`). Unset: the desktop's default
    /// application (`xdg-open`).
    pub editor: Option<String>,
    /// Keyboard shortcuts that differ from the defaults, by action name
    /// (`[shortcuts]`); see `shortcuts`.
    pub shortcuts: BTreeMap<String, Bindings>,
}

/// Which font a font setting is for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FontSlot {
    /// The console (and the tab bar).
    Terminal,
    /// Menus and panels.
    Ui,
}

impl FontSlot {
    pub fn key(self) -> &'static str {
        match self {
            Self::Terminal => "font_family",
            Self::Ui => "ui_font_family",
        }
    }
}

impl Default for Config {
    fn default() -> Self {
        Self {
            theme: None,
            font_family: None,
            ui_font_family: None,
            opacity: 1.0,
            blur: true,
            font_size: 15.0,
            line_height_factor: 1.25,
            padding: 8.0,
            scrollback_lines: 10_000,
            // Like Alacritty and most desktops.
            scroll_lines: 3.0,
            // Marking up a screenful at a time is what the wheel is for
            // here; dragging to the edge covers the slow case.
            scroll_select_factor: 3.0,
            default_width: 1000.0,
            default_height: 650.0,
            cursor_blink: true,
            cursor_blink_interval_ms: 600,
            notify_after_secs: 10,
            silence_secs: 15,
            tab_bar: true,
            shell: None,
            sidebar: true,
            sidebar_width: 300.0,
            splash: true,
            restore_session: true,
            quake_height: 50.0,
            quake_hide_on_unfocus: true,
            language: None,
            commands_run: true,
            commands_assume_yes: false,
            commands_warned: false,
            commands_collapsed: Vec::new(),
            paste_warning: true,
            system: None,
            editor: None,
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
        Self::edit(|doc| write_text(doc, "language", language.map(Language::code)))?;
        self.language = language.map(|language| language.code().to_string());
        Ok(())
    }

    /// Switch to the theme called `name` and persist that; the default
    /// theme removes the key.
    pub fn save_theme(&mut self, name: &str) -> Result<(), String> {
        let name = (!name.eq_ignore_ascii_case(crate::theme::DEFAULT)).then_some(name);
        Self::edit(|doc| write_text(doc, "theme", name))?;
        self.theme = name.map(str::to_string);
        Ok(())
    }

    /// The system the built-in commands should be tailored to, if the
    /// config names one. An unknown value counts as unset.
    pub fn system(&self) -> Option<Family> {
        self.system.as_deref().and_then(Family::parse).filter(|family| *family != Family::Unknown)
    }

    /// Tailor the built-in commands to `family` and persist that; `None`
    /// (detect it) removes the key.
    pub fn save_system(&mut self, family: Option<Family>) -> Result<(), String> {
        let key = family.map(Family::key);
        Self::edit(|doc| write_text(doc, "system", key))?;
        self.system = key.map(str::to_string);
        Ok(())
    }

    /// The editor for files from the server, if one is set.
    pub fn editor(&self) -> Option<&str> {
        self.editor.as_deref().map(str::trim).filter(|editor| !editor.is_empty())
    }

    /// Set the editor command and persist it; empty removes the key.
    pub fn save_editor(&mut self, editor: &str) -> Result<(), String> {
        let editor = Some(editor.trim()).filter(|editor| !editor.is_empty());
        Self::edit(|doc| write_text(doc, "editor", editor))?;
        self.editor = editor.map(str::to_string);
        Ok(())
    }

    /// The font family chosen for `slot`, if any.
    pub fn font(&self, slot: FontSlot) -> Option<&str> {
        match slot {
            FontSlot::Terminal => self.font_family.as_deref(),
            FontSlot::Ui => self.ui_font_family.as_deref(),
        }
    }

    /// Choose the font family for `slot` and persist it; `None` (the
    /// default) removes the key.
    pub fn save_font(&mut self, slot: FontSlot, family: Option<&str>) -> Result<(), String> {
        Self::edit(|doc| write_text(doc, slot.key(), family))?;
        let family = family.map(str::to_string);
        match slot {
            FontSlot::Terminal => self.font_family = family,
            FontSlot::Ui => self.ui_font_family = family,
        }
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

    /// Fold `group` away in the sidebar's commands, or open it again,
    /// and remember that in the config file. Like the shortcuts, this is
    /// a list rather than one value, so it doesn't go through
    /// [`Setting`].
    pub fn collapse_group(&mut self, group: &str, collapsed: bool) -> Result<(), String> {
        let mut groups = self.commands_collapsed.clone();
        groups.retain(|other| other != group);
        if collapsed {
            groups.push(group.to_string());
        }
        Self::edit(|doc| set_value(doc, "commands_collapsed", toml_edit::Array::from_iter(groups.iter().map(String::as_str))))?;
        self.commands_collapsed = groups;
        Ok(())
    }

    /// Whether `group` is folded away in the sidebar.
    pub fn collapsed(&self, group: &str) -> bool {
        self.commands_collapsed.iter().any(|other| other == group)
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

    /// The wheel's multiplier while a selection is dragged, within
    /// [`SELECT_FACTORS`]; not a number counts as the default.
    pub fn scroll_select_factor(&self) -> f32 {
        if self.scroll_select_factor.is_finite() {
            self.scroll_select_factor.clamp(*SELECT_FACTORS.start(), *SELECT_FACTORS.end())
        } else {
            Self::default().scroll_select_factor
        }
    }

    /// The drop-down window's height in percent, within [`QUAKE_HEIGHTS`];
    /// not a number counts as the default.
    pub fn quake_height(&self) -> f32 {
        if self.quake_height.is_finite() {
            self.quake_height.clamp(*QUAKE_HEIGHTS.start(), *QUAKE_HEIGHTS.end())
        } else {
            Self::default().quake_height
        }
    }

    /// The window's opacity, within [`OPACITIES`]; not a number counts as
    /// opaque.
    pub fn opacity(&self) -> f32 {
        if self.opacity.is_finite() { self.opacity.clamp(*OPACITIES.start(), *OPACITIES.end()) } else { 1.0 }
    }

    /// Take over `setting` in memory only.
    pub fn set(&mut self, setting: Setting) {
        match setting {
            Setting::FontSize(size) => self.font_size = size,
            Setting::LineHeight(factor) => self.line_height_factor = factor,
            Setting::Padding(padding) => self.padding = padding,
            Setting::ScrollbackLines(lines) => self.scrollback_lines = lines,
            Setting::ScrollLines(lines) => self.scroll_lines = lines,
            Setting::ScrollSelectFactor(factor) => self.scroll_select_factor = factor,
            Setting::WindowSize { width, height } => (self.default_width, self.default_height) = (width, height),
            Setting::CursorBlink(on) => self.cursor_blink = on,
            Setting::CursorBlinkInterval(ms) => self.cursor_blink_interval_ms = ms,
            Setting::NotifyAfter(secs) => self.notify_after_secs = secs,
            Setting::SilenceAfter(secs) => self.silence_secs = secs.max(1),
            Setting::TabBar(on) => self.tab_bar = on,
            Setting::Sidebar(on) => self.sidebar = on,
            Setting::SidebarWidth(width) => self.sidebar_width = width,
            Setting::Splash(on) => self.splash = on,
            Setting::RestoreSession(on) => self.restore_session = on,
            Setting::QuakeHeight(percent) => self.quake_height = percent,
            Setting::QuakeHideOnUnfocus(on) => self.quake_hide_on_unfocus = on,
            Setting::Opacity(opacity) => self.opacity = opacity,
            Setting::Blur(on) => self.blur = on,
            Setting::CommandsRun(on) => self.commands_run = on,
            Setting::CommandsAssumeYes(on) => self.commands_assume_yes = on,
            Setting::CommandsWarned(on) => self.commands_warned = on,
            Setting::PasteWarning(on) => self.paste_warning = on,
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
    /// How much faster the wheel scrolls while marking text.
    ScrollSelectFactor(f32),
    /// Window size at start; both keys at once.
    WindowSize { width: f64, height: f64 },
    CursorBlink(bool),
    CursorBlinkInterval(u64),
    /// Seconds a command has to run to be notified about; 0: never.
    NotifyAfter(u64),
    /// Seconds without output a watched terminal counts as quiet after.
    SilenceAfter(u64),
    TabBar(bool),
    /// Show the sidebar at start.
    Sidebar(bool),
    SidebarWidth(f32),
    Splash(bool),
    RestoreSession(bool),
    /// Drop-down window height in percent.
    QuakeHeight(f32),
    QuakeHideOnUnfocus(bool),
    Opacity(f32),
    Blur(bool),
    /// Run built-in commands right away instead of typing them out.
    CommandsRun(bool),
    CommandsAssumeYes(bool),
    CommandsWarned(bool),
    PasteWarning(bool),
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
            Self::ScrollSelectFactor(factor) => set_value(doc, "scroll_select_factor", float(factor.into())),
            Self::WindowSize { width, height } => {
                set_value(doc, "default_width", float(width.round()));
                set_value(doc, "default_height", float(height.round()));
            }
            Self::CursorBlink(on) => set_value(doc, "cursor_blink", on),
            Self::CursorBlinkInterval(ms) => set_value(doc, "cursor_blink_interval_ms", int(ms)),
            Self::NotifyAfter(secs) => set_value(doc, "notify_after_secs", int(secs)),
            Self::SilenceAfter(secs) => set_value(doc, "silence_secs", int(secs)),
            Self::TabBar(on) => set_value(doc, "tab_bar", on),
            Self::Sidebar(on) => set_value(doc, "sidebar", on),
            Self::SidebarWidth(width) => set_value(doc, "sidebar_width", float(width.into())),
            Self::Splash(on) => set_value(doc, "splash", on),
            Self::RestoreSession(on) => set_value(doc, "restore_session", on),
            Self::QuakeHeight(percent) => set_value(doc, "quake_height", float(percent.round().into())),
            Self::QuakeHideOnUnfocus(on) => set_value(doc, "quake_hide_on_unfocus", on),
            Self::Opacity(opacity) => set_value(doc, "opacity", float(opacity.into())),
            Self::Blur(on) => set_value(doc, "blur", on),
            Self::CommandsRun(on) => set_value(doc, "commands_run", on),
            Self::CommandsAssumeYes(on) => set_value(doc, "commands_assume_yes", on),
            Self::CommandsWarned(on) => set_value(doc, "commands_warned", on),
            Self::PasteWarning(on) => set_value(doc, "paste_warning", on),
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

/// Set `key` to `value`, or with `None` remove it.
fn write_text(doc: &mut toml_edit::DocumentMut, key: &str, value: Option<&str>) {
    match value {
        Some(value) => set_value(doc, key, value),
        None => drop(doc.remove(key)),
    }
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
    use super::{Config, Family, Setting, write_shortcut};
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
            Setting::NotifyAfter(30),
            Setting::SilenceAfter(45),
            Setting::Opacity(0.85),
            Setting::Blur(false),
            Setting::CommandsRun(false),
            Setting::CommandsAssumeYes(true),
            Setting::CommandsWarned(true),
            Setting::PasteWarning(false),
            Setting::RestoreSession(false),
            Setting::QuakeHeight(40.4),
            Setting::QuakeHideOnUnfocus(false),
        ] {
            setting.write(&mut doc);
        }
        let text = doc.to_string();
        assert!(text.starts_with("# mine\nfont_size = 14.0 # small\ntab_bar = false\n"), "{text}");
        assert!(text.contains("line_height_factor = 1.3\n"), "{text}");
        assert!(text.contains("opacity = 0.85\n"), "{text}");

        let config: Config = toml::from_str(&text).unwrap();
        assert_eq!(config.font_size, 14.0);
        assert_eq!(config.line_height_factor, 1.3);
        assert_eq!(config.scrollback_lines, 20_000);
        assert_eq!((config.default_width, config.default_height), (1200.0, 700.0));
        assert!(!config.tab_bar);
        assert_eq!(config.cursor_blink_interval_ms, 450);
        assert_eq!(config.notify_after_secs, 30);
        assert_eq!(config.silence_secs, 45);
        assert_eq!(config.opacity(), 0.85);
        assert!(!config.blur);
        assert!(!config.commands_run);
        assert!(config.commands_assume_yes);
        assert!(!config.paste_warning);
        assert!(config.commands_warned);
        assert!(!config.restore_session);
        assert_eq!(config.quake_height(), 40.0);
        assert!(!config.quake_hide_on_unfocus);
        assert_eq!(toml::from_str::<Config>("quake_height = 5").unwrap().quake_height(), 20.0);
    }

    /// The sidebar folding a group of command buttons away, as it lands
    /// in the file. Not through `Config::collapse_group`: that writes to
    /// the real config file.
    #[test]
    fn collapsed_groups_are_a_list_in_the_file() {
        let mut doc: toml_edit::DocumentMut = "font_size = 12\n".parse().unwrap();
        let groups = ["packages", "custom:Docker"];
        super::set_value(&mut doc, "commands_collapsed", toml_edit::Array::from_iter(groups));
        assert!(doc.to_string().contains(r#"commands_collapsed = ["packages", "custom:Docker"]"#));
        let config: Config = toml::from_str(&doc.to_string()).unwrap();
        assert!(config.collapsed("packages"));
        assert!(config.collapsed("custom:Docker"));
        assert!(!config.collapsed("network"));
        assert!(!Config::default().collapsed("packages"));
    }

    /// The system for the built-in commands, as the settings write it.
    #[test]
    fn the_system_is_written_and_removed_again() {
        let mut doc: toml_edit::DocumentMut = "font_size = 12
".parse().unwrap();
        super::write_text(&mut doc, "system", Some(Family::Debian.key()));
        let config: Config = toml::from_str(&doc.to_string()).unwrap();
        assert_eq!(config.system(), Some(Family::Debian));
        // Written by hand, and nonsense at that: detect it instead.
        let config: Config = toml::from_str("system = \"plan9\"\n").unwrap();
        assert_eq!(config.system(), None);
        super::write_text(&mut doc, "system", None);
        assert_eq!(doc.to_string().trim(), "font_size = 12");
    }

    #[test]
    fn select_factor_is_clamped_to_its_range() {
        let parse = |text| toml::from_str::<Config>(text).unwrap().scroll_select_factor();
        assert_eq!(parse(""), 3.0);
        assert_eq!(parse("scroll_select_factor = 1"), 1.0);
        assert_eq!(parse("scroll_select_factor = 25"), 10.0);
        assert_eq!(parse("scroll_select_factor = 0"), 1.0);
        assert_eq!(parse("scroll_select_factor = nan"), 3.0);
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

    #[test]
    fn opacity_stays_in_range() {
        let parse = |text| toml::from_str::<Config>(text).unwrap().opacity();
        assert_eq!(parse(""), 1.0);
        assert_eq!(parse("opacity = 0.5"), 0.5);
        assert_eq!(parse("opacity = 1"), 1.0);
        assert_eq!(parse("opacity = 0"), 0.2);
        assert_eq!(parse("opacity = 3.0"), 1.0);
        assert_eq!(parse("opacity = nan"), 1.0);
    }

    #[test]
    fn text_settings_are_set_and_removed() {
        use super::{FontSlot, write_text};

        let mut doc: toml_edit::DocumentMut = "theme = \"Nord\" # mine\n".parse().unwrap();
        write_text(&mut doc, "theme", Some("Dracula"));
        write_text(&mut doc, FontSlot::Terminal.key(), Some("Hack"));
        let text = doc.to_string();
        assert!(text.starts_with("theme = \"Dracula\" # mine\n"), "{text}");

        let config: Config = toml::from_str(&text).unwrap();
        assert_eq!(config.theme.as_deref(), Some("Dracula"));
        assert_eq!(config.font(FontSlot::Terminal), Some("Hack"));
        assert_eq!(config.font(FontSlot::Ui), None);

        write_text(&mut doc, "theme", None);
        write_text(&mut doc, FontSlot::Terminal.key(), None);
        assert_eq!(doc.to_string().trim(), "");
    }
}
