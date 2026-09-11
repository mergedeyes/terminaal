//! Small egui building blocks shared by the sidebar sections.

use std::path::{Path, PathBuf};

use egui::{Align2, CornerRadius, FontId, Rect, RichText, Sense, Ui, pos2, vec2};

use crate::ui::theme;

pub fn section_title(ui: &mut Ui, text: &str) {
    ui.label(RichText::new(text.to_uppercase()).size(11.0).strong().color(theme::colors().text_weak));
}

pub fn weak(text: impl Into<String>) -> RichText {
    RichText::new(text).color(theme::colors().text_weak)
}

/// One clickable list entry: a title, a monospace subtitle below it and
/// an optional badge on the right.
pub fn list_row(ui: &mut Ui, title: &str, subtitle: &str, selected: bool, badge: Option<&str>, hint: &str) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(vec2(ui.available_width(), 42.0), Sense::click());
    let painter = ui.painter();
    if selected {
        painter.rect_filled(rect, CornerRadius::same(4), theme::colors().selected);
        painter.rect_filled(Rect::from_min_size(rect.min, vec2(3.0, rect.height())), CornerRadius::same(2), theme::colors().accent);
    } else if response.hovered() {
        painter.rect_filled(rect, CornerRadius::same(4), theme::colors().hover);
    }
    let x = rect.left() + 12.0;
    painter.text(pos2(x, rect.top() + 13.0), Align2::LEFT_CENTER, title, FontId::proportional(14.0), theme::colors().text);
    painter.text(pos2(x, rect.top() + 30.0), Align2::LEFT_CENTER, subtitle, FontId::monospace(11.0), theme::colors().text_weak);
    if let Some(badge) = badge {
        painter.text(
            pos2(rect.right() - 10.0, rect.center().y),
            Align2::RIGHT_CENTER,
            badge,
            FontId::proportional(11.0),
            theme::colors().accent,
        );
    }
    response.on_hover_cursor(egui::CursorIcon::PointingHand).on_hover_text(hint)
}

/// The outcome of the last action, shown under a section.
pub struct Status {
    pub text: String,
    pub error: bool,
}

impl Status {
    pub fn from_result(result: Result<String, String>) -> Self {
        match result {
            Ok(text) => Self { text, error: false },
            Err(text) => Self { text, error: true },
        }
    }

    pub fn show(&self, ui: &mut Ui) {
        let color = if self.error { theme::colors().error } else { theme::colors().success };
        ui.label(RichText::new(&self.text).color(color));
    }
}

/// `/home/jan/x` → `~/x`, for display.
pub fn tilde(path: &Path) -> String {
    let home = std::env::var_os("HOME").map(PathBuf::from);
    match home.as_deref().and_then(|home| path.strip_prefix(home).ok()) {
        Some(rest) => format!("~/{}", rest.display()),
        None => path.display().to_string(),
    }
}
