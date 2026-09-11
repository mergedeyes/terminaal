//! The sidebar's settings section (⚙): every option in config.toml.
//!
//! Changes apply right away -- sliders already while being dragged, but
//! they're only written to the config once let go. Both are `app.rs`'s job
//! ([`SidebarAction::ChangeSetting`]); this only lays out the widgets.

use std::ops::RangeInclusive;

use egui::emath::Numeric;
use egui::{Align2, DragValue, Response, TextStyle, Ui, pos2};

use crate::config::{Config, Setting};
use crate::i18n::{Language, t};
use crate::shells::InstalledShell;
use crate::ui::sidebar::SidebarAction;
use crate::ui::theme;
use crate::ui::widgets::{Status, section_title, weak};

/// Fixed rather than following the sidebar: the sidebar-width slider
/// would otherwise stretch under the mouse while being dragged.
const SLIDER_WIDTH: f32 = 200.0;
const SECTION_GAP: f32 = 18.0;

#[derive(Default)]
pub struct SettingsPanel {
    status: Option<Status>,
}

impl SettingsPanel {
    pub fn report(&mut self, result: Result<String, String>) {
        self.status = Some(Status::from_result(result));
    }

    /// `window_size` is the window's current size in logical pixels.
    pub fn show(
        &mut self,
        ui: &mut Ui,
        config: &Config,
        shells: &[InstalledShell],
        default: &InstalledShell,
        window_size: [f64; 2],
        actions: &mut Vec<SidebarAction>,
    ) {
        let before = actions.len();
        ui.spacing_mut().slider_width = SLIDER_WIDTH;

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
        section_title(ui, &t!("settings-appearance"));
        let mut size = config.font_size;
        let moved = slider(ui, &t!("settings-font-size"), "font_size", &mut size, 6.0..=36.0, 1.0, pixels);
        change(actions, moved, Setting::FontSize(size));
        let mut factor = config.line_height_factor;
        let moved = slider(ui, &t!("settings-line-height"), "line_height_factor", &mut factor, 1.0..=2.0, 0.05, |f| {
            t!("settings-line-height-value", factor = format!("{f:.2}"))
        });
        change(actions, moved, Setting::LineHeight(factor));
        let mut padding = config.padding;
        let moved = slider(ui, &t!("settings-padding"), "padding", &mut padding, 0.0..=40.0, 1.0, pixels);
        change(actions, moved, Setting::Padding(padding));
        let mut width = config.sidebar_width;
        // Narrower and the section tabs in the header no longer fit.
        let moved = slider(ui, &t!("settings-sidebar-width"), "sidebar_width", &mut width, 260.0..=600.0, 10.0, pixels);
        change(actions, moved, Setting::SidebarWidth(width));
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

        ui.add_space(SECTION_GAP);
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

        ui.add_space(SECTION_GAP);
        section_title(ui, &t!("settings-shell"));
        let hint = format!("{}\n{}", t!("shells-make-default-hint"), key_hint("shell"));
        egui::ComboBox::from_id_salt("settings-shell")
            .width(SLIDER_WIDTH)
            .selected_text(default.name.as_str())
            .show_ui(ui, |ui| {
                for shell in shells {
                    let row = ui.selectable_label(shell.is(default), &shell.name);
                    if row.on_hover_text(shell.path.display().to_string()).clicked() && !shell.is(default) {
                        actions.push(SidebarAction::SetDefaultShell(shell.clone()));
                    }
                }
            })
            .response
            .on_hover_text(hint);

        ui.add_space(SECTION_GAP);
        section_title(ui, &t!("settings-startup"));
        let moved = checkbox(ui, t!("settings-startup-sidebar"), "sidebar", config.sidebar);
        change(actions, moved.map(|_| true), Setting::Sidebar(moved.unwrap_or(config.sidebar)));
        let moved = checkbox(ui, t!("settings-startup-splash"), "splash", config.splash);
        change(actions, moved.map(|_| true), Setting::Splash(moved.unwrap_or(config.splash)));
        ui.add_space(4.0);
        ui.label(t!("settings-window-size"));
        let (mut width, mut height) = (config.default_width, config.default_height);
        let mut moved = None;
        ui.horizontal(|ui| {
            let drag = |value| DragValue::new(value).range(300.0..=8000.0).speed(5.0).max_decimals(0);
            let w = ui.add(drag(&mut width)).on_hover_text(key_hint("default_width"));
            ui.label("×");
            let h = ui.add(drag(&mut height)).on_hover_text(key_hint("default_height"));
            moved = [changed(&w), changed(&h)].into_iter().flatten().reduce(|a, b| a || b);
        });
        let [current_w, current_h] = window_size.map(f64::round);
        let current = t!("settings-window-size-current-hint", width = current_w, height = current_h);
        if ui.button(t!("settings-window-size-current")).on_hover_text(current).clicked() {
            (width, height) = (current_w, current_h);
            moved = Some(true);
        }
        change(actions, moved, Setting::WindowSize { width, height });
        ui.label(weak(t!("settings-startup-note")).size(11.0));

        ui.add_space(SECTION_GAP);
        ui.label(weak(t!("settings-note")).size(11.0));

        if actions.len() > before {
            self.status = None;
        }
        if let Some(status) = &self.status {
            ui.add_space(8.0);
            status.show(ui);
        }
    }
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
/// right end. See [`changed`] for the result.
fn slider<N: Numeric>(
    ui: &mut Ui,
    name: &str,
    key: &str,
    value: &mut N,
    range: RangeInclusive<N>,
    step: f64,
    shown: impl Fn(N) -> String,
) -> Option<bool> {
    let label = ui.label(name);
    // `Edits`: an out-of-range value from the config file only gets
    // clamped once the user moves the slider, not by merely showing it.
    let slider = egui::Slider::new(value, range)
        .step_by(step)
        .show_value(false)
        .clamping(egui::SliderClamping::Edits);
    let response = ui.add(slider).on_hover_text(key_hint(key));
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
    use super::*;

    #[test]
    fn renders_headless_without_changing_anything() {
        let ctx = egui::Context::default();
        let shell = InstalledShell::new("/bin/bash");
        // Out of range on purpose: showing must not clamp it into a change.
        let config = Config { font_size: 80.0, cursor_blink: false, ..Config::default() };
        let mut panel = SettingsPanel::default();
        let mut actions = Vec::new();
        for _ in 0..2 {
            ctx.run_ui(egui::RawInput::default(), |ui| {
                panel.show(ui, &config, std::slice::from_ref(&shell), &shell, [1000.0, 650.0], &mut actions)
            })
            .drop_without_applying_deltas();
        }
        assert!(actions.is_empty());
    }
}
