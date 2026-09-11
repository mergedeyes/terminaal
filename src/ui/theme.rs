//! Colors and sizes for the egui chrome, matching the tab bar
//! (`render::tab_bar`) so sidebar and tab bar read as one piece.

use egui::{Color32, CornerRadius, FontFamily, FontId, Stroke, TextStyle, ThemePreference, Visuals, vec2};

pub const BG: Color32 = Color32::from_rgb(22, 22, 22);
pub const ROW_BG: Color32 = Color32::from_rgb(28, 28, 28);
pub const HOVER_BG: Color32 = Color32::from_rgb(36, 36, 36);
pub const SELECTED_BG: Color32 = Color32::from_rgb(33, 41, 54);
pub const BORDER: Color32 = Color32::from_rgb(48, 48, 48);
pub const ACCENT: Color32 = Color32::from_rgb(59, 142, 234);
pub const TEXT: Color32 = Color32::from_rgb(229, 229, 229);
pub const TEXT_WEAK: Color32 = Color32::from_rgb(135, 135, 135);
pub const ERROR: Color32 = Color32::from_rgb(241, 76, 76);
pub const SUCCESS: Color32 = Color32::from_rgb(35, 209, 139);

pub fn apply(ctx: &egui::Context) {
    // Always dark, regardless of the desktop theme egui_winit reports,
    // since the terminal itself is dark too.
    ctx.set_theme(ThemePreference::Dark);

    let mut visuals = Visuals::dark();
    visuals.panel_fill = BG;
    visuals.window_fill = BG;
    visuals.extreme_bg_color = Color32::from_rgb(12, 12, 12);
    visuals.faint_bg_color = ROW_BG;
    visuals.hyperlink_color = ACCENT;
    visuals.error_fg_color = ERROR;
    visuals.selection.bg_fill = Color32::from_rgb(38, 79, 120);
    visuals.selection.stroke = Stroke::new(1.0, TEXT);

    let widgets = &mut visuals.widgets;
    widgets.noninteractive.bg_stroke = Stroke::new(1.0, BORDER);
    widgets.noninteractive.fg_stroke = Stroke::new(1.0, TEXT);
    widgets.inactive.bg_fill = HOVER_BG;
    widgets.inactive.weak_bg_fill = HOVER_BG;
    widgets.inactive.fg_stroke = Stroke::new(1.0, TEXT);
    widgets.hovered.bg_fill = BORDER;
    widgets.hovered.weak_bg_fill = BORDER;
    widgets.hovered.bg_stroke = Stroke::new(1.0, Color32::from_rgb(72, 72, 72));
    widgets.active.bg_fill = ACCENT;
    widgets.active.weak_bg_fill = ACCENT;
    for w in [
        &mut widgets.noninteractive,
        &mut widgets.inactive,
        &mut widgets.hovered,
        &mut widgets.active,
        &mut widgets.open,
    ] {
        w.corner_radius = CornerRadius::same(4);
    }
    ctx.set_visuals_of(egui::Theme::Dark, visuals);

    ctx.all_styles_mut(|style| {
        style.text_styles = [
            (TextStyle::Small, FontId::new(11.0, FontFamily::Proportional)),
            (TextStyle::Body, FontId::new(14.0, FontFamily::Proportional)),
            (TextStyle::Button, FontId::new(14.0, FontFamily::Proportional)),
            (TextStyle::Heading, FontId::new(17.0, FontFamily::Proportional)),
            (TextStyle::Monospace, FontId::new(13.0, FontFamily::Monospace)),
        ]
        .into();
        style.spacing.item_spacing = vec2(8.0, 6.0);
        style.spacing.button_padding = vec2(10.0, 4.0);
    });
}
