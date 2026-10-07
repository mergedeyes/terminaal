//! The question whether a program may read the clipboard (OSC 52 with
//! `?`, `clipboard_read = "ask"`). Like the update dialog it sits at the
//! top of the console, egui only draws it and the choice goes back to
//! `app.rs`. It never takes the keyboard: the request comes from the
//! program, not from the user, and an Enter typed on the way must not
//! answer it.

use egui::{Align2, Area, Button, Frame, Id, Order, Rect, RichText, UiKind};

use crate::i18n::t;
use crate::ui::theme;

/// Widest the dialog gets, in points.
const WIDTH: f32 = 420.0;

/// What was clicked.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Choice {
    /// This once.
    Allow,
    /// This time and every later request of the same terminal.
    AllowTerminal,
    Deny,
}

pub struct ClipboardPrompt {
    /// The asking terminal's title.
    title: String,
    /// Characters in the clipboard right now -- not the text itself: it
    /// may be a password, and the screen may be shared.
    chars: usize,
    /// Where the dialog ended up in the last pass, in points.
    rect: Option<Rect>,
}

impl ClipboardPrompt {
    pub fn new(title: String, chars: usize) -> Self {
        Self { title, chars, rect: None }
    }

    /// `pos` (points) lies on the dialog as last shown.
    pub fn contains(&self, pos: egui::Pos2) -> bool {
        self.rect.is_some_and(|rect| rect.contains(pos))
    }

    /// Show the dialog at the top of `area` (points). Returns a button
    /// clicked in this pass.
    pub fn show(&mut self, ctx: &egui::Context, area: Rect) -> Option<Choice> {
        let colors = theme::colors();
        let width = WIDTH.min(area.width() - 32.0).max(220.0);
        let mut choice = None;
        let response = Area::new(Id::new("clipboard_request"))
            .kind(UiKind::Popup)
            .order(Order::Foreground)
            .pivot(Align2::CENTER_TOP)
            .fixed_pos(egui::pos2(area.center().x, area.top() + 24.0))
            .constrain(true)
            .show(ctx, |ui| {
                Frame::popup(ui.style()).inner_margin(12).show(ui, |ui| {
                    ui.set_width(width);
                    ui.label(RichText::new(t!("clipboard-request-title")).strong().size(15.0));
                    ui.label(t!("clipboard-request-body", title = &self.title, chars = self.chars));
                    ui.add_space(8.0);
                    ui.horizontal_wrapped(|ui| {
                        if ui.add(Button::new(RichText::new(t!("clipboard-request-allow")))).clicked() {
                            choice = Some(Choice::Allow);
                        }
                        if ui.add(Button::new(t!("clipboard-request-allow-terminal"))).clicked() {
                            choice = Some(Choice::AllowTerminal);
                        }
                        if ui.add(Button::new(RichText::new(t!("clipboard-request-deny")).strong())).clicked() {
                            choice = Some(Choice::Deny);
                        }
                    });
                    ui.add_space(4.0);
                    ui.label(RichText::new(t!("clipboard-request-hint")).size(11.0).color(colors.text_weak));
                });
            })
            .response;
        self.rect = Some(response.rect);
        choice
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Draws at the top in the middle; each button reports its choice.
    #[test]
    fn renders_and_reports_each_button() {
        let ctx = egui::Context::default();
        let area = Rect::from_min_size(egui::pos2(0.0, 0.0), egui::vec2(900.0, 600.0));
        let mut prompt = ClipboardPrompt::new("nvim".into(), 12);
        let mut choice = None;
        ctx.run_ui(egui::RawInput::default(), |ui| choice = prompt.show(ui.ctx(), area)).drop_without_applying_deltas();
        assert_eq!(choice, None, "nothing clicked");
        assert!(prompt.contains(egui::pos2(450.0, 40.0)), "at the top in the middle");
        assert!(!prompt.contains(egui::pos2(10.0, 590.0)));

        // Click along the row of buttons, left to right: each one in turn.
        let rect = prompt.rect.unwrap();
        let mut picked = Vec::new();
        for y in (rect.top() as i32..rect.bottom() as i32).step_by(3) {
            for x in (rect.left() as i32..rect.right() as i32).step_by(6) {
                let pos = egui::pos2(x as f32, y as f32);
                let click = |pressed| egui::Event::PointerButton { pos, button: egui::PointerButton::Primary, pressed, modifiers: Default::default() };
                let mut hit = None;
                for _ in 0..2 {
                    let input = egui::RawInput { events: vec![egui::Event::PointerMoved(pos), click(true), click(false)], ..Default::default() };
                    ctx.run_ui(input, |ui| hit = prompt.show(ui.ctx(), area).or(hit)).drop_without_applying_deltas();
                }
                if let Some(hit) = hit
                    && picked.last() != Some(&hit)
                {
                    picked.push(hit);
                }
            }
            if !picked.is_empty() {
                break;
            }
        }
        assert_eq!(picked, [Choice::Allow, Choice::AllowTerminal, Choice::Deny]);
    }
}
