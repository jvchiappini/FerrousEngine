//! Atlas packing and GPU texture management for glyph MSDFs.
//!
//! The CPU half of an atlas — pixels and layout tables — is
//! [`crate::BakedAtlas`]; this module adds the GPU texture the renderer samples.
//! [`FontAtlas::new`] keeps the one-call behaviour of building and uploading.

use super::budget::map_bytes;
use super::inventory;
use super::layout::{
    layout_block, layout_line, measure_block, measure_line, GlyphLayout, LayoutCache, LayoutFont,
    LineMetrics, TextAlign, TextMetrics, TextStyle,
};
use super::parser::{FontMetrics, GposKerning};
use super::BakedAtlas;
use anyhow::Result;
use rustc_hash::FxHashMap as HashMap;
use std::sync::Mutex;

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
    /// LRU byte-budgeted cache of block layouts.
    pub(crate) layout_cache: Mutex<LayoutCache>,
    /// Stable prefix for every inventory label this atlas owns.
    pub(crate) inventory_prefix: String,
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
        // A hit copies the exact records the uncached path produced; the
        // output is bit-identical, only the per-character work is skipped.
        let cached_metrics = {
            let mut cache = self
                .layout_cache
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            cache.get(text, style).map(|entry| {
                out.clear();
                out.extend_from_slice(&entry.glyphs);
                entry.metrics.clone()
            })
        };
        if let Some(metrics) = cached_metrics {
            return metrics;
        }

        let metrics = layout_block(self.layout_font(), text, style, out);
        let bytes = {
            let mut cache = self
                .layout_cache
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            cache.insert(text, style, out, &metrics)
        };
        if bytes > 0 {
            inventory::register_cpu(
                &format!("{} layout cache", self.inventory_prefix),
                bytes,
                inventory::Subsystem::Fonts,
            );
        }
        metrics
    }

    /// Bytes of the GPU texture this atlas owns (`Rgba8`, one mip level).
    #[must_use]
    pub fn gpu_bytes(&self) -> u64 {
        let width = u64::from(self.texture.width());
        let height = u64::from(self.texture.height());
        width * height * 4
    }

    /// Bytes of the CPU glyph tables this atlas keeps.
    #[must_use]
    pub fn table_bytes(&self) -> u64 {
        self.metrics_bytes()
            + map_bytes::<char, u16>(self.glyph_indices.len())
            + map_bytes::<(u16, u16), f32>(self.kerning.len())
            + self.gpos.byte_size()
            + self
                .notdef
                .as_ref()
                .map_or(0, |_| std::mem::size_of::<GlyphMetrics>() as u64)
    }

    /// Bytes of the per-character metric map.
    #[must_use]
    pub fn metrics_bytes(&self) -> u64 {
        map_bytes::<char, GlyphMetrics>(self.metrics.len())
    }

    /// Bytes currently charged to the layout cache.
    #[must_use]
    pub fn layout_cache_bytes(&self) -> u64 {
        self.layout_cache
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .bytes()
    }

    /// CPU + GPU bytes this atlas owns, for a memory report.
    #[must_use]
    pub fn cpu_bytes(&self) -> u64 {
        self.table_bytes() + self.layout_cache_bytes()
    }

    /// Every inventory entry this atlas owns, with stable labels.
    ///
    /// A caller that has the `ferrous_core` inventory available can forward
    /// these with `register_cpu` / `register_gpu`; the engine's `GuiRenderer`
    /// installs the bridge so this happens automatically.
    #[must_use]
    pub fn inventory_entries(&self) -> Vec<(String, u64)> {
        vec![
            (
                format!("{} texture", self.inventory_prefix),
                self.gpu_bytes(),
            ),
            (
                format!("{} glyph metrics", self.inventory_prefix),
                self.metrics_bytes(),
            ),
            (
                format!("{} glyph indices", self.inventory_prefix),
                map_bytes::<char, u16>(self.glyph_indices.len()),
            ),
            (
                format!("{} kern pairs", self.inventory_prefix),
                map_bytes::<(u16, u16), f32>(self.kerning.len()),
            ),
            (
                format!("{} gpos kerning", self.inventory_prefix),
                self.gpos.byte_size(),
            ),
            (
                format!("{} layout cache", self.inventory_prefix),
                self.layout_cache_bytes(),
            ),
        ]
    }

    /// Measure a whole block without allocating glyph records (see
    /// [`measure_block`]).
    pub fn measure_block(&self, text: &str, style: TextStyle) -> TextMetrics {
        measure_block(self.layout_font(), text, style)
    }
}

impl Drop for FontAtlas {
    fn drop(&mut self) {
        let prefix = &self.inventory_prefix;
        inventory::unregister_gpu(&format!("{prefix} texture"), inventory::Subsystem::Fonts);
        inventory::unregister_cpu(
            &format!("{prefix} glyph metrics"),
            inventory::Subsystem::Fonts,
        );
        inventory::unregister_cpu(
            &format!("{prefix} glyph indices"),
            inventory::Subsystem::Fonts,
        );
        inventory::unregister_cpu(&format!("{prefix} kern pairs"), inventory::Subsystem::Fonts);
        inventory::unregister_cpu(
            &format!("{prefix} gpos kerning"),
            inventory::Subsystem::Fonts,
        );
        inventory::unregister_cpu(
            &format!("{prefix} layout cache"),
            inventory::Subsystem::Fonts,
        );
    }
}
