//! Atlas packing and GPU texture management for glyph MSDFs.
//!
//! The CPU half of an atlas — pixels and layout tables — is
//! [`crate::BakedAtlas`]; this module adds the GPU texture the renderer samples.
//! [`FontAtlas::new`] keeps the one-call behaviour of building and uploading.

use super::layout::{
    layout_block, layout_line, measure_block, measure_line, GlyphLayout, LayoutFont, LineMetrics,
    TextAlign, TextMetrics, TextStyle,
};
use super::parser::{FontMetrics, GposKerning};
use super::BakedAtlas;
use anyhow::Result;
use rustc_hash::FxHashMap as HashMap;

/// Default MSDF cell size in texels.  One atlas cell stores one glyph.
pub const DEFAULT_GLYPH_SIZE: u32 = 64;

/// UV rectangle and placement data for one glyph in the MSDF atlas.
///
/// The atlas bitmap covers a fixed em-space cell (`plane_bounds`, see
/// [`FontAtlas::cell_bounds`]), so every glyph quad maps the **whole** cell to
/// the atlas UV rectangle; the MSDF shader clips the actual ink.
#[derive(Debug, Clone, PartialEq)]
pub struct GlyphMetrics {
    /// Atlas UV rectangle `[u0, v0, u1, v1]`.
    pub uv: [f32; 4],
    /// Cell size in atlas texels `[width, height]`.
    pub size: [f32; 2],
    /// Horizontal advance in em units.
    pub advance: f32,
    /// Quad rectangle in em units relative to the pen position on the baseline:
    /// `[left, bottom, right, top]`, Y up.  This is the explicit em → cell
    /// mapping; it is the same for every glyph of an atlas.
    pub plane_bounds: [f32; 4],
    /// Ink rectangle in em units relative to the pen position on the baseline:
    /// `[left, bottom, right, top]`, Y up.  All zero for a glyph with no ink
    /// (for example a space).
    pub ink_bounds: [f32; 4],
}

/// A baked MSDF font atlas plus the data needed to lay text out on the CPU.
pub struct FontAtlas {
    /// GPU texture holding the MSDF cells.
    pub texture: wgpu::Texture,
    /// View of [`Self::texture`].
    pub view: wgpu::TextureView,
    /// Linear-filtering sampler for [`Self::view`].
    pub sampler: wgpu::Sampler,
    /// Per-character glyph metrics.
    pub metrics: HashMap<char, GlyphMetrics>,
    /// Cell size in texels.
    pub glyph_size: u32,
    /// Distance range used by `generate_msdf`, in atlas texels.  Shaders must
    /// use this value instead of hard-coding it.
    pub px_range: f32,
    /// Em-space rectangle covered by one atlas cell: `[left, bottom, right,
    /// top]`.  It is copied into every [`GlyphMetrics::plane_bounds`].
    pub cell_bounds: [f32; 4],
    /// Vertical metrics in em units.
    pub font_metrics: FontMetrics,
    /// Character → glyph index, used for kerning lookups during layout.
    pub glyph_indices: HashMap<char, u16>,
    /// `kern` format 0 pairs in em units keyed by glyph indices.
    pub kerning: HashMap<(u16, u16), f32>,
    /// GPOS `PairPos` kerning; wins over [`Self::kerning`] for a covered pair.
    pub gpos: GposKerning,
    /// The `.notdef` glyph cell, when the font has a non-empty glyph 0.
    pub notdef: Option<GlyphMetrics>,
    /// Key of the font bytes and character set this atlas was baked from.
    ///
    /// Two registrations with the same key produce the same atlas, which is
    /// what lets the renderer share one atlas between them.
    pub key: super::FontKey,
}

impl FontAtlas {
    /// Build an MSDF atlas for `chars` and upload it to the GPU.
    ///
    /// This is [`BakedAtlas::build`] followed by [`BakedAtlas::upload`]; use
    /// those directly to cache or share the CPU half.  Chapters are packed in
    /// the given order; each cell is [`DEFAULT_GLYPH_SIZE`] texels.  Glyph 0
    /// (`.notdef`) is appended as an extra cell when it has an outline, so
    /// missing characters can be drawn as a visible replacement.
    pub fn new<I: IntoIterator<Item = char>>(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        parser: &super::parser::FontParser,
        chars: I,
    ) -> Result<Self> {
        let char_list: Vec<char> = chars.into_iter().collect();
        Ok(BakedAtlas::build(parser, &char_list)?.upload(device, queue))
    }

    /// A pure-data view of this atlas for [`layout_line`].
    pub fn layout_font(&self) -> LayoutFont<'_> {
        LayoutFont {
            metrics: &self.metrics,
            notdef: self.notdef.as_ref(),
            glyph_indices: &self.glyph_indices,
            kerning: &self.kerning,
            gpos: &self.gpos,
            font: self.font_metrics,
        }
    }

    /// Lay out a single line using this atlas (see [`layout_line`]).
    pub fn layout_line(
        &self,
        text: &str,
        size_px: f32,
        align: TextAlign,
        out: &mut Vec<super::layout::GlyphQuad>,
    ) -> LineMetrics {
        layout_line(self.layout_font(), text, size_px, align, out)
    }

    /// Measure a single line using this atlas without allocating any quads
    /// (see [`measure_line`]).
    pub fn measure_line(&self, text: &str, size_px: f32, align: TextAlign) -> LineMetrics {
        measure_line(self.layout_font(), text, size_px, align)
    }

    /// Lay out a whole block (one or more `\n`-separated lines) into `out`
    /// (see [`layout_block`]).
    pub fn layout_block(
        &self,
        text: &str,
        style: TextStyle,
        out: &mut Vec<GlyphLayout>,
    ) -> TextMetrics {
        layout_block(self.layout_font(), text, style, out)
    }

    /// Measure a whole block without allocating glyph records (see
    /// [`measure_block`]).
    pub fn measure_block(&self, text: &str, style: TextStyle) -> TextMetrics {
        measure_block(self.layout_font(), text, style)
    }
}
