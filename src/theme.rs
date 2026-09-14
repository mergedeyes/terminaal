//! Color themes: the console's colors and those of the chrome around it
//! (sidebar, tab bar, settings). A few are built in (`themes/*.toml`,
//! compiled in), one follows the COSMIC desktop's theme ([`cosmic`]), and
//! your own are TOML files in `~/.config/terminaal/themes/`.
//!
//! The format is Alacritty's -- `[colors.primary]`, `[colors.normal]`,
//! `[colors.bright]`, ... -- so its theme files work as they are. Keys
//! Terminaal has no use for are ignored, and so are Alacritty's
//! cell-relative values (`CellForeground`, `CellBackground`): they count
//! as unset. Required are the primary foreground and background and the
//! eight normal colors; bright colors default to the normal ones, dim ones
//! to darkened normal ones. An optional `[ui]` table sets the chrome's
//! colors; whatever it leaves out is derived from the console's.

pub mod cosmic;

use std::path::{Path, PathBuf};

use alacritty_terminal::vte::ansi::Rgb;
use serde::Deserialize;

use crate::config::Config;
use crate::i18n::t;

/// The theme used when none is configured, or the configured one is gone.
pub const DEFAULT: &str = "Terminaal";

/// Name and file of every built-in theme, [`DEFAULT`] first.
const BUILTIN: [(&str, &str); 7] = [
    (DEFAULT, include_str!("../themes/terminaal.toml")),
    ("Catppuccin Mocha", include_str!("../themes/catppuccin-mocha.toml")),
    ("Dracula", include_str!("../themes/dracula.toml")),
    ("Gruvbox Dark", include_str!("../themes/gruvbox-dark.toml")),
    ("Nord", include_str!("../themes/nord.toml")),
    ("Solarized Dark", include_str!("../themes/solarized-dark.toml")),
    ("Solarized Light", include_str!("../themes/solarized-light.toml")),
];

#[derive(Clone, Debug, PartialEq)]
pub struct Theme {
    pub name: String,
    pub source: Source,
    pub terminal: TerminalColors,
    pub ui: UiColors,
}

/// Where a theme comes from.
#[derive(Clone, Debug, PartialEq)]
pub enum Source {
    Builtin,
    /// The COSMIC desktop's theme ([`cosmic`]).
    Cosmic,
    /// One of your own, from this file.
    File(PathBuf),
}

/// The console's colors, sRGB.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TerminalColors {
    pub foreground: Rgb,
    pub background: Rgb,
    pub bright_foreground: Rgb,
    pub dim_foreground: Rgb,
    pub cursor: Rgb,
    /// Background of selected cells.
    pub selection: Rgb,
    /// Text of selected cells; `None` keeps each cell's own.
    pub selection_text: Option<Rgb>,
    /// Background and text of search matches on screen (`[colors.search]`);
    /// no text color keeps each cell's own.
    pub search_match: (Rgb, Option<Rgb>),
    /// The same for the match in focus, the one n/N jumped to.
    pub search_focus: (Rgb, Option<Rgb>),
    /// Black, red, green, yellow, blue, magenta, cyan, white.
    pub normal: [Rgb; 8],
    pub bright: [Rgb; 8],
    pub dim: [Rgb; 8],
}

/// The chrome's colors, sRGB.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct UiColors {
    /// Sidebar, tab bar, settings page.
    pub background: Rgb,
    /// Cards and list rows on the background.
    pub row: Rgb,
    pub hover: Rgb,
    /// The selected list row.
    pub selected: Rgb,
    pub border: Rgb,
    /// Borders of hovered widgets, a hovered close button.
    pub border_strong: Rgb,
    pub accent: Rgb,
    pub text: Rgb,
    pub text_weak: Rgb,
    pub error: Rgb,
    pub success: Rgb,
    /// Text fields.
    pub input: Rgb,
    /// Selected text in text fields.
    pub text_selection: Rgb,
}

impl UiColors {
    /// Chrome colors to go with `colors`: shades between the console's
    /// background and foreground, so it works for dark and light themes
    /// alike.
    fn derive(colors: &TerminalColors, accent: Rgb) -> Self {
        let (base, fg) = (colors.background, colors.foreground);
        let background = mix(base, fg, 0.095);
        Self {
            background,
            row: mix(base, fg, 0.12),
            hover: mix(base, fg, 0.157),
            selected: mix(background, accent, 0.18),
            border: mix(base, fg, 0.21),
            border_strong: mix(base, fg, 0.28),
            accent,
            text: fg,
            text_weak: mix(base, fg, 0.59),
            error: colors.bright[1],
            success: colors.bright[2],
            input: mix(base, fg, 0.05),
            text_selection: mix(background, accent, 0.4),
        }
    }

    /// A dark theme, going by its background.
    pub fn is_dark(&self) -> bool {
        let Rgb { r, g, b } = self.background;
        (0.2126 * f32::from(r) + 0.7152 * f32::from(g) + 0.0722 * f32::from(b)) / 255.0 < 0.5
    }
}

/// `a` moved `amount` (0..=1) of the way towards `b`, in sRGB.
pub fn mix(a: Rgb, b: Rgb, amount: f32) -> Rgb {
    let ch = |a: u8, b: u8| (f32::from(a) + (f32::from(b) - f32::from(a)) * amount).round() as u8;
    Rgb { r: ch(a.r, b.r), g: ch(a.g, b.g), b: ch(a.b, b.b) }
}

/// A dim color as terminals draw it: two thirds as bright.
pub fn dim(c: Rgb) -> Rgb {
    mix(c, Rgb::default(), 0.34)
}

impl Theme {
    /// Read a theme file. It's called `name` unless it names itself.
    fn parse(name: &str, text: &str, source: Source) -> Result<Self, String> {
        let file: File = toml::from_str(text).map_err(|err| err.to_string())?;
        let c = &file.colors;
        let required = |key: &str, value: &Option<String>| {
            color(key, value)?.ok_or_else(|| t!("theme-missing-color", key = key))
        };
        let foreground = required("colors.primary.foreground", &c.primary.foreground)?;
        let background = required("colors.primary.background", &c.primary.background)?;
        let normal = eight("colors.normal", &c.normal, |_| None)?;
        let bright = eight("colors.bright", &c.bright, |i| Some(normal[i]))?;
        let dim_colors = eight("colors.dim", &c.dim, |i| Some(dim(normal[i])))?;
        let optional = |key: &str, value: &Option<String>, fallback: Rgb| Ok::<_, String>(color(key, value)?.unwrap_or(fallback));
        let terminal = TerminalColors {
            foreground,
            background,
            bright_foreground: optional("colors.primary.bright_foreground", &c.primary.bright_foreground, foreground)?,
            dim_foreground: optional("colors.primary.dim_foreground", &c.primary.dim_foreground, dim(foreground))?,
            cursor: optional("colors.cursor.cursor", &c.cursor.cursor, foreground)?,
            selection: optional("colors.selection.background", &c.selection.background, mix(background, normal[4], 0.5))?,
            selection_text: color("colors.selection.text", &c.selection.text)?,
            search_match: (
                optional("colors.search.matches.background", &c.search.matches.background, mix(background, normal[3], 0.4))?,
                color("colors.search.matches.foreground", &c.search.matches.foreground)?,
            ),
            search_focus: (
                optional("colors.search.focused_match.background", &c.search.focused_match.background, normal[3])?,
                Some(optional("colors.search.focused_match.foreground", &c.search.focused_match.foreground, background)?),
            ),
            normal,
            bright,
            dim: dim_colors,
        };

        let u = &file.ui;
        let accent = optional("ui.accent", &u.accent, normal[4])?;
        let derived = UiColors::derive(&terminal, accent);
        let ui = UiColors {
            background: optional("ui.background", &u.background, derived.background)?,
            row: optional("ui.row", &u.row, derived.row)?,
            hover: optional("ui.hover", &u.hover, derived.hover)?,
            selected: optional("ui.selected", &u.selected, derived.selected)?,
            border: optional("ui.border", &u.border, derived.border)?,
            border_strong: optional("ui.border_strong", &u.border_strong, derived.border_strong)?,
            accent,
            text: optional("ui.text", &u.text, derived.text)?,
            text_weak: optional("ui.text_weak", &u.text_weak, derived.text_weak)?,
            error: optional("ui.error", &u.error, derived.error)?,
            success: optional("ui.success", &u.success, derived.success)?,
            input: optional("ui.input", &u.input, derived.input)?,
            text_selection: optional("ui.text_selection", &u.text_selection, derived.text_selection)?,
        };

        let name = file.name.filter(|name| !name.trim().is_empty()).unwrap_or_else(|| name.to_string());
        Ok(Self { name, source, terminal, ui })
    }
}

/// The [`DEFAULT`] theme as built in.
pub fn default_theme() -> Theme {
    Theme::parse(DEFAULT, BUILTIN[0].1, Source::Builtin).expect("built-in theme")
}

/// Every theme to choose from: the built-in ones, the COSMIC desktop's,
/// then your own, sorted by name. One of your own replaces another theme
/// of the same name.
pub struct Themes {
    list: Vec<Theme>,
    /// What couldn't be read -- files in the themes folder, the COSMIC
    /// theme -- as messages.
    errors: Vec<String>,
}

impl Themes {
    pub fn load() -> Self {
        Self::load_from(Self::dir().as_deref(), Some(&cosmic::Roots::system()))
    }

    /// Just the built-in themes.
    #[cfg(test)]
    pub fn builtin() -> Self {
        Self::load_from(None, None)
    }

    /// `~/.config/terminaal/themes`.
    pub fn dir() -> Option<PathBuf> {
        Some(Config::dir()?.join("themes"))
    }

    fn load_from(dir: Option<&Path>, cosmic: Option<&cosmic::Roots>) -> Self {
        let mut list: Vec<Theme> = BUILTIN
            .iter()
            .map(|(name, text)| Theme::parse(name, text, Source::Builtin).expect("built-in theme"))
            .collect();
        let mut own = Vec::new();
        let mut errors = Vec::new();
        match cosmic.and_then(cosmic::load) {
            Some(Ok(theme)) => list.push(theme),
            Some(Err(err)) => {
                log::warn!("the COSMIC theme is incomplete");
                errors.push(err);
            }
            None => {}
        }
        if let Some(dir) = dir {
            read_dir(dir, &mut own, &mut errors);
        }
        own.sort_by_key(|theme| theme.name.to_lowercase());
        for theme in own {
            list.retain(|other| !other.name.eq_ignore_ascii_case(&theme.name));
            list.push(theme);
        }
        Self { list, errors }
    }

    /// The theme called `name` (any case), if there is one.
    pub fn find(&self, name: &str) -> Option<&Theme> {
        self.list.iter().find(|theme| theme.name.eq_ignore_ascii_case(name))
    }

    pub fn all(&self) -> &[Theme] {
        &self.list
    }

    pub fn errors(&self) -> &[String] {
        &self.errors
    }

    /// The theme called `name` (any case), or [`DEFAULT`] for `None` or a
    /// name there's no theme for.
    pub fn get(&self, name: Option<&str>) -> &Theme {
        let find = |name: &str| self.list.iter().find(|theme| theme.name.eq_ignore_ascii_case(name));
        let name = name.unwrap_or(DEFAULT);
        find(name)
            .or_else(|| {
                log::warn!("unknown theme {name:?}, using {DEFAULT}");
                find(DEFAULT)
            })
            .unwrap_or(&self.list[0])
    }
}

/// The `*.toml` files in `dir`, by file name.
fn read_dir(dir: &Path, themes: &mut Vec<Theme>, errors: &mut Vec<String>) {
    let entries = match std::fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => return,
        Err(err) => {
            log::warn!("failed to read theme folder {}: {err}", dir.display());
            errors.push(t!("common-file-unreadable", path = dir.display().to_string(), err = err.to_string()));
            return;
        }
    };
    let mut paths: Vec<PathBuf> = entries
        .filter_map(|entry| Some(entry.ok()?.path()))
        .filter(|path| path.extension().is_some_and(|ext| ext == "toml") && path.is_file())
        .collect();
    paths.sort();
    for path in paths {
        let shown = path.display().to_string();
        let name = path.file_stem().map(|stem| stem.to_string_lossy().into_owned()).unwrap_or_default();
        let theme = std::fs::read_to_string(&path)
            .map_err(|err| t!("common-file-unreadable", path = &shown, err = err.to_string()))
            .and_then(|text| {
                Theme::parse(&name, &text, Source::File(path.clone()))
                    .map_err(|err| t!("common-file-invalid", path = &shown, err = err))
            });
        match theme {
            Ok(theme) => themes.push(theme),
            Err(err) => {
                log::warn!("skipping theme file {shown}");
                errors.push(err);
            }
        }
    }
}

/// A color value: `#rrggbb`, `#rgb` or `0xrrggbb`. `None` when unset, or
/// one of Alacritty's cell-relative values.
fn color(key: &str, value: &Option<String>) -> Result<Option<Rgb>, String> {
    let Some(value) = value else { return Ok(None) };
    if value.starts_with("Cell") {
        return Ok(None);
    }
    parse_hex(value).map(Some).ok_or_else(|| t!("theme-bad-color", key = key, value = value.as_str()))
}

pub fn parse_hex(value: &str) -> Option<Rgb> {
    let value = value.trim();
    let hex = value.strip_prefix('#').or_else(|| value.strip_prefix("0x")).or_else(|| value.strip_prefix("0X"))?;
    if !hex.bytes().all(|b| b.is_ascii_hexdigit()) {
        return None;
    }
    let digits = |i: usize, n: usize| u8::from_str_radix(&hex[i..i + n], 16).ok();
    match hex.len() {
        6 => Some(Rgb { r: digits(0, 2)?, g: digits(2, 2)?, b: digits(4, 2)? }),
        3 => Some(Rgb { r: digits(0, 1)? * 17, g: digits(1, 1)? * 17, b: digits(2, 1)? * 17 }),
        _ => None,
    }
}

/// The eight colors of `colors` (called `table` in messages); a missing
/// one is `fallback`'s, or an error if that has none either.
fn eight(table: &str, colors: &Eight, fallback: impl Fn(usize) -> Option<Rgb>) -> Result<[Rgb; 8], String> {
    let mut out = [Rgb::default(); 8];
    for (i, (key, value)) in colors.slots().into_iter().enumerate() {
        let key = format!("{table}.{key}");
        out[i] = match color(&key, value)?.or_else(|| fallback(i)) {
            Some(color) => color,
            None => return Err(t!("theme-missing-color", key = key.as_str())),
        };
    }
    Ok(out)
}

/// A theme file as written; checked by [`Theme::parse`].
#[derive(Default, Deserialize)]
#[serde(default)]
struct File {
    name: Option<String>,
    colors: FileColors,
    ui: FileUi,
}

#[derive(Default, Deserialize)]
#[serde(default)]
struct FileColors {
    primary: Primary,
    cursor: CursorColors,
    selection: SelectionColors,
    search: SearchColors,
    normal: Eight,
    bright: Eight,
    dim: Eight,
}

#[derive(Default, Deserialize)]
#[serde(default)]
struct Primary {
    foreground: Option<String>,
    background: Option<String>,
    bright_foreground: Option<String>,
    dim_foreground: Option<String>,
}

#[derive(Default, Deserialize)]
#[serde(default)]
struct CursorColors {
    cursor: Option<String>,
}

#[derive(Default, Deserialize)]
#[serde(default)]
struct SelectionColors {
    background: Option<String>,
    text: Option<String>,
}

#[derive(Default, Deserialize)]
#[serde(default)]
struct SearchColors {
    matches: MatchColors,
    focused_match: MatchColors,
}

#[derive(Default, Deserialize)]
#[serde(default)]
struct MatchColors {
    foreground: Option<String>,
    background: Option<String>,
}

#[derive(Default, Deserialize)]
#[serde(default)]
struct Eight {
    black: Option<String>,
    red: Option<String>,
    green: Option<String>,
    yellow: Option<String>,
    blue: Option<String>,
    magenta: Option<String>,
    cyan: Option<String>,
    white: Option<String>,
}

impl Eight {
    fn slots(&self) -> [(&'static str, &Option<String>); 8] {
        [
            ("black", &self.black),
            ("red", &self.red),
            ("green", &self.green),
            ("yellow", &self.yellow),
            ("blue", &self.blue),
            ("magenta", &self.magenta),
            ("cyan", &self.cyan),
            ("white", &self.white),
        ]
    }
}

#[derive(Default, Deserialize)]
#[serde(default)]
struct FileUi {
    background: Option<String>,
    row: Option<String>,
    hover: Option<String>,
    selected: Option<String>,
    border: Option<String>,
    border_strong: Option<String>,
    accent: Option<String>,
    text: Option<String>,
    text_weak: Option<String>,
    error: Option<String>,
    success: Option<String>,
    input: Option<String>,
    text_selection: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    const fn rgb(r: u8, g: u8, b: u8) -> Rgb {
        Rgb { r, g, b }
    }

    const NORMAL: &str = "[colors.normal]
black = '#000000'
red = '#cd3131'
green = '#0dbc79'
yellow = '#e5e510'
blue = '#2472c8'
magenta = '#bc3fbc'
cyan = '#11a8cd'
white = '#e5e5e5'
";

    fn parse(name: &str, text: &str) -> Result<Theme, String> {
        Theme::parse(name, text, Source::Builtin)
    }

    #[test]
    fn builtin_themes_parse_and_are_found_by_name() {
        let themes = Themes::builtin();
        assert_eq!(themes.all().len(), BUILTIN.len());
        assert!(themes.errors().is_empty());
        for (name, _) in BUILTIN {
            assert_eq!(themes.get(Some(name)).name, name);
        }
        assert_eq!(themes.get(None).name, DEFAULT);
        assert_eq!(themes.get(Some("dracula")).name, "Dracula");
        assert_eq!(themes.get(Some("no such theme")).name, DEFAULT);
        assert!(!themes.get(Some("Solarized Light")).ui.is_dark());
        assert!(themes.get(Some("Nord")).ui.is_dark());
    }

    /// The default theme is what Terminaal looked like before themes.
    #[test]
    fn default_theme_keeps_the_old_colors() {
        let theme = default_theme();
        assert_eq!(theme.terminal.background, rgb(0, 0, 0));
        assert_eq!(theme.terminal.foreground, rgb(229, 229, 229));
        assert_eq!(theme.terminal.bright[4], rgb(59, 142, 234));
        assert_eq!(theme.terminal.dim[1], dim(rgb(205, 49, 49)));
        assert_eq!(theme.ui.background, rgb(22, 22, 22));
        assert_eq!(theme.ui.accent, rgb(59, 142, 234));
        assert!(theme.ui.is_dark());
        // Derived chrome colors stay close to the hand-picked ones.
        let derived = UiColors::derive(&theme.terminal, theme.ui.accent);
        for (hand, derived) in [(theme.ui.background, derived.background), (theme.ui.border, derived.border)] {
            assert!(hand.r.abs_diff(derived.r) <= 1, "{hand:?} vs {derived:?}");
        }
    }

    #[test]
    fn reads_alacritty_theme_files() {
        let text = format!(
            "[colors.primary]
background = '#282a36'
foreground = '0xF8F8F2'

[colors.cursor]
text = 'CellBackground'
cursor = 'CellForeground'

[colors.selection]
text = '#1e1e2e'
background = '#44475a'

[colors.search.focused_match]
background = '#ffb86c'
foreground = 'CellBackground'

[colors.hints.start]
foreground = '#1e1e2e'

{NORMAL}
[colors.bright]
red = '#f00'
"
        );
        let theme = parse("mine", &text).unwrap();
        assert_eq!(theme.name, "mine");
        let colors = theme.terminal;
        assert_eq!(colors.foreground, rgb(248, 248, 242));
        assert_eq!(colors.cursor, colors.foreground);
        assert_eq!(colors.selection, rgb(0x44, 0x47, 0x5a));
        assert_eq!(colors.selection_text, Some(rgb(0x1e, 0x1e, 0x2e)));
        assert_eq!(colors.search_focus, (rgb(0xff, 0xb8, 0x6c), Some(colors.background)));
        assert_eq!(colors.search_match, (mix(colors.background, colors.normal[3], 0.4), None));
        assert_eq!(colors.bright[1], rgb(255, 0, 0));
        assert_eq!(colors.bright[2], colors.normal[2]);
        assert_eq!(colors.dim[3], dim(colors.normal[3]));
        assert_eq!(theme.ui.accent, colors.normal[4]);
        assert_eq!(theme.ui.text, colors.foreground);
    }

    #[test]
    fn ui_table_and_name_override() {
        let text = format!(
            "name = 'Mine'
[colors.primary]
background = '#fdf6e3'
foreground = '#657b83'
{NORMAL}
[ui]
accent = '#ff0000'
row = '#010203'
"
        );
        let theme = parse("file-stem", &text).unwrap();
        assert_eq!(theme.name, "Mine");
        assert_eq!(theme.ui.accent, rgb(255, 0, 0));
        assert_eq!(theme.ui.row, rgb(1, 2, 3));
        // Derived from the new accent, not the console's blue.
        assert_eq!(theme.ui.selected, mix(theme.ui.background, rgb(255, 0, 0), 0.18));
        assert!(!theme.ui.is_dark());
    }

    #[test]
    fn broken_theme_files_say_what_is_wrong() {
        let err = parse("x", NORMAL).unwrap_err();
        assert!(err.contains("colors.primary.foreground"), "{err}");

        let text = format!("[colors.primary]\nbackground = '#000'\nforeground = '#fff'\n{NORMAL}");
        let err = parse("x", &text.replace("'#cd3131'", "'#cd313'")).unwrap_err();
        assert!(err.contains("colors.normal.red") && err.contains("#cd313"), "{err}");
        let err = parse("x", &text.replace("cyan = '#11a8cd'", "")).unwrap_err();
        assert!(err.contains("colors.normal.cyan"), "{err}");
        let err = parse("x", &format!("{text}[ui]\nborder = 'blue'\n")).unwrap_err();
        assert!(err.contains("ui.border"), "{err}");
        assert!(parse("x", "colors = 1").is_err());
    }

    #[test]
    fn own_themes_replace_builtin_ones_and_broken_files_are_listed() {
        let dir = std::env::temp_dir().join(format!("terminaal-themes-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let theme = format!("[colors.primary]\nbackground = '#123456'\nforeground = '#fff'\n{NORMAL}");
        std::fs::write(dir.join("dracula.toml"), &theme).unwrap();
        std::fs::write(dir.join("Mine.toml"), &theme).unwrap();
        std::fs::write(dir.join("broken.toml"), "colors = 1").unwrap();
        std::fs::write(dir.join("notes.txt"), "not a theme").unwrap();

        let themes = Themes::load_from(Some(&dir), None);
        std::fs::remove_dir_all(&dir).unwrap();
        assert_eq!(themes.all().len(), BUILTIN.len() + 1);
        assert_eq!(themes.errors().len(), 1);
        assert!(themes.errors()[0].contains("broken.toml"), "{:?}", themes.errors());
        let dracula = themes.get(Some("Dracula"));
        assert_eq!(dracula.terminal.background, rgb(0x12, 0x34, 0x56));
        assert!(matches!(dracula.source, Source::File(_)));
        assert_eq!(themes.get(Some("mine")).name, "Mine");
        assert!(themes.all().iter().any(|theme| theme.name == DEFAULT && theme.source == Source::Builtin));
    }
}
