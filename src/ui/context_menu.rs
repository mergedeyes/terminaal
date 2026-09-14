//! Right-click menu over the terminal grid: copy, paste, paste and run,
//! whether the terminal takes part in the broadcast, and splitting or
//! closing its pane.
//!
//! `app.rs` opens it at the mouse (right-click on the grid) and closes it
//! on a pick, a click anywhere else or a key press. It never takes the
//! keyboard -- that stays with the terminal (see `keyboard_to_ui` there).

use egui::{Align, Area, Button, Frame, Id, Layout, Order, Pos2, Rect, TextWrapMode, UiKind};

use crate::i18n::t;

/// Buttons are at least this wide (points), so short labels still leave
/// room for the shortcut beside them.
const MIN_WIDTH: f32 = 200.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MenuAction {
    Copy,
    Paste,
    PasteAndRun,
    ToggleBroadcast,
    SplitRight,
    SplitDown,
    ClosePane,
}

/// Entries in the menu, top to bottom.
const ENTRIES: usize = 7;

pub struct ContextMenu {
    /// Where the right-click happened, in points; the menu's top-left
    /// corner unless that would push it off screen.
    pos: Pos2,
    can_copy: bool,
    can_paste: bool,
    /// The terminal takes part in the broadcast.
    broadcast: bool,
    /// The shortcuts shown beside the entries.
    shortcuts: [Option<String>; ENTRIES],
    /// Where the menu ended up in the last pass, in points.
    rect: Option<Rect>,
}

impl ContextMenu {
    /// `can_copy`: there's a selection; `can_paste`: the clipboard holds
    /// text. Both are decided once, when the menu opens. `shortcuts`:
    /// the combinations for the entries in their order, where bound.
    pub fn new(
        pos: Pos2,
        can_copy: bool,
        can_paste: bool,
        broadcast: bool,
        shortcuts: [Option<String>; ENTRIES],
    ) -> Self {
        Self { pos, can_copy, can_paste, broadcast, shortcuts, rect: None }
    }

    /// `pos` (points) lies on the menu as last shown.
    pub fn contains(&self, pos: Pos2) -> bool {
        self.rect.is_some_and(|rect| rect.contains(pos))
    }

    /// Show the menu; returns the entry clicked in this pass, if any.
    pub fn show(&mut self, ctx: &egui::Context) -> Option<MenuAction> {
        let [copy, paste, paste_and_run, broadcast, split_right, split_down, close_pane] = self.shortcuts.clone();
        let broadcast_label = if self.broadcast { t!("menu-broadcast-off") } else { t!("menu-broadcast-on") };
        let items = [
            (MenuAction::Copy, t!("menu-copy"), copy, self.can_copy),
            (MenuAction::Paste, t!("menu-paste"), paste, self.can_paste),
            (MenuAction::PasteAndRun, t!("menu-paste-run"), paste_and_run, self.can_paste),
            (MenuAction::ToggleBroadcast, broadcast_label, broadcast, true),
            (MenuAction::SplitRight, t!("menu-split-right"), split_right, true),
            (MenuAction::SplitDown, t!("menu-split-down"), split_down, true),
            (MenuAction::ClosePane, t!("menu-close-pane"), close_pane, true),
        ];
        let mut picked = None;
        let response = Area::new(Id::new("context_menu"))
            .kind(UiKind::Menu)
            .order(Order::Foreground)
            .fixed_pos(self.pos)
            .constrain(true)
            .show(ctx, |ui| {
                Frame::menu(ui.style()).show(ui, |ui| {
                    ui.style_mut().wrap_mode = Some(TextWrapMode::Extend);
                    ui.set_min_width(MIN_WIDTH);
                    // Justified: every entry as wide as the menu, shortcuts
                    // flush right.
                    ui.with_layout(Layout::top_down_justified(Align::Min), |ui| {
                        for (action, label, shortcut, enabled) in items {
                            let mut button = Button::new(label).frame_when_inactive(false);
                            if let Some(shortcut) = shortcut {
                                button = button.shortcut_text(shortcut);
                            }
                            if ui.add_enabled(enabled, button).clicked() {
                                picked = Some(action);
                            }
                        }
                    });
                });
            })
            .response;
        self.rect = Some(response.rect);
        picked
    }
}

#[cfg(test)]
mod tests {
    use egui::{PointerButton, RawInput, pos2, vec2};

    use super::*;

    /// One egui pass with `events`; returns what the menu reported.
    fn pass(ctx: &egui::Context, menu: &mut ContextMenu, events: Vec<egui::Event>) -> Option<MenuAction> {
        let input = RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(800.0, 600.0))),
            events,
            ..Default::default()
        };
        let mut picked = None;
        ctx.run_ui(input, |ui| picked = menu.show(ui.ctx()).or(picked)).drop_without_applying_deltas();
        picked
    }

    /// Open a menu, let egui size it, then click at `at` -- a fraction
    /// of the menu's height from its top.
    fn click(can_copy: bool, can_paste: bool, at: f32) -> Option<MenuAction> {
        let ctx = egui::Context::default();
        let mut menu = ContextMenu::new(pos2(100.0, 100.0), can_copy, can_paste, false, Default::default());
        for _ in 0..3 {
            pass(&ctx, &mut menu, Vec::new());
        }
        let rect = menu.rect.expect("menu shown");
        let point = pos2(rect.center().x, rect.top() + rect.height() * at);
        let button = |pressed| egui::Event::PointerButton {
            pos: point,
            button: PointerButton::Primary,
            pressed,
            modifiers: Default::default(),
        };
        [vec![egui::Event::PointerMoved(point)], vec![button(true)], vec![button(false)]]
            .into_iter()
            .fold(None, |picked, events| pass(&ctx, &mut menu, events).or(picked))
    }

    #[test]
    fn clicks_pick_their_entry() {
        let entries = [
            MenuAction::Copy,
            MenuAction::Paste,
            MenuAction::PasteAndRun,
            MenuAction::ToggleBroadcast,
            MenuAction::SplitRight,
            MenuAction::SplitDown,
            MenuAction::ClosePane,
        ];
        for (i, action) in entries.into_iter().enumerate() {
            assert_eq!(click(true, true, entry(i)), Some(action), "entry {i}");
        }
        assert_eq!(click(false, false, entry(3)), Some(MenuAction::ToggleBroadcast));
    }

    /// The middle of entry `i`, as a fraction of the menu's height.
    fn entry(i: usize) -> f32 {
        (2 * i + 1) as f32 / (2 * ENTRIES) as f32
    }

    #[test]
    fn disabled_entries_ignore_clicks() {
        assert_eq!(click(false, true, entry(0)), None);
        assert_eq!(click(true, false, entry(1)), None);
        assert_eq!(click(true, false, entry(2)), None);
    }

    #[test]
    fn contains_follows_the_shown_menu() {
        let ctx = egui::Context::default();
        let mut menu = ContextMenu::new(pos2(100.0, 100.0), true, true, false, Default::default());
        assert!(!menu.contains(pos2(110.0, 110.0)), "nothing shown yet");
        pass(&ctx, &mut menu, Vec::new());
        assert!(menu.contains(pos2(110.0, 110.0)));
        assert!(!menu.contains(pos2(90.0, 90.0)));
    }

    #[test]
    fn stays_on_screen_near_the_corner() {
        let ctx = egui::Context::default();
        let mut menu = ContextMenu::new(pos2(790.0, 590.0), true, true, true, Default::default());
        for _ in 0..3 {
            pass(&ctx, &mut menu, Vec::new());
        }
        let rect = menu.rect.unwrap();
        assert!(rect.right() <= 800.0 && rect.bottom() <= 600.0, "{rect:?}");
    }
}
