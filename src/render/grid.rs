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
use std::ops::{Range, RangeInclusive};

use alacritty_terminal::event::EventListener;
use alacritty_terminal::grid::Dimensions;
use alacritty_terminal::selection::SelectionRange;
use alacritty_terminal::term::cell::Flags;
use alacritty_terminal::index::Point;
use alacritty_terminal::term::search::Match;
use alacritty_terminal::term::Term;
use alacritty_terminal::vte::ansi::{CursorShape, NamedColor, Rgb};
use glyphon::{
    Attrs, Buffer as TextBuffer, Color as TextColor, FontSystem, Metrics, Shaping, Style, TextArea, TextBounds, Weight,
    Wrap,
};

use crate::render::palette::{to_linear, Palette};
use crate::render::quad::QuadInstance;
use crate::render::text::{CellMetrics, TextRendererState};

/// Shaped rows kept around beyond the visible ones, as a multiple of the
/// visible row count (with a floor for tiny windows). Covers scrolling
/// back and forth through recent output without re-shaping it.
const CACHE_ROWS_FACTOR: usize = 4;
const CACHE_MIN: usize = 256;

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
        // Every cell is one column wide, control characters included: a
        // tab, which `alacritty_terminal` keeps in the cell it started
        // from (the ones it skipped hold spaces), would otherwise shape
        // to the next tab stop and slide the rest of the row out from
        // under its cells -- selecting what you see then copies
        // something else (`dig` output, whose columns are tabs).
        let ch = if ch.is_control() { ' ' } else { ch };
        if self.runs.last().is_none_or(|(_, k)| *k != key) {
            self.runs.push((self.text.len(), key));
        }
        self.text.push(ch);
    }

    /// How many cells the row's text covers, give or take wide characters.
    pub fn len(&self) -> usize {
        self.text.chars().count()
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

/// What's highlighted besides the selection.
#[derive(Clone, Copy, Default)]
pub struct Highlights<'a> {
    /// Search matches on screen, first to last (`terminal::search`).
    pub matches: &'a [Match],
    pub focus: Option<&'a Match>,
    /// The link under the mouse while Ctrl is held: underlined.
    pub link: Option<&'a RangeInclusive<Point>>,
}

/// How the cursor is drawn.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CursorStyle {
    /// A block, in the pane with the keyboard.
    Block,
    /// An outline, in the other panes of a split tab.
    Outline,
    /// Not at all: the "off" half of a blink cycle.
    Hidden,
}

/// Append the terminal's current state to `quads` and return the text of
/// every visible, non-blank row as `(row index, text)` for
/// [`GridText::update`]. Cells in `selection_range` (already resolved from
/// `Term::selection` via `Selection::to_range`) and in a search match get
/// the theme's colors for them, the selection winning.
pub fn build_frame<T: EventListener>(
    term: &Term<T>,
    selection_range: Option<SelectionRange>,
    search: Highlights,
    cursor: CursorStyle,
    palette: &Palette,
    quads: &mut Vec<QuadInstance>,
    geometry: GridGeometry,
) -> Vec<(usize, RowText)> {
    let GridGeometry { origin_x, origin_y, cell } = geometry;
    let (cell_w, cell_h) = (cell.width, cell.height);

    let screen_lines = term.screen_lines() as i32;
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

    // Cells come first to last, like the matches: skip those that ended.
    let mut next_match = 0;
    for indexed in content.display_iter {
        let point = indexed.point;
        let cell = indexed.cell;
        let selected = selection_range.is_some_and(|range| range.contains(point));
        while search.matches.get(next_match).is_some_and(|m| *m.end() < point) {
            next_match += 1;
        }
        let focused = search.focus.is_some_and(|m| m.contains(&point));
        let matched = focused || search.matches.get(next_match).is_some_and(|m| m.contains(&point));

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
            fg = crate::theme::dim(fg);
        }
        // Like Alacritty, the theme's search and selection colors replace
        // the cell's.
        if matched {
            let (match_bg, match_fg) = palette.search(focused);
            bg = match_bg;
            fg = match_fg.unwrap_or(fg);
        }
        if selected {
            let (selection_bg, selection_fg) = palette.selection();
            bg = selection_bg;
            fg = selection_fg.unwrap_or(fg);
        }

        if bg != default_bg {
            quads.push(QuadInstance {
                offset: [origin_x + col as f32 * cell_w, origin_y + row as f32 * cell_h],
                size: [cell_w, cell_h],
                color: to_linear(bg, 1.0),
            });
        }

        if search.link.is_some_and(|link| link.contains(&point)) {
            let thickness = (cell_h / 16.0).round().max(1.0);
            quads.push(QuadInstance {
                offset: [origin_x + col as f32 * cell_w, origin_y + (row + 1) as f32 * cell_h - thickness],
                size: [cell_w, thickness],
                color: to_linear(fg, 1.0),
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
    if cursor != CursorStyle::Hidden && content.cursor.shape != CursorShape::Hidden {
        let cursor_row = content.cursor.point.line.0 + display_offset;
        // Scrolled into the history, the cursor is below the screen -- and
        // drawn there, it would land in the pane underneath.
        if (0..screen_lines).contains(&cursor_row) {
            let cursor_row = cursor_row as usize;
            let cursor_col = content.cursor.point.column.0;
            let cursor_color = palette.named(NamedColor::Cursor);
            let (x, y) = (origin_x + cursor_col as f32 * cell_w, origin_y + cursor_row as f32 * cell_h);
            if cursor == CursorStyle::Block {
                quads.push(QuadInstance { offset: [x, y], size: [cell_w, cell_h], color: to_linear(cursor_color, 0.55) });
            } else {
                let t = (cell_h / 16.0).round().max(1.0);
                let color = to_linear(cursor_color, 1.0);
                for (offset, size) in [
                    ([x, y], [cell_w, t]),
                    ([x, y + cell_h - t], [cell_w, t]),
                    ([x, y], [t, cell_h]),
                    ([x + cell_w - t, y], [t, cell_h]),
                ] {
                    quads.push(QuadInstance { offset, size, color });
                }
            }
        }
    }

    rows
}

struct CachedRow {
    buffer: TextBuffer,
    /// [`GridText::frame`] this row was last on screen.
    last_used: u64,
}

/// Where one pane's rows go: its grid, what its text is clipped to and
/// where rows are covered (see [`GridText::text_areas`]).
#[derive(Clone, Debug)]
pub struct PaneText {
    pub geometry: GridGeometry,
    pub bounds: TextBounds,
    pub cutout: Option<(Range<usize>, f32)>,
}

/// Shaped glyphon buffers for the grid's rows, one per row, cached by
/// content across frames -- and across panes, which share the cache.
#[derive(Default)]
pub struct GridText {
    cache: HashMap<RowText, CachedRow>,
    /// This frame's rows per pane, looked up in `cache` by
    /// [`GridText::text_areas`].
    visible: Vec<Vec<(usize, RowText)>>,
    frame: u64,
}

impl GridText {
    /// Make sure every row from [`build_frame`] is shaped, shaping only
    /// the ones not already in the cache. One list of rows per pane.
    pub fn update(&mut self, text: &mut TextRendererState, panes: Vec<Vec<(usize, RowText)>>) {
        self.frame += 1;
        let default_attrs = text.default_attrs();
        for (_, row) in panes.iter().flatten() {
            match self.cache.get_mut(row) {
                Some(cached) => cached.last_used = self.frame,
                None => {
                    let buffer = shape_row(&mut text.font_system, text.metrics, text.cell, &default_attrs, row);
                    self.cache.insert(row.clone(), CachedRow { buffer, last_used: self.frame });
                }
            }
        }
        let rows: usize = panes.iter().map(Vec::len).sum();
        if self.cache.len() > (rows * CACHE_ROWS_FACTOR).max(CACHE_MIN) {
            let frame = self.frame;
            self.cache.retain(|_, cached| cached.last_used == frame);
        }
        self.visible = panes;
    }

    /// One text area per visible row, pane by pane as in `panes` (same
    /// order as given to [`GridText::update`]), clipped to the pane's
    /// bounds. The rows in a cutout end at its x: something is drawn over
    /// them from there (the search bar), and text always comes out on top
    /// of quads.
    pub fn text_areas<'a>(
        &'a self,
        panes: &'a [PaneText],
        default_color: TextColor,
    ) -> impl Iterator<Item = TextArea<'a>> {
        self.visible.iter().zip(panes).flat_map(move |(rows, pane)| {
            rows.iter().filter_map(move |(row, text)| self.text_area(pane, *row, text, default_color))
        })
    }

    fn text_area(&self, pane: &PaneText, row: usize, text: &RowText, default_color: TextColor) -> Option<TextArea<'_>> {
        let PaneText { geometry, bounds, cutout } = pane;
        let cached = self.cache.get(text)?;
        let bounds = match cutout {
            Some((rows, x)) if rows.contains(&row) => TextBounds { right: bounds.right.min(*x as i32), ..*bounds },
            _ => *bounds,
        };
        Some(TextArea {
            buffer: &cached.buffer,
            left: geometry.origin_x,
            top: geometry.origin_y + row as f32 * geometry.cell.height,
            scale: 1.0,
            bounds,
            default_color,
            custom_glyphs: &[],
        })
    }
}

fn shape_row(
    font_system: &mut FontSystem,
    metrics: Metrics,
    cell: CellMetrics,
    default_attrs: &Attrs<'static>,
    row: &RowText,
) -> TextBuffer {
    let mut buffer = TextBuffer::new(font_system, metrics);
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
    fn a_tab_cell_stays_one_column_wide() {
        let r = row(&[('a', RED), ('\t', RED), ('b', RED)]);
        assert_eq!(r.text, "a b");
        assert_eq!(r.len(), 3);
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

    /// Every glyph of a long row has to start exactly at its cell, at
    /// fractional scale factors too (cosmic-text's monospace snapping,
    /// see `text::metrics`).
    #[test]
    fn glyphs_stay_on_their_cells() {
        use crate::render::text::{measure_cell, metrics};
        use glyphon::Family;

        let mut font_system = FontSystem::new();
        let text: String = format!("echo verrrrrrr {} looonnggg teeeext", "y".repeat(120));
        for scale in [1.0, 1.1, 1.25, 1.5, 1.75, 2.0, 2.25] {
            let metrics = metrics(15.0 * scale, 1.2);
            let cell = measure_cell(&mut font_system, metrics);
            let attrs = Attrs::new().family(Family::Monospace).metrics(metrics);
            let row = row(&text.chars().map(|c| (c, RED)).collect::<Vec<_>>());
            let buffer = shape_row(&mut font_system, metrics, cell, &attrs, &row);
            let glyphs = buffer.layout_runs().next().unwrap().glyphs;
            assert_eq!(glyphs.len(), text.len());
            for (col, glyph) in glyphs.iter().enumerate() {
                let drift = glyph.x - col as f32 * cell.width;
                assert!(drift.abs() < 0.01, "scale {scale}: column {col} is {drift} px off");
            }
        }
    }

    /// Quads `build_frame` draws for the cursor of a 20×5 terminal whose
    /// output left the cursor on its last row, scrolled up `scroll` lines.
    fn cursor_quads(scroll: i32, cursor: CursorStyle) -> Vec<QuadInstance> {
        use alacritty_terminal::event::VoidListener;
        use alacritty_terminal::grid::Scroll;
        use alacritty_terminal::term::Config;
        use alacritty_terminal::vte::ansi::Processor;

        use crate::terminal::GridSize;

        let size = GridSize { columns: 20, screen_lines: 5 };
        let mut term = Term::new(Config { scrolling_history: 100, ..Config::default() }, &size, VoidListener);
        let output: String = (0..12).map(|i| format!("line {i}\r\n")).collect();
        Processor::<alacritty_terminal::vte::ansi::StdSyncHandler>::new().advance(&mut term, output.as_bytes());
        term.scroll_display(Scroll::Delta(scroll));
        let palette = Palette::new(&crate::theme::Themes::builtin().get(None).terminal);
        let geometry = GridGeometry { origin_x: 0.0, origin_y: 0.0, cell: CellMetrics { width: 10.0, height: 20.0 } };
        let mut quads = Vec::new();
        build_frame(&term, None, Highlights::default(), cursor, &palette, &mut quads, geometry);
        quads
    }

    #[test]
    fn the_cursor_stays_inside_the_grid() {
        for style in [CursorStyle::Block, CursorStyle::Outline] {
            let on_screen = cursor_quads(0, style);
            assert!(!on_screen.is_empty(), "{style:?}");
            assert!(on_screen.iter().all(|q| q.offset[1] >= 80.0 && q.offset[1] + q.size[1] <= 100.0), "{style:?}");
            // Scrolled away, it's below the grid: into the pane underneath,
            // if drawn.
            assert!(cursor_quads(2, style).is_empty(), "{style:?}");
        }
        assert!(cursor_quads(0, CursorStyle::Hidden).is_empty());
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
