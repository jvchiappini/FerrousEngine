//! Atlas packing and GPU texture management for glyph MSDFs.

use super::layout::{
    layout_block, layout_line, measure_block, measure_line, GlyphLayout, LayoutFont, LineMetrics,
    TextAlign, TextMetrics, TextStyle,
};
use super::msdf_gen::{generate_msdf, CELL_MAX, CELL_MIN, PIXEL_RANGE};
use super::parser::{FontMetrics, GposKerning};
use anyhow::Result;
use std::collections::HashMap;
// rayon is used to parallelize glyph MSDF generation and metrics computation
#[cfg(not(target_arch = "wasm32"))]
use rayon::prelude::*;

/// One baked glyph: cell index, optional character (`None` = `.notdef`),
/// MSDF bitmap, advance and glyph index.
type GlyphBake = (usize, Option<char>, Vec<u8>, f32, u16);

/// Default MSDF cell size in texels.  One atlas cell stores one glyph.
pub const DEFAULT_GLYPH_SIZE: u32 = 64;

/// UV rectangle and placement data for one glyph in the MSDF atlas.
///
/// The atlas bitmap covers a fixed em-space cell (`plane_bounds`, see
/// [`FontAtlas::cell_bounds`]), so every glyph quad maps the **whole** cell to
/// the atlas UV rectangle; the MSDF shader clips the actual ink.
#[derive(Debug, Clone)]
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
}

impl FontAtlas {
    /// Build an MSDF atlas for `chars` and upload it to the GPU.
    ///
    /// Chapters are packed in the given order; each cell is
    /// [`DEFAULT_GLYPH_SIZE`] texels.  Glyph 0 (`.notdef`) is appended as an
    /// extra cell when it has an outline, so missing characters can be drawn as
    /// a visible replacement.
    pub fn new<I: IntoIterator<Item = char>>(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        parser: &super::parser::FontParser,
        chars: I,
    ) -> Result<Self> {
        let glyph_size = DEFAULT_GLYPH_SIZE;
        let char_list: Vec<char> = chars.into_iter().collect();
        if char_list.is_empty() {
            return Err(anyhow::anyhow!("no characters"));
        }

        // `.notdef` (glyph 0) gets its own cell after the requested characters
        // when the font provides an outline for it.
        let notdef_outline = parser.get_glyph_outline_by_index(0);
        let has_notdef = !notdef_outline.is_empty();
        let count = char_list.len() as u32 + u32::from(has_notdef);

        let atlas_dim = ((count as f32).sqrt().ceil() as u32).max(1);
        let tex_size = atlas_dim * glyph_size;
        log::debug!(
            "Building atlas {}x{} for {} glyphs",
            tex_size,
            tex_size,
            count
        );
        let cell_bounds = [CELL_MIN, CELL_MIN, CELL_MAX, CELL_MAX];

        // Jobs: `(cell index, character, glyph index)`.  `None` is `.notdef`.
        let mut jobs: Vec<(usize, Option<char>, u16)> = char_list
            .iter()
            .enumerate()
            .map(|(i, &c)| (i, Some(c), parser.get_glyph_index(c)))
            .collect();
        if has_notdef {
            jobs.push((char_list.len(), None, 0));
        }

        let mut pixels = vec![255u8; (tex_size * tex_size * 4) as usize];
        let mut metrics = HashMap::new();
        let mut notdef = None;

        #[cfg(not(target_arch = "wasm32"))]
        let glyph_results: Vec<GlyphBake> = jobs
            .par_iter()
            .map(|&(i, c, gid)| {
                let outline = parser.get_glyph_outline_by_index(gid);
                let advance = parser.get_glyph_advance_by_index(gid);
                let bmp = generate_msdf(&outline, glyph_size);
                (i, c, bmp, advance, gid)
            })
            .collect();
        #[cfg(target_arch = "wasm32")]
        let glyph_results: Vec<GlyphBake> = jobs
            .iter()
            .map(|&(i, c, gid)| {
                let outline = parser.get_glyph_outline_by_index(gid);
                let advance = parser.get_glyph_advance_by_index(gid);
                let bmp = generate_msdf(&outline, glyph_size);
                (i, c, bmp, advance, gid)
            })
            .collect();

        let mut glyph_indices = HashMap::new();
        for (i, c, bmp, advance, gid) in glyph_results {
            let x = (i as u32 % atlas_dim) * glyph_size;
            let y = (i as u32 / atlas_dim) * glyph_size;

            for row in 0..glyph_size {
                let dst = (((y + row) * tex_size + x) * 4) as usize;
                let src = (row * glyph_size * 4) as usize;
                pixels[dst..dst + (glyph_size * 4) as usize]
                    .copy_from_slice(&bmp[src..src + (glyph_size * 4) as usize]);
            }

            let ink = parser.get_glyph_bounds_by_index(gid).unwrap_or_default();
            let glyph = GlyphMetrics {
                uv: [
                    x as f32 / tex_size as f32,
                    y as f32 / tex_size as f32,
                    (x + glyph_size) as f32 / tex_size as f32,
                    (y + glyph_size) as f32 / tex_size as f32,
                ],
                size: [glyph_size as f32, glyph_size as f32],
                advance,
                plane_bounds: cell_bounds,
                ink_bounds: [ink.left, ink.bottom, ink.right, ink.top],
            };
            if let Some(c) = c {
                glyph_indices.insert(c, gid);
                metrics.insert(c, glyph);
            } else {
                notdef = Some(glyph);
            }
        }

        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("FontAtlas"),
            size: wgpu::Extent3d {
                width: tex_size,
                height: tex_size,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        queue.write_texture(
            wgpu::ImageCopyTexture {
                texture: &texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            &pixels,
            wgpu::ImageDataLayout {
                offset: 0,
                bytes_per_row: Some(tex_size * 4),
                rows_per_image: None,
            },
            wgpu::Extent3d {
                width: tex_size,
                height: tex_size,
                depth_or_array_layers: 1,
            },
        );

        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("FontAtlas Sampler"),
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });

        Ok(FontAtlas {
            texture,
            view,
            sampler,
            metrics,
            glyph_size,
            px_range: PIXEL_RANGE,
            cell_bounds,
            font_metrics: parser.metrics(),
            glyph_indices,
            kerning: parser.kerning_pairs().clone(),
            gpos: parser.gpos_kerning().clone(),
            notdef,
        })
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
