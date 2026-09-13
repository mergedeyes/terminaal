//! Short one-line labels outside the grid (tab bar, search bar): a pool of
//! glyphon buffers reused across frames, each re-shaped only when its text
//! or color changed. Labels get their own buffers rather than going into
//! the grid's so they don't disturb its per-row shaping cache.

use glyphon::{Buffer as TextBuffer, Color as TextColor, Shaping, TextArea, TextBounds, Wrap};

use crate::render::text::TextRendererState;

/// A rectangle in physical pixels.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Rect {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

impl Rect {
    pub fn contains(&self, x: f32, y: f32) -> bool {
        x >= self.x && x < self.x + self.w && y >= self.y && y < self.y + self.h
    }
}

/// Where and how one label buffer gets drawn this frame.
struct Placement {
    left: f32,
    top: f32,
    clip: Rect,
    color: TextColor,
}

/// The first `placements.len()` buffers are live this frame.
#[derive(Default)]
pub struct Labels {
    buffers: Vec<TextBuffer>,
    /// What each buffer in `buffers` currently holds.
    shaped: Vec<(String, TextColor)>,
    placements: Vec<Placement>,
}

impl Labels {
    /// Start a new frame's labels.
    pub fn clear(&mut self) {
        self.placements.clear();
    }

    /// Draw `label` with its top left corner at `left`/`top`, cut off
    /// outside `clip`.
    pub fn push(&mut self, text: &mut TextRendererState, label: &str, left: f32, top: f32, clip: Rect, color: TextColor) {
        let metrics = text.metrics;
        let idx = self.placements.len();
        if idx == self.buffers.len() {
            let font_system = &mut text.font_system;
            let mut buffer = TextBuffer::new(font_system, metrics);
            {
                let mut buffer = buffer.borrow_with(font_system);
                buffer.set_wrap(Wrap::None);
                buffer.set_size(None, Some(metrics.line_height));
            }
            self.buffers.push(buffer);
            self.shaped.push((String::new(), color));
        }
        if self.shaped[idx].0 != label || self.shaped[idx].1 != color {
            let attrs = text.default_attrs().color(color);
            let mut buffer = self.buffers[idx].borrow_with(&mut text.font_system);
            buffer.set_text(label, &attrs, Shaping::Advanced, None);
            buffer.shape_until_scroll(false);
            self.shaped[idx] = (label.to_string(), color);
        }
        self.placements.push(Placement { left, top, clip, color });
    }

    /// This frame's labels, ready to hand to glyphon's `prepare`
    /// alongside the terminal's own text areas.
    pub fn text_areas(&self) -> impl Iterator<Item = TextArea<'_>> {
        self.placements.iter().zip(&self.buffers).map(|(p, buffer)| TextArea {
            buffer,
            left: p.left,
            top: p.top,
            scale: 1.0,
            bounds: TextBounds {
                left: p.clip.x.floor() as i32,
                top: p.clip.y.floor() as i32,
                right: (p.clip.x + p.clip.w).ceil() as i32,
                bottom: (p.clip.y + p.clip.h).ceil() as i32,
            },
            default_color: p.color,
            custom_glyphs: &[],
        })
    }
}
