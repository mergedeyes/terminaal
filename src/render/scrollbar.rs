//! The scrollbar at the right edge of a pane: where the thumb sits for a
//! scrollback position, and which position a dragged thumb stands for.
//! Pure geometry in physical pixels; when it shows, and the dragging,
//! are `app.rs`'s.
//!
//! It sits in the pane's right padding, so the grid's text -- which
//! glyphon draws after all quads -- doesn't land on it unless the padding
//! is narrower than the bar.

use super::label::Rect;

/// How wide the bar is drawn, in logical pixels; wider while the pointer
/// is on it or it's being dragged.
pub const WIDTH: f32 = 5.0;
pub const WIDTH_ACTIVE: f32 = 9.0;
/// How far in from the pane's right edge it can be taken hold of.
pub const HIT_WIDTH: f32 = 14.0;
/// The thumb never gets shorter than this, however long the scrollback.
const MIN_THUMB: f32 = 24.0;
/// Space between the bar and the pane's right edge.
const MARGIN: f32 = 2.0;

/// The scrollbar of one pane as laid out for a frame.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Scrollbar {
    /// The whole height it moves along: the grid's rows.
    pub track: Rect,
    pub thumb: Rect,
    /// Lines above the screen; the thumb at the top shows the oldest.
    history: usize,
}

impl Scrollbar {
    /// The bar for a pane at `pane` whose grid runs from `top` to
    /// `bottom`, scrolled back `offset` of its `history` lines with
    /// `screen_lines` on screen. `None` without any scrollback. `scale` is
    /// the window's scale factor, `active` draws it wide.
    pub fn new(
        pane: Rect,
        (top, bottom): (f32, f32),
        (history, screen_lines, offset): (usize, usize, usize),
        scale: f32,
        active: bool,
    ) -> Option<Self> {
        let height = bottom - top;
        if history == 0 || screen_lines == 0 || height <= 0.0 {
            return None;
        }
        let width = (if active { WIDTH_ACTIVE } else { WIDTH } * scale).round();
        let x = (pane.x + pane.w - width - MARGIN * scale).round();
        let track = Rect { x, y: top, w: width, h: height };
        let total = (history + screen_lines) as f32;
        let thumb_h = (height * screen_lines as f32 / total).max(MIN_THUMB * scale).min(height).round();
        // At the bottom without scrolling back, at the top all the way.
        let back = offset.min(history) as f32 / history as f32;
        let thumb_y = (top + (height - thumb_h) * (1.0 - back)).round();
        Some(Self { track, thumb: Rect { x, y: thumb_y, w: width, h: thumb_h }, history })
    }

    /// The scrollback position for the thumb's top edge at `thumb_top`,
    /// within the track.
    pub fn offset_at(&self, thumb_top: f32) -> usize {
        let room = self.track.h - self.thumb.h;
        if room <= 0.0 {
            return 0;
        }
        let from_top = ((thumb_top - self.track.y) / room).clamp(0.0, 1.0);
        ((1.0 - from_top) * self.history as f32).round() as usize
    }
}

/// Whether `(x, y)` is where the bar of a pane at `pane` can be taken
/// hold of: the strip along its right edge, between `top` and `bottom`.
pub fn hit(pane: Rect, (top, bottom): (f32, f32), scale: f32, x: f32, y: f32) -> bool {
    let right = pane.x + pane.w;
    x >= right - HIT_WIDTH * scale && x < right && y >= top && y < bottom
}

#[cfg(test)]
mod tests {
    use super::*;

    const PANE: Rect = Rect { x: 0.0, y: 0.0, w: 800.0, h: 600.0 };

    fn bar(history: usize, offset: usize) -> Scrollbar {
        Scrollbar::new(PANE, (10.0, 510.0), (history, 50, offset), 1.0, false).unwrap()
    }

    #[test]
    fn no_scrollback_no_bar() {
        assert_eq!(Scrollbar::new(PANE, (10.0, 510.0), (0, 50, 0), 1.0, false), None);
    }

    #[test]
    fn thumb_shows_where_the_screen_is() {
        // 50 of 100 lines: half the track, at the bottom, at the top all
        // the way back, in the middle half way.
        let bottom = bar(50, 0);
        assert_eq!(bottom.thumb.h, 250.0);
        assert_eq!(bottom.thumb.y + bottom.thumb.h, 510.0);
        assert_eq!(bar(50, 50).thumb.y, 10.0);
        assert_eq!(bar(50, 25).thumb.y, 135.0);
        // Along the right edge, inside the pane.
        assert_eq!(bottom.thumb.x + bottom.thumb.w, 798.0);
        // A long scrollback doesn't shrink it to nothing.
        assert_eq!(bar(100_000, 0).thumb.h, MIN_THUMB);
    }

    #[test]
    fn dragging_the_thumb_picks_a_position() {
        let b = bar(50, 0);
        assert_eq!(b.offset_at(b.thumb.y), 0);
        assert_eq!(b.offset_at(10.0), 50);
        assert_eq!(b.offset_at(135.0), 25);
        // Past either end it stops there.
        assert_eq!(b.offset_at(-100.0), 50);
        assert_eq!(b.offset_at(1000.0), 0);
        // Where it's put is where it's drawn.
        for offset in [0, 7, 25, 49, 50] {
            assert_eq!(b.offset_at(bar(50, offset).thumb.y), offset);
        }
    }

    #[test]
    fn hit_strip_along_the_right_edge() {
        let grid = (10.0, 510.0);
        assert!(hit(PANE, grid, 1.0, 795.0, 100.0));
        assert!(hit(PANE, grid, 1.0, 787.0, 100.0));
        assert!(!hit(PANE, grid, 1.0, 780.0, 100.0));
        assert!(!hit(PANE, grid, 1.0, 795.0, 5.0));
        assert!(!hit(PANE, grid, 1.0, 800.0, 100.0));
        // Twice as wide at 200 %.
        assert!(hit(PANE, grid, 2.0, 780.0, 100.0));
    }
}
