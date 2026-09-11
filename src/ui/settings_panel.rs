//! The settings tab (⚙ in the sidebar, Ctrl+,): every option in
//! config.toml, on pages by topic -- general, appearance, terminal, shell
//! and keyboard shortcuts.
//!
//! Changes apply right away -- sliders already while being dragged, but
//! they're only written to the config once let go. Both are `app.rs`'s job
//! ([`SidebarAction::ChangeSetting`]); this only lays out the widgets. So
//! is recording a shortcut: the page marks which action waits for a key
//! ([`SettingsPanel::recording`]), and `app.rs` catches the next one.

use std::ops::RangeInclusive;

use egui::emath::Numeric;
use egui::{
    Align2, Button, CornerRadius, DragValue, Frame, Margin, Rect, Response, RichText, ScrollArea, Stroke, TextStyle, Ui,
    pos2, vec2,
};

use crate::config::{self, Config, Setting};
use crate::i18n::{Language, t};
use crate::shells::InstalledShell;
use crate::shortcuts::{Action, Group, KeyCombo, Keymap};
use crate::ui::sidebar::SidebarAction;
use crate::ui::theme;
use crate::ui::widgets::{Status, section_title, weak};

/// Fixed rather than stretching across the whole tab.
const SLIDER_WIDTH: f32 = 200.0;
const SECTION_GAP: f32 = 18.0;

/// What the pages show, lent by `app.rs` for one pass.
pub struct SettingsView<'a> {
    pub config: &'a Config,
    pub shells: &'a [InstalledShell],
    /// The default shell.
    pub default: &'a InstalledShell,
    /// The window's current size in logical pixels.
    pub window_size: [f64; 2],
    pub keymap: &'a Keymap,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
enum Page {
    #[default]
    General,
    Appearance,
    Terminal,
    Shell,
    Shortcuts,
}

impl Page {
    const ALL: [Page; 5] = [Page::General, Page::Appearance, Page::Terminal, Page::Shell, Page::Shortcuts];

    fn label(self) -> String {
        match self {
            Page::General => t!("settings-page-general"),
            Page::Appearance => t!("settings-appearance"),
            Page::Terminal => t!("settings-page-terminal"),
            Page::Shell => t!("settings-page-shell"),
            Page::Shortcuts => t!("settings-page-shortcuts"),
        }
    }
}

#[derive(Default)]
pub struct SettingsPanel {
    page: Page,
    status: Option<Status>,
    /// The sidebar width while its slider is dragged. Only applied once
    /// let go: the tab sits right of the sidebar, so a live change would
    /// move the slider away under the mouse.
    sidebar_width: Option<f32>,
    /// The action the next key press becomes a shortcut for.
    recording: Option<Action>,
}

impl SettingsPanel {
    pub fn report(&mut self, result: Result<String, String>) {
        self.status = Some(Status::from_result(result));
    }

    /// Waiting for a key to bind to this action.
    pub fn recording(&self) -> Option<Action> {
        self.recording
    }

    /// Stop waiting for a key; returns which action it was for.
    pub fn stop_recording(&mut self) -> Option<Action> {
        self.recording.take()
    }

    /// The whole tab: fills `ui`'s max rect (below the tab bar, right of
    /// the sidebar) -- the page tabs on top, the page below them, which
    /// scrolls when it doesn't fit.
    pub fn show_tab(&mut self, ui: &mut Ui, view: &SettingsView, actions: &mut Vec<SidebarAction>) {
        ui.painter().rect_filled(ui.max_rect(), CornerRadius::ZERO, theme::BG);
        Frame::new().inner_margin(Margin { left: 24, right: 24, top: 4, bottom: 0 }).show(ui, |ui| {
            if let Some(page) = page_tabs(ui, self.page) {
                self.page = page;
                self.status = None;
                self.recording = None;
            }
        });
        ScrollArea::vertical().id_salt(("settings-page", self.page)).auto_shrink([false; 2]).show(ui, |ui| {
            Frame::new().inner_margin(Margin::symmetric(24, 16)).show(ui, |ui| self.show(ui, view, actions));
        });
    }

    /// The current page.
    fn show(&mut self, ui: &mut Ui, view: &SettingsView, actions: &mut Vec<SidebarAction>) {
        let before = actions.len();
        ui.spacing_mut().slider_width = SLIDER_WIDTH;
        match self.page {
            Page::General => general(ui, view, actions),
            Page::Appearance => self.appearance(ui, view.config, actions),
            Page::Terminal => terminal(ui, view.config, actions),
            Page::Shell => shell(ui, view, actions),
            Page::Shortcuts => self.shortcuts(ui, view.keymap, actions),
        }

        ui.add_space(SECTION_GAP);
        let note = if self.page == Page::Shortcuts { t!("shortcuts-note") } else { t!("settings-note") };
        ui.label(weak(note).size(11.0));

        if actions.len() > before {
            self.status = None;
        }
        if let Some(status) = &self.status {
            ui.add_space(8.0);
            status.show(ui);
        }
    }

    fn appearance(&mut self, ui: &mut Ui, config: &Config, actions: &mut Vec<SidebarAction>) {
        section_title(ui, &t!("settings-font"));
        let mut size = config.font_size;
        let moved = slider(ui, &t!("settings-font-size"), "font_size", &mut size, config::FONT_SIZES, 1.0, pixels);
        change(actions, moved, Setting::FontSize(size));
        let mut factor = config.line_height_factor;
        let moved = slider(ui, &t!("settings-line-height"), "line_height_factor", &mut factor, 1.0..=2.0, 0.05, |f| {
            t!("settings-line-height-value", factor = format!("{f:.2}"))
        });
        change(actions, moved, Setting::LineHeight(factor));

        ui.add_space(SECTION_GAP);
        section_title(ui, &t!("settings-layout"));
        let mut padding = config.padding;
        let moved = slider(ui, &t!("settings-padding"), "padding", &mut padding, 0.0..=40.0, 1.0, pixels);
        change(actions, moved, Setting::Padding(padding));
        let mut width = self.sidebar_width.unwrap_or(config.sidebar_width);
        // Narrower and the section tabs in the header no longer fit.
        let moved = slider(ui, &t!("settings-sidebar-width"), "sidebar_width", &mut width, 260.0..=600.0, 10.0, pixels);
        match moved {
            Some(false) => self.sidebar_width = Some(width),
            Some(true) => {
                self.sidebar_width = None;
                change(actions, moved, Setting::SidebarWidth(width));
            }
            None => {}
        }
        ui.add_space(4.0);
        let moved = checkbox(ui, t!("settings-tab-bar"), "tab_bar", config.tab_bar);
        change(actions, moved.map(|_| true), Setting::TabBar(moved.unwrap_or(config.tab_bar)));

        ui.add_space(SECTION_GAP);
        section_title(ui, &t!("settings-cursor"));
        let moved = checkbox(ui, t!("settings-cursor-blink"), "cursor_blink", config.cursor_blink);
        change(actions, moved.map(|_| true), Setting::CursorBlink(moved.unwrap_or(config.cursor_blink)));
        let mut interval = config.cursor_blink_interval_ms;
        let moved = ui
            .add_enabled_ui(config.cursor_blink, |ui| {
                slider(ui, &t!("settings-cursor-interval"), "cursor_blink_interval_ms", &mut interval, 100..=2000, 50.0, |ms| {
                    t!("settings-milliseconds", ms = ms)
                })
            })
            .inner;
        change(actions, moved, Setting::CursorBlinkInterval(interval));
    }

    fn shortcuts(&mut self, ui: &mut Ui, keymap: &Keymap, actions: &mut Vec<SidebarAction>) {
        for (i, group) in Group::ALL.into_iter().enumerate() {
            if i > 0 {
                ui.add_space(SECTION_GAP);
            }
            section_title(ui, &group.label());
            egui::Grid::new(("shortcuts", i)).num_columns(2).min_col_width(240.0).spacing(vec2(12.0, 6.0)).show(
                ui,
                |ui| {
                    for action in Action::ALL.into_iter().filter(|action| action.group() == group) {
                        let key = t!("shortcuts-key-hint", key = action.name().into_owned());
                        ui.label(action.label()).on_hover_text(key);
                        ui.horizontal(|ui| self.shortcut_combos(ui, keymap, action, actions));
                        ui.end_row();
                    }
                },
            );
            if group == Group::Font {
                ui.label(weak(t!("shortcuts-font-note")).size(11.0));
            }
        }
    }

    /// One action's combinations, each with a button to remove it, then
    /// one to record another and one back to the defaults.
    fn shortcut_combos(&mut self, ui: &mut Ui, keymap: &Keymap, action: Action, actions: &mut Vec<SidebarAction>) {
        let combos = keymap.combos(action);
        let recording = self.recording == Some(action);
        if combos.is_empty() && !recording {
            ui.label(weak(t!("shortcuts-none")));
        }
        for (i, combo) in combos.iter().enumerate() {
            // Also bound to an action listed earlier, which gets it.
            let taken_by = keymap.action(combo).filter(|owner| *owner != action);
            let mut text = RichText::new(combo.label()).monospace();
            if taken_by.is_some() {
                text = text.strikethrough().color(theme::TEXT_WEAK);
            }
            let chip = Frame::new()
                .fill(theme::HOVER_BG)
                .corner_radius(4)
                .inner_margin(Margin::symmetric(6, 2))
                .show(ui, |ui| ui.label(text))
                .inner;
            if let Some(owner) = taken_by {
                let _ = chip.on_hover_text(t!("shortcuts-shadowed", action = owner.label()));
            }
            let remove = t!("shortcuts-remove-hint", combo = combo.label());
            if ui.small_button("×").on_hover_text(remove).clicked() {
                let mut rest = combos.to_vec();
                rest.remove(i);
                actions.push(SidebarAction::SetShortcut(action, rest));
            }
        }

        let (text, hint) = if recording {
            (RichText::new(t!("shortcuts-press")).color(theme::ACCENT), t!("shortcuts-press-hint"))
        } else {
            (RichText::new("+"), t!("shortcuts-add-hint"))
        };
        if ui.add(Button::new(text).small()).on_hover_text(hint).clicked() {
            self.recording = if recording { None } else { Some(action) };
            self.status = None;
        }

        let defaults = action.defaults();
        if combos != defaults.as_slice() {
            let shown = if defaults.is_empty() {
                t!("shortcuts-none")
            } else {
                defaults.iter().map(KeyCombo::label).collect::<Vec<_>>().join(", ")
            };
            if ui.small_button("↺").on_hover_text(t!("shortcuts-reset-hint", combos = shown)).clicked() {
                actions.push(SidebarAction::SetShortcut(action, defaults));
            }
        }
    }
}

/// The page tabs along the top, underlined like the sidebar's sections.
/// Returns the page clicked.
fn page_tabs(ui: &mut Ui, current: Page) -> Option<Page> {
    let mut picked = None;
    ui.horizontal(|ui| {
        for page in Page::ALL {
            let active = page == current;
            let color = if active { theme::TEXT } else { theme::TEXT_WEAK };
            let button = Button::new(RichText::new(page.label()).color(color)).frame(false).min_size(vec2(0.0, 34.0));
            let response = ui.add(button).on_hover_cursor(egui::CursorIcon::PointingHand);
            if active {
                let rect = response.rect;
                let underline = Rect::from_min_max(pos2(rect.left() + 6.0, rect.bottom() - 2.0), pos2(rect.right() - 6.0, rect.bottom()));
                ui.painter().rect_filled(underline, CornerRadius::ZERO, theme::ACCENT);
            }
            if response.clicked() && !active {
                picked = Some(page);
            }
        }
    });
    let bottom = ui.min_rect().bottom();
    ui.painter().hline(ui.max_rect().x_range(), bottom, Stroke::new(1.0, theme::BORDER));
    picked
}

fn general(ui: &mut Ui, view: &SettingsView, actions: &mut Vec<SidebarAction>) {
    let config = view.config;
    section_title(ui, &t!("settings-language"));
    let language = config.chosen_language();
    let mut choice = language;
    let auto = t!("settings-language-auto", language = Language::from_locale().native_name());
    ui.radio_value(&mut choice, None, auto).on_hover_text(t!("settings-language-auto-hint"));
    for option in Language::ALL {
        ui.radio_value(&mut choice, Some(option), option.native_name());
    }
    if choice != language {
        actions.push(SidebarAction::SetLanguage(choice));
    }

    ui.add_space(SECTION_GAP);
    section_title(ui, &t!("settings-startup"));
    let moved = checkbox(ui, t!("settings-startup-sidebar"), "sidebar", config.sidebar);
    change(actions, moved.map(|_| true), Setting::Sidebar(moved.unwrap_or(config.sidebar)));
    let moved = checkbox(ui, t!("settings-startup-splash"), "splash", config.splash);
    change(actions, moved.map(|_| true), Setting::Splash(moved.unwrap_or(config.splash)));
    ui.add_space(4.0);
    let keys = format!("{}\n{}", key_hint("default_width"), key_hint("default_height"));
    ui.label(t!("settings-window-size")).on_hover_text(keys);
    let (mut width, mut height) = (config.default_width, config.default_height);
    let mut moved = None;
    ui.horizontal(|ui| {
        let drag = |value| DragValue::new(value).range(300.0..=8000.0).speed(5.0).max_decimals(0);
        let w = ui.add(drag(&mut width));
        ui.label("×");
        let h = ui.add(drag(&mut height));
        moved = [changed(&w), changed(&h)].into_iter().flatten().reduce(|a, b| a || b);
    });
    let [current_w, current_h] = view.window_size.map(f64::round);
    let current = t!("settings-window-size-current-hint", width = current_w, height = current_h);
    if ui.button(t!("settings-window-size-current")).on_hover_text(current).clicked() {
        (width, height) = (current_w, current_h);
        moved = Some(true);
    }
    change(actions, moved, Setting::WindowSize { width, height });
    ui.label(weak(t!("settings-startup-note")).size(11.0));
}

fn terminal(ui: &mut Ui, config: &Config, actions: &mut Vec<SidebarAction>) {
    section_title(ui, &t!("settings-scroll"));
    let mut lines = config.scroll_lines();
    let moved = slider(ui, &t!("settings-scroll-speed"), "scroll_lines", &mut lines, 1.0..=20.0, 1.0, |lines| {
        t!("settings-scroll-lines", lines = f64::from(lines))
    });
    change(actions, moved, Setting::ScrollLines(lines));
    ui.label(weak(t!("settings-scroll-speed-hint")).size(11.0));
    ui.add_space(4.0);
    let mut scrollback = config.scrollback_lines;
    let moved = slider(ui, &t!("settings-scrollback"), "scrollback_lines", &mut scrollback, 0..=100_000, 1000.0, |lines| {
        t!("settings-scrollback-lines", lines = lines)
    });
    change(actions, moved, Setting::ScrollbackLines(scrollback));
    ui.label(weak(t!("settings-scrollback-note")).size(11.0));
}

fn shell(ui: &mut Ui, view: &SettingsView, actions: &mut Vec<SidebarAction>) {
    section_title(ui, &t!("settings-shell"));
    let hint = format!("{}\n{}", t!("shells-make-default-hint"), key_hint("shell"));
    egui::ComboBox::from_id_salt("settings-shell")
        .width(SLIDER_WIDTH)
        .selected_text(view.default.name.as_str())
        .show_ui(ui, |ui| {
            for shell in view.shells {
                let row = ui.selectable_label(shell.is(view.default), &shell.name);
                if row.on_hover_text(shell.path.display().to_string()).clicked() && !shell.is(view.default) {
                    actions.push(SidebarAction::SetDefaultShell(shell.clone()));
                }
            }
        })
        .response
        .on_hover_text(hint);
    ui.label(weak(t!("settings-shell-aliases-hint")).size(11.0));
}

fn change(actions: &mut Vec<SidebarAction>, moved: Option<bool>, setting: Setting) {
    if let Some(save) = moved {
        actions.push(SidebarAction::ChangeSetting { setting, save });
    }
}

fn key_hint(key: &str) -> String {
    t!("settings-key-hint", key = key)
}

fn pixels(px: f32) -> String {
    t!("settings-pixels", px = f64::from(px))
}

/// `Some(save)` if a dragged or typed-into widget changed the value:
/// sent on every step, `save` once it's let go (or set without dragging).
fn changed(response: &Response) -> Option<bool> {
    let save = response.drag_stopped() || (response.changed() && !response.dragged());
    (response.changed() || save).then_some(save)
}

/// A named slider for one config key, its value shown above the slider's
/// right end and the key in the name's tooltip. See [`changed`] for the
/// result.
fn slider<N: Numeric>(
    ui: &mut Ui,
    name: &str,
    key: &str,
    value: &mut N,
    range: RangeInclusive<N>,
    step: f64,
    shown: impl Fn(N) -> String,
) -> Option<bool> {
    let label = ui.label(name).on_hover_text(key_hint(key));
    // `Edits`: an out-of-range value from the config file only gets
    // clamped once the user moves the slider, not by merely showing it.
    let slider = egui::Slider::new(value, range)
        .step_by(step)
        .show_value(false)
        .clamping(egui::SliderClamping::Edits);
    let response = ui.add(slider);
    let color = if ui.is_enabled() { theme::TEXT_WEAK } else { theme::BORDER };
    ui.painter().text(
        pos2(response.rect.right(), label.rect.center().y),
        Align2::RIGHT_CENTER,
        shown(*value),
        TextStyle::Body.resolve(ui.style()),
        color,
    );
    changed(&response)
}

/// A checkbox for one config key; `Some(new value)` when clicked.
fn checkbox(ui: &mut Ui, text: String, key: &str, value: bool) -> Option<bool> {
    let mut on = value;
    ui.checkbox(&mut on, text).on_hover_text(key_hint(key)).changed().then_some(on)
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use super::*;

    #[test]
    fn every_page_renders_headless_without_changing_anything() {
        let ctx = egui::Context::default();
        let shell = InstalledShell::new("/bin/bash");
        // Out of range on purpose: showing must not clamp it into a change.
        let config = Config { font_size: 80.0, cursor_blink: false, ..Config::default() };
        let keymap = Keymap::new(&BTreeMap::new());
        let view = SettingsView {
            config: &config,
            shells: std::slice::from_ref(&shell),
            default: &shell,
            window_size: [1000.0, 650.0],
            keymap: &keymap,
        };
        let mut panel = SettingsPanel::default();
        let mut actions = Vec::new();
        for page in Page::ALL {
            panel.page = page;
            for _ in 0..2 {
                ctx.run_ui(egui::RawInput::default(), |ui| panel.show_tab(ui, &view, &mut actions))
                    .drop_without_applying_deltas();
            }
        }
        assert!(actions.is_empty());
    }

    /// Texts egui painted in a pass, with where.
    fn texts(shapes: &[egui::epaint::ClippedShape]) -> Vec<(String, egui::Rect)> {
        shapes
            .iter()
            .filter_map(|clipped| match &clipped.shape {
                egui::Shape::Text(text) => Some((text.galley.text().to_owned(), text.visual_bounding_rect())),
                _ => None,
            })
            .collect()
    }

    /// Hover the font size's name, then run passes only when egui asks
    /// for one -- like `app.rs` does -- until its key shows up.
    #[test]
    fn hovering_a_setting_name_shows_its_key() {
        let ctx = egui::Context::default();
        theme::apply(&ctx);
        let shell = InstalledShell::new("/bin/bash");
        let config = Config::default();
        let keymap = Keymap::new(&BTreeMap::new());
        let view = SettingsView {
            config: &config,
            shells: std::slice::from_ref(&shell),
            default: &shell,
            window_size: [1000.0, 650.0],
            keymap: &keymap,
        };
        let mut panel = SettingsPanel { page: Page::Appearance, ..SettingsPanel::default() };
        let mut actions = Vec::new();
        let mut pass = |time: f64, events: Vec<egui::Event>| {
            let input = egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1000.0, 900.0))),
                time: Some(time),
                events,
                ..Default::default()
            };
            // Placed like `app.rs` does: right of the sidebar, below the tab bar.
            let page = egui::Rect::from_min_max(pos2(300.0, 36.0), pos2(1000.0, 900.0));
            let mut output = ctx.run_ui(input, |ui| {
                ui.scope_builder(egui::UiBuilder::new().max_rect(page), |ui| panel.show_tab(ui, &view, &mut actions));
            });
            let shapes = std::mem::take(&mut output.shapes);
            let delay = output.viewport_output.get(&egui::ViewportId::ROOT).map(|v| v.repaint_delay);
            output.drop_without_applying_deltas();
            (texts(&shapes), delay)
        };
        let (shown, _) = pass(0.0, Vec::new());
        let label = shown.iter().find(|(text, _)| text == &t!("settings-font-size")).expect("label").1;

        let mut time = 1.0;
        let (mut shown, mut delay) = pass(time, vec![egui::Event::PointerMoved(label.center())]);
        // The tooltip waits for the mouse to rest; well within 2 s.
        while time < 3.0 {
            if shown.iter().any(|(text, _)| text == &key_hint("font_size")) {
                return;
            }
            let Some(wait) = delay.filter(|d| d.as_secs() < 3600) else { break };
            time += wait.as_secs_f64().max(1.0 / 60.0);
            (shown, delay) = pass(time, Vec::new());
        }
        panic!("no tooltip; stopped at {time:.3} with delay {delay:?}");
    }
}
