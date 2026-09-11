//! The "COSMIC" theme: colors taken from the COSMIC desktop's own theme,
//! following its dark/light mode. COSMIC keeps one RON file per key under
//! `~/.config/cosmic/com.system76.CosmicTheme.{Mode,Dark,Light}/v1/`, with
//! the system's defaults under `/usr/share/cosmic` for every key the user
//! never changed. [`watch`] notices when the desktop's theme changes.
//!
//! The console gets COSMIC's window background and text, its palette's
//! accent colors as the ANSI colors and its neutrals as black/white; the
//! chrome gets the "primary" container (as COSMIC's own navigation bars
//! do) and the accent color.

use std::ffi::CString;
use std::iter::Peekable;
use std::os::unix::ffi::OsStrExt;
use std::path::{Path, PathBuf};
use std::str::Chars;

use alacritty_terminal::vte::ansi::Rgb;

use super::{Source, TerminalColors, Theme, UiColors, mix};
use crate::i18n::t;

/// The theme's name in the list.
pub const NAME: &str = "COSMIC";

const MODE: &str = "com.system76.CosmicTheme.Mode";
const DARK: &str = "com.system76.CosmicTheme.Dark";
const LIGHT: &str = "com.system76.CosmicTheme.Light";

/// A change to the theme usually touches many files at once; [`watch`]
/// waits until none has changed for this long.
const SETTLE_MS: i32 = 200;

/// Where COSMIC's config lives: the user's, then the system's defaults.
#[derive(Clone, Debug)]
pub struct Roots {
    user: Option<PathBuf>,
    system: Vec<PathBuf>,
}

impl Roots {
    /// `$XDG_CONFIG_HOME/cosmic` (or `~/.config/cosmic`), then `cosmic` in
    /// each of `$XDG_DATA_DIRS` (or `/usr/local/share`, `/usr/share`).
    pub fn system() -> Self {
        let user = std::env::var_os("XDG_CONFIG_HOME")
            .filter(|dir| !dir.is_empty())
            .map(PathBuf::from)
            .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".config")))
            .map(|dir| dir.join("cosmic"));
        let data_dirs = std::env::var("XDG_DATA_DIRS")
            .ok()
            .filter(|dirs| !dirs.is_empty())
            .unwrap_or_else(|| "/usr/local/share:/usr/share".to_string());
        let system = data_dirs.split(':').filter(|dir| !dir.is_empty()).map(|dir| Path::new(dir).join("cosmic")).collect();
        Self { user, system }
    }

    /// One key's value: the user's if it's there and readable, else the
    /// system default.
    fn read(&self, component: &str, key: &str) -> Option<Value> {
        self.user.iter().chain(&self.system).find_map(|root| {
            let text = std::fs::read_to_string(root.join(component).join("v1").join(key)).ok()?;
            Value::parse(&text)
        })
    }
}

/// The COSMIC theme as the desktop has it right now. `None` if there's no
/// COSMIC theme on this system; an error if there is one but it lacks
/// colors.
pub fn load(roots: &Roots) -> Option<Result<Theme, String>> {
    let dark = roots.read(MODE, "is_dark").and_then(|value| value.as_bool()).unwrap_or(true);
    let component = if dark { DARK } else { LIGHT };
    roots.read(component, "background")?;
    Some(build(roots, component).map_err(|key| t!("theme-missing-color", key = format!("{component}/v1/{key}"))))
}

/// The theme from `component`'s files; the error names what's missing.
fn build(roots: &Roots, component: &str) -> Result<Theme, String> {
    let read = |key: &str| roots.read(component, key).ok_or_else(|| key.to_string());
    let (background, primary, accent, palette) = (read("background")?, read("primary")?, read("accent")?, read("palette")?);
    let color = |value: &Value, file: &str, path: &str| value.rgba(path).map(|(rgb, _)| rgb).ok_or(format!("{file} → {path}"));
    let p = |name: &str| color(&palette, "palette", name);

    let bg = color(&background, "background", "base")?;
    let fg = color(&background, "background", "on")?;
    let panel = color(&primary, "primary", "base")?;
    let panel_text = color(&primary, "primary", "on")?;
    let (divider, divider_alpha) = primary.rgba("divider").ok_or("primary → divider")?;
    let accent = color(&accent, "accent", "base")?;
    let optional = |key: &str| roots.read(component, key).and_then(|value| value.rgba("base")).map(|(rgb, _)| rgb);

    // Dark to light, whichever order the palette has them in.
    let mut neutrals = (0..=10).map(|i| p(&format!("neutral_{i}"))).collect::<Result<Vec<_>, _>>()?;
    neutrals.sort_by(|a, b| luminance(*a).total_cmp(&luminance(*b)));
    let white = Rgb { r: 255, g: 255, b: 255 };
    let normal = [
        neutrals[2],
        p("accent_red")?,
        p("accent_green")?,
        p("accent_yellow")?,
        p("accent_blue")?,
        p("accent_purple")?,
        p("ext_blue")?,
        neutrals[8],
    ];
    let bright = [
        neutrals[5],
        p("bright_red")?,
        p("bright_green")?,
        p("ext_yellow")?,
        mix(normal[4], white, 0.25),
        p("accent_pink")?,
        mix(normal[6], white, 0.25),
        neutrals[10],
    ];
    // Towards the background rather than black, so light themes dim too.
    let terminal = TerminalColors {
        foreground: fg,
        background: bg,
        bright_foreground: fg,
        dim_foreground: mix(fg, bg, 0.35),
        cursor: accent,
        selection: mix(bg, accent, 0.35),
        selection_text: None,
        normal,
        bright,
        dim: normal.map(|c| mix(c, bg, 0.35)),
    };
    let ui = UiColors {
        background: panel,
        row: color(&primary, "primary", "component.base")?,
        hover: color(&primary, "primary", "component.hover")?,
        selected: mix(panel, accent, 0.2),
        border: mix(panel, divider, divider_alpha),
        border_strong: color(&primary, "primary", "component.pressed")?,
        accent,
        text: panel_text,
        text_weak: mix(panel, panel_text, 0.6),
        error: optional("destructive").unwrap_or(bright[1]),
        success: optional("success").unwrap_or(bright[2]),
        input: bg,
        text_selection: mix(panel, accent, 0.4),
    };
    Ok(Theme { name: NAME.to_string(), source: Source::Cosmic, terminal, ui })
}

fn luminance(Rgb { r, g, b }: Rgb) -> f32 {
    0.2126 * f32::from(r) + 0.7152 * f32::from(g) + 0.0722 * f32::from(b)
}

/// Call `changed` whenever the desktop's theme changes. Runs a thread
/// blocked on inotify, so a quiet desktop costs nothing; `changed` comes
/// once a change has settled ([`SETTLE_MS`]). No thread without a COSMIC
/// theme folder to watch.
pub fn watch(roots: &Roots, changed: impl Fn() + Send + 'static) {
    let Some(user) = &roots.user else { return };
    let dirs: Vec<PathBuf> =
        [MODE, DARK, LIGHT].iter().map(|component| user.join(component).join("v1")).filter(|dir| dir.is_dir()).collect();
    if dirs.is_empty() {
        return;
    }
    // SAFETY: plain syscalls; the descriptor belongs to the thread below
    // (or is closed right here when that can't start).
    let fd = unsafe { libc::inotify_init1(libc::IN_CLOEXEC) };
    if fd < 0 {
        log::warn!("can't watch the COSMIC theme: {}", std::io::Error::last_os_error());
        return;
    }
    // COSMIC replaces a file by renaming a new one over it.
    let mask = libc::IN_CLOSE_WRITE | libc::IN_MOVED_TO | libc::IN_CREATE | libc::IN_DELETE;
    for dir in &dirs {
        let Ok(path) = CString::new(dir.as_os_str().as_bytes()) else { continue };
        if unsafe { libc::inotify_add_watch(fd, path.as_ptr(), mask) } < 0 {
            log::warn!("can't watch {}: {}", dir.display(), std::io::Error::last_os_error());
        }
    }
    let thread = std::thread::Builder::new().name("cosmic-theme".into()).spawn(move || {
        let mut events = [0u8; 4096];
        loop {
            if !readable_within(fd, -1) || !drain(fd, &mut events) {
                break;
            }
            while readable_within(fd, SETTLE_MS) {
                if !drain(fd, &mut events) {
                    return;
                }
            }
            log::debug!("COSMIC theme changed");
            changed();
        }
        log::warn!("stopped watching the COSMIC theme: {}", std::io::Error::last_os_error());
    });
    if let Err(err) = thread {
        log::warn!("can't watch the COSMIC theme: {err}");
        unsafe { libc::close(fd) };
    }
}

/// Whether `fd` has something to read within `ms` milliseconds (-1:
/// however long it takes).
fn readable_within(fd: libc::c_int, ms: libc::c_int) -> bool {
    let mut poll = libc::pollfd { fd, events: libc::POLLIN, revents: 0 };
    loop {
        match unsafe { libc::poll(&mut poll, 1, ms) } {
            n if n > 0 => return true,
            0 => return false,
            _ if std::io::Error::last_os_error().kind() == std::io::ErrorKind::Interrupted => continue,
            _ => return false,
        }
    }
}

/// Read (and drop) the pending inotify events; `false` if that failed.
fn drain(fd: libc::c_int, buf: &mut [u8]) -> bool {
    loop {
        let n = unsafe { libc::read(fd, buf.as_mut_ptr().cast(), buf.len()) };
        if n > 0 {
            return true;
        }
        if n < 0 && std::io::Error::last_os_error().kind() == std::io::ErrorKind::Interrupted {
            continue;
        }
        return false;
    }
}

/// Just enough of RON for COSMIC's theme files: structs (named or not),
/// tuples, lists, numbers, bools, strings and bare identifiers (`None`,
/// enum variants). `Some(x)` and named structs lose their name.
#[derive(Clone, Debug, PartialEq)]
enum Value {
    Number(f64),
    Bool(bool),
    Text(String),
    Ident(String),
    Fields(Vec<(String, Value)>),
    Items(Vec<Value>),
}

impl Value {
    fn parse(text: &str) -> Option<Self> {
        let mut parser = Parser { chars: text.chars().peekable() };
        let value = parser.value()?;
        parser.skip_blank();
        parser.chars.peek().is_none().then_some(value)
    }

    /// The value at a `.`-separated path of field names.
    fn get(&self, path: &str) -> Option<&Value> {
        path.split('.').try_fold(self, |value, key| match value {
            Value::Fields(fields) => fields.iter().find(|(name, _)| name == key).map(|(_, value)| value),
            _ => None,
        })
    }

    fn as_bool(&self) -> Option<bool> {
        match self {
            Value::Bool(value) => Some(*value),
            _ => None,
        }
    }

    fn as_number(&self) -> Option<f64> {
        match self {
            Value::Number(value) => Some(*value),
            _ => None,
        }
    }

    /// The `(red, green, blue, alpha)` struct at `path`: channels from 0 to
    /// 1, sRGB.
    fn rgba(&self, path: &str) -> Option<(Rgb, f32)> {
        let color = self.get(path)?;
        let channel = |key: &str| color.get(key)?.as_number();
        let byte = |key: &str| channel(key).map(|value| (value.clamp(0.0, 1.0) * 255.0).round() as u8);
        let alpha = channel("alpha").unwrap_or(1.0).clamp(0.0, 1.0) as f32;
        Some((Rgb { r: byte("red")?, g: byte("green")?, b: byte("blue")? }, alpha))
    }
}

struct Parser<'a> {
    chars: Peekable<Chars<'a>>,
}

impl Parser<'_> {
    /// Skip whitespace and `//` comments.
    fn skip_blank(&mut self) {
        loop {
            match self.chars.peek() {
                Some(c) if c.is_whitespace() => {
                    self.chars.next();
                }
                Some('/') => {
                    let mut ahead = self.chars.clone();
                    ahead.next();
                    if ahead.peek() != Some(&'/') {
                        return;
                    }
                    while self.chars.next().is_some_and(|c| c != '\n') {}
                }
                _ => return,
            }
        }
    }

    fn value(&mut self) -> Option<Value> {
        self.skip_blank();
        match *self.chars.peek()? {
            '(' => self.group(')'),
            '[' => self.group(']'),
            '"' => self.text(),
            c if c.is_ascii_digit() || matches!(c, '-' | '+' | '.') => self.number(),
            c if c.is_alphabetic() || c == '_' => {
                let ident = self.ident();
                match ident.as_str() {
                    "true" => Some(Value::Bool(true)),
                    "false" => Some(Value::Bool(false)),
                    _ => {
                        self.skip_blank();
                        if self.chars.peek() == Some(&'(') { self.group(')') } else { Some(Value::Ident(ident)) }
                    }
                }
            }
            _ => None,
        }
    }

    /// `(...)` or `[...]`: fields if it has `name: value` entries, items
    /// otherwise.
    fn group(&mut self, close: char) -> Option<Value> {
        self.chars.next();
        let (mut fields, mut items) = (Vec::new(), Vec::new());
        loop {
            self.skip_blank();
            if self.chars.peek() == Some(&close) {
                self.chars.next();
                break;
            }
            let checkpoint = self.chars.clone();
            let name = if close == ')' && self.chars.peek().is_some_and(|c| c.is_alphabetic() || *c == '_') {
                let name = self.ident();
                self.skip_blank();
                if self.chars.peek() == Some(&':') {
                    self.chars.next();
                    Some(name)
                } else {
                    self.chars = checkpoint;
                    None
                }
            } else {
                None
            };
            let value = self.value()?;
            match name {
                Some(name) => fields.push((name, value)),
                None => items.push(value),
            }
            self.skip_blank();
            match self.chars.next()? {
                ',' => {}
                c if c == close => break,
                _ => return None,
            }
        }
        Some(if fields.is_empty() { Value::Items(items) } else { Value::Fields(fields) })
    }

    fn ident(&mut self) -> String {
        let mut ident = String::new();
        while let Some(&c) = self.chars.peek().filter(|c| c.is_alphanumeric() || **c == '_') {
            ident.push(c);
            self.chars.next();
        }
        ident
    }

    fn number(&mut self) -> Option<Value> {
        let mut number = String::new();
        while let Some(&c) = self.chars.peek().filter(|c| c.is_ascii_digit() || matches!(c, '-' | '+' | '.' | 'e' | 'E')) {
            number.push(c);
            self.chars.next();
        }
        number.parse().ok().map(Value::Number)
    }

    fn text(&mut self) -> Option<Value> {
        self.chars.next();
        let mut text = String::new();
        loop {
            match self.chars.next()? {
                '"' => return Some(Value::Text(text)),
                '\\' => text.push(self.chars.next()?),
                c => text.push(c),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn color(v: f32) -> String {
        format!("(red: {v}, green: {v}, blue: {v}, alpha: 1.0)")
    }

    fn container(base: f32, on: f32) -> String {
        format!(
            "(\n    base: {},\n    component: (base: {}, hover: {}, pressed: {}),\n    divider: (red: {on}, green: {on}, blue: {on}, alpha: 0.2),\n    on: {},\n)",
            color(base),
            color(base + 0.05),
            color(base + 0.1),
            color(base + 0.15),
            color(on)
        )
    }

    fn palette(neutrals_descending: bool) -> String {
        let mut fields = vec!["    name: \"test\"".to_string()];
        for i in 0..=10 {
            let v = if neutrals_descending { 1.0 - i as f32 / 10.0 } else { i as f32 / 10.0 };
            fields.push(format!("    neutral_{i}: {}", color(v)));
        }
        for (i, name) in [
            "accent_red", "accent_green", "accent_yellow", "accent_blue", "accent_purple", "accent_pink", "ext_blue",
            "ext_yellow", "bright_red", "bright_green",
        ]
        .iter()
        .enumerate()
        {
            fields.push(format!("    {name}: (red: 0.{i}, green: 0.5, blue: 0.9, alpha: 1.0)"));
        }
        format!("(\n{},\n)", fields.join(",\n"))
    }

    fn write(root: &Path, component: &str, key: &str, text: &str) {
        let dir = root.join(component).join("v1");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join(key), text).unwrap();
    }

    fn temp(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("terminaal-cosmic-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        dir
    }

    #[test]
    fn parses_cosmic_ron() {
        let text = "(
    base: (
        red: 0.3882353,
        green: 0.8156863,
        blue: 0.8745098,
        alpha: 0.85,
    ),
    // a comment
    list: [1, 2.5, -3e-1],
    name: \"cosmic-\\\"dark\\\"\",
    maybe: Some((red: 1.0, green: 0.0, blue: 0.0, alpha: 1.0)),
    nothing: None,
    flag: true,
)
";
        let value = Value::parse(text).unwrap();
        assert_eq!(value.rgba("base"), Some((Rgb { r: 99, g: 208, b: 223 }, 0.85)));
        assert_eq!(value.get("list"), Some(&Value::Items(vec![Value::Number(1.0), Value::Number(2.5), Value::Number(-0.3)])));
        assert_eq!(value.get("name"), Some(&Value::Text("cosmic-\"dark\"".into())));
        assert_eq!(value.get("nothing"), Some(&Value::Ident("None".into())));
        assert_eq!(value.get("flag").and_then(Value::as_bool), Some(true));
        assert_eq!(Value::parse("true\n"), Some(Value::Bool(true)));
        assert_eq!(Value::parse("(base: (red: 1.0"), None);
        assert_eq!(Value::parse("(a: 1) trailing"), None);
    }

    #[test]
    fn follows_the_mode_and_falls_back_to_system_defaults() {
        let (user, system) = (temp("user"), temp("system"));
        // The system has both variants; the user switched to light and
        // changed the accent there.
        for (component, bg, fg) in [(DARK, 0.1, 0.9), (LIGHT, 0.95, 0.2)] {
            write(&system, component, "background", &container(bg, fg));
            write(&system, component, "primary", &container(bg - 0.05, fg));
            write(&system, component, "accent", &format!("(base: {})", color(0.5)));
            write(&system, component, "palette", &palette(component == LIGHT));
        }
        write(&user, MODE, "is_dark", "false\n");
        write(&user, LIGHT, "accent", "(base: (red: 1.0, green: 0.0, blue: 0.0, alpha: 1.0))");
        let roots = Roots { user: Some(user.clone()), system: vec![system.clone()] };

        let theme = load(&roots).unwrap().unwrap();
        assert_eq!(theme.name, NAME);
        assert_eq!(theme.source, Source::Cosmic);
        assert!(!theme.ui.is_dark());
        assert_eq!(theme.ui.accent, Rgb { r: 255, g: 0, b: 0 });
        assert_eq!(theme.terminal.background, Rgb { r: 242, g: 242, b: 242 });
        let [black, .., white] = theme.terminal.normal;
        assert!(luminance(black) < luminance(white), "{black:?} vs {white:?}");

        write(&user, MODE, "is_dark", "true\n");
        let theme = load(&roots).unwrap().unwrap();
        assert!(theme.ui.is_dark());
        assert_eq!(theme.ui.accent, Rgb { r: 128, g: 128, b: 128 });

        // Unreadable in the user's folder: the system's value counts.
        write(&user, DARK, "palette", "(broken");
        assert!(load(&roots).unwrap().is_ok());
        std::fs::remove_dir_all(&user).unwrap();
        std::fs::remove_dir_all(&system).unwrap();
    }

    #[test]
    fn missing_theme_or_colors() {
        let user = temp("missing");
        let roots = Roots { user: Some(user.clone()), system: Vec::new() };
        assert!(load(&roots).is_none());

        write(&user, DARK, "background", &container(0.1, 0.9));
        let err = load(&roots).unwrap().unwrap_err();
        assert!(err.contains("primary"), "{err}");
        write(&user, DARK, "primary", "(base: (red: 1.0))");
        write(&user, DARK, "accent", &format!("(base: {})", color(0.5)));
        write(&user, DARK, "palette", &palette(false));
        let err = load(&roots).unwrap().unwrap_err();
        assert!(err.contains("primary → base"), "{err}");
        std::fs::remove_dir_all(&user).unwrap();
    }

    #[test]
    fn watch_reports_a_changed_theme() {
        let user = temp("watch");
        write(&user, MODE, "is_dark", "true\n");
        let roots = Roots { user: Some(user.clone()), system: Vec::new() };
        let (tx, rx) = std::sync::mpsc::channel();
        watch(&roots, move || {
            let _ = tx.send(());
        });
        std::thread::sleep(std::time::Duration::from_millis(100));
        write(&user, MODE, "is_dark", "false\n");
        assert!(rx.recv_timeout(std::time::Duration::from_secs(5)).is_ok());
        std::fs::remove_dir_all(&user).unwrap();
    }
}
