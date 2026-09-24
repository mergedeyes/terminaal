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

/// `rect` with the part `cover` lies over cut away: whichever side of it
/// is left over, so a label can't spill out from under the tab that's
/// being dragged across it.
fn cut(rect: Rect, cover: Option<Rect>) -> Rect {
    let Some(cover) = cover else { return rect };
    let (left, right) = (rect.x.max(cover.x), (rect.x + rect.w).min(cover.x + cover.w));
    if right <= left {
        return rect;
    }
    let (before, after) = (left - rect.x, rect.x + rect.w - right);
    if before >= after {
        Rect { w: before, ..rect }
    } else {
        Rect { x: right, w: after, ..rect }
    }
}

/// What's under a given point in the tab bar.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TabBarHit {
    ToggleSidebar,
    Tab(usize),
    Close(usize),
    NewTab,
}

/// What the bar shows this frame besides the tabs themselves.
#[derive(Clone, Copy, Debug, Default)]
pub struct BarState {
    /// The tab in front.
    pub active: usize,
    pub hovered: Option<TabBarHit>,
    /// The tab being dragged and how far sideways it has come: it
    /// follows the pointer and is drawn over its neighbours.
    pub drag: Option<(usize, f32)>,
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

    /// Where a tab dragged to `x` belongs: the slot the pointer is over,
    /// clamped to the ends of the bar. Only `x` counts -- a drag that
    /// wanders out of the bar keeps sorting.
    pub fn drop_index(&self, x: f32) -> usize {
        match self.tabs.iter().position(|slot| x < slot.rect.x + slot.rect.w) {
            Some(idx) => idx,
            None => self.tabs.len().saturating_sub(1),
        }
    }

    /// Where the tab at `index` sits while it's dragged `dx` sideways:
    /// its own slot, moved, but never out of the row of tabs.
    pub fn dragged_rect(&self, index: usize, dx: f32) -> Option<Rect> {
        let slot = self.tabs.get(index)?;
        let left = self.tabs.first()?.rect.x;
        let right = (self.new_tab.x - slot.rect.w).max(left);
        Some(Rect { x: (slot.rect.x + dx).clamp(left, right), ..slot.rect })
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
        state: BarState,
        text: &mut TextRendererState,
        quads: &mut Vec<QuadInstance>,
    ) {
        let BarState { active, hovered, drag } = state;
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

        // Where the dragged tab actually sits this frame; the others keep
        // their slots, they have already sorted themselves around it.
        let dragged = drag.and_then(|(index, dx)| Some((index, layout.dragged_rect(index, dx)?)));
        let cover = dragged.map(|(_, rect)| rect);

        // The bar's background and bottom border leave the active tab out:
        // under its own see-through background they'd show through.
        let active_rect = match dragged {
            Some((index, rect)) if index == active => Some(rect),
            _ => layout.tabs.get(active).map(|slot| slot.rect),
        };
        let spans = match active_rect {
            Some(rect) => [(layout.left, rect.x), (rect.x + rect.w, layout.right)],
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

        // The dragged tab goes last so it lies over its neighbours.
        let looks: Vec<TabLook<'t>> = tabs.take(layout.tabs.len()).collect();
        let order = (0..looks.len()).filter(|i| dragged.is_none_or(|(index, _)| *i != index)).chain(dragged.map(|(index, _)| index));
        for i in order {
            let (slot, look) = (&layout.tabs[i], looks[i]);
            let TabLook { title, broadcast, accent, background, activity } = look;
            let is_dragged = dragged.is_some_and(|(index, _)| index == i);
            let r = match dragged {
                Some((_, rect)) if is_dragged => rect,
                _ => slot.rect,
            };
            // A label of a tab the dragged one covers stops at its edge;
            // where that would leave only the tail of a title standing,
            // with its beginning hidden, nothing is drawn at all.
            let clip_to = |rect: Rect| if is_dragged { rect } else { cut(rect, cover) };
            let shows = |left: f32, clip: &Rect| clip.w > 0.0 && clip.x <= left + 0.5;
            let close_rect = slot.close.map(|close| Rect { x: close.x + (r.x - slot.rect.x), ..close });
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
            // active tab (its own background already delimits it) or a
            // tab is being dragged across the row.
            if !is_active && i + 1 != active && dragged.is_none() {
                quads.push(separator(r.x + r.w));
            }

            let show_close = is_active || is_hovered;
            let label_right = match close_rect {
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
                let clip = clip_to(Rect { x: label_left, y: 0.0, w: cell.width * 2.0, h });
                if shows(label_left, &clip) {
                    self.labels.push(text, mark, label_left, text_top, clip, text_color(color));
                }
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
            let clip = clip_to(Rect { x: label_left, y: 0.0, w: (label_right - label_left).max(0.0), h });
            if shows(label_left, &clip) {
                self.labels.push(text, &truncate(title, max_chars), label_left, text_top, clip, color);
            }

            if let Some(close) = close_rect.filter(|_| show_close) {
                let close_hovered = hovered == Some(TabBarHit::Close(i));
                if close_hovered {
                    quads.push(fill(close, c.border_strong));
                }
                let color = if close_hovered { text_active } else { text_inactive };
                let left = (close.x + (close.w - cell.width) * 0.5).round();
                let clip = clip_to(close);
                if shows(left, &clip) {
                    self.labels.push(text, "×", left, text_top, clip, color);
                }
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

/// Cut `title` down to at most `max_chars` characters, taking it out of
/// the middle: both ends of a tab title carry something worth seeing --
/// the directory or host it starts with, and the program that's running,
/// which shells put at the end ("~/projects/x: btop - btop").
fn truncate(title: &str, max_chars: usize) -> String {
    let chars: Vec<char> = title.chars().collect();
    if chars.len() <= max_chars {
        return title.to_string();
    }
    match max_chars {
        0 => String::new(),
        1 => "…".to_string(),
        _ => {
            // The end gets the odd character: that's where the program is.
            let keep = max_chars - 1;
            let head = keep / 2;
            let tail = keep - head;
            let mut out: String = chars[..head].iter().collect();
            out.push('…');
            out.extend(&chars[chars.len() - tail..]);
            out
        }
    }
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
    fn a_covered_label_keeps_whichever_side_is_left() {
        let rect = Rect { x: 100.0, y: 0.0, w: 100.0, h: 30.0 };
        let over = |x: f32| Some(Rect { x, y: 0.0, w: 60.0, h: 30.0 });
        // Nothing in the way.
        assert_eq!(cut(rect, None).x, 100.0);
        assert_eq!(cut(rect, over(300.0)).w, 100.0);
        // Covered from the left: what's right of it is left over.
        let right = cut(rect, over(80.0));
        assert_eq!((right.x, right.w), (140.0, 60.0));
        // Covered from the right: it ends where the cover starts.
        let left = cut(rect, over(170.0));
        assert_eq!((left.x, left.w), (100.0, 70.0));
    }

    #[test]
    fn a_dragged_tab_lands_in_the_slot_under_the_pointer() {
        // Toggle at 0..34, then three 260px tabs.
        let layout = TabBarLayout::compute(3, 0.0, 1000.0, CELL, 1.0);
        assert_eq!(layout.drop_index(0.0), 0);
        assert_eq!(layout.drop_index(40.0), 0);
        assert_eq!(layout.drop_index(293.0), 0);
        assert_eq!(layout.drop_index(295.0), 1);
        assert_eq!(layout.drop_index(600.0), 2);
        // Past the last tab, over "+" or beyond the window.
        assert_eq!(layout.drop_index(900.0), 2);
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
    fn truncate_takes_it_out_of_the_middle() {
        assert_eq!(truncate("bash", 4), "bash");
        assert_eq!(truncate("bash", 3), "b…h");
        assert_eq!(truncate("bash", 1), "…");
        assert_eq!(truncate("bash", 0), "");
        // Both ends stay: the directory and the program that's running.
        assert_eq!(truncate("~/Projekte/Terminal: btop - btop", 20), "~/Projekt…top - btop");
        assert_eq!(truncate("jan@server: ~/src", 12), "jan@s… ~/src");
    }
}
