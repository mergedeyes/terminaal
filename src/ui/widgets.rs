//! Small egui building blocks shared by the sidebar sections.

use std::path::Path;
use std::sync::Arc;

use egui::{Align2, Color32, CornerRadius, FontId, Galley, Rect, RichText, Sense, Ui, pos2, vec2};

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
    // Title and subtitle end before the badge.
    let mut right = rect.right() - 10.0;
    if let Some(badge) = badge {
        let badge = painter.text(
            pos2(right, rect.center().y),
            Align2::RIGHT_CENTER,
            badge,
            FontId::proportional(11.0),
            theme::colors().accent,
        );
        right = badge.left() - 8.0;
    }
    let width = (right - x).max(0.0);
    let line = |y: f32, text: &str, font: FontId, color| {
        let galley = elided(painter, text, font, color, width);
        painter.galley(pos2(x, y - galley.size().y / 2.0), galley, color);
    };
    line(rect.top() + 13.0, title, FontId::proportional(14.0), theme::colors().text);
    line(rect.top() + 30.0, subtitle, FontId::monospace(11.0), theme::colors().text_weak);
    response.on_hover_cursor(egui::CursorIcon::PointingHand).on_hover_text(hint)
}

/// `text` laid out in one line no wider than `width`: if it's too wide,
/// with `…` in the middle -- the start of a path and its file name
/// both stay (`C:\WINDOWS\Sys…\powershell.exe`).
fn elided(painter: &egui::Painter, text: &str, font: FontId, color: Color32, width: f32) -> Arc<Galley> {
    let layout = |text: String| painter.layout_no_wrap(text, font.clone(), color);
    let full = layout(text.to_string());
    if full.size().x <= width {
        return full;
    }
    let chars: Vec<char> = text.chars().collect();
    // The most characters (head and tail together) that still fit.
    let keep = |n: usize| {
        let head = n * 2 / 5;
        let mut shortened: String = chars[..head].iter().collect();
        shortened.push('…');
        shortened.extend(&chars[chars.len() - (n - head)..]);
        shortened
    };
    let (mut fits, mut too_many) = (0, chars.len());
    while too_many - fits > 1 {
        let n = (fits + too_many) / 2;
        if layout(keep(n)).size().x <= width { fits = n } else { too_many = n }
    }
    layout(keep(fits))
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

/// `/home/jan/x` → `~/x` (Windows: `~\x`), for display.
pub fn tilde(path: &Path) -> String {
    let home = crate::platform::home_dir();
    match home.as_deref().and_then(|home| path.strip_prefix(home).ok()) {
        Some(rest) => format!("~{}{}", std::path::MAIN_SEPARATOR, rest.display()),
        None => path.display().to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn long_text_is_shortened_in_the_middle() {
        let ctx = egui::Context::default();
        let path = r"C:\WINDOWS\System32\WindowsPowerShell\v1.0\powershell.exe";
        let mut texts = Vec::new();
        ctx.run_ui(egui::RawInput::default(), |ui| {
            for width in [1000.0, 200.0, 0.0] {
                let galley = elided(ui.painter(), path, FontId::monospace(11.0), Color32::WHITE, width);
                if width > 0.0 {
                    assert!(galley.size().x <= width, "{width}: {}", galley.size().x);
                }
                texts.push(galley.text().to_string());
            }
        })
        .drop_without_applying_deltas();
        assert_eq!(texts[0], path, "fits: unchanged");
        assert!(texts[1].starts_with(r"C:\W") && texts[1].ends_with("powershell.exe"), "{}", texts[1]);
        assert!(texts[1].contains('…') && texts[1].len() < path.len(), "{}", texts[1]);
        assert_eq!(texts[2], "…", "no room: just the ellipsis");
    }
}
