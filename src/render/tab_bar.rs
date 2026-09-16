//! The tab bar across the top of the console: a sidebar toggle, one
//! clickable tab per terminal session, a close button on the
//! active/hovered tab and a "+" button to open a new one.
//!
//! Split into two halves that share one [`TabBarLayout`]:
//!
//! - [`TabBarLayout::compute`] / [`TabBarLayout::hit_test`] is pure
//!   geometry, used both for drawing and by `app.rs` to turn a click or
//!   hover position into a [`TabBarHit`] -- so what's drawn and what's
//!   clickable can never drift apart.
//! - [`TabBar`] turns a layout into draw data: flat rectangles appended
//!   to the same `QuadInstance` list the grid uses, and a handful of labels
//!   (`render::label`) that become extra `TextArea`s in the same `prepare`
//!   call as the terminal text.
//!
//! Everything is in physical pixels, same as the rest of `render/`. The
//! colors are the theme's chrome colors, the same egui's panels use.

use alacritty_terminal::vte::ansi::Rgb;
use glyphon::{Color as TextColor, TextArea};

use crate::render::label::{Labels, Rect};
use crate::render::palette::to_linear;
use crate::render::quad::QuadInstance;
use crate::render::text::{CellMetrics, TextRendererState};
use crate::theme::{UiColors, mix};

/// Widest a single tab gets, in cells; tabs shrink below this once
/// they no longer all fit side by side.
const MAX_TAB_CELLS: f32 = 26.0;
/// Below this width (in cells) a tab drops its close button so the
/// title keeps at least a few characters.
const MIN_CLOSE_TAB_CELLS: f32 = 8.0;

/// A solid quad covering `rect`.
fn quad(rect: Rect, color: Rgb) -> QuadInstance {
    QuadInstance { offset: [rect.x, rect.y], size: [rect.w, rect.h], color: to_linear(color, 1.0) }
}

/// What's under a given point in the tab bar.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TabBarHit {
    ToggleSidebar,
    Tab(usize),
    Close(usize),
    NewTab,
}

/// What a tab shows besides its place.
#[derive(Clone, Copy, Debug)]
pub struct TabLook<'a> {
    pub title: &'a str,
    /// One of its terminals takes part in the broadcast.
    pub broadcast: bool,
    /// Its host's warning color.
    pub accent: Option<Rgb>,
    /// Its terminal's background, where its host has a theme of its own.
    pub background: Option<Rgb>,
    /// What happened in it while in the background, or that one of its
    /// terminals is watched for silence.
    pub activity: Option<Activity>,
}

/// A tab's mark in front of its title, most urgent first.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Activity {
    /// A terminal rang the bell.
    Bell,
    /// A watched terminal has gone quiet.
    Silent,
    /// New output.
    Output,
    /// A terminal is watched for silence, nothing yet.
    Watching,
}

pub struct TabSlot {
    pub rect: Rect,
    /// `None` when the tab is too narrow to fit one.
    pub close: Option<Rect>,
}

pub struct TabBarLayout {
    pub height: f32,
    pub toggle: Rect,
    pub tabs: Vec<TabSlot>,
    pub new_tab: Rect,
    /// The bar spans `left..right` -- everything right of the sidebar.
    left: f32,
    right: f32,
    /// One device pixel, for borders and separators.
    px: f32,
    /// Horizontal inset of a tab's title from its left edge.
    label_pad: f32,
}

/// Total bar height for a given cell size: one text line plus some
/// breathing room above and below.
pub fn bar_height(cell: CellMetrics, scale_factor: f32) -> f32 {
    (cell.height + 14.0 * scale_factor).round()
}

impl TabBarLayout {
    /// `left` is where the bar starts (the sidebar's right edge, or 0).
    pub fn compute(tab_count: usize, left: f32, window_width: f32, cell: CellMetrics, scale_factor: f32) -> Self {
        let height = bar_height(cell, scale_factor);
        let label_pad = (cell.width * 1.2).round();
        let button_w = height;

        let toggle = Rect { x: left, y: 0.0, w: button_w, h: height };
        let tabs_left = left + button_w;
        let max_tab_w = (cell.width * MAX_TAB_CELLS).round();
        let avail = (window_width - tabs_left - button_w).max(0.0);
        let tab_w = if tab_count == 0 { 0.0 } else { (avail / tab_count as f32).floor().min(max_tab_w) };

        let close_size = cell.height.round();
        let tabs = (0..tab_count)
            .map(|i| {
                let rect = Rect { x: tabs_left + i as f32 * tab_w, y: 0.0, w: tab_w, h: height };
                let close = (tab_w >= cell.width * MIN_CLOSE_TAB_CELLS).then(|| Rect {
                    x: rect.x + rect.w - label_pad * 0.5 - close_size,
                    y: ((height - close_size) * 0.5).round(),
                    w: close_size,
                    h: close_size,
                });
                TabSlot { rect, close }
            })
            .collect();

        let new_tab = Rect { x: tabs_left + tab_count as f32 * tab_w, y: 0.0, w: button_w, h: height };

        Self {
            height,
            toggle,
            tabs,
            new_tab,
            left,
            right: window_width,
            px: scale_factor.round().max(1.0),
            label_pad,
        }
    }

    pub fn hit_test(&self, x: f32, y: f32) -> Option<TabBarHit> {
        if y < 0.0 || y >= self.height {
            return None;
        }
        if self.toggle.contains(x, y) {
            return Some(TabBarHit::ToggleSidebar);
        }
        if self.new_tab.contains(x, y) {
            return Some(TabBarHit::NewTab);
        }
        let idx = self.tabs.iter().position(|t| t.rect.contains(x, y))?;
        if self.tabs[idx].close.is_some_and(|c| c.contains(x, y)) {
            return Some(TabBarHit::Close(idx));
        }
        Some(TabBarHit::Tab(idx))
    }
}

/// Draw-side state: colors and the labels of the last build.
pub struct TabBar {
    /// The terminal's default background. The active tab is filled with
    /// it so it visually merges into the terminal content below.
    terminal_bg: Rgb,
    colors: UiColors,
    /// The window's opacity, for the bar's and tabs' backgrounds.
    opacity: f32,
    labels: Labels,
}

impl TabBar {
    pub fn new(terminal_bg: Rgb, colors: UiColors, opacity: f32) -> Self {
        Self { terminal_bg, colors, opacity, labels: Labels::default() }
    }

    /// Append the bar's rectangles to `quads` and re-fill the label
    /// buffers for [`TabBar::text_areas`].
    pub fn build<'t>(
        &mut self,
        layout: &TabBarLayout,
        tabs: impl Iterator<Item = TabLook<'t>>,
        active: usize,
        hovered: Option<TabBarHit>,
        text: &mut TextRendererState,
        quads: &mut Vec<QuadInstance>,
    ) {
        self.labels.clear();
        let (h, px) = (layout.height, layout.px);
        let cell = text.cell;
        let (c, op) = (self.colors, self.opacity);
        // Backgrounds are as see-through as the window; lines, icons and
        // text stay solid.
        let fill = |rect: Rect, color: Rgb| QuadInstance {
            offset: [rect.x, rect.y],
            size: [rect.w, rect.h],
            color: to_linear(color, op),
        };
        let text_color = |Rgb { r, g, b }: Rgb| TextColor::rgb(r, g, b);
        let text_active = text_color(c.text);
        let text_hover = text_color(mix(c.text_weak, c.text, 0.7));
        let text_inactive = text_color(c.text_weak);

        // The bar's background and bottom border leave the active tab out:
        // under its own see-through background they'd show through.
        let spans = match layout.tabs.get(active) {
            Some(slot) => [(layout.left, slot.rect.x), (slot.rect.x + slot.rect.w, layout.right)],
            None => [(layout.left, layout.right), (0.0, 0.0)],
        };
        for &(x0, x1) in spans.iter().filter(|(x0, x1)| x1 > x0) {
            quads.push(fill(Rect { x: x0, y: 0.0, w: x1 - x0, h }, c.background));
            quads.push(quad(Rect { x: x0, y: h - px, w: x1 - x0, h: px }, c.border));
        }

        let sep_h = (h * 0.5).round();
        let separator = |x: f32| quad(Rect { x: x - px, y: ((h - sep_h) * 0.5).round(), w: px, h: sep_h }, c.border);

        // Sidebar toggle: a hamburger icon drawn from three quads, so it
        // doesn't depend on the monospace font having the glyph.
        let t = layout.toggle;
        let toggle_hovered = hovered == Some(TabBarHit::ToggleSidebar);
        if toggle_hovered {
            quads.push(fill(Rect { h: h - px, ..t }, c.hover));
        }
        let (line_w, gap, thick) = ((h * 0.4).round(), (h * 0.14).round(), px.max((h * 0.05).round()));
        let line_x = (t.x + (t.w - line_w) * 0.5).round();
        let line_y = (t.y + (h - thick) * 0.5).round();
        for dy in [-gap, 0.0, gap] {
            let icon = if toggle_hovered { c.text } else { c.text_weak };
            quads.push(quad(Rect { x: line_x, y: line_y + dy, w: line_w, h: thick }, icon));
        }
        if active != 0 {
            quads.push(separator(t.x + t.w));
        }

        let text_top = ((h - text.metrics.line_height) * 0.5).round();

        for (i, (slot, look)) in layout.tabs.iter().zip(tabs).enumerate() {
            let TabLook { title, broadcast, accent, background, activity } = look;
            let r = slot.rect;
            let is_active = i == active;
            let is_hovered = matches!(hovered, Some(TabBarHit::Tab(j) | TabBarHit::Close(j)) if j == i);

            if is_active {
                // Covers the bottom border too, so the active tab opens
                // straight into the terminal below. A marked host's tab is
                // tinted in its color on top of that; in the background,
                // its line alone marks it.
                let background = background.unwrap_or(self.terminal_bg);
                quads.push(fill(r, accent.map_or(background, |accent| mix(background, accent, 0.22))));
            } else if is_hovered {
                quads.push(fill(Rect { h: h - px, ..r }, c.hover));
            }
            // A tab in the broadcast gets a line in the error color on top,
            // thicker than the active tab's -- typing goes further than it
            // seems. A marked host's tab gets one in its color, active or not.
            let line = match (broadcast, accent) {
                (true, _) => Some((c.error, 3.0)),
                (false, Some(accent)) => Some((accent, 3.0)),
                (false, None) => is_active.then_some((c.accent, 2.0)),
            };
            if let Some((color, thickness)) = line {
                quads.push(quad(Rect { x: r.x, y: 0.0, w: r.w, h: thickness * px }, color));
            }
            // Separator on the right edge, unless the neighbour is the
            // active tab (its own background already delimits it).
            if !is_active && i + 1 != active {
                quads.push(separator(r.x + r.w));
            }

            let show_close = is_active || is_hovered;
            let label_right = match slot.close {
                Some(c) if show_close => c.x,
                _ => r.x + r.w - layout.label_pad * 0.5,
            };
            let mut label_left = r.x + layout.label_pad;
            // The mark before the title: a dot, colored by what happened.
            if let Some(activity) = activity {
                let (mark, color) = match activity {
                    Activity::Bell => ("●", c.error),
                    Activity::Silent => ("●", c.success),
                    Activity::Output => ("●", c.accent),
                    Activity::Watching => ("○", c.text_weak),
                };
                let clip = Rect { x: label_left, y: 0.0, w: cell.width * 2.0, h };
                self.labels.push(text, mark, label_left, text_top, clip, text_color(color));
                label_left += cell.width * 2.0;
            }
            let max_chars = ((label_right - label_left) / cell.width).floor().max(0.0) as usize;
            let color = if is_active {
                text_active
            } else if is_hovered || matches!(activity, Some(Activity::Bell | Activity::Silent | Activity::Output)) {
                // Something happened there: easier to read than the others.
                text_hover
            } else {
                text_inactive
            };
            let clip = Rect { x: label_left, y: 0.0, w: (label_right - label_left).max(0.0), h };
            self.labels.push(text, &truncate(title, max_chars), label_left, text_top, clip, color);

            if let Some(close) = slot.close.filter(|_| show_close) {
                let close_hovered = hovered == Some(TabBarHit::Close(i));
                if close_hovered {
                    quads.push(fill(close, c.border_strong));
                }
                let color = if close_hovered { text_active } else { text_inactive };
                let left = (close.x + (close.w - cell.width) * 0.5).round();
                self.labels.push(text, "×", left, text_top, close, color);
            }
        }

        let b = layout.new_tab;
        let new_hovered = hovered == Some(TabBarHit::NewTab);
        if new_hovered {
            quads.push(fill(Rect { h: h - px, ..b }, c.hover));
        }
        let color = if new_hovered { text_active } else { text_inactive };
        let left = (b.x + (b.w - cell.width) * 0.5).round();
        self.labels.push(text, "+", left, text_top, b, color);
    }

    /// The labels built by the last [`TabBar::build`].
    pub fn text_areas(&self) -> impl Iterator<Item = TextArea<'_>> {
        self.labels.text_areas()
    }
}

/// Cut `title` down to at most `max_chars` characters, ending in "…"
/// when anything had to go.
fn truncate(title: &str, max_chars: usize) -> String {
    if title.chars().count() <= max_chars {
        return title.to_string();
    }
    if max_chars == 0 {
        return String::new();
    }
    let mut out: String = title.chars().take(max_chars - 1).collect();
    out.push('…');
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const CELL: CellMetrics = CellMetrics { width: 10.0, height: 20.0 };

    #[test]
    fn hit_test_resolves_every_element() {
        // Bar is 20 + 14 = 34px high: toggle at 0..34, then 3 tabs at the
        // max width (26 cells = 260px), then "+" right after the last.
        let layout = TabBarLayout::compute(3, 0.0, 1000.0, CELL, 1.0);
        assert_eq!(layout.height, 34.0);
        assert_eq!(layout.hit_test(5.0, 10.0), Some(TabBarHit::ToggleSidebar));
        assert_eq!(layout.hit_test(40.0, 10.0), Some(TabBarHit::Tab(0)));
        assert_eq!(layout.hit_test(300.0, 10.0), Some(TabBarHit::Tab(1)));
        let close = layout.tabs[2].close.unwrap();
        assert_eq!(layout.hit_test(close.x + 1.0, close.y + 1.0), Some(TabBarHit::Close(2)));
        assert_eq!(layout.hit_test(820.0, 10.0), Some(TabBarHit::NewTab));
        assert_eq!(layout.hit_test(900.0, 10.0), None);
        assert_eq!(layout.hit_test(40.0, 34.0), None);
    }

    #[test]
    fn bar_starts_right_of_the_sidebar() {
        let layout = TabBarLayout::compute(1, 200.0, 1000.0, CELL, 1.0);
        assert_eq!(layout.hit_test(100.0, 10.0), None);
        assert_eq!(layout.hit_test(205.0, 10.0), Some(TabBarHit::ToggleSidebar));
        assert_eq!(layout.hit_test(240.0, 10.0), Some(TabBarHit::Tab(0)));
    }

    #[test]
    fn narrow_tabs_shrink_and_drop_close_button() {
        let layout = TabBarLayout::compute(10, 0.0, 368.0, CELL, 1.0);
        assert_eq!(layout.tabs[0].rect.w, 30.0);
        assert!(layout.tabs.iter().all(|t| t.close.is_none()));
        assert_eq!(layout.new_tab.x, 334.0);
    }

    #[test]
    fn truncate_adds_ellipsis_only_when_needed() {
        assert_eq!(truncate("bash", 4), "bash");
        assert_eq!(truncate("bash", 3), "ba…");
        assert_eq!(truncate("bash", 0), "");
    }
}
