//! Thin wrapper around glyphon/cosmic-text.
//!
//! Owns the font system, glyph atlas and text renderer, and knows how to
//! measure a monospace cell (advance width + line height) so the grid
//! renderer (`super::grid`) can lay terminal cells out on an exact pixel
//! grid instead of guessing.
//!
//! cosmic-text 0.19 requires most mutating `Buffer` calls to go through a
//! short-lived `BorrowedWithFontSystem` wrapper (`buffer.borrow_with(&mut
//! font_system)`), since the buffer itself doesn't hold a reference to the
//! font system. Read-only access (`layout_runs`) doesn't need it.

use glyphon::{
    Attrs, Buffer as TextBuffer, Cache, Family, FontSystem, Metrics, Shaping, SwashCache,
    TextAtlas, TextRenderer, Viewport, Wrap,
};

/// Advance width and line height of one monospace cell, in physical pixels.
#[derive(Clone, Copy, Debug)]
pub struct CellMetrics {
    pub width: f32,
    pub height: f32,
}

pub struct TextRendererState {
    pub font_system: FontSystem,
    pub swash_cache: SwashCache,
    pub viewport: Viewport,
    pub atlas: TextAtlas,
    pub renderer: TextRenderer,
    pub cell: CellMetrics,
    pub metrics: Metrics,
}

impl TextRendererState {
    pub fn new(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        format: wgpu::TextureFormat,
        font_size: f32,
        line_height_factor: f32,
    ) -> Self {
        let mut font_system = FontSystem::new();
        let swash_cache = SwashCache::new();
        let cache = Cache::new(device);
        let viewport = Viewport::new(device, &cache);
        let mut atlas = TextAtlas::new(device, queue, &cache, format);
        let renderer = TextRenderer::new(&mut atlas, device, wgpu::MultisampleState::default(), None);

        let metrics = Metrics::new(font_size, font_size * line_height_factor);
        let cell = measure_cell(&mut font_system, metrics);

        Self { font_system, swash_cache, viewport, atlas, renderer, cell, metrics }
    }

    /// Default text attributes: monospace family at our configured metrics.
    pub fn default_attrs(&self) -> Attrs<'static> {
        Attrs::new().family(Family::Monospace).metrics(self.metrics)
    }
}

/// Shape a single reference glyph to find out how wide a monospace
/// character actually is at this font size on this system, rather than
/// assuming a fixed ratio to the font size.
fn measure_cell(font_system: &mut FontSystem, metrics: Metrics) -> CellMetrics {
    let mut probe = TextBuffer::new(font_system, metrics);
    {
        let mut probe = probe.borrow_with(font_system);
        probe.set_wrap(Wrap::None);
        probe.set_text("M", &Attrs::new().family(Family::Monospace), Shaping::Advanced, None);
        probe.shape_until_scroll(false);
    }

    let width = probe
        .layout_runs()
        .next()
        .and_then(|run| run.glyphs.first())
        .map(|glyph| glyph.w)
        .unwrap_or(metrics.font_size * 0.6);

    CellMetrics { width, height: metrics.line_height }
}
