//! The egui chrome's look: colors from the current theme (`crate::theme`),
//! the same the tab bar (`render::tab_bar`) uses, so sidebar and tab bar
//! read as one piece -- plus the menu fonts.

use std::sync::{Arc, LazyLock, PoisonError, RwLock};

use alacritty_terminal::vte::ansi::Rgb;
use egui::{
    Color32, CornerRadius, FontData, FontDefinitions, FontFamily, FontId, Stroke, TextStyle, ThemePreference, Visuals,
    vec2,
};

use crate::theme::UiColors;

/// [`UiColors`] as egui colors.
#[derive(Clone, Copy, Debug)]
pub struct Colors {
    pub bg: Color32,
    /// `bg` as see-through as the window: for the sidebar and the settings
    /// page, which fill a whole side of it.
    pub panel: Color32,
    pub row: Color32,
    pub hover: Color32,
    pub selected: Color32,
    pub border: Color32,
    pub border_strong: Color32,
    pub accent: Color32,
    pub text: Color32,
    pub text_weak: Color32,
    pub error: Color32,
    pub success: Color32,
    pub input: Color32,
    pub text_selection: Color32,
}

impl Colors {
    fn new(ui: &UiColors, opacity: f32) -> Self {
        let c = |Rgb { r, g, b }: Rgb| Color32::from_rgb(r, g, b);
        let Rgb { r, g, b } = ui.background;
        Self {
            bg: c(ui.background),
            panel: Color32::from_rgba_unmultiplied(r, g, b, (opacity.clamp(0.0, 1.0) * 255.0).round() as u8),
            row: c(ui.row),
            hover: c(ui.hover),
            selected: c(ui.selected),
            border: c(ui.border),
            border_strong: c(ui.border_strong),
            accent: c(ui.accent),
            text: c(ui.text),
            text_weak: c(ui.text_weak),
            error: c(ui.error),
            success: c(ui.success),
            input: c(ui.input),
            text_selection: c(ui.text_selection),
        }
    }
}

/// The colors [`apply`] set last. Process-wide like the language: only
/// the main thread sets them, and tests never switch away from the default.
static COLORS: LazyLock<RwLock<Colors>> =
    LazyLock::new(|| RwLock::new(Colors::new(&crate::theme::default_theme().ui, 1.0)));

/// The current theme's colors, for what the panels paint themselves.
pub fn colors() -> Colors {
    *COLORS.read().unwrap_or_else(PoisonError::into_inner)
}

/// Switch the chrome to `ui`'s colors: egui's own widgets as well as
/// [`colors`]. `opacity` is the window's (1: opaque).
pub fn apply(ctx: &egui::Context, ui: &UiColors, opacity: f32) {
    let c = Colors::new(ui, opacity);
    *COLORS.write().unwrap_or_else(PoisonError::into_inner) = c;

    // Dark or light going by the theme, regardless of the desktop theme
    // egui_winit reports.
    let (theme, mut visuals) = if ui.is_dark() {
        (egui::Theme::Dark, Visuals::dark())
    } else {
        (egui::Theme::Light, Visuals::light())
    };
    ctx.set_theme(match theme {
        egui::Theme::Dark => ThemePreference::Dark,
        egui::Theme::Light => ThemePreference::Light,
    });

    // Popups and menus stay opaque, only the panels are see-through.
    visuals.panel_fill = c.panel;
    visuals.window_fill = c.bg;
    visuals.window_stroke = Stroke::new(1.0, c.border);
    visuals.extreme_bg_color = c.input;
    visuals.faint_bg_color = c.row;
    visuals.hyperlink_color = c.accent;
    visuals.error_fg_color = c.error;
    visuals.selection.bg_fill = c.text_selection;
    visuals.selection.stroke = Stroke::new(1.0, c.text);

    let widgets = &mut visuals.widgets;
    widgets.noninteractive.bg_fill = c.bg;
    widgets.noninteractive.weak_bg_fill = c.bg;
    widgets.noninteractive.bg_stroke = Stroke::new(1.0, c.border);
    widgets.noninteractive.fg_stroke = Stroke::new(1.0, c.text);
    widgets.inactive.bg_fill = c.hover;
    widgets.inactive.weak_bg_fill = c.hover;
    widgets.inactive.fg_stroke = Stroke::new(1.0, c.text);
    widgets.hovered.bg_fill = c.border;
    widgets.hovered.weak_bg_fill = c.border;
    widgets.hovered.bg_stroke = Stroke::new(1.0, c.border_strong);
    widgets.hovered.fg_stroke = Stroke::new(1.5, c.text);
    widgets.active.bg_fill = c.accent;
    widgets.active.weak_bg_fill = c.accent;
    widgets.open.bg_fill = c.hover;
    widgets.open.weak_bg_fill = c.hover;
    widgets.open.fg_stroke = Stroke::new(1.0, c.text);
    for w in [
        &mut widgets.noninteractive,
        &mut widgets.inactive,
        &mut widgets.hovered,
        &mut widgets.active,
        &mut widgets.open,
    ] {
        w.corner_radius = CornerRadius::same(4);
    }
    ctx.set_visuals_of(theme, visuals);

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

/// A font file's bytes and the index of the face in it.
pub struct FontFace {
    pub data: Vec<u8>,
    pub index: u32,
}

/// Draw the chrome's text in `proportional` and its monospace bits in
/// `monospace`, ahead of egui's own fonts -- those stay as the fallback
/// for glyphs a font lacks. `None` keeps egui's. `fallback` comes after
/// all of them: egui's fonts have no arrows (shortcuts like Ctrl+Alt+↑)
/// or block elements.
pub fn set_fonts(ctx: &egui::Context, proportional: Option<FontFace>, monospace: Option<FontFace>, fallback: Option<FontFace>) {
    let mut fonts = FontDefinitions::default();
    let font = |face: FontFace| Arc::new(FontData { index: face.index, ..FontData::from_owned(face.data) });
    for (family, face) in [(FontFamily::Proportional, proportional), (FontFamily::Monospace, monospace)] {
        let Some(face) = face else { continue };
        let name = format!("terminaal-{family:?}");
        fonts.font_data.insert(name.clone(), font(face));
        fonts.families.entry(family).or_default().insert(0, name);
    }
    if let Some(face) = fallback {
        let name = "terminaal-fallback".to_string();
        fonts.font_data.insert(name.clone(), font(face));
        for family in [FontFamily::Proportional, FontFamily::Monospace] {
            fonts.families.entry(family).or_default().push(name.clone());
        }
    }
    ctx.set_fonts(fonts);
}
