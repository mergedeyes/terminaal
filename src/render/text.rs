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
//!
//! The console font is the font database's monospace family: everything
//! here asks for `Family::Monospace`, so choosing another font only means
//! pointing that family elsewhere ([`TextRendererState::set_family`]).

use std::collections::BTreeSet;

use glyphon::{
    Attrs, Buffer as TextBuffer, Cache, Family, FontSystem, Metrics, Shaping, SwashCache, TextAtlas, TextRenderer,
    Viewport, Wrap, fontdb,
};

/// Advance width and line height of one monospace cell, in physical pixels.
#[derive(Clone, Copy, Debug)]
pub struct CellMetrics {
    pub width: f32,
    pub height: f32,
}

/// Installed font families by name, sorted.
#[derive(Clone, Debug, Default)]
pub struct FontFamilies {
    pub all: Vec<String>,
    pub monospace: Vec<String>,
}

pub struct TextRendererState {
    pub font_system: FontSystem,
    pub swash_cache: SwashCache,
    pub viewport: Viewport,
    pub atlas: TextAtlas,
    pub renderer: TextRenderer,
    pub cell: CellMetrics,
    pub metrics: Metrics,
    /// cosmic-text's own monospace family, for when none is chosen.
    default_family: String,
}

impl TextRendererState {
    pub fn new(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        format: wgpu::TextureFormat,
        font_size: f32,
        line_height_factor: f32,
        family: Option<&str>,
    ) -> Self {
        let mut font_system = FontSystem::new();
        let swash_cache = SwashCache::new();
        let cache = Cache::new(device);
        let viewport = Viewport::new(device, &cache);
        let mut atlas = TextAtlas::new(device, queue, &cache, format);
        let renderer = TextRenderer::new(&mut atlas, device, wgpu::MultisampleState::default(), None);

        let metrics = metrics(font_size, line_height_factor);
        let cell = measure_cell(&mut font_system, metrics);
        let default_family = font_system.db().family_name(&fontdb::Family::Monospace).to_string();

        let mut state = Self { font_system, swash_cache, viewport, atlas, renderer, cell, metrics, default_family };
        if family.is_some() {
            state.set_family(family);
        }
        state
    }

    /// Switch to another font size or line height. Buffers shaped with
    /// the old metrics are stale afterwards.
    pub fn set_font(&mut self, font_size: f32, line_height_factor: f32) {
        self.metrics = metrics(font_size, line_height_factor);
        self.cell = measure_cell(&mut self.font_system, self.metrics);
    }

    /// Switch the console to font `family`; `None`, or one that isn't
    /// installed, is cosmic-text's default. Buffers shaped before are stale
    /// afterwards.
    pub fn set_family(&mut self, family: Option<&str>) {
        let family = match family {
            Some(family) if self.face_id(family).is_some() => family.to_string(),
            Some(family) => {
                log::warn!("font {family:?} is not installed, using {}", self.default_family);
                self.default_family.clone()
            }
            None => self.default_family.clone(),
        };
        // Also drops cosmic-text's cache of which fonts match which family.
        self.font_system.db_mut().set_monospace_family(family);
        self.cell = measure_cell(&mut self.font_system, self.metrics);
    }

    /// The console font when none is chosen.
    pub fn default_family(&self) -> &str {
        &self.default_family
    }

    /// Default text attributes: monospace family at our configured metrics.
    pub fn default_attrs(&self) -> Attrs<'static> {
        Attrs::new().family(Family::Monospace).metrics(self.metrics)
    }

    /// Every installed font family, and the monospaced ones among them.
    pub fn families(&self) -> FontFamilies {
        let (mut all, mut monospace) = (BTreeSet::new(), BTreeSet::new());
        for face in self.font_system.db().faces() {
            let Some((name, _)) = face.families.first() else { continue };
            if name.starts_with('.') {
                continue;
            }
            if face.monospaced {
                monospace.insert(name.clone());
            }
            all.insert(name.clone());
        }
        FontFamilies { all: all.into_iter().collect(), monospace: monospace.into_iter().collect() }
    }

    /// The regular face of `family`: its font file's bytes and the face's
    /// index in it.
    pub fn face_data(&self, family: &str) -> Option<(Vec<u8>, u32)> {
        let id = self.face_id(family)?;
        self.font_system.db().with_face_data(id, |data, index| (data.to_vec(), index))
    }

    fn face_id(&self, family: &str) -> Option<fontdb::ID> {
        let families = [fontdb::Family::Name(family)];
        self.font_system.db().query(&fontdb::Query { families: &families, ..fontdb::Query::default() })
    }
}

/// Text metrics for a physical font size, rounded to whole pixels.
///
/// `Buffer::set_monospace_width` (which the grid relies on to keep glyphs
/// on the cell grid) snaps each advance to a multiple of the cell width
/// *in ems* while the advance itself is in pixels -- which amounts to
/// rounding the font size. At a fractional size (15 pt at 125 % = 18.75)
/// every glyph came out a fraction of a pixel wider or narrower than a
/// cell, so long rows ran away from their cells: the cursor trailed a
/// long command line, nano's status bar looked cut off. With a whole
/// pixel size the snapping is exact.
pub fn metrics(font_size: f32, line_height_factor: f32) -> Metrics {
    let font_size = font_size.round().max(1.0);
    Metrics::new(font_size, font_size * line_height_factor)
}

/// Shape a single reference glyph to find out how wide a monospace
/// character actually is at this font size on this system, rather than
/// assuming a fixed ratio to the font size.
pub(crate) fn measure_cell(font_system: &mut FontSystem, metrics: Metrics) -> CellMetrics {
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
