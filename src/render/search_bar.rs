//! The search bar over the console (`terminal::search`): a box in the
//! bottom right corner with the query, a text cursor while typing and, on
//! the right, the keys to use or that nothing matched. Moves to the top
//! right when it would cover the match in focus. Drawn like the tab bar,
//! quads plus labels, in the theme's chrome colors; solid like other
//! popups, whatever the window's opacity.
//!
//! glyphon draws all text after all quads, so the grid's text would show
//! on top of the bar. The bar therefore covers whole cells -- two rows,
//! the text in the middle -- and the rows under it are cut off where it
//! starts ([`SearchBar::cutout`]).

use alacritty_terminal::vte::ansi::Rgb;
use glyphon::{Color as TextColor, TextArea};

use crate::render::grid::GridGeometry;
use crate::render::label::{Labels, Rect};
use crate::render::palette::to_linear;
use crate::render::quad::QuadInstance;
use crate::render::text::TextRendererState;
use crate::theme::UiColors;

/// Narrowest the bar gets, in cells, unless the console is narrower.
const MIN_CELLS: usize = 46;
/// Rows the bar covers.
const ROWS: usize = 2;

/// What the bar shows.
pub struct SearchBarView<'a> {
    /// "Search", before the query.
    pub prompt: &'a str,
    pub query: &'a str,
    /// The query is being typed: draw the text cursor.
    pub editing: bool,
    /// On the right, in the error color when `no_match`.
    pub status: &'a str,
    pub no_match: bool,
    /// First and last row of the match in focus, if it's on screen.
    pub focus_rows: Option<(i32, i32)>,
}

pub struct SearchBar {
    colors: UiColors,
    labels: Labels,
    /// The bar's place in the last build: its rectangle and the first of
    /// its rows.
    placed: Option<(Rect, usize)>,
}

impl SearchBar {
    pub fn new(colors: UiColors) -> Self {
        Self { colors, labels: Labels::default(), placed: None }
    }

    /// Nothing to draw this frame.
    pub fn hide(&mut self) {
        self.labels.clear();
        self.placed = None;
    }

    /// Whether the bar covers the physical pixel `x`/`y`.
    pub fn contains(&self, x: f32, y: f32) -> bool {
        self.placed.is_some_and(|(rect, _)| rect.contains(x, y))
    }

    /// Where the grid's rows are covered: the rows and the left edge of
    /// the bar.
    pub fn cutout(&self) -> Option<(std::ops::Range<usize>, f32)> {
        self.placed.map(|(rect, row)| (row..row + ROWS, rect.x))
    }

    /// Append the bar's quads for `view` over the grid of `cols`×`rows`
    /// cells at `geometry`.
    pub fn build(
        &mut self,
        geometry: GridGeometry,
        (cols, rows): (usize, usize),
        view: &SearchBarView,
        scale_factor: f32,
        text: &mut TextRendererState,
        quads: &mut Vec<QuadInstance>,
    ) {
        self.hide();
        // A cell of padding left and right, one of gap before the status.
        let chars = |s: &str| s.chars().count();
        let wanted = 1 + chars(view.prompt) + 1 + chars(view.query) + 2 + chars(view.status) + 1;
        let bar_cols = wanted.max(MIN_CELLS).min(cols);
        if rows < ROWS * 2 || bar_cols < 8 {
            return;
        }
        let c = self.colors;
        let cell = geometry.cell;
        let px = scale_factor.round().max(1.0);

        let bottom = rows - ROWS;
        let covers_focus = view.focus_rows.is_some_and(|(first, last)| first < rows as i32 && last >= bottom as i32);
        let row = if covers_focus { 0 } else { bottom };
        let first_col = cols - bar_cols;
        let x = geometry.origin_x + first_col as f32 * cell.width;
        let y = geometry.origin_y + row as f32 * cell.height;
        let (w, h) = (bar_cols as f32 * cell.width, ROWS as f32 * cell.height);
        let rect = Rect { x, y, w, h };
        self.placed = Some((rect, row));

        let solid = |rect: Rect, color: Rgb| QuadInstance {
            offset: [rect.x, rect.y],
            size: [rect.w, rect.h],
            color: to_linear(color, 1.0),
        };
        quads.push(solid(rect, c.border_strong));
        quads.push(solid(Rect { x: x + px, y: y + px, w: w - 2.0 * px, h: h - 2.0 * px }, c.background));

        let color = |Rgb { r, g, b }: Rgb| TextColor::rgb(r, g, b);
        let top = (y + (h - text.metrics.line_height) * 0.5).round();
        let at = |col: usize| x + (1 + col) as f32 * cell.width;
        let cells = bar_cols - 2;
        let inner = Rect { x: at(0), y, w: cells as f32 * cell.width, h };

        // Prompt and query from the left; the status only if it fits
        // beside them, and the query's start goes first if even it doesn't.
        let prompt_cells = chars(view.prompt) + 1;
        let query_room = cells.saturating_sub(prompt_cells + 1);
        let query: String = view.query.chars().skip(chars(view.query).saturating_sub(query_room)).collect();
        let used = prompt_cells + chars(&query) + 1;
        self.labels.push(text, view.prompt, at(0), top, inner, color(c.text_weak));
        self.labels.push(text, &query, at(prompt_cells), top, inner, color(c.text));
        if view.editing {
            let bar = Rect { x: at(used - 1), y: top, w: 2.0 * px, h: text.metrics.line_height };
            quads.push(solid(bar, c.accent));
        }
        let status_cells = chars(view.status);
        if used + 1 + status_cells <= cells {
            let status_color = if view.no_match { c.error } else { c.text_weak };
            self.labels.push(text, view.status, at(cells - status_cells), top, inner, color(status_color));
        }
    }

    pub fn text_areas(&self) -> impl Iterator<Item = TextArea<'_>> {
        self.labels.text_areas()
    }
}
