//! Turns the current `alacritty_terminal` grid into draw data: background
//! quads (`render::quad`) for cell backgrounds, selection highlight and
//! the cursor block, plus one shaped glyphon buffer per visible row.
//!
//! Split in two so the terminal lock is held as briefly as possible:
//!
//! - [`build_frame`] runs under the lock. `Term::renderable_content()`
//!   already does the hard part (resolving the visible viewport, cursor
//!   position/shape, vi-mode, etc.) -- this just walks its iterator,
//!   pushes quads and copies each row's text plus its style runs into a
//!   [`RowText`]. Cheap, no shaping.
//! - [`GridText::update`] runs after the lock is released and shapes the
//!   rows. Shaping (cosmic-text, `Shaping::Advanced`) is by far the most
//!   expensive part of a frame, so shaped rows are cached by their
//!   content: a row that didn't change -- or only moved because output
//!   scrolled -- is never shaped again. Re-shaping the whole grid every
//!   frame used to cost 50+ ms per frame even in release builds.
//!
//! Within a row, adjacent cells that share the same fg color/bold/italic
//! form one style run, so cosmic-text shapes once per *styled run* rather
//! than once per cell.

use std::collections::HashMap;

use alacritty_terminal::selection::SelectionRange;
use alacritty_terminal::term::cell::Flags;
use alacritty_terminal::term::Term;
use alacritty_terminal::vte::ansi::{CursorShape, NamedColor, Rgb};
use glyphon::{Attrs, Buffer as TextBuffer, Color as TextColor, Shaping, Style, TextArea, TextBounds, Weight, Wrap};

use crate::render::palette::{to_linear, Palette};
use crate::render::quad::QuadInstance;
use crate::render::text::{CellMetrics, TextRendererState};
use crate::terminal::listener::EventProxyListener;

/// Selection highlight (sRGB), blended over each covered cell.
const SELECTION: Rgb = Rgb { r: 89, g: 140, b: 217 };
const SELECTION_ALPHA: f32 = 0.45;

/// Shaped rows kept around beyond the visible ones, as a multiple of the
/// visible row count (with a floor for tiny windows). Covers scrolling
/// back and forth through recent output without re-shaping it.
const CACHE_ROWS_FACTOR: usize = 4;
const CACHE_MIN: usize = 256;

fn dim(c: Rgb) -> Rgb {
    Rgb { r: (c.r as f32 * 0.66) as u8, g: (c.g as f32 * 0.66) as u8, b: (c.b as f32 * 0.66) as u8 }
}

/// Everything about a cell's text styling that can make it join or break
/// a run with its neighbour. Plain `PartialEq`/`Hash`-able so consecutive
/// cells can be compared cheaply and rows can key the shaping cache.
type StyleKey = (u8, u8, u8, bool, bool);

fn style_key(fg: Rgb, bold: bool, italic: bool) -> StyleKey {
    (fg.r, fg.g, fg.b, bold, italic)
}

fn attrs_for(default_attrs: &Attrs<'static>, (r, g, b, bold, italic): StyleKey) -> Attrs<'static> {
    let mut attrs = default_attrs.clone().color(TextColor::rgb(r, g, b));
    if bold {
        attrs = attrs.weight(Weight::BOLD);
    }
    if italic {
        attrs = attrs.style(Style::Italic);
    }
    attrs
}

/// One row's text and the byte offsets where its style runs start --
/// everything its shaped glyphs depend on, so it doubles as the key of
/// [`GridText`]'s cache.
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
pub struct RowText {
    text: String,
    runs: Vec<(usize, StyleKey)>,
}

impl RowText {
    fn push(&mut self, ch: char, key: StyleKey) {
        if self.runs.last().is_none_or(|(_, k)| *k != key) {
            self.runs.push((self.text.len(), key));
        }
        self.text.push(ch);
    }

    /// Trailing blanks draw nothing (backgrounds are quads), so dropping
    /// them saves shaping and lets more rows share a cache entry.
    fn trim_end(&mut self) {
        let len = self.text.trim_end().len();
        self.text.truncate(len);
        self.runs.retain(|(start, _)| *start < len);
    }

    fn spans<'a>(&'a self, default_attrs: &'a Attrs<'static>) -> impl Iterator<Item = (&'a str, Attrs<'static>)> + 'a {
        self.runs.iter().enumerate().map(move |(i, &(start, key))| {
            let end = self.runs.get(i + 1).map_or(self.text.len(), |&(next, _)| next);
            (&self.text[start..end], attrs_for(default_attrs, key))
        })
    }
}

/// Where the grid sits in the window and how big a cell is.
#[derive(Clone, Copy, Debug)]
pub struct GridGeometry {
    /// Top-left corner of the first cell, in physical pixels.
    pub origin_x: f32,
    pub origin_y: f32,
    pub cell: CellMetrics,
}

/// Rebuild `quads` from the terminal's current state and return the text
/// of every visible, non-blank row as `(row index, text)` for
/// [`GridText::update`]. `selection_range` (already resolved from
/// `Term::selection` via `Selection::to_range`) gets a translucent
/// highlight quad per covered cell; `cursor_visible` is `false` on the
/// "off" half of a blink cycle.
pub fn build_frame(
    term: &Term<EventProxyListener>,
    selection_range: Option<SelectionRange>,
    cursor_visible: bool,
    palette: &Palette,
    quads: &mut Vec<QuadInstance>,
    geometry: GridGeometry,
) -> Vec<(usize, RowText)> {
    let GridGeometry { origin_x, origin_y, cell } = geometry;
    let (cell_w, cell_h) = (cell.width, cell.height);
    quads.clear();

    let content = term.renderable_content();
    let display_offset = content.display_offset as i32;
    let default_bg = palette.named(NamedColor::Background);

    let mut rows = Vec::new();
    let mut current_row = None;
    let mut line = RowText::default();
    let mut finish_row = |row: Option<usize>, line: &mut RowText| {
        let Some(row) = row else { return };
        let mut done = std::mem::take(line);
        done.trim_end();
        if !done.text.is_empty() {
            rows.push((row, done));
        }
    };

    for indexed in content.display_iter {
        let point = indexed.point;
        let cell = indexed.cell;
        let selected = selection_range.is_some_and(|range| range.contains(point));

        let row = point.line.0 + display_offset;
        if row < 0 {
            continue;
        }
        let row = row as usize;
        let col = point.column.0;

        if current_row != Some(row) {
            finish_row(current_row, &mut line);
            current_row = Some(row);
        }

        let mut fg = palette.resolve(cell.fg, content.colors);
        let mut bg = palette.resolve(cell.bg, content.colors);
        if cell.flags.contains(Flags::INVERSE) {
            std::mem::swap(&mut fg, &mut bg);
        }
        if cell.flags.contains(Flags::DIM) {
            fg = dim(fg);
        }

        if bg != default_bg {
            quads.push(QuadInstance {
                offset: [origin_x + col as f32 * cell_w, origin_y + row as f32 * cell_h],
                size: [cell_w, cell_h],
                color: to_linear(bg, 1.0),
            });
        }

        // Drawn after the background quad (and before text, since text
        // renders in a separate later pass) so a selection is visible as
        // a translucent overlay regardless of the cell's own background.
        if selected {
            quads.push(QuadInstance {
                offset: [origin_x + col as f32 * cell_w, origin_y + row as f32 * cell_h],
                size: [cell_w, cell_h],
                color: to_linear(SELECTION, SELECTION_ALPHA),
            });
        }

        let ch = if cell.flags.contains(Flags::HIDDEN) { ' ' } else { cell.c };
        let bold = cell.flags.intersects(Flags::BOLD);
        let italic = cell.flags.contains(Flags::ITALIC);
        line.push(ch, style_key(fg, bold, italic));
    }
    finish_row(current_row, &mut line);

    // Cursor block. `cursor.shape` is already `Hidden` when the cursor
    // shouldn't be drawn at all (blur/unfocused/vi-mode/etc handled by
    // `renderable_content()` itself); `cursor_visible` additionally
    // covers the "off" half of a blink cycle.
    if cursor_visible && content.cursor.shape != CursorShape::Hidden {
        let cursor_row = content.cursor.point.line.0 + display_offset;
        if cursor_row >= 0 {
            let cursor_row = cursor_row as usize;
            let cursor_col = content.cursor.point.column.0;
            let cursor_color = palette.named(NamedColor::Cursor);
            quads.push(QuadInstance {
                offset: [origin_x + cursor_col as f32 * cell_w, origin_y + cursor_row as f32 * cell_h],
                size: [cell_w, cell_h],
                color: to_linear(cursor_color, 0.55),
            });
        }
    }

    rows
}

struct CachedRow {
    buffer: TextBuffer,
    /// [`GridText::frame`] this row was last on screen.
    last_used: u64,
}

/// Shaped glyphon buffers for the grid's rows, one per row, cached by
/// content across frames.
#[derive(Default)]
pub struct GridText {
    cache: HashMap<RowText, CachedRow>,
    /// This frame's rows, looked up in `cache` by [`GridText::text_areas`].
    visible: Vec<(usize, RowText)>,
    frame: u64,
}

impl GridText {
    /// Make sure every row from [`build_frame`] is shaped, shaping only
    /// the ones not already in the cache.
    pub fn update(&mut self, text: &mut TextRendererState, rows: Vec<(usize, RowText)>) {
        self.frame += 1;
        let default_attrs = text.default_attrs();
        for (_, row) in &rows {
            match self.cache.get_mut(row) {
                Some(cached) => cached.last_used = self.frame,
                None => {
                    let buffer = shape_row(text, &default_attrs, row);
                    self.cache.insert(row.clone(), CachedRow { buffer, last_used: self.frame });
                }
            }
        }
        if self.cache.len() > (rows.len() * CACHE_ROWS_FACTOR).max(CACHE_MIN) {
            let frame = self.frame;
            self.cache.retain(|_, cached| cached.last_used == frame);
        }
        self.visible = rows;
    }

    /// One text area per visible row, clipped to `bounds`.
    pub fn text_areas(
        &self,
        geometry: GridGeometry,
        bounds: TextBounds,
        default_color: TextColor,
    ) -> impl Iterator<Item = TextArea<'_>> {
        self.visible.iter().filter_map(move |(row, text)| {
            let cached = self.cache.get(text)?;
            Some(TextArea {
                buffer: &cached.buffer,
                left: geometry.origin_x,
                top: geometry.origin_y + *row as f32 * geometry.cell.height,
                scale: 1.0,
                bounds,
                default_color,
                custom_glyphs: &[],
            })
        })
    }
}

fn shape_row(text: &mut TextRendererState, default_attrs: &Attrs<'static>, row: &RowText) -> TextBuffer {
    let TextRendererState { font_system, metrics, cell, .. } = text;
    let mut buffer = TextBuffer::new(font_system, *metrics);
    {
        let mut buffer = buffer.borrow_with(font_system);
        buffer.set_wrap(Wrap::None);
        // Snap every glyph's advance to our measured cell width so the
        // text lines up with the background-quad grid pixel-for-pixel.
        buffer.set_monospace_width(Some(cell.width));
        buffer.set_size(None, Some(metrics.line_height));
        buffer.set_rich_text(row.spans(default_attrs), default_attrs, Shaping::Advanced, None);
        buffer.shape_until_scroll(false);
    }
    buffer
}

#[cfg(test)]
mod tests {
    use super::*;

    const RED: StyleKey = (255, 0, 0, false, false);
    const BLUE: StyleKey = (0, 0, 255, true, false);

    fn row(cells: &[(char, StyleKey)]) -> RowText {
        let mut row = RowText::default();
        for &(ch, key) in cells {
            row.push(ch, key);
        }
        row
    }

    #[test]
    fn runs_start_where_the_style_changes() {
        let r = row(&[('a', RED), ('b', RED), ('c', BLUE), ('d', RED)]);
        assert_eq!(r.text, "abcd");
        assert_eq!(r.runs, vec![(0, RED), (2, BLUE), (3, RED)]);
        let attrs = Attrs::new();
        let spans: Vec<&str> = r.spans(&attrs).map(|(s, _)| s).collect();
        assert_eq!(spans, ["ab", "c", "d"]);
    }

    #[test]
    fn trim_end_drops_trailing_blanks_and_their_runs() {
        let mut r = row(&[('ä', RED), (' ', RED), (' ', BLUE), (' ', RED)]);
        r.trim_end();
        assert_eq!(r.text, "ä");
        assert_eq!(r.runs, vec![(0, RED)]);

        let mut blank = row(&[(' ', RED), (' ', BLUE)]);
        blank.trim_end();
        assert!(blank.text.is_empty() && blank.runs.is_empty());
    }

    #[test]
    fn same_content_same_key() {
        let a = row(&[('x', RED), ('y', BLUE)]);
        let b = row(&[('x', RED), ('y', BLUE)]);
        let c = row(&[('x', RED), ('y', RED)]);
        assert_eq!(a, b);
        assert_ne!(a, c);
    }
}
