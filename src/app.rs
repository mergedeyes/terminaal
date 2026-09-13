//! Ties the window, GPU state, terminal sessions (one per tab), the
//! renderers and the egui sidebar together behind winit's
//! `ApplicationHandler`.
//!
//! Screen layout, left to right: the sidebar (`ui::sidebar`, optional),
//! then the console column -- tab bar on top (`render::tab_bar`), the
//! terminal grid below it (or, in the settings tab, egui's settings page).

use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

use alacritty_terminal::event::{Event as TermEvent, WindowSize};
use alacritty_terminal::grid::Scroll;
use alacritty_terminal::index::{Column, Line, Point, Side};
use alacritty_terminal::selection::{Selection, SelectionType};
use alacritty_terminal::term::TermMode;
use alacritty_terminal::vte::ansi::NamedColor;
use arboard::Clipboard;
use winit::application::ApplicationHandler;
use winit::dpi::{LogicalSize, PhysicalPosition};
use winit::event::{ElementState, KeyEvent, MouseButton, MouseScrollDelta, StartCause, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoopProxy};
use winit::keyboard::{Key, ModifiersState, NamedKey};
use winit::platform::wayland::WindowAttributesExtWayland;
use winit::platform::x11::WindowAttributesExtX11;
use winit::window::{CursorIcon, Icon, UserAttentionType, Window, WindowId};

/// Wayland app_id and X11 WM_CLASS; a desktop entry named
/// `terminaal.desktop` gets matched to the window through it.
const APP_ID: &str = "terminaal";

/// Window icon for X11 (`_NET_WM_ICON`), RGBA. Wayland has no window-icon
/// request in winit; there the compositor takes the icon from
/// `terminaal.desktop` (installed by `install.sh`).
const ICON_PNG: &[u8] = include_bytes!("../assets/terminaal_icon_128.png");

fn window_icon() -> Option<Icon> {
    let decode = || -> Result<Icon, Box<dyn std::error::Error>> {
        let mut decoder = png::Decoder::new(std::io::Cursor::new(ICON_PNG));
        decoder.set_transformations(png::Transformations::normalize_to_color8() | png::Transformations::ALPHA);
        let mut reader = decoder.read_info()?;
        let mut rgba = vec![0; reader.output_buffer_size().ok_or("icon too large")?];
        let info = reader.next_frame(&mut rgba)?;
        rgba.truncate(info.buffer_size());
        Ok(Icon::from_rgba(rgba, info.width, info.height)?)
    };
    decode().inspect_err(|e| log::warn!("window icon: {e}")).ok()
}

use crate::commands::{System, Target};
use crate::config::{self, Config, FontSlot, Setting};
use crate::i18n::{self, t};
use crate::gpu::GpuState;
use crate::input;
use crate::render::grid::{self, GridText};
use crate::render::palette::{to_linear, Palette};
use crate::render::quad::{QuadInstance, QuadRenderer};
use crate::render::search_bar::{SearchBar, SearchBarView};
use crate::render::tab_bar::{self, TabBar, TabBarHit, TabBarLayout};
use crate::render::text::{CellMetrics, FontFamilies, TextRendererState};
use crate::theme::{Theme, Themes};
use crate::ui::theme::FontFace;
use crate::blur::Blur;
use winit::raw_window_handle::{HasDisplayHandle, RawDisplayHandle};
use crate::shells::{self, launch, InstalledShell};
use crate::shortcuts::{Action, KeyCombo, Keymap};
use crate::ssh::SshTarget;
use crate::render::label::{Labels, Rect as LabelRect};
use crate::terminal::integration::{self, ShellEvent};
use crate::terminal::{links, prompts};
use crate::terminal::search::Search;
use crate::terminal::{EventProxyListener, GridSize, TerminalSession};
use crate::ui::context_menu::{ContextMenu, MenuAction};
use crate::ui::settings_panel::{SettingsPanel, SettingsView, Transparency};
use crate::ui::sidebar::{Sidebar, SidebarAction, TabForwards};
use crate::ui::splash::{self, Splash, SplashFrames};
use crate::ui::UiLayer;

/// A splash decoded later than this after startup is skipped: by then the
/// shell is up and the user may already be reading or typing.
const SPLASH_MAX_WAIT: Duration = Duration::from_millis(750);

/// Events routed through winit's event loop from other threads -- one
/// PTY-reader thread per tab, all feeding the same `EventLoopProxy`. The
/// `usize` is the originating tab's id (see `terminal::listener`).
#[derive(Debug)]
pub enum UserEvent {
    Terminal(usize, TermEvent),
    /// The splash animation finished decoding (`ui::splash`).
    SplashReady(Result<SplashFrames, String>),
    /// The COSMIC desktop's theme changed (`theme::cosmic::watch`).
    CosmicThemeChanged,
    /// A tab's shell said something about itself (`terminal::integration`).
    Shell(usize, ShellEvent),
}

pub struct App {
    proxy: EventLoopProxy<UserEvent>,
    config: Config,
    /// Open the first tab as this SSH connection instead of a local shell
    /// (`--connect`, see `main.rs`).
    connect: Option<SshTarget>,
    state: Option<AppState>,
}

impl App {
    pub fn new(proxy: EventLoopProxy<UserEvent>, config: Config, connect: Option<SshTarget>) -> Self {
        Self { proxy, config, connect, state: None }
    }
}

/// One tab: a shell session or the settings page. Rendering resources
/// (window, GPU, font/text state, quad renderer) live once on `AppState`
/// and are shared -- only the terminal session itself differs per tab,
/// since only the active tab's content is ever built into
/// `AppState::quads`/`text.buffer` on a given redraw.
struct Tab {
    id: usize,
    content: TabContent,
    /// As shown: the program's title, else the working directory, else
    /// `default_title` ([`Tab::refresh_title`]).
    title: String,
    /// The shell's name, or `user@host` for SSH.
    default_title: String,
    /// Set by the program (OSC 0/2) until it resets it.
    program_title: Option<String>,
    /// Working directory the shell reported (OSC 7): host and path.
    cwd: Option<(String, PathBuf)>,
    /// When the running command started (OSC 133;C).
    command_started: Option<Instant>,
    /// Takes part in the broadcast: input typed into one such tab goes to
    /// all of them.
    broadcast: bool,
}

enum TabContent {
    Terminal(TerminalSession),
    /// The settings page (`ui::settings_panel`), drawn by egui where the
    /// grid would be. At most one tab has it.
    Settings,
}

impl Tab {
    /// `None` for the settings tab.
    fn terminal(&self) -> Option<&TerminalSession> {
        match &self.content {
            TabContent::Terminal(terminal) => Some(terminal),
            TabContent::Settings => None,
        }
    }

    fn is_settings(&self) -> bool {
        matches!(self.content, TabContent::Settings)
    }

    fn refresh_title(&mut self, local_host: &str) {
        self.title = match (&self.program_title, &self.cwd) {
            (Some(title), _) => title.clone(),
            (None, Some((host, path))) => {
                let home = std::env::var_os("HOME").map(PathBuf::from);
                integration::short_path(host, path, home.as_deref(), local_host)
            }
            (None, None) => self.default_title.clone(),
        };
    }

    /// The working directory, if it's a folder on this machine: where a
    /// new tab opened from this one starts.
    fn local_cwd(&self, local_host: &str) -> Option<PathBuf> {
        let (host, path) = self.cwd.as_ref()?;
        let here = host.is_empty() || host == local_host || host == "localhost";
        (here && matches!(self.content, TabContent::Terminal(ref t) if t.is_local()) && path.is_dir()).then(|| path.clone())
    }

    /// Input or a reply for the tab's shell; the settings tab has none.
    fn send_input(&self, bytes: Vec<u8>) {
        if let Some(terminal) = self.terminal() {
            terminal.send_input(bytes);
        }
    }
}

/// Where a [`SidebarAction`] came from; its outcome is reported there.
#[derive(Clone, Copy)]
enum Origin {
    Sidebar,
    Settings,
}

struct AppState {
    /// Blur behind the window where the compositor has ext-background-effect.
    /// Declared first, so it goes before the window whose surface it borrows.
    blur: Option<Blur>,
    window: Arc<Window>,
    gpu: GpuState,
    quad_renderer: QuadRenderer,
    text: TextRendererState,
    /// The grid's shaped rows; shared by all tabs like `text`.
    grid_text: GridText,
    tab_bar: TabBar,
    /// Physical-pixel height of the tab bar; 0 when it's disabled in
    /// config. The terminal grid starts below it.
    tab_bar_height: f32,
    /// Tab-bar element under the mouse, for hover highlighting.
    hovered: Option<TabBarHit>,
    palette: Palette,
    /// Every theme to pick from, and the one in use (`config.theme`).
    themes: Themes,
    theme: Theme,
    /// Installed font families, for the settings.
    fonts: FontFamilies,
    /// The window is blurred behind right now (`blur`, or winit's KWin blur).
    blurred: bool,
    /// Running on X11, where a window can't become see-through later on.
    x11: bool,
    proxy: EventLoopProxy<UserEvent>,
    config: Config,

    ui: UiLayer,
    sidebar: Sidebar,
    /// The settings tab's page; kept while the tab is closed.
    settings: SettingsPanel,
    sidebar_visible: bool,
    /// The user last clicked into the sidebar or the settings page, so
    /// the keyboard may go to egui there. Otherwise it belongs to the terminal alone: egui never
    /// sees key presses then and any focus it holds is dropped -- else a
    /// Tab typed for the shell would move egui's focus into the sidebar
    /// (egui's keyboard navigation), swallowing all further input and
    /// turning Enter into a click on the focused sidebar element.
    keyboard_to_ui: bool,
    /// Physical-pixel width the sidebar takes up (0 while hidden); the
    /// console starts right of it. Corrected from egui's actual layout
    /// after every UI pass.
    sidebar_width: f32,
    /// When egui next wants to run without new input (animations, a
    /// blinking text cursor); folded into the event loop's `WaitUntil`.
    ui_repaint_at: Option<Instant>,
    /// Startup splash, until it's over or dismissed.
    splash: Option<Splash>,
    /// When the splash's picture changes next (GIF frame, fade step).
    splash_redraw_at: Option<Instant>,
    /// Right-click menu over the grid, while it's open.
    context_menu: Option<ContextMenu>,
    /// This machine's name, to tell its working directories from others'.
    hostname: String,
    /// The window has the keyboard focus.
    focused: bool,
    /// Exit codes next to the prompts of failed commands.
    prompt_labels: Labels,
    links: links::Finder,
    /// The link under the mouse while Ctrl is held.
    link: Option<links::Link>,
    /// Search in the active tab's scrollback, while its bar is open.
    search: Option<Search>,
    search_bar: SearchBar,
    /// Which key combination does what (`[shortcuts]` in the config).
    keymap: Keymap,
    /// Font size change by shortcut, in points on top of the configured
    /// size; not saved (see `zoom`).
    font_zoom: f32,
    /// Scrolled distance not yet worth a whole line; see `wheel_lines`.
    scroll_remainder: f32,

    tabs: Vec<Tab>,
    active_tab: usize,
    next_tab_id: usize,

    quads: Vec<QuadInstance>,
    modifiers: ModifiersState,
    cols: usize,
    rows: usize,

    left_button_down: bool,
    last_cursor_pos: (f64, f64),

    cursor_visible: bool,
    next_blink: Instant,

    clipboard: Option<Clipboard>,
}

impl AppState {
    fn new(
        window: Arc<Window>,
        event_loop: &ActiveEventLoop,
        proxy: EventLoopProxy<UserEvent>,
        config: Config,
        connect: Option<SshTarget>,
    ) -> Self {
        let gpu = pollster::block_on(GpuState::new(window.clone(), event_loop));
        let themes = Themes::load();
        let theme = themes.get(config.theme.as_deref()).clone();

        // Everything downstream (surface size, buffer bounds, cell math)
        // is kept in *physical* pixels, so the font size we hand to
        // cosmic-text needs to be scaled up from the logical size in
        // `config` too, or text would come out too small on HiDPI panels.
        let scale_factor = window.scale_factor() as f32;
        let physical_font_size = config.font_size * scale_factor;
        let text = TextRendererState::new(
            &gpu.device,
            &gpu.queue,
            gpu.format,
            physical_font_size,
            config.line_height_factor,
            config.font(FontSlot::Terminal),
        );
        let fonts = text.families();
        let quad_renderer = QuadRenderer::new(&gpu.device, gpu.format);
        let ui = UiLayer::new(&window, &gpu.device, gpu.format, &theme.ui);

        let padding = config.padding * scale_factor;
        let tab_bar_height = tab_bar_height(&config, text.cell, scale_factor);
        // A first guess; the first UI pass measures the real width.
        let sidebar_width = if config.sidebar { config.sidebar_width * scale_factor } else { 0.0 };
        let size = window.inner_size();
        let (cols, rows) = grid_size(size.width, size.height, padding, (sidebar_width, tab_bar_height), text.cell);

        quad_renderer.resize(&gpu.queue, size.width as f32, size.height as f32);

        let clipboard = Clipboard::new()
            .inspect_err(|err| log::warn!("system clipboard unavailable: {err}"))
            .ok();

        let next_blink = Instant::now() + Duration::from_millis(config.cursor_blink_interval_ms);
        let blur = Blur::new(&window);
        let x11 = matches!(
            window.display_handle().map(|handle| handle.as_raw()),
            Ok(RawDisplayHandle::Xlib(_) | RawDisplayHandle::Xcb(_))
        );
        let palette = Palette::new(&theme.terminal);
        let default_shell = shells::default_shell(config.shell.as_deref());
        let splash = if config.splash { spawn_splash_decoder(proxy.clone(), scale_factor) } else { None };

        let ui_colors = theme.ui;
        let mut state = Self {
            window,
            gpu,
            quad_renderer,
            text,
            grid_text: GridText::default(),
            // Opaque until `apply_transparency` below.
            tab_bar: TabBar::new(palette.named(NamedColor::Background), theme.ui, 1.0),
            tab_bar_height,
            hovered: None,
            palette,
            themes,
            theme,
            fonts,
            blur,
            blurred: false,
            x11,
            proxy,
            ui,
            sidebar: Sidebar::new(&default_shell),
            settings: SettingsPanel::default(),
            sidebar_visible: config.sidebar,
            keyboard_to_ui: false,
            sidebar_width,
            ui_repaint_at: None,
            splash,
            splash_redraw_at: None,
            context_menu: None,
            hostname: hostname(),
            focused: true,
            prompt_labels: Labels::default(),
            links: links::Finder::default(),
            link: None,
            search: None,
            search_bar: SearchBar::new(ui_colors),
            keymap: Keymap::new(&config.shortcuts),
            font_zoom: 0.0,
            scroll_remainder: 0.0,
            config,
            tabs: Vec::new(),
            active_tab: 0,
            next_tab_id: 0,
            quads: Vec::new(),
            modifiers: ModifiersState::empty(),
            cols,
            rows,
            left_button_down: false,
            last_cursor_pos: (0.0, 0.0),
            cursor_visible: true,
            next_blink,
            clipboard,
        };
        state.apply_ui_fonts();
        state.apply_transparency();
        let proxy = state.proxy.clone();
        crate::theme::cosmic::watch(&crate::theme::cosmic::Roots::system(), move || {
            let _ = proxy.send_event(UserEvent::CosmicThemeChanged);
        });
        // The very first tab failing to spawn means we can't do anything
        // useful at all -- that's the same "just crash" behaviour the
        // single-session version had. Later tabs (Ctrl+Shift+T) are
        // handled leniently instead; see `add_tab`.
        match connect {
            Some(target) => state.add_ssh_tab(&target).expect("failed to start SSH connection"),
            None => state.add_tab(&default_shell).expect("failed to spawn initial shell"),
        }
        state
    }

    /// The shell new tabs get unless one is picked explicitly.
    fn default_shell(&self) -> InstalledShell {
        shells::default_shell(self.config.shell.as_deref())
    }

    /// Spawn a new tab running `shell` and switch to it.
    fn add_tab(&mut self, shell: &InstalledShell) -> std::io::Result<()> {
        let id = self.next_tab_id;
        self.next_tab_id += 1;
        let listener = EventProxyListener::new(self.proxy.clone(), id);
        let terminal = TerminalSession::spawn_local_shell(
            listener,
            &launch::launch(shell),
            self.tabs.get(self.active_tab).and_then(|tab| tab.local_cwd(&self.hostname)),
            GridSize { columns: self.cols, screen_lines: self.rows },
            self.text.cell.width,
            self.text.cell.height,
            self.config.scrollback_lines,
        )?;
        self.push_tab(id, TabContent::Terminal(terminal), shell.name.clone());
        Ok(())
    }

    /// Open a tab that connects to `target` over SSH and switch to it.
    /// Connecting plays out in the tab itself (progress, prompts,
    /// errors), so this only fails if the worker thread can't start.
    fn add_ssh_tab(&mut self, target: &SshTarget) -> std::io::Result<()> {
        let id = self.next_tab_id;
        self.next_tab_id += 1;
        let listener = EventProxyListener::new(self.proxy.clone(), id);
        let terminal = TerminalSession::connect_ssh(
            listener,
            target.clone(),
            GridSize { columns: self.cols, screen_lines: self.rows },
            self.text.cell.width,
            self.text.cell.height,
            self.config.scrollback_lines,
        )?;
        self.push_tab(id, TabContent::Terminal(terminal), target.label.clone());
        Ok(())
    }

    fn push_tab(&mut self, id: usize, content: TabContent, title: String) {
        self.tabs.push(Tab {
            id,
            content,
            title: title.clone(),
            default_title: title,
            program_title: None,
            cwd: None,
            command_started: None,
            broadcast: false,
        });
        self.active_tab = self.tabs.len() - 1;
        self.switched_tab();
    }

    /// Switch to the settings tab, opening it if there's none yet.
    fn open_settings(&mut self) {
        match self.tabs.iter().position(Tab::is_settings) {
            Some(idx) => self.select_tab(idx),
            None => {
                let id = self.next_tab_id;
                self.next_tab_id += 1;
                self.push_tab(id, TabContent::Settings, t!("sidebar-settings"));
            }
        }
    }

    /// Another tab is on screen now. The keyboard comes back from egui: a
    /// widget focused in the settings tab mustn't keep it once a shell
    /// is showing -- and a new shell tab is there to type into. Recording
    /// a shortcut ends with leaving the settings tab.
    fn switched_tab(&mut self) {
        self.settings.stop_recording();
        self.search = None;
        self.give_keyboard_to_terminal();
        self.update_window_title();
        self.window.request_redraw();
    }

    fn add_default_tab(&mut self) {
        let shell = self.default_shell();
        if let Err(err) = self.add_tab(&shell) {
            log::error!("failed to open new tab: {err}");
        }
    }

    /// Close the tab at `idx`. Quits the app once the last tab is closed
    /// -- see the comment on `TermEvent::Exit` in `user_event` for why
    /// that's still correct today and what needs to change if it isn't
    /// later.
    fn close_tab(&mut self, idx: usize, event_loop: &ActiveEventLoop) {
        if idx >= self.tabs.len() {
            return;
        }
        self.tabs.remove(idx);
        if self.tabs.is_empty() {
            event_loop.exit();
            return;
        }
        let was_active = idx == self.active_tab;
        if self.active_tab > idx {
            self.active_tab -= 1;
        }
        self.active_tab = self.active_tab.min(self.tabs.len() - 1);
        if was_active {
            self.switched_tab();
        } else {
            self.update_window_title();
            self.window.request_redraw();
        }
    }

    fn next_tab(&mut self) {
        if self.tabs.len() < 2 {
            return;
        }
        self.active_tab = (self.active_tab + 1) % self.tabs.len();
        self.switched_tab();
    }

    fn prev_tab(&mut self) {
        if self.tabs.len() < 2 {
            return;
        }
        self.active_tab = (self.active_tab + self.tabs.len() - 1) % self.tabs.len();
        self.switched_tab();
    }

    fn select_tab(&mut self, idx: usize) {
        if idx >= self.tabs.len() || idx == self.active_tab {
            return;
        }
        self.active_tab = idx;
        self.switched_tab();
    }

    fn current_tab(&self) -> &Tab {
        &self.tabs[self.active_tab]
    }

    /// The active tab's shell session; `None` on the settings tab.
    fn current_terminal(&self) -> Option<&TerminalSession> {
        self.tabs.get(self.active_tab).and_then(Tab::terminal)
    }

    /// What the sidebar's command buttons should build for: the active
    /// tab's system, with the configured one winning for a local shell.
    /// `None` while the settings tab is showing -- no shell to send to.
    fn command_target(&self) -> Option<Target> {
        let mut target = self.current_terminal()?.command_target();
        target.tabs = self.input_tabs().len();
        if target.host.is_none() && let Some(family) = self.config.system() {
            target.system = Some(System { family, root: target.system.is_some_and(|system| system.root) });
            target.configured = true;
        }
        Some(target)
    }

    /// A built-in command or a snippet, sent like a paste (several lines
    /// arrive as one) and run with `commands_run`.
    fn run_command(&mut self, line: String) {
        let run = self.config.commands_run;
        self.send_to_input_tabs(|mode| input::paste_to_bytes(&line, mode, run));
    }

    fn settings_active(&self) -> bool {
        self.tabs.get(self.active_tab).is_some_and(Tab::is_settings)
    }

    fn toggle_sidebar(&mut self) {
        self.sidebar_visible = !self.sidebar_visible;
        let scale_factor = self.window.scale_factor() as f32;
        self.sidebar_width = if self.sidebar_visible { self.config.sidebar_width * scale_factor } else { 0.0 };
        if !self.sidebar_visible {
            self.give_keyboard_to_terminal();
        }
        self.set_hovered(None);
        self.relayout();
    }

    fn over_sidebar(&self, x: f64) -> bool {
        self.sidebar_visible && x < self.sidebar_width as f64
    }

    /// A physical-pixel position in egui points.
    fn to_points(&self, (x, y): (f64, f64)) -> egui::Pos2 {
        let ppp = self.ui.ctx.pixels_per_point();
        egui::pos2(x as f32 / ppp, y as f32 / ppp)
    }

    fn over_context_menu(&self, pos: (f64, f64)) -> bool {
        self.context_menu.as_ref().is_some_and(|menu| menu.contains(self.to_points(pos)))
    }

    /// The settings page is at `pos`: everything below the tab bar right
    /// of the sidebar, while the settings tab is showing.
    fn over_settings(&self, (x, y): (f64, f64)) -> bool {
        self.settings_active() && !self.over_sidebar(x) && y >= self.tab_bar_height as f64
    }

    /// egui owns the mouse at `pos`: the sidebar, the settings page or the
    /// open context menu.
    fn over_ui(&self, pos: (f64, f64)) -> bool {
        self.over_sidebar(pos.0) || self.over_settings(pos) || self.over_context_menu(pos)
    }

    /// Key presses go to egui rather than the terminal: the user clicked
    /// into the sidebar or the settings page and a widget there has focus.
    fn ui_has_keyboard(&self) -> bool {
        self.keyboard_to_ui && self.ui.wants_keyboard()
    }

    fn give_keyboard_to_terminal(&mut self) {
        if self.keyboard_to_ui {
            self.keyboard_to_ui = false;
            self.ui.release_keyboard();
            self.window.request_redraw();
        }
    }

    /// Top-left corner of the terminal grid in physical pixels: the
    /// configured padding, right of the sidebar and below the tab bar.
    fn grid_origin(&self) -> (f32, f32) {
        let padding = self.config.padding * self.window.scale_factor() as f32;
        (self.sidebar_width + padding, self.tab_bar_height + padding)
    }

    /// `None` when the tab bar is disabled in config.
    fn tab_bar_layout(&self) -> Option<TabBarLayout> {
        (self.tab_bar_height > 0.0).then(|| {
            TabBarLayout::compute(
                self.tabs.len(),
                self.sidebar_width,
                self.gpu.surface_config.width as f32,
                self.text.cell,
                self.window.scale_factor() as f32,
            )
        })
    }

    fn tab_bar_hit(&self, x: f64, y: f64) -> Option<TabBarHit> {
        self.tab_bar_layout()?.hit_test(x as f32, y as f32)
    }

    fn set_hovered(&mut self, hovered: Option<TabBarHit>) {
        if hovered == self.hovered {
            return;
        }
        self.hovered = hovered;
        self.update_cursor_icon();
        self.window.request_redraw();
    }

    fn update_cursor_icon(&self) {
        let pointer = self.hovered.is_some() || self.link.is_some();
        self.window.set_cursor(if pointer { CursorIcon::Pointer } else { CursorIcon::Default });
    }

    /// Find the link under the mouse while Ctrl is held; underlined and
    /// opened by a click.
    fn update_link(&mut self) {
        let (x, y) = self.last_cursor_pos;
        let over_grid = self.modifiers.control_key()
            && !self.left_button_down
            && self.context_menu.is_none()
            && !self.over_ui((x, y))
            && y >= self.tab_bar_height as f64
            && !self.search_bar.contains(x as f32, y as f32);
        let link = over_grid.then(|| self.link_at(x, y)).flatten();
        if link != self.link {
            self.link = link;
            self.update_cursor_icon();
            self.window.request_redraw();
        }
    }

    fn link_at(&mut self, x: f64, y: f64) -> Option<links::Link> {
        let tab = self.tabs.get(self.active_tab)?;
        let terminal = tab.terminal()?;
        let (files, cwd) = (terminal.is_local(), tab.local_cwd(&self.hostname));
        let term = terminal.term.clone();
        let term = term.lock();
        if term.mode().contains(TermMode::ALT_SCREEN) && term.mode().intersects(TermMode::MOUSE_MODE) {
            return None;
        }
        let (col, row, _) = pixel_to_cell(x, y, self.grid_origin(), self.text.cell, self.cols, self.rows);
        let offset = term.grid().display_offset() as i32;
        let point = Point::new(Line(row as i32 - offset), Column(col));
        self.links.at(&term, point, files, cwd.as_deref())
    }

    fn update_window_title(&self) {
        let n = self.tabs.len();
        let title = &self.current_tab().title;
        if n > 1 {
            self.window.set_title(&format!("[{}/{}] {title}", self.active_tab + 1, n));
        } else {
            self.window.set_title(title);
        }
    }

    fn reset_cursor_blink(&mut self) {
        self.cursor_visible = true;
        if self.config.cursor_blink {
            self.next_blink = Instant::now() + Duration::from_millis(self.config.cursor_blink_interval_ms);
        }
    }

    /// The splash's frames arrived. Dropped if the user already typed or
    /// clicked, or if they took so long that the shell is up anyway.
    fn start_splash(&mut self, frames: Result<SplashFrames, String>) {
        let Some(Splash::Loading { since }) = &self.splash else { return };
        let late = since.elapsed() > SPLASH_MAX_WAIT;
        self.splash = match frames {
            Ok(_) if late => {
                log::debug!("splash decoded too late, skipped");
                None
            }
            Ok(frames) => Some(Splash::play(&self.ui.ctx, frames)),
            Err(err) => {
                log::warn!("splash animation: {err}");
                None
            }
        };
        self.window.request_redraw();
    }

    /// End the splash early. Returns whether it was on screen.
    fn dismiss_splash(&mut self) -> bool {
        let Some(splash) = self.splash.take() else { return false };
        self.splash_redraw_at = None;
        self.window.request_redraw();
        splash.is_playing()
    }

    /// Earliest moment something needs a redraw without new input.
    fn next_wakeup(&self) -> Option<Instant> {
        [self.config.cursor_blink.then_some(self.next_blink), self.ui_repaint_at, self.splash_redraw_at]
            .into_iter()
            .flatten()
            .min()
    }

    fn copy_selection(&mut self) {
        let text = self.current_terminal().and_then(|terminal| terminal.term.lock().selection_to_string());
        let (Some(text), Some(clipboard)) = (text, self.clipboard.as_mut()) else { return };
        if let Err(err) = clipboard.set_text(text) {
            log::warn!("failed to set clipboard: {err}");
        }
    }

    /// Type the clipboard's text into the terminal. With `run`, followed
    /// by exactly one Enter, whether or not the text already ended in a
    /// line break.
    fn paste_clipboard(&mut self, run: bool) {
        let Some(clipboard) = self.clipboard.as_mut() else { return };
        let text = match clipboard.get_text() {
            Ok(text) => text,
            Err(err) => return log::warn!("failed to read clipboard: {err}"),
        };
        let Some(term) = self.current_terminal().map(|terminal| terminal.term.clone()) else { return };
        // Into the query while it's being typed.
        if let Some(search) = self.search.as_mut().filter(|search| search.editing()) {
            search.push_str(&mut term.lock(), &text);
            self.window.request_redraw();
            return;
        }
        self.send_to_input_tabs(|mode| input::paste_to_bytes(&text, mode, run));
    }

    /// Input the user typed or pasted. Snaps the view back to the bottom
    /// if they had scrolled into history, and resets the blink cycle so
    /// the cursor doesn't look like it vanished mid-keystroke.
    fn send_typed(&mut self, bytes: Vec<u8>) {
        self.send_to_input_tabs(|_| Some(bytes.clone()));
    }

    /// The tabs typed input goes to: the active one, or while it takes part
    /// in the broadcast, every terminal tab that does.
    fn input_tabs(&self) -> Vec<usize> {
        match self.tabs.get(self.active_tab) {
            Some(tab) if tab.broadcast => {
                (0..self.tabs.len()).filter(|&i| self.tabs[i].broadcast && self.tabs[i].terminal().is_some()).collect()
            }
            Some(tab) if tab.terminal().is_some() => vec![self.active_tab],
            _ => Vec::new(),
        }
    }

    /// Send input to [`AppState::input_tabs`], encoded for each tab's mode
    /// (a paste is bracketed only where the program asked for that).
    fn send_to_input_tabs(&mut self, encode: impl Fn(TermMode) -> Option<Vec<u8>>) {
        self.reset_cursor_blink();
        for idx in self.input_tabs() {
            let Some(terminal) = self.tabs[idx].terminal() else { continue };
            let bytes = {
                let mut term = terminal.term.lock();
                if term.renderable_content().display_offset != 0 {
                    term.scroll_display(Scroll::Bottom);
                }
                encode(*term.mode())
            };
            if let Some(bytes) = bytes {
                terminal.send_input(bytes);
            }
        }
    }

    /// Put the active tab into the broadcast, or take it out.
    fn toggle_broadcast(&mut self) {
        if let Some(tab) = self.tabs.get_mut(self.active_tab).filter(|tab| tab.terminal().is_some()) {
            tab.broadcast = !tab.broadcast;
            self.window.request_redraw();
        }
    }

    /// Open the context menu at the mouse. Whether copying and pasting
    /// are possible is decided now, not while it's open.
    fn open_context_menu(&mut self) {
        let Some(terminal) = self.current_terminal() else { return };
        let can_copy = terminal.term.lock().selection_to_string().is_some_and(|s| !s.is_empty());
        let can_paste = self.clipboard.as_mut().is_some_and(|c| c.get_text().is_ok_and(|text| !text.is_empty()));
        let shortcuts =
            [Action::Copy, Action::Paste, Action::PasteAndRun, Action::ToggleBroadcast].map(|action| self.keymap.label(action));
        let broadcast = self.current_tab().broadcast;
        let pos = self.to_points(self.last_cursor_pos);
        self.context_menu = Some(ContextMenu::new(pos, can_copy, can_paste, broadcast, shortcuts));
        self.set_hovered(None);
        self.window.request_redraw();
    }

    fn close_context_menu(&mut self) {
        if self.context_menu.take().is_some() {
            self.window.request_redraw();
        }
    }

    fn apply_menu_action(&mut self, action: MenuAction) {
        match action {
            MenuAction::Copy => self.copy_selection(),
            MenuAction::Paste => self.paste_clipboard(false),
            MenuAction::PasteAndRun => self.paste_clipboard(true),
            MenuAction::ToggleBroadcast => self.toggle_broadcast(),
        }
    }

    fn handle_keyboard_input(&mut self, event: KeyEvent, event_loop: &ActiveEventLoop) {
        if event.state != ElementState::Pressed {
            return;
        }
        // The settings page waits for a new shortcut: this key is it.
        if self.settings.recording().is_some() {
            self.record_shortcut(&event);
            return;
        }
        // A key closes the context menu -- modifiers aside, they may be
        // the start of Ctrl+Shift+C. Escape is used up by that.
        if self.context_menu.is_some() {
            match &event.logical_key {
                Key::Named(NamedKey::Control | NamedKey::Shift | NamedKey::Alt | NamedKey::Super) => {}
                Key::Named(NamedKey::Escape) => {
                    self.close_context_menu();
                    return;
                }
                _ => self.close_context_menu(),
            }
        }
        // Shortcuts. Those acting on the terminal (copy, paste, scrolling)
        // leave the key to egui while a widget in the sidebar or the
        // settings tab has focus; the others work wherever the focus is.
        if let Some(action) = KeyCombo::from_event(&event, self.modifiers).and_then(|combo| self.keymap.action(&combo))
            && (action.is_global() || !self.ui_has_keyboard())
            && self.run_shortcut(action, event_loop)
        {
            return;
        }

        // Everything below is typing. While a widget in the sidebar or the
        // settings tab has focus it belongs to egui, which already got the
        // event.
        if self.ui_has_keyboard() {
            return;
        }
        if self.search.is_some() && self.search_key(&event) {
            return;
        }

        if let Some(bytes) = input::key_event_to_bytes(&event, self.modifiers) {
            self.send_typed(bytes);
        }
    }

    /// Carry out a shortcut. `false` if it doesn't apply right now and the
    /// key goes on to the terminal instead: keyboard scrolling while a
    /// full-screen program (less, vim) has the screen.
    fn run_shortcut(&mut self, action: Action, event_loop: &ActiveEventLoop) -> bool {
        match action {
            Action::NewTab => self.add_default_tab(),
            Action::CloseTab => self.close_tab(self.active_tab, event_loop),
            Action::NextTab => self.next_tab(),
            Action::PreviousTab => self.prev_tab(),
            Action::SelectTab(number) => self.select_tab(usize::from(number).saturating_sub(1)),
            Action::MoveTabLeft => self.move_tab(-1),
            Action::MoveTabRight => self.move_tab(1),
            Action::ToggleBroadcast => self.toggle_broadcast(),
            Action::ToggleSidebar => self.toggle_sidebar(),
            Action::OpenSettings => self.open_settings(),
            Action::Copy => self.copy_selection(),
            Action::Paste => self.paste_clipboard(false),
            Action::PasteAndRun => self.paste_clipboard(true),
            Action::ScrollPageUp => return self.scroll_by_key(Scroll::PageUp),
            Action::ScrollPageDown => return self.scroll_by_key(Scroll::PageDown),
            Action::ScrollToTop => return self.scroll_by_key(Scroll::Top),
            Action::ScrollToBottom => return self.scroll_by_key(Scroll::Bottom),
            Action::Search => return self.open_search(),
            Action::PreviousPrompt => return self.jump_prompt(true),
            Action::NextPrompt => return self.jump_prompt(false),
            Action::FontBigger => self.zoom(1.0),
            Action::FontSmaller => self.zoom(-1.0),
            Action::FontReset => {
                self.font_zoom = 0.0;
                self.update_font();
            }
        }
        true
    }

    /// Scroll the scrollback from the keyboard. Not on the settings tab,
    /// and not in the alternate screen: a full-screen program has no
    /// scrollback and wants such keys itself.
    fn scroll_by_key(&self, scroll: Scroll) -> bool {
        let Some(terminal) = self.current_terminal() else { return false };
        {
            let mut term = terminal.term.lock();
            if term.mode().contains(TermMode::ALT_SCREEN) {
                return false;
            }
            term.scroll_display(scroll);
        }
        self.window.request_redraw();
        true
    }

    /// Scroll to the prompt above (`older`) or below. Not in the alternate
    /// screen, which has no prompts: the key goes to the program there.
    fn jump_prompt(&self, older: bool) -> bool {
        let Some(terminal) = self.current_terminal() else { return false };
        let mut term = terminal.term.lock();
        if term.mode().contains(TermMode::ALT_SCREEN) {
            return false;
        }
        if prompts::jump(&mut term, older) {
            self.window.request_redraw();
        }
        true
    }

    /// What a tab's shell said about itself.
    fn shell_event(&mut self, tab_id: usize, event: ShellEvent) {
        let Some(idx) = self.tabs.iter().position(|tab| tab.id == tab_id) else { return };
        let tab = &mut self.tabs[idx];
        match event {
            ShellEvent::Cwd { host, path } => {
                tab.cwd = Some((host, path));
                tab.refresh_title(&self.hostname);
                if idx == self.active_tab {
                    self.update_window_title();
                }
                self.window.request_redraw();
            }
            ShellEvent::CommandStarted => tab.command_started = Some(Instant::now()),
            ShellEvent::CommandFinished { exit } => {
                let Some(started) = tab.command_started.take() else { return };
                let elapsed = started.elapsed();
                let threshold = self.config.notify_after_secs;
                let in_view = self.focused && idx == self.active_tab;
                if threshold > 0 && elapsed.as_secs() >= threshold && !in_view {
                    notify(&tab.title, exit, elapsed);
                    self.window.request_user_attention(Some(UserAttentionType::Informational));
                }
            }
        }
    }

    /// Open the search bar over the active tab, or go back to typing the
    /// query if it's open. Not on the settings tab.
    fn open_search(&mut self) -> bool {
        let Some(term) = self.current_terminal().map(|terminal| terminal.term.clone()) else { return false };
        match &mut self.search {
            Some(search) => search.set_editing(true),
            None => self.search = Some(Search::new(&term.lock())),
        }
        self.window.request_redraw();
        true
    }

    /// A key while the search bar is open. Typing the query: text,
    /// Backspace, Enter/Shift+Enter jump up/down and stop typing. After
    /// that n/N jump, / or Backspace types again. Escape closes the bar
    /// either way. `false` for any other key after typing: it closes the
    /// bar and goes on to the terminal.
    fn search_key(&mut self, event: &KeyEvent) -> bool {
        let Some(term) = self.current_terminal().map(|terminal| terminal.term.clone()) else { return false };
        let Some(search) = self.search.as_mut() else { return false };
        let mut term = term.lock();
        let mods = self.modifiers;
        let plain = !(mods.control_key() || mods.alt_key() || mods.super_key());
        let char_key = |c: &str| matches!(&event.logical_key, Key::Character(key) if plain && key == c);
        match &event.logical_key {
            Key::Named(NamedKey::Control | NamedKey::Shift | NamedKey::Alt | NamedKey::Super) => return true,
            Key::Named(NamedKey::Escape) => self.search = None,
            Key::Named(NamedKey::Enter) => {
                search.jump(&mut term, !mods.shift_key());
                search.set_editing(false);
            }
            _ if search.editing() => match (&event.logical_key, &event.text) {
                (Key::Named(NamedKey::Backspace), _) => search.pop(&mut term),
                (_, Some(text)) if plain => search.push_str(&mut term, text),
                _ => {}
            },
            _ if char_key("n") => search.jump(&mut term, true),
            _ if char_key("N") => search.jump(&mut term, false),
            Key::Named(NamedKey::Backspace) => search.set_editing(true),
            _ if char_key("/") => search.set_editing(true),
            _ => {
                self.search = None;
                self.window.request_redraw();
                return false;
            }
        }
        self.window.request_redraw();
        true
    }

    /// Move the active tab one place left (`-1`) or right (`1`).
    fn move_tab(&mut self, by: isize) {
        let Some(to) = self.active_tab.checked_add_signed(by).filter(|&to| to < self.tabs.len()) else { return };
        self.tabs.swap(self.active_tab, to);
        self.active_tab = to;
        self.update_window_title();
        self.window.request_redraw();
    }

    /// Font size by shortcut, `by` points at a time on top of the
    /// configured size -- not saved, like zooming in a browser.
    fn zoom(&mut self, by: f32) {
        let size = (self.font_size() + by).clamp(*config::FONT_SIZES.start(), *config::FONT_SIZES.end());
        self.font_zoom = size - self.config.font_size;
        self.update_font();
    }

    /// A key pressed while the settings page records a shortcut: bound to
    /// the action, unless it's taken, would swallow typing, or is Escape
    /// (cancels).
    fn record_shortcut(&mut self, event: &KeyEvent) {
        // A modifier on its own is only the start of the combination.
        let Some(combo) = KeyCombo::from_event(event, self.modifiers) else { return };
        let Some(action) = self.settings.stop_recording() else { return };
        self.window.request_redraw();
        if combo == KeyCombo::ESCAPE {
            return;
        }
        let label = combo.label();
        if !combo.leaves_typing_alone() {
            self.settings.report(Err(t!("shortcuts-swallows-typing", combo = &label)));
            return;
        }
        match self.keymap.action(&combo) {
            Some(owner) if owner == action => {}
            Some(owner) => self.settings.report(Err(t!("shortcuts-taken", combo = &label, action = owner.label()))),
            None => {
                let mut combos = self.keymap.combos(action).to_vec();
                combos.push(combo);
                self.set_shortcut(action, combos, Origin::Settings);
            }
        }
    }

    fn set_shortcut(&mut self, action: Action, combos: Vec<KeyCombo>, origin: Origin) {
        match self.config.save_shortcut(action, &combos) {
            Ok(()) => self.keymap = Keymap::new(&self.config.shortcuts),
            Err(err) => self.report(origin, Err(err)),
        }
        self.window.request_redraw();
    }

    fn on_cursor_moved(&mut self, position: PhysicalPosition<f64>) {
        self.last_cursor_pos = (position.x, position.y);
        if !self.left_button_down {
            // Over the sidebar or the context menu, egui does the hovering.
            let hit = if self.over_ui(self.last_cursor_pos) { None } else { self.tab_bar_hit(position.x, position.y) };
            self.set_hovered(hit);
            if self.modifiers.control_key() || self.link.is_some() {
                self.update_link();
            }
            return;
        }
        let origin = self.grid_origin();
        let (col, row, side) = pixel_to_cell(position.x, position.y, origin, self.text.cell, self.cols, self.rows);
        if let Some(terminal) = self.current_terminal() {
            let mut term = terminal.term.lock();
            let display_offset = term.renderable_content().display_offset as i32;
            let point = Point::new(Line(row as i32 - display_offset), Column(col));
            if let Some(sel) = term.selection.as_mut() {
                sel.update(point, side);
            }
        }
        self.window.request_redraw();
    }

    /// Clicks on the tab bar: left-click toggles the sidebar, selects a
    /// tab, hits its close button or opens a new tab; middle-click closes
    /// the tab under the mouse. Returns whether the click landed on the
    /// bar at all, so it doesn't also start a text selection.
    fn on_tab_bar_click(&mut self, button: MouseButton, event_loop: &ActiveEventLoop) -> bool {
        let (x, y) = self.last_cursor_pos;
        if y >= self.tab_bar_height as f64 {
            return false;
        }
        match (button, self.tab_bar_hit(x, y)) {
            (MouseButton::Left, Some(TabBarHit::ToggleSidebar)) => self.toggle_sidebar(),
            (MouseButton::Left, Some(TabBarHit::Tab(idx))) => self.select_tab(idx),
            (MouseButton::Left, Some(TabBarHit::Close(idx))) => self.close_tab(idx, event_loop),
            (MouseButton::Middle, Some(TabBarHit::Tab(idx) | TabBarHit::Close(idx))) => {
                self.close_tab(idx, event_loop)
            }
            (MouseButton::Left, Some(TabBarHit::NewTab)) => self.add_default_tab(),
            _ => {}
        }
        // Closing/adding/toggling shifts things around under a stationary
        // mouse, so re-resolve what it's hovering over now.
        if !self.tabs.is_empty() {
            self.set_hovered(self.tab_bar_hit(x, y));
        }
        true
    }

    fn on_mouse_input(&mut self, button: MouseButton, button_state: ElementState, event_loop: &ActiveEventLoop) {
        if button_state == ElementState::Pressed {
            // Presses on the open context menu are egui's; one anywhere
            // else just closes it -- unless it's a right-click, which
            // goes on to open the menu anew there.
            if self.context_menu.is_some() {
                if self.over_context_menu(self.last_cursor_pos) {
                    return;
                }
                self.close_context_menu();
                if button != MouseButton::Right {
                    return;
                }
            }
            // Presses on the sidebar or the settings page are egui's
            // alone; with them the keyboard moves over too, and back with
            // a press anywhere else.
            if self.over_sidebar(self.last_cursor_pos.0) || self.over_settings(self.last_cursor_pos) {
                self.keyboard_to_ui = true;
                return;
            }
            self.give_keyboard_to_terminal();
            if self.on_tab_bar_click(button, event_loop) {
                return;
            }
            // Ctrl+click on a link opens it.
            if button == MouseButton::Left
                && self.modifiers.control_key()
                && let Some(link) = self.link.take()
            {
                open_link(&link.target);
                self.update_cursor_icon();
                self.window.request_redraw();
                return;
            }
            // A click on the search bar types the query again.
            let (x, y) = self.last_cursor_pos;
            if self.search_bar.contains(x as f32, y as f32) {
                if let Some(search) = self.search.as_mut() {
                    search.set_editing(true);
                }
                self.window.request_redraw();
                return;
            }
            if button == MouseButton::Right {
                self.open_context_menu();
                return;
            }
        }
        if button != MouseButton::Left {
            return;
        }
        match button_state {
            ElementState::Pressed => {
                self.left_button_down = true;
                self.set_hovered(None);
                let origin = self.grid_origin();
                let (x, y) = self.last_cursor_pos;
                let (col, row, side) = pixel_to_cell(x, y, origin, self.text.cell, self.cols, self.rows);
                if let Some(terminal) = self.current_terminal() {
                    let mut term = terminal.term.lock();
                    let display_offset = term.renderable_content().display_offset as i32;
                    let point = Point::new(Line(row as i32 - display_offset), Column(col));
                    term.selection = Some(Selection::new(SelectionType::Simple, point, side));
                }
            }
            ElementState::Released => {
                self.left_button_down = false;
            }
        }
        self.window.request_redraw();
    }

    fn on_mouse_wheel(&mut self, delta: MouseScrollDelta) {
        if self.over_ui(self.last_cursor_pos) {
            return;
        }
        let lines = wheel_lines(delta, self.config.scroll_lines(), self.text.cell.height, &mut self.scroll_remainder);
        if lines == 0 {
            return;
        }
        // Full-screen programs (less, htop, ...) may want the wheel
        // themselves, as arrow keys or mouse reports.
        let (x, y) = self.last_cursor_pos;
        let (col, row, _) = pixel_to_cell(x, y, self.grid_origin(), self.text.cell, self.cols, self.rows);
        let Some(terminal) = self.current_terminal() else { return };
        let mode = *terminal.term.lock().mode();
        if let Some(bytes) = input::wheel_to_bytes(lines, mode, (col, row), self.modifiers) {
            if !bytes.is_empty() {
                terminal.send_input(bytes);
            }
            return;
        }
        // Sign convention (positive = further into scrollback) matches
        // `Scroll::Delta`'s own doc; flip this if it turns out inverted
        // on your setup.
        terminal.term.lock().scroll_display(Scroll::Delta(lines));
        self.window.request_redraw();
    }

    fn resize(&mut self, width: u32, height: u32) {
        if width == 0 || height == 0 {
            return;
        }
        self.gpu.resize(width, height);
        self.quad_renderer.resize(&self.gpu.queue, width as f32, height as f32);
        self.relayout();
        if self.blurred {
            self.update_blur();
        }
    }

    /// Recompute the grid size from the window size, sidebar and tab bar,
    /// and tell every tab's shell if it changed.
    fn relayout(&mut self) {
        let padding = self.config.padding * self.window.scale_factor() as f32;
        let (cols, rows) = grid_size(
            self.gpu.surface_config.width,
            self.gpu.surface_config.height,
            padding,
            (self.sidebar_width, self.tab_bar_height),
            self.text.cell,
        );
        if cols != self.cols || rows != self.rows {
            self.cols = cols;
            self.rows = rows;
            // All tabs share the one window/surface, so all of them --
            // not just the active one -- need to know about the new
            // size, or a background tab would present a stale grid size
            // to its shell the moment it becomes active.
            for tab in &mut self.tabs {
                if let TabContent::Terminal(terminal) = &mut tab.content {
                    terminal.resize(
                        GridSize { columns: cols, screen_lines: rows },
                        self.text.cell.width,
                        self.text.cell.height,
                    );
                }
            }
        }
        self.window.request_redraw();
    }

    /// One egui pass. Runs every frame -- with the sidebar hidden too, so
    /// egui's own state (focus, textures) stays consistent.
    fn run_ui(&mut self) {
        let scale_factor = self.window.scale_factor() as f32;
        let default_shell = self.default_shell();
        let visible = self.sidebar_visible;
        let header_height = if self.tab_bar_height > 0.0 { self.tab_bar_height / scale_factor } else { 36.0 };
        let size = self.window.inner_size().to_logical::<f64>(self.window.scale_factor());
        let window_size = [size.width, size.height];
        let settings_active = self.settings_active();
        // The settings page starts below the tab bar.
        let page_top = self.tab_bar_height / scale_factor;

        // Focus egui picked up without the user clicking into the sidebar
        // or the settings page doesn't count; see `keyboard_to_ui`.
        if !self.keyboard_to_ui {
            self.ui.release_keyboard();
        }
        let mut right_edge = 0.0;
        let mut actions = Vec::new();
        let mut settings_actions = Vec::new();
        let splash = self.splash.as_ref();
        let command_target = self.command_target();
        let tab_forwards = self.tabs.get(self.active_tab).and_then(|tab| {
            let forwards = tab.terminal()?.forwards();
            (!forwards.is_empty()).then(|| TabForwards { label: tab.default_title.clone(), forwards })
        });
        let mut splash_next = None;
        let context_menu = &mut self.context_menu;
        let mut menu_action = None;
        let repaint = self.ui.run(&self.window, &self.gpu.device, &self.gpu.queue, |ui| {
            // egui may run this more than once per frame; only the last
            // pass counts.
            (right_edge, actions) = if visible {
                self.sidebar.show(ui, &default_shell, &self.config, command_target.as_ref(), tab_forwards.as_ref(), header_height)
            } else {
                (0.0, Vec::new())
            };
            if settings_active {
                settings_actions.clear();
                let page = egui::Rect::from_min_max(egui::pos2(right_edge, page_top), ui.max_rect().max);
                ui.scope_builder(egui::UiBuilder::new().max_rect(page), |ui| {
                    let view = SettingsView {
                        config: &self.config,
                        shells: self.sidebar.shells(),
                        default: &default_shell,
                        window_size,
                        keymap: &self.keymap,
                        themes: &self.themes,
                        theme: &self.theme,
                        fonts: &self.fonts,
                        default_font: self.text.default_family(),
                        transparency: Transparency {
                            supported: self.gpu.supports_translucency(),
                            blur: self.blur.is_some(),
                            x11: self.x11,
                        },
                    };
                    self.settings.show_tab(ui, &view, &mut settings_actions);
                });
            }
            // A click only registers in the pass that saw it, so keep it
            // over a later one.
            if let Some(menu) = context_menu.as_mut() {
                menu_action = menu.show(ui.ctx()).or(menu_action);
            }
            // Painted last, on egui's foreground layer: over the sidebar
            // as well as the grid and tab bar drawn before egui.
            if let Some(splash) = splash {
                splash_next = splash.show(ui.ctx());
            }
        });
        self.splash_redraw_at = splash_next.map(|next| Instant::now() + next);
        // A playing splash with no next change is over.
        if splash_next.is_none() && self.splash.as_ref().is_some_and(Splash::is_playing) {
            self.splash = None;
        }

        self.ui_repaint_at = None;
        match repaint {
            Some(delay) if delay.is_zero() => self.window.request_redraw(),
            Some(delay) => self.ui_repaint_at = Some(Instant::now() + delay),
            None => {}
        }

        let sidebar_width = right_edge * self.ui.ctx.pixels_per_point();
        if (sidebar_width - self.sidebar_width).abs() >= 1.0 {
            self.sidebar_width = sidebar_width;
            self.relayout();
        }

        for action in actions {
            self.apply_action(action, Origin::Sidebar);
        }
        for action in settings_actions {
            self.apply_action(action, Origin::Settings);
        }
        // The menu is in this frame already; the redraw takes it away.
        if let Some(action) = menu_action {
            self.close_context_menu();
            self.apply_menu_action(action);
        }

        // egui just set the cursor for its own widgets; the tab bar isn't
        // one of them.
        if self.hovered.is_some() {
            self.window.set_cursor(CursorIcon::Pointer);
        }
    }

    fn apply_action(&mut self, action: SidebarAction, origin: Origin) {
        match action {
            SidebarAction::OpenTab(shell) => {
                if let Err(err) = self.add_tab(&shell) {
                    log::error!("failed to open {} tab: {err}", shell.name);
                    self.report(origin, Err(t!("app-shell-start-failed", shell = &shell.name, err = err.to_string())));
                }
            }
            SidebarAction::Connect(target) => {
                if let Err(err) = self.add_ssh_tab(&target) {
                    log::error!("failed to open SSH tab for {}: {err}", target.label);
                    self.report(origin, Err(t!("app-ssh-tab-failed", target = &target.label, err = err.to_string())));
                }
            }
            SidebarAction::SetDefaultShell(shell) => {
                let result = self
                    .config
                    .save_shell(&shell.path)
                    .map(|()| t!("app-default-shell-set", shell = &shell.name));
                self.report(origin, result);
                self.window.request_redraw();
            }
            SidebarAction::SetLanguage(language) => {
                let result = self.config.save_language(language).map(|()| {
                    i18n::set(self.config.language());
                    t!("app-language-changed")
                });
                // Its title would stay in the old language otherwise.
                for tab in self.tabs.iter_mut().filter(|tab| tab.is_settings()) {
                    tab.title = t!("sidebar-settings");
                    tab.default_title = tab.title.clone();
                }
                self.update_window_title();
                self.report(origin, result);
                self.window.request_redraw();
            }
            SidebarAction::ChangeSetting { setting, save } => self.change_setting(setting, save),
            SidebarAction::OpenSettings => self.open_settings(),
            SidebarAction::SetShortcut(action, combos) => self.set_shortcut(action, combos, origin),
            SidebarAction::SetTheme(name) => {
                if let Err(err) = self.config.save_theme(&name) {
                    self.report(origin, Err(err));
                }
                self.apply_theme();
            }
            SidebarAction::ReloadThemes => {
                self.themes = Themes::load();
                self.apply_theme();
                self.report(origin, Ok(t!("settings-themes-reloaded", count = self.themes.all().len())));
            }
            SidebarAction::RunCommand(line) => self.run_command(line),
            SidebarAction::SetForward(index, enabled) => {
                if let Some(terminal) = self.current_terminal() {
                    terminal.set_forward(index, enabled);
                }
            }
            SidebarAction::SetSystem(family) => {
                let result = self.config.save_system(family).map(|()| match family {
                    Some(family) => t!("cmd-system", system = family.label()),
                    None => t!("cmd-system-detect"),
                });
                self.report(origin, result);
            }
            SidebarAction::SetFont(slot, family) => {
                if let Err(err) = self.config.save_font(slot, family.as_deref()) {
                    self.report(origin, Err(err));
                }
                if slot == FontSlot::Terminal {
                    self.text.set_family(self.config.font(FontSlot::Terminal));
                    self.update_font();
                }
                self.apply_ui_fonts();
                self.window.request_redraw();
            }
        }
    }

    /// Switch to the configured theme: the console's palette, the tab bar
    /// and the egui chrome. Rows shaped in the old colors just drop out of
    /// the grid's cache, since their colors are part of its key.
    fn apply_theme(&mut self) {
        self.theme = self.themes.get(self.config.theme.as_deref()).clone();
        self.palette = Palette::new(&self.theme.terminal);
        self.tab_bar = TabBar::new(self.palette.named(NamedColor::Background), self.theme.ui, self.opacity());
        self.search_bar = SearchBar::new(self.theme.ui);
        crate::ui::theme::apply(&self.ui.ctx, &self.theme.ui, self.opacity());
        self.window.request_redraw();
    }

    /// Read the themes again -- the COSMIC desktop's changed -- and apply
    /// the configured one, in case that's it.
    fn reload_themes(&mut self) {
        self.themes = Themes::load();
        self.apply_theme();
    }

    /// How opaque the window's backgrounds are drawn: the configured
    /// opacity, as long as the surface can be see-through at all.
    fn opacity(&self) -> f32 {
        if self.gpu.translucent() { self.config.opacity() } else { 1.0 }
    }

    /// Make the window as see-through as `opacity` says, blurred behind
    /// with `blur` -- as far as driver and compositor allow.
    fn apply_transparency(&mut self) {
        let was = self.gpu.translucent();
        self.gpu.set_translucent(self.config.opacity() < 1.0);
        let translucent = self.gpu.translucent();
        if translucent != was {
            // On Wayland this drops the opaque region, so the compositor
            // blends the window at all. X11 only takes it at creation.
            self.window.set_transparent(translucent);
        }
        let blur = translucent && self.config.blur;
        if blur != self.blurred {
            self.blurred = blur;
            // KWin's blur; `self.blur` is everyone else's.
            self.window.set_blur(blur);
            self.update_blur();
        }
        self.apply_theme();
    }

    /// Tell the compositor what to blur: the whole window, or nothing.
    fn update_blur(&mut self) {
        let size = self.window.inner_size().to_logical::<f64>(self.window.scale_factor());
        let size = (size.width.ceil() as i32, size.height.ceil() as i32);
        if let Some(blur) = &mut self.blur {
            blur.set(self.blurred, size);
        }
    }

    /// Hand egui the chosen menu font, and the chosen console font for its
    /// monospace bits; one left at the default keeps egui's own.
    fn apply_ui_fonts(&self) {
        let face = |family: Option<&str>| {
            let family = family?;
            let face = self.text.face_data(family);
            if face.is_none() {
                log::warn!("font {family:?} is not installed");
            }
            face.map(|(data, index)| FontFace { data, index })
        };
        let (ui, monospace) = (face(self.config.font(FontSlot::Ui)), face(self.config.font(FontSlot::Terminal)));
        crate::ui::theme::set_fonts(&self.ui.ctx, ui, monospace);
    }

    /// Show an action's outcome where it was triggered.
    fn report(&mut self, origin: Origin, result: Result<String, String>) {
        match origin {
            Origin::Sidebar => self.sidebar.report(result),
            Origin::Settings => self.settings.report(result),
        }
    }

    /// An option changed under ⚙: applied right away where it can be,
    /// persisted with `save`. Sliders send one of these per step while
    /// dragged and a saving one once let go.
    fn change_setting(&mut self, setting: Setting, save: bool) {
        self.config.set(setting);
        match setting {
            // A size picked in the settings replaces any zoom.
            Setting::FontSize(_) => {
                self.font_zoom = 0.0;
                self.update_font();
            }
            Setting::LineHeight(_) => self.update_font(),
            Setting::Padding(_) => self.relayout(),
            Setting::TabBar(_) => {
                self.tab_bar_height = tab_bar_height(&self.config, self.text.cell, self.window.scale_factor() as f32);
                self.set_hovered(None);
                self.relayout();
            }
            // The panel gets the new width in the next UI pass, and
            // `run_ui` moves the console along.
            Setting::SidebarWidth(_) => {}
            Setting::CursorBlink(_) | Setting::CursorBlinkInterval(_) => self.reset_cursor_blink(),
            Setting::NotifyAfter(_) => {}
            Setting::Opacity(_) | Setting::Blur(_) => self.apply_transparency(),
            // Only once let go: dragging down and back up would have
            // dropped the oldest lines on the way.
            Setting::ScrollbackLines(lines) if save => {
                for terminal in self.tabs.iter().filter_map(Tab::terminal) {
                    terminal.set_scrollback(lines);
                }
            }
            // Read where they're used, or only at the next start.
            Setting::ScrollbackLines(_)
            | Setting::ScrollLines(_)
            | Setting::WindowSize { .. }
            | Setting::Sidebar(_)
            | Setting::Splash(_)
            | Setting::CommandsRun(_)
            | Setting::CommandsAssumeYes(_)
            | Setting::CommandsWarned(_) => {}
        }
        if save && let Err(err) = self.config.save(setting) {
            self.settings.report(Err(err));
        }
        self.window.request_redraw();
    }

    /// The font size in use: the configured one plus any zoom.
    fn font_size(&self) -> f32 {
        self.config.font_size + self.font_zoom
    }

    /// Font size or line height changed: new cell metrics, so everything
    /// shaped with the old ones goes, and the grid is laid out anew.
    fn update_font(&mut self) {
        let scale_factor = self.window.scale_factor() as f32;
        self.text.set_font(self.font_size() * scale_factor, self.config.line_height_factor);
        self.grid_text = GridText::default();
        self.tab_bar = TabBar::new(self.palette.named(NamedColor::Background), self.theme.ui, self.opacity());
        self.tab_bar_height = tab_bar_height(&self.config, self.text.cell, scale_factor);
        // Forces the resize: the shells learn the new cell size even if
        // the grid keeps its columns and rows.
        self.cols = 0;
        self.relayout();
    }

    /// `✘ code` at the right end of the prompts whose command failed, where
    /// the row has room for it -- never over text.
    fn build_prompt_labels(
        &mut self,
        prompts: &[(usize, Option<i32>)],
        row_lengths: &std::collections::HashMap<usize, usize>,
        geometry: grid::GridGeometry,
    ) {
        self.prompt_labels.clear();
        let cutout = self.search_bar.cutout().map(|(rows, _)| rows);
        let red = self.palette.named(NamedColor::Red);
        let color = glyphon::Color::rgb(red.r, red.g, red.b);
        let cell = geometry.cell;
        for &(row, exit) in prompts {
            let Some(code) = exit.filter(|&code| code != 0) else { continue };
            if cutout.as_ref().is_some_and(|rows| rows.contains(&row)) {
                continue;
            }
            let label = t!("prompt-exit", code = code);
            let len = label.chars().count();
            let used = row_lengths.get(&row).copied().unwrap_or(0);
            let Some(col) = self.cols.checked_sub(len + 1).filter(|&col| col >= used + 2) else { continue };
            let (left, top) = (geometry.origin_x + col as f32 * cell.width, geometry.origin_y + row as f32 * cell.height);
            let clip = LabelRect { x: left, y: top, w: (len + 1) as f32 * cell.width, h: cell.height };
            self.prompt_labels.push(&mut self.text, &label, left, top, clip, color);
        }
    }

    fn redraw(&mut self) {
        let t0 = Instant::now();
        self.run_ui();
        let t_ui = Instant::now();
        let (origin_x, origin_y) = self.grid_origin();
        let geometry = grid::GridGeometry { origin_x, origin_y, cell: self.text.cell };

        // Clone the `Arc` (cheap refcount bump) *before* locking, so the
        // resulting `MutexGuard` doesn't keep an immutable borrow of
        // `self` alive -- locking through `self.current_terminal()`
        // would, and that then collides with the `&mut self.quads`
        // needed right below for `build_frame`. The lock only covers
        // copying the grid out; shaping happens after it's released, so
        // the PTY thread isn't blocked from parsing new output meanwhile.
        let term_arc = self.current_terminal().map(|terminal| terminal.term.clone());
        // The settings tab has no grid; egui paints its page there.
        let show_grid = term_arc.is_some();
        match term_arc {
            Some(term_arc) => {
                let (rows, focus_rows, prompts) = {
                    let term = term_arc.lock();
                    let prompts =
                        if term.mode().contains(TermMode::ALT_SCREEN) { Vec::new() } else { prompts::visible(&term) };
                    let selection_range = term.selection.as_ref().and_then(|s| s.to_range(&term));
                    let matches = self.search.as_mut().map(|search| search.visible_matches(&term)).unwrap_or_default();
                    let focus = self.search.as_ref().and_then(Search::focus);
                    let highlight = grid::Highlights { matches: &matches, focus, link: self.link.as_ref().map(|link| &link.cells) };
                    let offset = term.grid().display_offset() as i32;
                    let focus_rows = focus.map(|m| (m.start().line.0 + offset, m.end().line.0 + offset));
                    let rows = grid::build_frame(
                        &term,
                        selection_range,
                        highlight,
                        self.cursor_visible,
                        &self.palette,
                        &mut self.quads,
                        geometry,
                    );
                    (rows, focus_rows, prompts)
                };
                let row_lengths: std::collections::HashMap<usize, usize> =
                    rows.iter().map(|(row, text)| (*row, text.len())).collect();
                self.grid_text.update(&mut self.text, rows);
                match &self.search {
                    Some(search) => {
                        let prompt = t!("search-prompt");
                        let status = if search.no_match() {
                            t!("search-no-match")
                        } else if search.editing() {
                            t!("search-hint-typing")
                        } else {
                            t!("search-hint-jumping")
                        };
                        let view = SearchBarView {
                            prompt: &prompt,
                            query: search.query(),
                            editing: search.editing(),
                            status: &status,
                            no_match: search.no_match(),
                            focus_rows,
                        };
                        let scale_factor = self.window.scale_factor() as f32;
                        let size = (self.cols, self.rows);
                        self.search_bar.build(geometry, size, &view, scale_factor, &mut self.text, &mut self.quads);
                    }
                    None => self.search_bar.hide(),
                }
                self.build_prompt_labels(&prompts, &row_lengths, geometry);
            }
            None => {
                self.quads.clear();
                self.search_bar.hide();
                self.prompt_labels.clear();
            }
        }

        if let Some(layout) = self.tab_bar_layout() {
            self.tab_bar.build(
                &layout,
                self.tabs.iter().map(|t| (t.title.as_str(), t.broadcast)),
                self.active_tab,
                self.hovered,
                &mut self.text,
                &mut self.quads,
            );
        }

        // A see-through window starts out empty (see the clear color): the
        // console's background is a quad of its own, drawn first, and the
        // sidebar's is egui's -- one under the other would double up.
        let opacity = self.opacity();
        if opacity < 1.0 && show_grid {
            let (width, height) = (self.gpu.surface_config.width as f32, self.gpu.surface_config.height as f32);
            let background = QuadInstance {
                offset: [self.sidebar_width, self.tab_bar_height],
                size: [width - self.sidebar_width, height - self.tab_bar_height],
                color: to_linear(self.palette.named(NamedColor::Background), opacity),
            };
            self.quads.insert(0, background);
        }

        let t_build = Instant::now();
        self.quad_renderer.upload(&self.gpu.device, &self.gpu.queue, &self.quads);

        self.text.viewport.update(
            &self.gpu.queue,
            glyphon::Resolution {
                width: self.gpu.surface_config.width,
                height: self.gpu.surface_config.height,
            },
        );

        let default_fg = self.palette.named(NamedColor::Foreground);
        // Only the console area, so nothing from the grid can bleed into
        // the tab bar or under the sidebar.
        let grid_bounds = glyphon::TextBounds {
            left: self.sidebar_width as i32,
            top: self.tab_bar_height as i32,
            right: self.gpu.surface_config.width as i32,
            bottom: self.gpu.surface_config.height as i32,
        };
        let grid_areas = show_grid
            .then(|| {
                self.grid_text.text_areas(
                    geometry,
                    grid_bounds,
                    self.search_bar.cutout(),
                    glyphon::Color::rgb(default_fg.r, default_fg.g, default_fg.b),
                )
            })
            .into_iter()
            .flatten();

        if let Err(err) = self.text.renderer.prepare(
            &self.gpu.device,
            &self.gpu.queue,
            &mut self.text.font_system,
            &mut self.text.atlas,
            &self.text.viewport,
            grid_areas
                .chain(self.prompt_labels.text_areas())
                .chain(self.search_bar.text_areas())
                .chain(self.tab_bar.text_areas()),
            &mut self.text.swash_cache,
        ) {
            log::error!("glyphon prepare failed: {err:?}");
            return;
        }
        let t_prepare = Instant::now();

        // `Suboptimal` still hands us a presentable frame, so we render and
        // present it like `Success` and only reconfigure afterwards, once
        // `present` has actually consumed the SurfaceTexture. Reconfiguring
        // while it's still alive is what caused an earlier panic.
        let (frame, needs_reconfigure) = match self.gpu.surface.get_current_texture() {
            wgpu::CurrentSurfaceTexture::Success(frame) => (frame, false),
            wgpu::CurrentSurfaceTexture::Suboptimal(frame) => (frame, true),
            wgpu::CurrentSurfaceTexture::Timeout | wgpu::CurrentSurfaceTexture::Occluded => {
                self.window.request_redraw();
                return;
            }
            wgpu::CurrentSurfaceTexture::Outdated => {
                self.gpu.surface.configure(&self.gpu.device, &self.gpu.surface_config);
                self.window.request_redraw();
                return;
            }
            wgpu::CurrentSurfaceTexture::Lost => {
                self.gpu.surface =
                    self.gpu.instance.create_surface(self.window.clone()).expect("recreate wgpu surface");
                self.gpu.surface.configure(&self.gpu.device, &self.gpu.surface_config);
                self.window.request_redraw();
                return;
            }
            wgpu::CurrentSurfaceTexture::Validation => panic!("wgpu surface validation error"),
        };

        let t_acquire = Instant::now();
        let view = frame.texture.create_view(&wgpu::TextureViewDescriptor::default());
        let mut encoder = self
            .gpu
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("frame_encoder") });

        let clear = if opacity < 1.0 { [0.0; 4] } else { to_linear(self.palette.named(NamedColor::Background), 1.0) };
        let [r, g, b, a] = clear.map(f64::from);
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("main_pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color { r, g, b, a }),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });

            self.quad_renderer.render(&mut pass, self.quads.len() as u32);
            self.text.renderer.render(&self.text.atlas, &self.text.viewport, &mut pass).unwrap();
        }

        let size = [self.gpu.surface_config.width, self.gpu.surface_config.height];
        let ui_uploads = self.ui.paint(&self.gpu.device, &self.gpu.queue, &mut encoder, &view, size);

        self.gpu.queue.submit(ui_uploads.into_iter().chain(Some(encoder.finish())));
        // On Wayland this requests a frame callback with the commit that
        // `present` does, and winit then holds back the next
        // `RedrawRequested` until the compositor sends it. A minimized or
        // otherwise hidden window gets no callbacks, so it simply stops
        // drawing -- instead of the next FIFO present blocking the event
        // loop (seen with NVIDIA), which left the window unrestorable.
        self.window.pre_present_notify();
        self.gpu.queue.present(frame);
        self.text.atlas.trim();
        self.ui.free_textures();
        let t_end = Instant::now();
        let ms = |a: Instant, b: Instant| (b - a).as_secs_f64() * 1000.0;
        log::debug!(
            target: "terminaal::timing",
            "frame {:.2}ms: ui {:.2} build {:.2} prepare {:.2} acquire {:.2} submit+present {:.2}",
            ms(t0, t_end),
            ms(t0, t_ui),
            ms(t_ui, t_build),
            ms(t_build, t_prepare),
            ms(t_prepare, t_acquire),
            ms(t_acquire, t_end),
        );

        if needs_reconfigure {
            self.gpu.surface.configure(&self.gpu.device, &self.gpu.surface_config);
            self.window.request_redraw();
        }
    }
}

/// This machine's host name, as shells put it into OSC 7.
fn hostname() -> String {
    let mut buf = [0u8; 256];
    // SAFETY: `buf` is valid for `buf.len()` bytes; gethostname writes at
    // most that many, NUL-terminated unless truncated.
    if unsafe { libc::gethostname(buf.as_mut_ptr().cast(), buf.len()) } != 0 {
        return String::new();
    }
    let len = buf.iter().position(|&b| b == 0).unwrap_or(buf.len());
    String::from_utf8_lossy(&buf[..len]).into_owned()
}

/// A desktop notification that the command in the tab titled `tab`
/// finished, through `notify-send` if it's installed.
fn notify(tab: &str, exit: Option<i32>, elapsed: Duration) {
    let title = match exit {
        Some(code) if code != 0 => t!("notify-failed", code = code),
        _ => t!("notify-finished"),
    };
    let body = t!("notify-body", tab = tab, duration = duration_text(elapsed));
    let spawned = std::process::Command::new("notify-send")
        .args(["--app-name=Terminaal", "--icon=terminaal", "--"])
        .args([title, body])
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn();
    match spawned {
        // Reaped on a thread of its own, so it doesn't linger as a zombie.
        Ok(mut child) => drop(std::thread::spawn(move || child.wait())),
        Err(err) => log::debug!("no desktop notification (notify-send): {err}"),
    }
}

/// Open a link with the desktop's default application (`xdg-open`). An
/// executable file isn't opened -- that could run it -- but the folder
/// it's in.
fn open_link(target: &links::Target) {
    use std::os::unix::fs::PermissionsExt;
    let arg = match target {
        links::Target::Uri(uri) => uri.clone().into(),
        links::Target::Path(path) => {
            let executable = path.metadata().is_ok_and(|meta| meta.is_file() && meta.permissions().mode() & 0o111 != 0);
            match path.parent().filter(|_| executable) {
                Some(dir) => dir.as_os_str().to_owned(),
                None => path.as_os_str().to_owned(),
            }
        }
    };
    let spawned = std::process::Command::new("xdg-open")
        .arg(&arg)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn();
    match spawned {
        Ok(mut child) => drop(std::thread::spawn(move || child.wait())),
        Err(err) => log::warn!("failed to open {arg:?} with xdg-open: {err}"),
    }
}

/// `42 s`, `3 min 5 s`, `2 h 10 min`.
fn duration_text(elapsed: Duration) -> String {
    let secs = elapsed.as_secs();
    match (secs / 3600, secs / 60 % 60, secs % 60) {
        (0, 0, secs) => t!("duration-seconds", secs = secs),
        (0, mins, secs) => t!("duration-minutes", mins = mins, secs = secs),
        (hours, mins, _) => t!("duration-hours", hours = hours, mins = mins),
    }
}

/// Decode the splash on its own thread -- the first tab starts meanwhile;
/// the frames arrive as `UserEvent::SplashReady`.
fn spawn_splash_decoder(proxy: EventLoopProxy<UserEvent>, scale_factor: f32) -> Option<Splash> {
    let side = (splash::MAX_SIDE * scale_factor).round() as u32;
    let spawned = std::thread::Builder::new().name("splash".into()).spawn(move || {
        let started = Instant::now();
        let frames = splash::decode(side);
        log::debug!("splash decoded in {:.0?}", started.elapsed());
        let _ = proxy.send_event(UserEvent::SplashReady(frames));
    });
    match spawned {
        Ok(_) => Some(Splash::Loading { since: Instant::now() }),
        Err(err) => {
            log::warn!("failed to start splash decoder: {err}");
            None
        }
    }
}

/// Physical-pixel height of the tab bar; 0 when it's turned off.
fn tab_bar_height(config: &Config, cell: CellMetrics, scale_factor: f32) -> f32 {
    if config.tab_bar { tab_bar::bar_height(cell, scale_factor) } else { 0.0 }
}

/// `left`/`top` is space reserved beside/above the grid's own padding
/// (the sidebar and the tab bar).
fn grid_size(width: u32, height: u32, padding: f32, (left, top): (f32, f32), cell: CellMetrics) -> (usize, usize) {
    let usable_w = (width as f32 - left - 2.0 * padding).max(cell.width);
    let usable_h = (height as f32 - top - 2.0 * padding).max(cell.height);
    let cols = (usable_w / cell.width).floor().max(1.0) as usize;
    let rows = (usable_h / cell.height).floor().max(1.0) as usize;
    (cols, rows)
}

/// Convert a physical-pixel cursor position into a (col, row, side)
/// triple, clamped to the visible grid. `side` is which half of the
/// cell's width the point falls in -- matters for selection boundaries
/// and matches how upstream Alacritty resolves click positions.
fn pixel_to_cell(
    x: f64,
    y: f64,
    (origin_x, origin_y): (f32, f32),
    cell: CellMetrics,
    cols: usize,
    rows: usize,
) -> (usize, usize, Side) {
    let rel_x = (x as f32 - origin_x).max(0.0);
    let rel_y = (y as f32 - origin_y).max(0.0);
    let col_f = rel_x / cell.width;
    let col = (col_f.floor() as usize).min(cols.saturating_sub(1));
    let row = ((rel_y / cell.height).floor() as usize).min(rows.saturating_sub(1));
    let frac = col_f - col_f.floor();
    let side = if frac < 0.5 { Side::Left } else { Side::Right };
    (col, row, side)
}

/// Lines to scroll for one wheel event (positive: into the scrollback).
/// A mouse wheel moves `lines_per_step` per notch, a touchpad follows its
/// pixels. Fractions add up in `remainder` rather than getting lost --
/// rounded one by one, a touchpad's small steps would never scroll at all.
fn wheel_lines(delta: MouseScrollDelta, lines_per_step: f32, cell_height: f32, remainder: &mut f32) -> i32 {
    let lines = *remainder
        + match delta {
            MouseScrollDelta::LineDelta(_, y) => y * lines_per_step,
            MouseScrollDelta::PixelDelta(pos) => pos.y as f32 / cell_height.max(1.0),
        };
    let whole = lines.trunc();
    *remainder = lines - whole;
    whole as i32
}

impl ApplicationHandler<UserEvent> for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.state.is_some() {
            return;
        }
        let attrs = Window::default_attributes()
            .with_inner_size(LogicalSize::new(self.config.default_width, self.config.default_height))
            .with_title("Terminaal")
            // Deliberately never `with_transparent`: Wayland takes it later
            // (`apply_transparency`), and on X11 -- whose surface is opaque
            // anyway -- a window with an alpha visual crashed Xwayland
            // (abort in Mesa's libgallium, COSMIC), taking every X11 app
            // down with it.
            .with_window_icon(window_icon());
        // Without an app_id (Wayland) / WM_CLASS (X11) the dock can't tell
        // which app the window belongs to: COSMIC matched it to an unrelated
        // desktop entry, and clicking that never restored a minimized window.
        let attrs = WindowAttributesExtWayland::with_name(attrs, APP_ID, APP_ID);
        let attrs = WindowAttributesExtX11::with_name(attrs, APP_ID, APP_ID);
        let window = Arc::new(event_loop.create_window(attrs).expect("failed to create window"));
        self.state = Some(AppState::new(window, event_loop, self.proxy.clone(), self.config.clone(), self.connect.take()));
    }

    /// Fires the scheduled wakeups set in `about_to_wait`: flips the
    /// cursor blink and/or redraws for egui or the splash once their
    /// deadline is reached.
    fn new_events(&mut self, _event_loop: &ActiveEventLoop, cause: StartCause) {
        let Some(state) = &mut self.state else { return };
        if !matches!(cause, StartCause::ResumeTimeReached { .. }) {
            return;
        }
        let now = Instant::now();
        if state.config.cursor_blink && now >= state.next_blink {
            state.cursor_visible = !state.cursor_visible;
            state.next_blink = now + Duration::from_millis(state.config.cursor_blink_interval_ms);
            state.window.request_redraw();
        }
        if state.ui_repaint_at.is_some_and(|at| now >= at) {
            state.ui_repaint_at = None;
            state.window.request_redraw();
        }
        if state.splash_redraw_at.is_some_and(|at| now >= at) {
            state.splash_redraw_at = None;
            state.window.request_redraw();
        }
    }

    /// Runs right before the loop sleeps, after this iteration's redraw
    /// may have scheduled an egui repaint -- so the deadline is set here
    /// rather than in `new_events`.
    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        let Some(state) = &self.state else { return };
        event_loop.set_control_flow(state.next_wakeup().map_or(ControlFlow::Wait, ControlFlow::WaitUntil));
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _window_id: WindowId, event: WindowEvent) {
        let Some(state) = &mut self.state else { return };

        // The splash ends with the first key or click. A click is used up
        // by that (neither egui nor the tab bar below should get it); a
        // key still goes on to the terminal.
        if let WindowEvent::MouseInput { state: ElementState::Pressed, .. } = &event
            && state.dismiss_splash()
        {
            return;
        }
        if let WindowEvent::KeyboardInput { event: key, .. } = &event
            && key.state == ElementState::Pressed
        {
            state.dismiss_splash();
        }

        // egui sees every event first. A mouse move only needs a redraw
        // for egui if it's over (or just left) one of egui's parts -- the
        // sidebar, the settings page, the context menu (`over_ui`) --
        // otherwise every twitch over the terminal would redraw for nothing.
        // egui-winit also flags `RedrawRequested` itself as wanting a
        // repaint; honouring that would schedule the next frame from
        // every frame, redrawing nonstop at the display's refresh rate.
        // Key presses and IME input only reach egui while it has the
        // keyboard; releases always do, so no key stays "held".
        let for_ui = match &event {
            WindowEvent::KeyboardInput { event: key, .. } => {
                key.state == ElementState::Released || state.ui_has_keyboard()
            }
            WindowEvent::Ime(_) => state.ui_has_keyboard(),
            _ => true,
        };
        if for_ui && state.ui.on_window_event(&state.window, &event).repaint {
            let relevant = match &event {
                WindowEvent::CursorMoved { position, .. } => {
                    state.over_ui((position.x, position.y)) || state.over_ui(state.last_cursor_pos)
                }
                WindowEvent::RedrawRequested => false,
                _ => true,
            };
            if relevant {
                state.window.request_redraw();
            }
        }

        match event {
            WindowEvent::CloseRequested => event_loop.exit(),
            WindowEvent::Resized(size) => state.resize(size.width, size.height),
            WindowEvent::Focused(focused) => state.focused = focused,
            WindowEvent::ModifiersChanged(modifiers) => {
                state.modifiers = modifiers.state();
                state.update_link();
            }
            WindowEvent::KeyboardInput { event, .. } => state.handle_keyboard_input(event, event_loop),
            WindowEvent::CursorMoved { position, .. } => state.on_cursor_moved(position),
            WindowEvent::CursorLeft { .. } => state.set_hovered(None),
            WindowEvent::MouseInput { state: button_state, button, .. } => {
                state.on_mouse_input(button, button_state, event_loop);
            }
            WindowEvent::MouseWheel { delta, .. } => state.on_mouse_wheel(delta),
            WindowEvent::RedrawRequested => state.redraw(),
            _ => {}
        }
    }

    fn user_event(&mut self, event_loop: &ActiveEventLoop, event: UserEvent) {
        let Some(state) = &mut self.state else { return };
        let (tab_id, term_event) = match event {
            UserEvent::Terminal(tab_id, term_event) => (tab_id, term_event),
            UserEvent::SplashReady(frames) => {
                state.start_splash(frames);
                return;
            }
            UserEvent::CosmicThemeChanged => {
                state.reload_themes();
                return;
            }
            UserEvent::Shell(tab_id, event) => {
                state.shell_event(tab_id, event);
                return;
            }
        };

        // The event may have been queued before the tab closed (Exit
        // races a trailing Wakeup, for instance) -- nothing to do then.
        let Some(idx) = state.tabs.iter().position(|t| t.id == tab_id) else { return };

        match term_event {
            // Every tab's title is visible in the tab bar, so unlike most
            // events these redraw even when they come from a background tab.
            TermEvent::Title(title) => {
                state.tabs[idx].program_title = Some(title);
                let host = state.hostname.clone();
                state.tabs[idx].refresh_title(&host);
                if idx == state.active_tab {
                    state.update_window_title();
                }
                state.window.request_redraw();
            }
            TermEvent::ResetTitle => {
                state.tabs[idx].program_title = None;
                let host = state.hostname.clone();
                state.tabs[idx].refresh_title(&host);
                if idx == state.active_tab {
                    state.update_window_title();
                }
                state.window.request_redraw();
            }

            // The terminal library asking *us* to write a reply back into
            // the PTY -- `PtyWrite` verbatim, `ColorRequest`/
            // `TextAreaSizeRequest` via a formatter closure called with
            // the actual value, `ClipboardLoad` similarly but from the
            // system clipboard. Routed through `send_input`/the notifier
            // (same path as real keystrokes) rather than written directly
            // from the PTY-reader thread in `listener.rs`, so replies stay
            // in order relative to whatever the user is typing -- upstream
            // Alacritty does this deliberately for the same reason.
            TermEvent::PtyWrite(text) => {
                state.tabs[idx].send_input(text.into_bytes());
            }
            TermEvent::ColorRequest(index, format) => {
                let rgb = state.palette.get(index);
                let response = format(rgb);
                state.tabs[idx].send_input(response.into_bytes());
            }
            TermEvent::TextAreaSizeRequest(format) => {
                let window_size = WindowSize {
                    num_lines: state.rows as u16,
                    num_cols: state.cols as u16,
                    cell_width: state.text.cell.width as u16,
                    cell_height: state.text.cell.height as u16,
                };
                let response = format(window_size);
                state.tabs[idx].send_input(response.into_bytes());
            }
            TermEvent::ClipboardStore(_ty, text) => {
                // Both `ClipboardType` variants (`Clipboard` and the X11
                // "primary selection" `Selection`) go to the same system
                // clipboard for now -- keeping them distinct needs a
                // Linux-specific arboard extension trait, not done here.
                if let Some(clipboard) = state.clipboard.as_mut() {
                    let _ = clipboard.set_text(text);
                }
            }
            TermEvent::ClipboardLoad(_ty, format) => {
                let text = state.clipboard.as_mut().and_then(|c| c.get_text().ok()).unwrap_or_default();
                let response = format(&text);
                state.tabs[idx].send_input(response.into_bytes());
            }

            TermEvent::CursorBlinkingChange => {
                if idx == state.active_tab {
                    state.reset_cursor_blink();
                }
            }

            // The shell exited (Ctrl+D, `exit`, the process dying, ...).
            // Closes that one tab; quits only once it was the last one.
            TermEvent::Exit => {
                state.close_tab(idx, event_loop);
                return;
            }

            TermEvent::ChildExit(_)
            | TermEvent::Wakeup
            | TermEvent::Bell
            | TermEvent::MouseCursorDirty => {}
        }

        // Redrawing only matters if the event's tab is the one on screen.
        if idx == state.active_tab {
            state.window.request_redraw();
        }
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn embedded_window_icon_decodes() {
        assert!(super::window_icon().is_some());
    }

    #[test]
    fn wheel_scrolls_lines_per_notch_and_touchpad_adds_up() {
        use super::wheel_lines;
        use winit::dpi::PhysicalPosition;
        use winit::event::MouseScrollDelta::{LineDelta, PixelDelta};

        let mut rest = 0.0;
        assert_eq!(wheel_lines(LineDelta(0.0, 1.0), 3.0, 20.0, &mut rest), 3);
        assert_eq!(wheel_lines(LineDelta(0.0, -2.0), 3.0, 20.0, &mut rest), -6);

        // 2.5 lines per notch: the half lines carry over.
        assert_eq!(wheel_lines(LineDelta(0.0, 1.0), 2.5, 20.0, &mut rest), 2);
        assert_eq!(wheel_lines(LineDelta(0.0, 1.0), 2.5, 20.0, &mut rest), 3);

        // 5 px steps on 20 px cells: every fourth one scrolls a line.
        let mut rest = 0.0;
        let steps: Vec<i32> =
            (0..8).map(|_| wheel_lines(PixelDelta(PhysicalPosition::new(0.0, 5.0)), 3.0, 20.0, &mut rest)).collect();
        assert_eq!(steps, [0, 0, 0, 1, 0, 0, 0, 1]);
    }
}
