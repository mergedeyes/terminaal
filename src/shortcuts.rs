//! Keyboard shortcuts: what they can do ([`Action`]), key combinations
//! ([`KeyCombo`]) and which combination does what ([`Keymap`]).
//!
//! Configured under `[shortcuts]` in config.toml, one entry per action:
//! `new_tab = "Ctrl+Shift+T"`, several combinations as an array, `[]` for
//! none. Actions without an entry keep their defaults. Keys are matched as
//! the layout labels them without modifiers (`key_without_modifiers`), so
//! Shift doesn't turn `,` into `;` and Ctrl+Shift+T is the T key.

use std::borrow::Cow;
use std::collections::BTreeMap;
use std::fmt;

use serde::Deserialize;
use crate::input::KeyInput;
use winit::keyboard::{Key, ModifiersState, NamedKey};

use crate::i18n::t;
use crate::panes::Direction;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Action {
    NewTab,
    CloseTab,
    /// Start the tab that was closed last where it was.
    ReopenTab,
    NextTab,
    PreviousTab,
    /// Tab 1 to 9, counted from the left.
    SelectTab(u8),
    MoveTabLeft,
    MoveTabRight,
    /// Split the focused pane: the new one right of it / below it.
    SplitRight,
    SplitDown,
    /// Close the focused pane; the tab with its last one.
    ClosePane,
    /// Move the keyboard to the pane in that direction.
    FocusPane(Direction),
    /// Move the divider on that side of the focused pane: it grows
    /// towards `Direction`, its neighbour gives way.
    ResizePane(Direction),
    /// Trade places with the pane in that direction.
    SwapPane(Direction),
    /// Show the focused pane alone over the whole tab, or all again.
    ZoomPane,
    /// Take part in the broadcast (input to all such terminals) or not.
    ToggleBroadcast,
    /// The files of the focused SSH connection (SFTP).
    OpenFiles,
    /// Tell when the focused terminal has been quiet for a while, or stop.
    WatchSilence,
    ToggleSidebar,
    OpenSettings,
    /// Search actions, hosts, snippets, tabs and themes.
    CommandPalette,
    Copy,
    /// Copy what the last command printed (prompt marks).
    CopyLastOutput,
    Paste,
    PasteAndRun,
    ScrollPageUp,
    ScrollPageDown,
    ScrollToTop,
    ScrollToBottom,
    /// Search the scrollback.
    Search,
    /// Scroll to the prompt above / below (shell integration).
    PreviousPrompt,
    NextPrompt,
    FontBigger,
    FontSmaller,
    FontReset,
}

/// Where an action is listed on the settings page.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Group {
    Tabs,
    Panes,
    Window,
    Clipboard,
    Scrolling,
    Font,
}

impl Group {
    pub const ALL: [Group; 6] =
        [Group::Tabs, Group::Panes, Group::Window, Group::Clipboard, Group::Scrolling, Group::Font];

    pub fn label(self) -> String {
        match self {
            Group::Tabs => t!("shortcuts-group-tabs"),
            Group::Panes => t!("shortcuts-group-panes"),
            Group::Window => t!("shortcuts-group-window"),
            Group::Clipboard => t!("shortcuts-group-clipboard"),
            Group::Scrolling => t!("shortcuts-group-scroll"),
            Group::Font => t!("shortcuts-group-font"),
        }
    }
}

impl Action {
    /// Every action, in the order the settings page lists them. A
    /// combination bound to several belongs to the first.
    pub const ALL: [Action; 52] = [
        Action::NewTab,
        Action::CloseTab,
        Action::ReopenTab,
        Action::NextTab,
        Action::PreviousTab,
        Action::SelectTab(1),
        Action::SelectTab(2),
        Action::SelectTab(3),
        Action::SelectTab(4),
        Action::SelectTab(5),
        Action::SelectTab(6),
        Action::SelectTab(7),
        Action::SelectTab(8),
        Action::SelectTab(9),
        Action::MoveTabLeft,
        Action::MoveTabRight,
        Action::SplitRight,
        Action::SplitDown,
        Action::ClosePane,
        Action::FocusPane(Direction::Left),
        Action::FocusPane(Direction::Right),
        Action::FocusPane(Direction::Up),
        Action::FocusPane(Direction::Down),
        Action::ResizePane(Direction::Left),
        Action::ResizePane(Direction::Right),
        Action::ResizePane(Direction::Up),
        Action::ResizePane(Direction::Down),
        Action::SwapPane(Direction::Left),
        Action::SwapPane(Direction::Right),
        Action::SwapPane(Direction::Up),
        Action::SwapPane(Direction::Down),
        Action::ZoomPane,
        Action::ToggleBroadcast,
        Action::WatchSilence,
        Action::OpenFiles,
        Action::ToggleSidebar,
        Action::OpenSettings,
        Action::CommandPalette,
        Action::Copy,
        Action::CopyLastOutput,
        Action::Paste,
        Action::PasteAndRun,
        Action::ScrollPageUp,
        Action::ScrollPageDown,
        Action::ScrollToTop,
        Action::ScrollToBottom,
        Action::Search,
        Action::PreviousPrompt,
        Action::NextPrompt,
        Action::FontBigger,
        Action::FontSmaller,
        Action::FontReset,
    ];

    /// The action's key under `[shortcuts]`.
    pub fn name(self) -> Cow<'static, str> {
        Cow::Borrowed(match self {
            Action::NewTab => "new_tab",
            Action::CloseTab => "close_tab",
            Action::ReopenTab => "reopen_tab",
            Action::NextTab => "next_tab",
            Action::PreviousTab => "previous_tab",
            Action::SelectTab(number) => return Cow::Owned(format!("tab_{number}")),
            Action::MoveTabLeft => "move_tab_left",
            Action::MoveTabRight => "move_tab_right",
            Action::SplitRight => "split_right",
            Action::SplitDown => "split_down",
            Action::ClosePane => "close_pane",
            Action::FocusPane(Direction::Left) => "focus_pane_left",
            Action::FocusPane(Direction::Right) => "focus_pane_right",
            Action::FocusPane(Direction::Up) => "focus_pane_up",
            Action::FocusPane(Direction::Down) => "focus_pane_down",
            Action::ResizePane(Direction::Left) => "resize_pane_left",
            Action::ResizePane(Direction::Right) => "resize_pane_right",
            Action::ResizePane(Direction::Up) => "resize_pane_up",
            Action::ResizePane(Direction::Down) => "resize_pane_down",
            Action::SwapPane(Direction::Left) => "swap_pane_left",
            Action::SwapPane(Direction::Right) => "swap_pane_right",
            Action::SwapPane(Direction::Up) => "swap_pane_up",
            Action::SwapPane(Direction::Down) => "swap_pane_down",
            Action::ZoomPane => "zoom_pane",
            Action::ToggleBroadcast => "toggle_broadcast",
            Action::OpenFiles => "open_files",
            Action::WatchSilence => "watch_silence",
            Action::ToggleSidebar => "toggle_sidebar",
            Action::OpenSettings => "open_settings",
            Action::CommandPalette => "command_palette",
            Action::Copy => "copy",
            Action::CopyLastOutput => "copy_last_output",
            Action::Paste => "paste",
            Action::PasteAndRun => "paste_and_run",
            Action::ScrollPageUp => "scroll_page_up",
            Action::ScrollPageDown => "scroll_page_down",
            Action::ScrollToTop => "scroll_to_top",
            Action::ScrollToBottom => "scroll_to_bottom",
            Action::Search => "search",
            Action::PreviousPrompt => "previous_prompt",
            Action::NextPrompt => "next_prompt",
            Action::FontBigger => "font_bigger",
            Action::FontSmaller => "font_smaller",
            Action::FontReset => "font_reset",
        })
    }

    fn from_name(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|action| action.name() == name)
    }

    pub fn label(self) -> String {
        match self {
            Action::NewTab => t!("shortcut-new-tab"),
            Action::CloseTab => t!("shortcut-close-tab"),
            Action::ReopenTab => t!("shortcut-reopen-tab"),
            Action::NextTab => t!("shortcut-next-tab"),
            Action::PreviousTab => t!("shortcut-previous-tab"),
            Action::SelectTab(number) => t!("shortcut-select-tab", number = u32::from(number)),
            Action::MoveTabLeft => t!("shortcut-move-tab-left"),
            Action::MoveTabRight => t!("shortcut-move-tab-right"),
            Action::SplitRight => t!("shortcut-split-right"),
            Action::SplitDown => t!("shortcut-split-down"),
            Action::ClosePane => t!("shortcut-close-pane"),
            Action::FocusPane(Direction::Left) => t!("shortcut-focus-pane-left"),
            Action::FocusPane(Direction::Right) => t!("shortcut-focus-pane-right"),
            Action::FocusPane(Direction::Up) => t!("shortcut-focus-pane-up"),
            Action::FocusPane(Direction::Down) => t!("shortcut-focus-pane-down"),
            Action::ResizePane(Direction::Left) => t!("shortcut-resize-pane-left"),
            Action::ResizePane(Direction::Right) => t!("shortcut-resize-pane-right"),
            Action::ResizePane(Direction::Up) => t!("shortcut-resize-pane-up"),
            Action::ResizePane(Direction::Down) => t!("shortcut-resize-pane-down"),
            Action::SwapPane(Direction::Left) => t!("shortcut-swap-pane-left"),
            Action::SwapPane(Direction::Right) => t!("shortcut-swap-pane-right"),
            Action::SwapPane(Direction::Up) => t!("shortcut-swap-pane-up"),
            Action::SwapPane(Direction::Down) => t!("shortcut-swap-pane-down"),
            Action::ZoomPane => t!("shortcut-zoom-pane"),
            Action::ToggleBroadcast => t!("shortcut-toggle-broadcast"),
            Action::OpenFiles => t!("shortcut-open-files"),
            Action::WatchSilence => t!("shortcut-watch-silence"),
            Action::ToggleSidebar => t!("shortcut-toggle-sidebar"),
            Action::OpenSettings => t!("shortcut-open-settings"),
            Action::CommandPalette => t!("shortcut-command-palette"),
            Action::Copy => t!("shortcut-copy"),
            Action::CopyLastOutput => t!("shortcut-copy-last-output"),
            Action::Paste => t!("shortcut-paste"),
            Action::PasteAndRun => t!("shortcut-paste-and-run"),
            Action::ScrollPageUp => t!("shortcut-scroll-page-up"),
            Action::ScrollPageDown => t!("shortcut-scroll-page-down"),
            Action::ScrollToTop => t!("shortcut-scroll-to-top"),
            Action::ScrollToBottom => t!("shortcut-scroll-to-bottom"),
            Action::Search => t!("shortcut-search"),
            Action::PreviousPrompt => t!("shortcut-previous-prompt"),
            Action::NextPrompt => t!("shortcut-next-prompt"),
            Action::FontBigger => t!("shortcut-font-bigger"),
            Action::FontSmaller => t!("shortcut-font-smaller"),
            Action::FontReset => t!("shortcut-font-reset"),
        }
    }

    pub fn group(self) -> Group {
        match self {
            Action::NewTab
            | Action::CloseTab
            | Action::ReopenTab
            | Action::NextTab
            | Action::PreviousTab
            | Action::SelectTab(_)
            | Action::MoveTabLeft
            | Action::MoveTabRight
            | Action::OpenFiles => Group::Tabs,
            Action::SplitRight
            | Action::SplitDown
            | Action::ClosePane
            | Action::FocusPane(_)
            | Action::ResizePane(_)
            | Action::SwapPane(_)
            | Action::ZoomPane
            | Action::ToggleBroadcast
            | Action::WatchSilence => Group::Panes,
            Action::ToggleSidebar | Action::OpenSettings | Action::CommandPalette => Group::Window,
            Action::Copy | Action::CopyLastOutput | Action::Paste | Action::PasteAndRun => Group::Clipboard,
            Action::ScrollPageUp
            | Action::ScrollPageDown
            | Action::ScrollToTop
            | Action::ScrollToBottom
            | Action::Search
            | Action::PreviousPrompt
            | Action::NextPrompt => Group::Scrolling,
            Action::FontBigger | Action::FontSmaller | Action::FontReset => Group::Font,
        }
    }

    /// Also works while a widget in the sidebar or the settings tab has
    /// the keyboard. The others act on the terminal and leave the key to
    /// egui then (Ctrl+Shift+V in a text field pastes there).
    pub fn is_global(self) -> bool {
        !matches!(self.group(), Group::Clipboard | Group::Scrolling)
    }

    pub fn defaults(self) -> Vec<KeyCombo> {
        let combos: &[&str] = match self {
            Action::NewTab => &["Ctrl+Shift+T"],
            Action::CloseTab => &["Ctrl+Shift+Alt+W"],
            Action::ReopenTab => &["Ctrl+Shift+Alt+T"],
            Action::NextTab => &["Ctrl+Tab"],
            Action::PreviousTab => &["Ctrl+Shift+Tab"],
            Action::SelectTab(number) => {
                let digit = char::from(b'0' + number);
                return vec![KeyCombo { alt: true, ..KeyCombo::plain(KeyName::Char(digit)) }];
            }
            Action::MoveTabLeft => &["Ctrl+Shift+PageUp"],
            Action::MoveTabRight => &["Ctrl+Shift+PageDown"],
            // Like Tabby.
            Action::SplitRight => &["Ctrl+Shift+D"],
            Action::SplitDown => &["Ctrl+Shift+Alt+D"],
            Action::ClosePane => &["Ctrl+Shift+W"],
            Action::FocusPane(Direction::Left) => &["Ctrl+Alt+Left"],
            Action::FocusPane(Direction::Right) => &["Ctrl+Alt+Right"],
            Action::FocusPane(Direction::Up) => &["Ctrl+Alt+Up"],
            Action::FocusPane(Direction::Down) => &["Ctrl+Alt+Down"],
            Action::ResizePane(Direction::Left) => &["Ctrl+Shift+Alt+Left"],
            Action::ResizePane(Direction::Right) => &["Ctrl+Shift+Alt+Right"],
            Action::ResizePane(Direction::Up) => &["Ctrl+Shift+Alt+Up"],
            Action::ResizePane(Direction::Down) => &["Ctrl+Shift+Alt+Down"],
            // Swapping is rarer than the rest and every roomy combination
            // is taken; the settings page is where to give it one.
            Action::SwapPane(_) => &[],
            Action::ZoomPane => &["Ctrl+Shift+Enter"],
            Action::ToggleBroadcast => &["Ctrl+Shift+I"],
            Action::OpenFiles => &["Ctrl+Shift+O"],
            Action::WatchSilence => &[],
            Action::ToggleSidebar => &["Ctrl+Shift+B"],
            Action::OpenSettings => &["Ctrl+,"],
            Action::CommandPalette => &["Ctrl+Shift+P"],
            Action::Copy => &["Ctrl+Shift+C"],
            Action::CopyLastOutput => &["Ctrl+Shift+Alt+C"],
            Action::Paste => &["Ctrl+Shift+V"],
            Action::PasteAndRun => &[],
            Action::ScrollPageUp => &["Shift+PageUp"],
            Action::ScrollPageDown => &["Shift+PageDown"],
            Action::ScrollToTop => &["Shift+Home"],
            Action::ScrollToBottom => &["Shift+End"],
            Action::Search => &["Ctrl+Shift+F"],
            Action::PreviousPrompt => &["Ctrl+Shift+Up"],
            Action::NextPrompt => &["Ctrl+Shift+Down"],
            // `+` sits on its own key on a German layout, on `=` on a US one.
            Action::FontBigger => &["Ctrl+Plus", "Ctrl+="],
            Action::FontSmaller => &["Ctrl+Minus"],
            Action::FontReset => &["Ctrl+0"],
        };
        combos.iter().map(|text| KeyCombo::parse(text).expect("default shortcut parses")).collect()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum KeyName {
    /// A character key, lowercase.
    Char(char),
    Named(NamedKey),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct KeyCombo {
    pub ctrl: bool,
    pub shift: bool,
    pub alt: bool,
    /// Super, the Windows key.
    pub logo: bool,
    pub key: KeyName,
}

/// Named keys a shortcut can use: the key, its name in config.toml, and
/// other spellings accepted there.
const NAMED: &[(NamedKey, &str, &[&str])] = &[
    (NamedKey::Tab, "Tab", &[]),
    (NamedKey::Enter, "Enter", &["Return"]),
    (NamedKey::Escape, "Escape", &["Esc"]),
    (NamedKey::Space, "Space", &[]),
    (NamedKey::Backspace, "Backspace", &[]),
    (NamedKey::Delete, "Delete", &["Del"]),
    (NamedKey::Insert, "Insert", &["Ins"]),
    (NamedKey::Home, "Home", &[]),
    (NamedKey::End, "End", &[]),
    (NamedKey::PageUp, "PageUp", &["PgUp"]),
    (NamedKey::PageDown, "PageDown", &["PgDn"]),
    (NamedKey::ArrowUp, "Up", &["ArrowUp"]),
    (NamedKey::ArrowDown, "Down", &["ArrowDown"]),
    (NamedKey::ArrowLeft, "Left", &["ArrowLeft"]),
    (NamedKey::ArrowRight, "Right", &["ArrowRight"]),
    (NamedKey::F1, "F1", &[]),
    (NamedKey::F2, "F2", &[]),
    (NamedKey::F3, "F3", &[]),
    (NamedKey::F4, "F4", &[]),
    (NamedKey::F5, "F5", &[]),
    (NamedKey::F6, "F6", &[]),
    (NamedKey::F7, "F7", &[]),
    (NamedKey::F8, "F8", &[]),
    (NamedKey::F9, "F9", &[]),
    (NamedKey::F10, "F10", &[]),
    (NamedKey::F11, "F11", &[]),
    (NamedKey::F12, "F12", &[]),
];

/// Character keys with a name in config.toml: `+` separates the parts,
/// and `Ctrl+Minus` reads better than `Ctrl+-`. Only the first two are
/// written that way.
const CHAR_NAMES: &[(char, &str)] = &[('+', "Plus"), ('-', "Minus"), (',', "Comma"), ('.', "Period")];

impl KeyCombo {
    /// Escape on its own: cancels recording a shortcut.
    pub const ESCAPE: Self = Self::plain(KeyName::Named(NamedKey::Escape));

    const fn plain(key: KeyName) -> Self {
        Self { ctrl: false, shift: false, alt: false, logo: false, key }
    }

    /// `Ctrl+Shift+T`, `Alt+1`, `Ctrl+Plus`: any modifiers (Ctrl, Shift,
    /// Alt, Super) and one key, joined by `+`, in any case.
    pub fn parse(text: &str) -> Result<Self, String> {
        let text = text.trim();
        // `Ctrl++`: the key itself is a plus.
        let (modifiers, key) = match text.strip_suffix("++") {
            Some(modifiers) => (modifiers, "+"),
            None => text.rsplit_once('+').unwrap_or(("", text)),
        };
        let mut combo = Self::plain(parse_key(key.trim()).ok_or_else(|| format!("unknown key {key:?} in {text:?}"))?);
        for modifier in modifiers.split('+').map(str::trim).filter(|m| !m.is_empty()) {
            match modifier.to_ascii_lowercase().as_str() {
                "ctrl" | "control" => combo.ctrl = true,
                "shift" => combo.shift = true,
                "alt" => combo.alt = true,
                "super" | "meta" | "logo" | "win" => combo.logo = true,
                _ => return Err(format!("unknown modifier {modifier:?} in {text:?}")),
            }
        }
        Ok(combo)
    }

    /// The combination a key press makes; `None` for a modifier on its
    /// own and keys a shortcut can't use.
    pub fn from_event(event: &KeyInput, modifiers: ModifiersState) -> Option<Self> {
        let key = match event.key_without_modifiers.clone() {
            Key::Character(text) => {
                let mut chars = text.chars();
                match (chars.next(), chars.next()) {
                    (Some(c), None) => KeyName::Char(lowercase(c)),
                    _ => return None,
                }
            }
            Key::Named(named) if NAMED.iter().any(|(key, ..)| *key == named) => KeyName::Named(named),
            _ => return None,
        };
        Some(Self {
            ctrl: modifiers.control_key(),
            shift: modifiers.shift_key(),
            alt: modifiers.alt_key(),
            logo: modifiers.super_key(),
            key,
        })
    }

    /// Binding this leaves normal typing alone: it takes Ctrl, Alt or
    /// Super, is an F key, or Shift with a navigation key (Shift+PageUp).
    /// A letter, Shift+letter, Enter or Tab would take keys away from the
    /// terminal. Only recording on the settings page checks this -- what
    /// config.toml says is the user's call.
    pub fn leaves_typing_alone(&self) -> bool {
        if self.ctrl || self.alt || self.logo {
            return true;
        }
        let KeyName::Named(key) = self.key else { return false };
        let f_key = matches!(
            key,
            NamedKey::F1
                | NamedKey::F2
                | NamedKey::F3
                | NamedKey::F4
                | NamedKey::F5
                | NamedKey::F6
                | NamedKey::F7
                | NamedKey::F8
                | NamedKey::F9
                | NamedKey::F10
                | NamedKey::F11
                | NamedKey::F12
        );
        let navigation = matches!(
            key,
            NamedKey::PageUp
                | NamedKey::PageDown
                | NamedKey::Home
                | NamedKey::End
                | NamedKey::Insert
                | NamedKey::Delete
                | NamedKey::ArrowUp
                | NamedKey::ArrowDown
                | NamedKey::ArrowLeft
                | NamedKey::ArrowRight
        );
        f_key || self.shift && navigation
    }

    /// As shown in the UI, e.g. `Strg+Umschalt+T` in German.
    pub fn label(&self) -> String {
        let mut parts = Vec::new();
        for (on, name) in [
            (self.ctrl, t!("key-ctrl")),
            (self.shift, t!("key-shift")),
            (self.alt, t!("key-alt")),
            (self.logo, t!("key-super")),
        ] {
            if on {
                parts.push(name);
            }
        }
        parts.push(match self.key {
            KeyName::Char(c) => uppercase(c),
            KeyName::Named(key) => match key {
                NamedKey::Space => t!("key-space"),
                NamedKey::Backspace => t!("key-backspace"),
                NamedKey::Delete => t!("key-delete"),
                NamedKey::Insert => t!("key-insert"),
                NamedKey::Home => t!("key-home"),
                NamedKey::End => t!("key-end"),
                NamedKey::PageUp => t!("key-page-up"),
                NamedKey::PageDown => t!("key-page-down"),
                NamedKey::ArrowUp => "↑".into(),
                NamedKey::ArrowDown => "↓".into(),
                NamedKey::ArrowLeft => "←".into(),
                NamedKey::ArrowRight => "→".into(),
                _ => named_name(key).into(),
            },
        });
        parts.join("+")
    }
}

/// As written in config.toml: `Ctrl+Shift+T`, `Ctrl+Plus`.
impl fmt::Display for KeyCombo {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for (on, name) in [(self.ctrl, "Ctrl"), (self.shift, "Shift"), (self.alt, "Alt"), (self.logo, "Super")] {
            if on {
                write!(f, "{name}+")?;
            }
        }
        match self.key {
            KeyName::Char('+') => f.write_str("Plus"),
            KeyName::Char('-') => f.write_str("Minus"),
            KeyName::Char(c) => f.write_str(&uppercase(c)),
            KeyName::Named(key) => f.write_str(named_name(key)),
        }
    }
}

fn parse_key(name: &str) -> Option<KeyName> {
    let named = NAMED.iter().find(|(_, canonical, aliases)| {
        canonical.eq_ignore_ascii_case(name) || aliases.iter().any(|alias| alias.eq_ignore_ascii_case(name))
    });
    if let Some(&(key, ..)) = named {
        return Some(KeyName::Named(key));
    }
    if let Some(&(c, _)) = CHAR_NAMES.iter().find(|(_, char_name)| char_name.eq_ignore_ascii_case(name)) {
        return Some(KeyName::Char(c));
    }
    let mut chars = name.chars();
    match (chars.next(), chars.next()) {
        (Some(c), None) if !c.is_whitespace() => Some(KeyName::Char(lowercase(c))),
        _ => None,
    }
}

fn named_name(key: NamedKey) -> &'static str {
    NAMED.iter().find(|(named, ..)| *named == key).map_or("?", |(_, name, _)| name)
}

/// Unless that takes more than one character (`ß` → `SS`).
fn uppercase(c: char) -> String {
    let upper: String = c.to_uppercase().collect();
    if upper.chars().count() == 1 { upper } else { c.to_string() }
}

fn lowercase(c: char) -> char {
    let mut lower = c.to_lowercase();
    match (lower.next(), lower.next()) {
        (Some(lower), None) => lower,
        _ => c,
    }
}

/// A `[shortcuts]` entry as written in config.toml.
#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(untagged)]
pub enum Bindings {
    One(String),
    Many(Vec<String>),
    /// Neither -- warned about, then the defaults apply. Keeps a typo
    /// here from failing the whole config.
    Other(toml::Value),
}

impl Bindings {
    /// How `combos` are written: one as a string, else an array.
    pub fn of(combos: &[KeyCombo]) -> Self {
        match combos {
            [one] => Self::One(one.to_string()),
            _ => Self::Many(combos.iter().map(KeyCombo::to_string).collect()),
        }
    }
}

/// Which combination does what.
#[derive(Clone, Debug)]
pub struct Keymap {
    /// Every action with its combinations, in [`Action::ALL`]'s order.
    bindings: Vec<(Action, Vec<KeyCombo>)>,
}

impl Keymap {
    /// From config.toml's `[shortcuts]`: the defaults, replaced per action.
    pub fn new(config: &BTreeMap<String, Bindings>) -> Self {
        for name in config.keys().filter(|name| Action::from_name(name).is_none()) {
            log::warn!("unknown shortcut {name:?} in config");
        }
        let bindings = Action::ALL
            .into_iter()
            .map(|action| {
                let combos = match config.get(action.name().as_ref()) {
                    None => action.defaults(),
                    Some(Bindings::One(text)) => parse_all(action, std::slice::from_ref(text)),
                    Some(Bindings::Many(texts)) => parse_all(action, texts),
                    Some(Bindings::Other(value)) => {
                        log::warn!("shortcut {}: expected a string or a list of strings, got {value}", action.name());
                        action.defaults()
                    }
                };
                (action, combos)
            })
            .collect();
        Self { bindings }
    }

    /// What `combo` does, if anything.
    pub fn action(&self, combo: &KeyCombo) -> Option<Action> {
        self.bindings.iter().find(|(_, combos)| combos.contains(combo)).map(|(action, _)| *action)
    }

    pub fn combos(&self, action: Action) -> &[KeyCombo] {
        self.bindings.iter().find(|(bound, _)| *bound == action).map_or(&[], |(_, combos)| combos)
    }

    /// The first combination that actually triggers `action`, as shown in
    /// the UI.
    pub fn label(&self, action: Action) -> Option<String> {
        self.combos(action).iter().find(|combo| self.action(combo) == Some(action)).map(KeyCombo::label)
    }
}

/// The combinations in `texts`; blank ones mean none. If none of the rest
/// parses, that's a typo and the defaults stay.
fn parse_all(action: Action, texts: &[String]) -> Vec<KeyCombo> {
    let texts: Vec<&String> = texts.iter().filter(|text| !text.trim().is_empty()).collect();
    let combos: Vec<KeyCombo> = texts
        .iter()
        .filter_map(|text| KeyCombo::parse(text).inspect_err(|err| log::warn!("shortcut {}: {err}", action.name())).ok())
        .collect();
    if combos.is_empty() && !texts.is_empty() { action.defaults() } else { combos }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use super::*;

    fn combo(text: &str) -> KeyCombo {
        KeyCombo::parse(text).unwrap()
    }

    #[test]
    fn combinations_parse_and_print_back() {
        for (text, printed) in [
            ("Ctrl+Shift+T", "Ctrl+Shift+T"),
            ("shift+ctrl+t", "Ctrl+Shift+T"),
            ("Ctrl++", "Ctrl+Plus"),
            ("Ctrl+plus", "Ctrl+Plus"),
            ("Ctrl+-", "Ctrl+Minus"),
            ("Ctrl+,", "Ctrl+,"),
            ("Ctrl+Comma", "Ctrl+,"),
            ("Alt+1", "Alt+1"),
            ("Shift+PgUp", "Shift+PageUp"),
            ("Super+Return", "Super+Enter"),
            ("F11", "F11"),
            ("Ctrl+Alt+ß", "Ctrl+Alt+ß"),
        ] {
            let parsed = combo(text);
            assert_eq!(parsed.to_string(), printed, "{text}");
            assert_eq!(combo(printed), parsed, "{printed}");
        }
        for bad in ["", "Ctrl+", "Ctrl+Foo", "Hyper+T", "Ctrl+Shift"] {
            assert!(KeyCombo::parse(bad).is_err(), "{bad:?}");
        }
    }

    #[test]
    fn defaults_parse_leave_typing_alone_and_never_collide() {
        let keymap = Keymap::new(&BTreeMap::new());
        let mut seen = HashMap::new();
        for action in Action::ALL {
            for combo in action.defaults() {
                assert!(combo.leaves_typing_alone(), "{action:?}: {combo}");
                if let Some(other) = seen.insert(combo, action) {
                    panic!("{combo} for both {other:?} and {action:?}");
                }
                assert_eq!(keymap.action(&combo), Some(action));
            }
            assert_eq!(Action::from_name(&action.name()), Some(action));
        }
    }

    #[test]
    fn config_entries_replace_the_defaults() {
        let config: BTreeMap<String, Bindings> = toml::from_str(
            r#"
            new_tab = "Ctrl+Alt+N"
            copy = ["Ctrl+Insert", "Ctrl+Shift+C"]
            paste = []
            close_tab = 5
            next_tab = "Ctrl+Nonsense"
            no_such_action = "Ctrl+X"
            "#,
        )
        .unwrap();
        let keymap = Keymap::new(&config);
        assert_eq!(keymap.action(&combo("Ctrl+Alt+N")), Some(Action::NewTab));
        assert_eq!(keymap.action(&combo("Ctrl+Shift+T")), None);
        assert_eq!(keymap.combos(Action::Copy), [combo("Ctrl+Insert"), combo("Ctrl+Shift+C")]);
        assert!(keymap.combos(Action::Paste).is_empty());
        // Not a combination at all, or none that parses: the defaults stay.
        assert_eq!(keymap.combos(Action::CloseTab), Action::CloseTab.defaults());
        assert_eq!(keymap.combos(Action::NextTab), Action::NextTab.defaults());
    }

    #[test]
    fn a_combination_bound_twice_belongs_to_the_first_action() {
        let config = BTreeMap::from([("open_settings".to_string(), Bindings::One("Ctrl+Shift+T".into()))]);
        let keymap = Keymap::new(&config);
        assert_eq!(keymap.action(&combo("Ctrl+Shift+T")), Some(Action::NewTab));
        assert_eq!(keymap.label(Action::OpenSettings), None);
    }

    #[test]
    fn combinations_that_would_swallow_typing_are_told_apart() {
        for ok in ["Ctrl+A", "Alt+1", "Super+Space", "F5", "Shift+PageUp", "Shift+Up"] {
            assert!(combo(ok).leaves_typing_alone(), "{ok}");
        }
        for bad in ["A", "Shift+A", "Enter", "Tab", "Shift+Tab", "Space", "Escape", "PageUp"] {
            assert!(!combo(bad).leaves_typing_alone(), "{bad}");
        }
    }

    #[test]
    fn labels_are_translated() {
        // Tests run in German.
        assert_eq!(combo("Ctrl+Shift+T").label(), "Strg+Umschalt+T");
        assert_eq!(combo("Shift+PageUp").label(), "Umschalt+Bild↑");
        assert_eq!(combo("Ctrl+Plus").label(), "Strg++");
    }

    #[test]
    fn bindings_are_written_as_a_string_or_a_list() {
        assert_eq!(Bindings::of(&[combo("Ctrl+Plus")]), Bindings::One("Ctrl+Plus".into()));
        assert_eq!(Bindings::of(&[]), Bindings::Many(Vec::new()));
        assert_eq!(
            Bindings::of(&[combo("Ctrl+Plus"), combo("Ctrl+=")]),
            Bindings::Many(vec!["Ctrl+Plus".into(), "Ctrl+=".into()])
        );
    }
}
