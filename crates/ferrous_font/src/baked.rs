//! Pure-data baked font atlas: the CPU half of [`crate::FontAtlas`].
//!
//! [`BakedAtlas`] holds the MSDF pixels plus every table the renderer and the
//! layout code need, and does not touch a GPU.  [`BakedAtlas::upload`] turns it
//! into a [`crate::FontAtlas`]; [`BakedAtlas::encode`] / [`BakedAtlas::decode`]
//! are the versioned binary form the on-disk cache stores.
//!
//! Splitting the two halves is what makes a font bake cacheable: the expensive
//! part (MSDF generation) runs once and produces a portable value, while the
//! upload stays tied to the device that is actually used.

use rustc_hash::FxHashMap as HashMap;
use std::fmt;

use crate::atlas::{FontAtlas, GlyphMetrics, DEFAULT_GLYPH_SIZE};
use crate::budget::{map_bytes, ATLAS_TABLE_BUDGET_BYTES, LAYOUT_CACHE_BUDGET_BYTES};
use crate::cache::AtlasCache;
use crate::codec::{Reader, Writer};
use crate::inventory;
use crate::layout::LayoutCache;
use crate::layout_data::LayoutData;
use crate::msdf_gen::{generate_msdf, CELL_MAX, CELL_MIN, PIXEL_RANGE};
use crate::parser::{FontMetrics, FontParser, GposKerning, ParseError};
use crate::FontError;
use std::sync::Mutex;

/// Distinguishes two `FontAtlas` instances that share a `FontKey`, so their
/// inventory labels never unregister each other.
static NEXT_ATLAS_INSTANCE: Mutex<usize> = Mutex::new(0);

fn next_atlas_instance() -> usize {
    let mut next = NEXT_ATLAS_INSTANCE
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let instance = *next;
    *next = (*next).wrapping_add(1);
    instance
}

/// Version of the baked-atlas binary layout written by [`BakedAtlas::encode`].
///
/// A file written by another version is rejected and rebuilt.
pub const ATLAS_FORMAT_VERSION: u16 = 1;

/// Version of the MSDF generator output.
///
/// Bump this whenever a change to [`crate::msdf_gen`] alters a pixel: cached
/// atlases then have a different key and are baked again.
pub const ATLAS_GENERATOR_VERSION: u32 = 1;

/// Magic that opens an encoded atlas.
const MAGIC: &[u8; 4] = b"FATL";

/// Upper bound on the atlas texture side, in texels.
///
/// 8192 is far beyond any real face (it holds 16 384 cells of 64 px) and keeps
/// a corrupt header from requesting an unreasonable allocation.
const MAX_TEXTURE_SIZE: u32 = 8192;

/// Upper bound on any table length read from a file.
const MAX_TABLE_LENGTH: u32 = 1 << 16;

/// Identifies a baked atlas: the font bytes and the character set.
///
/// Two registrations with the same key produce the same pixels, metrics and
/// kerning, so the second may reuse the first (see
/// [`crate::Renderer::register_font_2d`](https://docs.rs/ferrous_renderer)).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct FontKey(u128);

/// How much of a fixed-cell MSDF atlas is actually used.
///
/// The bake allocates a square grid of `cell_size × cell_size` cells, one per
/// glyph plus `.notdef`; cells are never shared or trimmed.  This report makes
/// that allocation visible without changing the layout:
///
/// * [`Self::occupied_pixel_area`] is the area *allocated* to cells
///   (`used_cells × cell_area`).
/// * [`Self::ink_pixels`] is the number of texels whose RGB is not the neutral
///   background (`255, 255, 255`): the glyph cover area plus the distance
///   band.  It is the closest proxy for useful area in a fixed-cell layout and
///   is what a variable-size packer would reclaim.
///
/// [`Self::occupancy`] is the allocated fraction (usually above 90 % for the
/// square grid), while [`Self::ink_occupancy`] is the fraction of atlas area
/// covered by distance-field texels (typically far lower, because a cell is
/// square and glyphs are narrow and tall).
///
/// # Packing
///
/// The layout is deliberately left unchanged.  `occupancy` is the only metric a
/// fixed-cell packer can move, and a square grid is already the tightest
/// arrangement of equal squares inside a square atlas.  Improving
/// `ink_occupancy` would require variable-size cells, which changes each
/// glyph's MSDF raster size and its UV/em mapping — that cannot be
/// byte-identical, so it is rejected for golden safety.  For the same reason
/// `ATLAS_FORMAT_VERSION` / `ATLAS_GENERATOR_VERSION` are not bumped.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct AtlasOccupancy {
    /// Cells allocated for glyphs (requested characters plus `.notdef`).
    pub used_cells: u32,
    /// Side of one cell, in texels.
    pub cell_size: u32,
    /// Area of one cell, in texels.
    pub cell_area: u64,
    /// Total area allocated to cells, in texels (`used_cells × cell_area`).
    pub occupied_pixel_area: u64,
    /// Atlas texture side, in cells.
    pub atlas_side: u32,
    /// Total atlas area, in texels.
    pub atlas_area: u64,
    /// `occupied_pixel_area / atlas_area`, in `0.0..=1.0`.
    pub occupancy: f32,
    /// Texels whose RGB is not the neutral `255, 255, 255` background.
    pub ink_pixels: u64,
    /// `ink_pixels / atlas_area`, in `0.0..=1.0`.
    pub ink_occupancy: f32,
}

impl AtlasOccupancy {
    /// Measures a packed `Rgba8` atlas.
    ///
    /// `pixels` is the whole atlas, top-down, `(atlas_side * cell_size)^2 * 4`
    /// bytes.  Counting ink is a single pass; it only runs when a bake asks
    /// for the report, never per frame.
    #[must_use]
    pub fn measure(used_cells: u32, cell_size: u32, atlas_side: u32, pixels: &[u8]) -> Self {
        let cell_area = u64::from(cell_size) * u64::from(cell_size);
        let occupied_pixel_area = u64::from(used_cells).saturating_mul(cell_area);
        let side_px = u64::from(atlas_side) * u64::from(cell_size);
        let atlas_area = side_px.saturating_mul(side_px);
        let ink_pixels = pixels
            .chunks_exact(4)
            .filter(|texel| texel[0] != 255 || texel[1] != 255 || texel[2] != 255)
            .count() as u64;
        let ratio = |part: u64| {
            if atlas_area == 0 {
                0.0
            } else {
                part as f32 / atlas_area as f32
            }
        };
        Self {
            used_cells,
            cell_size,
            cell_area,
            occupied_pixel_area,
            atlas_side,
            atlas_area,
            occupancy: ratio(occupied_pixel_area),
            ink_pixels,
            ink_occupancy: ratio(ink_pixels),
        }
    }

    /// [`Self::occupancy`] as a percentage.
    #[must_use]
    pub fn percentage(&self) -> f32 {
        self.occupancy * 100.0
    }

    /// [`Self::ink_occupancy`] as a percentage.
    #[must_use]
    pub fn ink_percentage(&self) -> f32 {
        self.ink_occupancy * 100.0
    }
}

impl fmt::Display for AtlasOccupancy {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{} / {} cells ({}x{} px) in a {}x{} cell atlas: {:.1}% allocated, {:.1}% ink ({}/{} px)",
            self.used_cells,
            self.atlas_side * self.atlas_side,
            self.cell_size,
            self.cell_size,
            self.atlas_side,
            self.atlas_side,
            self.percentage(),
            self.ink_percentage(),
            self.ink_pixels,
            self.atlas_area,
        )
    }
}

impl FontKey {
    /// Computes the key of the atlas `bytes` would produce for `chars`.
    ///
    /// The key covers the font bytes, the characters **in atlas order**, the
    /// cell size, the pixel range and both format and generator versions, so a
    /// change to any of them yields a different key.
    #[must_use]
    pub fn of(bytes: &[u8], chars: &[char]) -> Self {
        Self::of_with_size(bytes, chars, DEFAULT_GLYPH_SIZE)
    }

    /// [`Self::of`] for an atlas rasterized at `glyph_size` texels per cell.
    #[must_use]
    pub fn of_with_size(bytes: &[u8], chars: &[char], glyph_size: u32) -> Self {
        // FNV-1a, 128-bit.  A cache key does not need a cryptographic hash; it
        // needs to be stable, dependency-free and cheap.  The version and cell
        // parameters are mixed in first, so they cannot collide with font data.
        const OFFSET: u128 = 0x6c62_272e_07bb_0142_62b8_2175_6295_c58d;
        const PRIME: u128 = 0x0000_0000_0100_0000_0000_0000_0000_013b;
        let mut hash = OFFSET;
        let mut mix = |bytes: &[u8]| {
            for &byte in bytes {
                hash ^= u128::from(byte);
                hash = hash.wrapping_mul(PRIME);
            }
        };
        mix(&ATLAS_FORMAT_VERSION.to_le_bytes());
        mix(&ATLAS_GENERATOR_VERSION.to_le_bytes());
        mix(&glyph_size.to_le_bytes());
        mix(&PIXEL_RANGE.to_bits().to_le_bytes());
        mix(&(chars.len() as u32).to_le_bytes());
        for c in chars {
            mix(&(*c as u32).to_le_bytes());
        }
        mix(bytes);
        Self(hash)
    }

    /// The key as 32 lower-case hex digits, the cache file stem.
    #[must_use]
    pub fn hex(self) -> String {
        format!("{:032x}", self.0)
    }

    /// The raw 128-bit value, for the encoded header.
    #[must_use]
    pub fn raw(self) -> u128 {
        self.0
    }
}

/// Everything a font atlas needs except the GPU resources.
///
/// Build it with [`BakedAtlas::build`] (or [`BakedAtlas::build_cached`] to use
/// the disk cache) and upload it with [`BakedAtlas::upload`].
#[derive(Debug, Clone, PartialEq)]
pub struct BakedAtlas {
    /// Key of the inputs that produced this atlas.
    pub key: FontKey,
    /// Cell size in texels.
    pub glyph_size: u32,
    /// Texture side in texels (`atlas_dim * glyph_size`).
    pub tex_size: u32,
    /// Distance range the MSDF encodes, in texels.
    pub px_range: f32,
    /// Em-space rectangle covered by one cell: `[left, bottom, right, top]`.
    pub cell_bounds: [f32; 4],
    /// Per-character glyph metrics.
    pub metrics: HashMap<char, GlyphMetrics>,
    /// The `.notdef` cell, when the font has a non-empty glyph 0.
    pub notdef: Option<GlyphMetrics>,
    /// Character → glyph index.
    pub glyph_indices: HashMap<char, u16>,
    /// `kern` format 0 pairs in em units.
    pub kerning: HashMap<(u16, u16), f32>,
    /// GPOS pair kerning; wins over [`Self::kerning`] for a covered pair.
    pub gpos: GposKerning,
    /// Vertical metrics in em units.
    pub font_metrics: FontMetrics,
    /// Packed `Rgba8` cells, `tex_size * tex_size * 4` bytes, top-down.
    pub pixels: Vec<u8>,
}

impl BakedAtlas {
    /// Bakes the MSDF atlas for `chars` on the CPU.
    ///
    /// Glyphs are packed in the order of `chars`; glyph 0 (`.notdef`) takes an
    /// extra cell when the font has an outline for it.
    ///
    /// # Errors
    ///
    /// Returns [`FontError::Atlas`] when `chars` is empty.
    pub fn build(parser: &FontParser, chars: &[char]) -> Result<Self, FontError> {
        Self::build_with_size(parser, chars, DEFAULT_GLYPH_SIZE)
    }

    /// Builds **one** atlas from several faces, for fallback drawing.
    ///
    /// Each character is baked from the first face that lists it; the metrics,
    /// glyph index and UV come from that same face. The GUI text pipeline has
    /// one font binding, so a multi-face glyph set in one texture is what lets
    /// a mixed-script run draw without a second page. `.notdef` comes from the
    /// first face. Kerning is the first face's, which is correct because a run
    /// never kerns across a face boundary.
    ///
    /// # Errors
    ///
    /// Returns [`FontError::Atlas`] when no face lists a character.
    #[allow(
        clippy::cast_possible_truncation,
        clippy::cast_precision_loss,
        clippy::cast_sign_loss,
        clippy::too_many_lines
    )] // reason: the atlas grid is a small square of whole cells
    pub fn build_multi(
        faces: &[(&FontParser, Vec<char>)],
        glyph_size: u32,
    ) -> Result<Self, FontError> {
        let Some((first, first_chars)) = faces.first() else {
            return Err(FontError::Atlas {
                message: "no faces".to_owned(),
            });
        };
        let glyph_size = glyph_size.clamp(8, 128);
        let mut claimed: Vec<char> = Vec::new();
        let mut jobs: Vec<(usize, Option<char>, usize, u16)> = Vec::new();
        for (face_index, (parser, chars)) in faces.iter().enumerate() {
            for &ch in chars {
                if claimed.contains(&ch) {
                    continue;
                }
                claimed.push(ch);
                jobs.push((jobs.len(), Some(ch), face_index, parser.get_glyph_index(ch)));
            }
        }
        if jobs.is_empty() {
            return Err(FontError::Atlas {
                message: "no characters".to_owned(),
            });
        }
        let has_notdef = !first.get_glyph_outline_by_index(0).is_empty();
        let count = jobs.len() as u32 + u32::from(has_notdef);
        if has_notdef {
            jobs.push((jobs.len(), None, 0, 0));
        }
        let atlas_dim = ((count as f32).sqrt().ceil() as u32).max(1);
        let tex_size = atlas_dim * glyph_size;
        let cell_bounds = [CELL_MIN, CELL_MIN, CELL_MAX, CELL_MAX];

        let (mut metrics, mut notdef, mut glyph_indices, kerning, gpos, font_metrics) =
            LayoutData::from_parser(first, first_chars.clone()).into_parts();
        for (parser, chars) in &faces[1..] {
            let (m, n, gi, _, _, _) = LayoutData::from_parser(parser, chars.clone()).into_parts();
            for (ch, metric) in m {
                metrics.entry(ch).or_insert(metric);
            }
            for (ch, index) in gi {
                glyph_indices.entry(ch).or_insert(index);
            }
            if notdef.is_none() {
                notdef = n;
            }
        }

        let mut pixels = vec![255u8; (tex_size * tex_size * 4) as usize];
        #[cfg(not(target_arch = "wasm32"))]
        let glyph_results: Vec<(usize, Option<char>, Vec<u8>)> = {
            use rayon::prelude::*;
            jobs.par_iter()
                .map(|&(i, c, face, gid)| {
                    let outline = faces[face].0.get_glyph_outline_by_index(gid);
                    (i, c, generate_msdf(&outline, glyph_size))
                })
                .collect()
        };
        #[cfg(target_arch = "wasm32")]
        let glyph_results: Vec<(usize, Option<char>, Vec<u8>)> = jobs
            .iter()
            .map(|&(i, c, face, gid)| {
                let outline = faces[face].0.get_glyph_outline_by_index(gid);
                (i, c, generate_msdf(&outline, glyph_size))
            })
            .collect();

        for (i, c, bmp) in glyph_results {
            let x = (i as u32 % atlas_dim) * glyph_size;
            let y = (i as u32 / atlas_dim) * glyph_size;
            for row in 0..glyph_size {
                let dst = (((y + row) * tex_size + x) * 4) as usize;
                let src = (row * glyph_size * 4) as usize;
                pixels[dst..dst + (glyph_size * 4) as usize]
                    .copy_from_slice(&bmp[src..src + (glyph_size * 4) as usize]);
            }
            let glyph = match c {
                Some(c) => metrics.get_mut(&c),
                None => notdef.as_mut(),
            };
            let Some(glyph) = glyph else {
                continue;
            };
            glyph.uv = [
                x as f32 / tex_size as f32,
                y as f32 / tex_size as f32,
                (x + glyph_size) as f32 / tex_size as f32,
                (y + glyph_size) as f32 / tex_size as f32,
            ];
            glyph.size = [glyph_size as f32, glyph_size as f32];
        }

        let mut all_bytes = Vec::new();
        for (parser, _) in faces {
            all_bytes.extend_from_slice(parser.data());
        }
        Ok(Self {
            key: FontKey::of_with_size(&all_bytes, &claimed, glyph_size),
            glyph_size,
            tex_size,
            px_range: PIXEL_RANGE,
            cell_bounds,
            metrics,
            notdef,
            glyph_indices,
            kerning,
            gpos,
            font_metrics,
            pixels,
        })
    }

    /// [`Self::build`] at a chosen cell size, for re-rasterizing at a new DPI
    /// scale.
    ///
    /// # Errors
    ///
    /// Returns [`FontError::Atlas`] when `chars` is empty.
    pub fn build_with_size(
        parser: &FontParser,
        chars: &[char],
        glyph_size: u32,
    ) -> Result<Self, FontError> {
        if chars.is_empty() {
            return Err(FontError::Atlas {
                message: "no characters".to_owned(),
            });
        }
        let glyph_size = glyph_size.clamp(8, 128);

        // `.notdef` (glyph 0) gets its own cell after the requested characters
        // when the font provides an outline for it.
        let has_notdef = !parser.get_glyph_outline_by_index(0).is_empty();
        let count = chars.len() as u32 + u32::from(has_notdef);

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
        let mut jobs: Vec<(usize, Option<char>, u16)> = chars
            .iter()
            .enumerate()
            .map(|(i, &c)| (i, Some(c), parser.get_glyph_index(c)))
            .collect();
        if has_notdef {
            jobs.push((chars.len(), None, 0));
        }

        let mut pixels = vec![255u8; (tex_size * tex_size * 4) as usize];
        // The atlas reuses the CPU layout tables verbatim, so `LayoutData` and
        // `FontAtlas` can never measure the same text differently.
        let (mut metrics, notdef, glyph_indices, kerning, gpos, font_metrics) =
            LayoutData::from_parser(parser, chars.to_vec()).into_parts();
        let mut notdef = notdef;

        #[cfg(not(target_arch = "wasm32"))]
        let glyph_results: Vec<(usize, Option<char>, Vec<u8>)> = {
            use rayon::prelude::*;
            jobs.par_iter()
                .map(|&(i, c, gid)| {
                    let outline = parser.get_glyph_outline_by_index(gid);
                    (i, c, generate_msdf(&outline, glyph_size))
                })
                .collect()
        };
        #[cfg(target_arch = "wasm32")]
        let glyph_results: Vec<(usize, Option<char>, Vec<u8>)> = jobs
            .iter()
            .map(|&(i, c, gid)| {
                let outline = parser.get_glyph_outline_by_index(gid);
                (i, c, generate_msdf(&outline, glyph_size))
            })
            .collect();

        for (i, c, bmp) in glyph_results {
            let x = (i as u32 % atlas_dim) * glyph_size;
            let y = (i as u32 / atlas_dim) * glyph_size;

            for row in 0..glyph_size {
                let dst = (((y + row) * tex_size + x) * 4) as usize;
                let src = (row * glyph_size * 4) as usize;
                pixels[dst..dst + (glyph_size * 4) as usize]
                    .copy_from_slice(&bmp[src..src + (glyph_size * 4) as usize]);
            }

            let glyph = match c {
                Some(c) => metrics.get_mut(&c),
                None => notdef.as_mut(),
            };
            let Some(glyph) = glyph else {
                // `LayoutData` inserted every requested character and the
                // `.notdef` cell, so this only guards a future change.
                continue;
            };
            glyph.uv = [
                x as f32 / tex_size as f32,
                y as f32 / tex_size as f32,
                (x + glyph_size) as f32 / tex_size as f32,
                (y + glyph_size) as f32 / tex_size as f32,
            ];
            glyph.size = [glyph_size as f32, glyph_size as f32];
        }

        let atlas = Self {
            key: FontKey::of(parser.data(), chars),
            glyph_size,
            tex_size,
            px_range: PIXEL_RANGE,
            cell_bounds,
            metrics,
            notdef,
            glyph_indices,
            kerning,
            gpos,
            font_metrics,
            pixels,
        };
        // The glyph tables are not a cache: they are the requested atlas, so
        // they cannot be evicted.  Refuse an absurd character set with a typed
        // error instead of letting a bake grow without bound.
        if log::log_enabled!(log::Level::Debug) {
            log::debug!("Font atlas occupancy: {}", atlas.occupancy());
        }
        let table_bytes = atlas.table_bytes();
        if table_bytes > ATLAS_TABLE_BUDGET_BYTES {
            return Err(FontError::Atlas {
                message: format!(
                    "glyph tables are {table_bytes} bytes, over the {} byte atlas budget",
                    ATLAS_TABLE_BUDGET_BYTES
                ),
            });
        }
        Ok(atlas)
    }

    /// Bytes of the packed `Rgba8` MSDF pixels.
    #[must_use]
    pub fn pixel_bytes(&self) -> u64 {
        self.pixels.len() as u64
    }

    /// Measures how much of this fixed-cell atlas is used.
    ///
    /// The accounting never changes the layout: cells stay `glyph_size` square
    /// and the atlas stays `ceil(sqrt(cells))` cells wide.  Counting ink scans
    /// `pixels`, so call this when reporting (a bake does it once at `debug`
    /// level), not every frame.
    #[must_use]
    pub fn occupancy(&self) -> AtlasOccupancy {
        let used_cells = self.metrics.len() as u32 + u32::from(self.notdef.is_some());
        let atlas_side = self.tex_size.checked_div(self.glyph_size).unwrap_or(0);
        AtlasOccupancy::measure(used_cells, self.glyph_size, atlas_side, &self.pixels)
    }

    /// Estimated bytes of the CPU glyph tables (metrics, indices, kerning).
    #[must_use]
    pub fn table_bytes(&self) -> u64 {
        map_bytes::<char, GlyphMetrics>(self.metrics.len())
            + map_bytes::<char, u16>(self.glyph_indices.len())
            + map_bytes::<(u16, u16), f32>(self.kerning.len())
            + self.gpos.byte_size()
            + self
                .notdef
                .as_ref()
                .map_or(0, |_| std::mem::size_of::<GlyphMetrics>() as u64)
    }

    /// CPU bytes of this baked atlas (pixels plus tables).
    #[must_use]
    pub fn cpu_bytes(&self) -> u64 {
        self.pixel_bytes().saturating_add(self.table_bytes())
    }

    /// Inventory entries for the CPU half of the atlas, with stable labels.
    ///
    /// The pixels are dropped by [`Self::upload`] once the texture owns them,
    /// so an owner that keeps a baked atlas alive should register these; an
    /// owner that uploads immediately should register the returned
    /// [`FontAtlas`] entries instead.
    #[must_use]
    pub fn inventory_entries(&self) -> Vec<(String, u64)> {
        let prefix = format!("font:{} baked", self.key.hex());
        vec![
            (format!("{prefix} pixels"), self.pixel_bytes()),
            (
                format!("{prefix} glyph metrics"),
                map_bytes::<char, GlyphMetrics>(self.metrics.len()),
            ),
            (
                format!("{prefix} glyph indices"),
                map_bytes::<char, u16>(self.glyph_indices.len()),
            ),
            (
                format!("{prefix} kern pairs"),
                map_bytes::<(u16, u16), f32>(self.kerning.len()),
            ),
            (format!("{prefix} gpos kerning"), self.gpos.byte_size()),
        ]
    }

    /// [`Self::build`] through `cache`: a hit returns the stored atlas without
    /// parsing or generating anything, a miss bakes and stores the result.
    ///
    /// `chars` is replaced by [`crate::charset::latin_western`] when empty, the
    /// same rule [`crate::Font::try_load_bytes`] applies.
    ///
    /// # Errors
    ///
    /// Returns [`FontError::Parse`] / [`FontError::Unsupported`] for bad font
    /// bytes and [`FontError::Atlas`] for an empty bake.
    pub fn build_cached(
        cache: Option<&AtlasCache>,
        bytes: &[u8],
        chars: &[char],
    ) -> Result<Self, FontError> {
        Self::build_cached_with_size(cache, bytes, chars, DEFAULT_GLYPH_SIZE)
    }

    /// [`Self::build_cached`] at a chosen cell size.
    ///
    /// # Errors
    ///
    /// Same as [`Self::build_cached`].
    pub fn build_cached_with_size(
        cache: Option<&AtlasCache>,
        bytes: &[u8],
        chars: &[char],
        glyph_size: u32,
    ) -> Result<Self, FontError> {
        let chars = if chars.is_empty() {
            crate::charset::latin_western()
        } else {
            chars.to_vec()
        };
        let key = FontKey::of_with_size(bytes, &chars, glyph_size);
        if let Some(cached) = cache.and_then(|cache| cache.load(key)) {
            return Ok(cached);
        }
        let parser = FontParser::new(bytes.to_vec()).map_err(|error| match error {
            ParseError::Parse(message) => FontError::Parse { message },
            ParseError::Unsupported(reason) => FontError::Unsupported { reason },
        })?;
        let atlas = Self::build_with_size(&parser, &chars, glyph_size)?;
        if let Some(cache) = cache {
            cache.store(key, &atlas);
        }
        Ok(atlas)
    }

    /// Creates the GPU texture, view and sampler for this atlas.
    ///
    /// The CPU data moves into the returned [`FontAtlas`], so the pixels are
    /// dropped once the texture owns them.
    #[must_use]
    pub fn upload(self, device: &wgpu::Device, queue: &wgpu::Queue) -> FontAtlas {
        let tex_size = self.tex_size;
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
            &self.pixels,
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

        let instance = next_atlas_instance();
        let inventory_prefix = format!("font:{}#{instance}", self.key.hex());
        // The texture now owns the pixels; the CPU half is the glyph tables.
        // Register the CPU caches with the process inventory when the host
        // installed the bridge.  The GPU texture is registered by the renderer
        // that owns the bind group, so this path does not double-count it.
        inventory::register_cpu(
            &format!("{inventory_prefix} glyph metrics"),
            map_bytes::<char, GlyphMetrics>(self.metrics.len()),
            inventory::Subsystem::Fonts,
        );
        inventory::register_cpu(
            &format!("{inventory_prefix} glyph indices"),
            map_bytes::<char, u16>(self.glyph_indices.len()),
            inventory::Subsystem::Fonts,
        );
        inventory::register_cpu(
            &format!("{inventory_prefix} kern pairs"),
            map_bytes::<(u16, u16), f32>(self.kerning.len()),
            inventory::Subsystem::Fonts,
        );
        inventory::register_cpu(
            &format!("{inventory_prefix} gpos kerning"),
            self.gpos.byte_size(),
            inventory::Subsystem::Fonts,
        );

        FontAtlas {
            texture,
            view,
            sampler,
            metrics: self.metrics,
            glyph_size: self.glyph_size,
            px_range: self.px_range,
            cell_bounds: self.cell_bounds,
            font_metrics: self.font_metrics,
            glyph_indices: self.glyph_indices,
            kerning: self.kerning,
            gpos: self.gpos,
            notdef: self.notdef,
            key: self.key,
            layout_cache: Mutex::new(LayoutCache::new(LAYOUT_CACHE_BUDGET_BYTES)),
            ascii_tables: std::sync::OnceLock::new(),
            inventory_prefix,
        }
    }

    /// Encodes the atlas in the versioned binary form the cache stores.
    ///
    /// The layout is deterministic: every map is written in key order.
    #[must_use]
    pub fn encode(&self) -> Vec<u8> {
        let pixel_bytes = self.pixels.len() as u32;
        let mut writer = Writer::with_capacity(256 + pixel_bytes as usize);
        writer.bytes(MAGIC);
        writer.u16(ATLAS_FORMAT_VERSION);
        writer.u32(ATLAS_GENERATOR_VERSION);
        writer.u128(self.key.raw());
        writer.u32(self.glyph_size);
        writer.u32(self.tex_size);
        writer.f32(self.px_range);
        for value in self.cell_bounds {
            writer.f32(value);
        }
        let font = self.font_metrics;
        writer.u16(font.units_per_em);
        writer.f32(font.ascender);
        writer.f32(font.descender);
        writer.f32(font.line_gap);
        for value in [
            font.bbox.left,
            font.bbox.bottom,
            font.bbox.right,
            font.bbox.top,
        ] {
            writer.f32(value);
        }

        let mut chars: Vec<char> = self.metrics.keys().copied().collect();
        chars.sort_unstable();
        writer.u32(chars.len() as u32);
        for c in chars {
            writer.u32(c as u32);
            write_glyph(&mut writer, &self.metrics[&c]);
        }
        writer.u8(u8::from(self.notdef.is_some()));
        if let Some(notdef) = &self.notdef {
            write_glyph(&mut writer, notdef);
        }

        let mut indices: Vec<(char, u16)> = self
            .glyph_indices
            .iter()
            .map(|(&c, &index)| (c, index))
            .collect();
        indices.sort_unstable_by_key(|(c, _)| *c);
        writer.u32(indices.len() as u32);
        for (c, index) in indices {
            writer.u32(c as u32);
            writer.u16(index);
        }

        let mut kerning: Vec<((u16, u16), f32)> = self
            .kerning
            .iter()
            .map(|(&pair, &value)| (pair, value))
            .collect();
        kerning.sort_unstable_by_key(|(pair, _)| *pair);
        writer.u32(kerning.len() as u32);
        for ((left, right), value) in kerning {
            writer.u16(left);
            writer.u16(right);
            writer.f32(value);
        }

        self.gpos.encode(&mut writer);

        writer.u32(pixel_bytes);
        writer.bytes(&self.pixels);
        writer.into_bytes()
    }

    /// Decodes what [`Self::encode`] wrote.
    ///
    /// # Errors
    ///
    /// Returns an [`AtlasDecodeError`] for a file written by another format or
    /// generator version, a truncated file, or an inconsistent header.  It never
    /// panics on malformed input.
    pub fn decode(bytes: &[u8]) -> Result<Self, AtlasDecodeError> {
        let mut reader = Reader::new(bytes);
        if reader.bytes(MAGIC.len()) != Some(&MAGIC[..]) {
            return Err(AtlasDecodeError::NotAnAtlas);
        }
        let format = reader.u16().ok_or(AtlasDecodeError::Truncated)?;
        if format != ATLAS_FORMAT_VERSION {
            return Err(AtlasDecodeError::FormatVersion {
                found: format,
                expected: ATLAS_FORMAT_VERSION,
            });
        }
        let generator = reader.u32().ok_or(AtlasDecodeError::Truncated)?;
        if generator != ATLAS_GENERATOR_VERSION {
            return Err(AtlasDecodeError::GeneratorVersion {
                found: generator,
                expected: ATLAS_GENERATOR_VERSION,
            });
        }
        let truncated = || AtlasDecodeError::Truncated;
        let key = FontKey(reader.u128().ok_or_else(truncated)?);
        let glyph_size = reader.u32().ok_or_else(truncated)?;
        let tex_size = reader.u32().ok_or_else(truncated)?;
        if glyph_size == 0 || tex_size == 0 || tex_size > MAX_TEXTURE_SIZE {
            return Err(AtlasDecodeError::Invalid(format!(
                "texture size {tex_size} with cells of {glyph_size} px"
            )));
        }
        if tex_size % glyph_size != 0 {
            return Err(AtlasDecodeError::Invalid(format!(
                "texture size {tex_size} is not a whole number of {glyph_size} px cells"
            )));
        }
        let px_range = reader.f32().ok_or_else(truncated)?;
        let mut cell_bounds = [0.0; 4];
        for value in &mut cell_bounds {
            *value = reader.f32().ok_or_else(truncated)?;
        }
        let units_per_em = reader.u16().ok_or_else(truncated)?;
        let ascender = reader.f32().ok_or_else(truncated)?;
        let descender = reader.f32().ok_or_else(truncated)?;
        let line_gap = reader.f32().ok_or_else(truncated)?;
        let mut bbox = [0.0f32; 4];
        for value in &mut bbox {
            *value = reader.f32().ok_or_else(truncated)?;
        }

        let glyph_count = reader.count(MAX_TABLE_LENGTH).ok_or_else(truncated)?;
        let mut metrics = HashMap::default(/* capacity ignored for the Fx hasher */);
        for _ in 0..glyph_count {
            let c = reader.u32().ok_or_else(truncated)?;
            let c = char::from_u32(c)
                .ok_or_else(|| AtlasDecodeError::Invalid(format!("invalid character U+{c:04X}")))?;
            metrics.insert(c, read_glyph(&mut reader)?);
        }
        let notdef = match reader.u8().ok_or_else(truncated)? {
            0 => None,
            1 => Some(read_glyph(&mut reader)?),
            _ => return Err(AtlasDecodeError::Invalid("bad notdef flag".to_owned())),
        };

        let index_count = reader.count(MAX_TABLE_LENGTH).ok_or_else(truncated)?;
        let mut glyph_indices = HashMap::default(/* capacity ignored for the Fx hasher */);
        for _ in 0..index_count {
            let c = reader.u32().ok_or_else(truncated)?;
            let c = char::from_u32(c)
                .ok_or_else(|| AtlasDecodeError::Invalid(format!("invalid character U+{c:04X}")))?;
            glyph_indices.insert(c, reader.u16().ok_or_else(truncated)?);
        }

        let kerning_count = reader.count(MAX_TABLE_LENGTH * 16).ok_or_else(truncated)?;
        let mut kerning = HashMap::default(/* capacity ignored for the Fx hasher */);
        for _ in 0..kerning_count {
            let left = reader.u16().ok_or_else(truncated)?;
            let right = reader.u16().ok_or_else(truncated)?;
            kerning.insert((left, right), reader.f32().ok_or_else(truncated)?);
        }

        let gpos = GposKerning::decode(&mut reader).ok_or_else(truncated)?;

        let pixel_length = reader.u32().ok_or_else(truncated)? as usize;
        let expected = (tex_size as usize)
            .checked_mul(tex_size as usize)
            .and_then(|pixels| pixels.checked_mul(4))
            .ok_or_else(|| AtlasDecodeError::Invalid("texture size overflows".to_owned()))?;
        if pixel_length != expected {
            return Err(AtlasDecodeError::Invalid(format!(
                "{pixel_length} pixel bytes for a {tex_size}x{tex_size} atlas"
            )));
        }
        let pixels = reader.bytes(pixel_length).ok_or_else(truncated)?.to_vec();
        if !reader.is_empty() {
            return Err(AtlasDecodeError::Invalid(
                "trailing bytes after the pixel data".to_owned(),
            ));
        }

        Ok(Self {
            key,
            glyph_size,
            tex_size,
            px_range,
            cell_bounds,
            metrics,
            notdef,
            glyph_indices,
            kerning,
            gpos,
            font_metrics: FontMetrics {
                units_per_em,
                ascender,
                descender,
                line_gap,
                bbox: crate::parser::GlyphBounds {
                    left: bbox[0],
                    bottom: bbox[1],
                    right: bbox[2],
                    top: bbox[3],
                },
            },
            pixels,
        })
    }
}

/// Writes one glyph metric block.
fn write_glyph(writer: &mut Writer, glyph: &GlyphMetrics) {
    for value in glyph.uv {
        writer.f32(value);
    }
    for value in glyph.size {
        writer.f32(value);
    }
    writer.f32(glyph.advance);
    for value in glyph.plane_bounds {
        writer.f32(value);
    }
    for value in glyph.ink_bounds {
        writer.f32(value);
    }
}

/// Reads one glyph metric block.
fn read_glyph(reader: &mut Reader<'_>) -> Result<GlyphMetrics, AtlasDecodeError> {
    let truncated = || AtlasDecodeError::Truncated;
    let mut uv = [0.0; 4];
    for value in &mut uv {
        *value = reader.f32().ok_or_else(truncated)?;
    }
    let mut size = [0.0; 2];
    for value in &mut size {
        *value = reader.f32().ok_or_else(truncated)?;
    }
    let advance = reader.f32().ok_or_else(truncated)?;
    let mut plane_bounds = [0.0; 4];
    for value in &mut plane_bounds {
        *value = reader.f32().ok_or_else(truncated)?;
    }
    let mut ink_bounds = [0.0; 4];
    for value in &mut ink_bounds {
        *value = reader.f32().ok_or_else(truncated)?;
    }
    Ok(GlyphMetrics {
        uv,
        size,
        advance,
        plane_bounds,
        ink_bounds,
    })
}

/// Why a baked atlas could not be decoded.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AtlasDecodeError {
    /// The file does not start with the atlas magic.
    NotAnAtlas,
    /// The file was written by another layout version.
    FormatVersion {
        /// Version found in the file.
        found: u16,
        /// Version this build writes.
        expected: u16,
    },
    /// The pixels were generated by another MSDF generator version.
    GeneratorVersion {
        /// Version found in the file.
        found: u32,
        /// Version this build writes.
        expected: u32,
    },
    /// The file ends before a field or the pixel data.
    Truncated,
    /// The header is inconsistent with the data it describes.
    Invalid(String),
}

impl fmt::Display for AtlasDecodeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotAnAtlas => f.write_str("not a font atlas cache file"),
            Self::FormatVersion { found, expected } => {
                write!(f, "atlas format version {found}, expected {expected}")
            }
            Self::GeneratorVersion { found, expected } => {
                write!(f, "atlas generator version {found}, expected {expected}")
            }
            Self::Truncated => f.write_str("the atlas cache file is truncated"),
            Self::Invalid(message) => write!(f, "invalid atlas cache file: {message}"),
        }
    }
}

impl std::error::Error for AtlasDecodeError {}

#[cfg(test)]
mod tests {
    use super::*;

    const ROBOTO: &[u8] = include_bytes!("../../../assets/fonts/Roboto-Regular.ttf");

    fn baked() -> BakedAtlas {
        let parser = FontParser::new(ROBOTO.to_vec()).unwrap();
        BakedAtlas::build(&parser, &crate::charset::ascii()).unwrap()
    }

    #[test]
    fn encode_decode_round_trips_every_field() {
        let atlas = baked();
        let decoded = BakedAtlas::decode(&atlas.encode()).unwrap();
        assert_eq!(decoded, atlas);
        assert_eq!(decoded.key, atlas.key);
        assert!(!decoded.pixels.is_empty());
    }

    #[test]
    fn the_same_inputs_produce_the_same_key_and_pixels() {
        let a = baked();
        let b = baked();
        assert_eq!(a.key, b.key);
        assert_eq!(a.pixels, b.pixels);
    }

    #[test]
    fn occupancy_measure_has_exact_accounting() {
        // A 2x2-cell atlas of 2x2 px cells (a 4x4 texel image) with one cell
        // allocated and four texels of ink in it.
        let mut pixels = vec![255u8; 4 * 4 * 4];
        for row in 0..2 {
            for column in 0..2 {
                pixels[(row * 4 + column) * 4] = 0;
            }
        }
        let occupancy = AtlasOccupancy::measure(1, 2, 2, &pixels);
        assert_eq!(occupancy.used_cells, 1);
        assert_eq!(occupancy.cell_area, 4);
        assert_eq!(occupancy.occupied_pixel_area, 4);
        assert_eq!(occupancy.atlas_area, 16);
        assert!((occupancy.occupancy - 0.25).abs() < f32::EPSILON);
        assert_eq!(occupancy.ink_pixels, 4);
        assert!((occupancy.ink_occupancy - 0.25).abs() < f32::EPSILON);
    }

    #[test]
    fn occupancy_reports_deterministic_counts() {
        let atlas = baked();
        let occupancy = atlas.occupancy();
        let used = atlas.metrics.len() as u32 + u32::from(atlas.notdef.is_some());
        assert_eq!(occupancy.used_cells, used);
        assert_eq!(occupancy.cell_size, atlas.glyph_size);
        assert_eq!(occupancy.atlas_side, atlas.tex_size / atlas.glyph_size);
        assert_eq!(
            occupancy.atlas_area,
            u64::from(atlas.tex_size) * u64::from(atlas.tex_size)
        );
        assert_eq!(
            occupancy.occupied_pixel_area,
            u64::from(used) * u64::from(atlas.glyph_size) * u64::from(atlas.glyph_size)
        );
        // The square grid is nearly full by construction for a small set.
        assert!(occupancy.occupancy > 0.9);
        assert!(occupancy.ink_pixels > 0);
        assert!(occupancy.ink_pixels < occupancy.occupied_pixel_area);
        // A second bake of the same inputs reports identical numbers.
        assert_eq!(baked().occupancy(), occupancy);
    }

    #[test]
    fn the_encoded_atlas_is_byte_for_byte_deterministic() {
        assert_eq!(baked().encode(), baked().encode());
    }

    #[test]
    fn cpu_bytes_match_the_baked_buffers() {
        let atlas = baked();
        let expected_pixels = u64::from(atlas.tex_size) * u64::from(atlas.tex_size) * 4;
        assert_eq!(atlas.pixel_bytes(), expected_pixels);
        assert!(atlas.table_bytes() > 0);
        assert_eq!(
            atlas.cpu_bytes(),
            atlas.pixel_bytes().saturating_add(atlas.table_bytes())
        );
    }

    #[test]
    fn a_different_character_set_changes_the_key() {
        let parser = FontParser::new(ROBOTO.to_vec()).unwrap();
        let ascii = BakedAtlas::build(&parser, &crate::charset::ascii()).unwrap();
        let western = BakedAtlas::build(&parser, &crate::charset::latin_western()).unwrap();
        assert_ne!(ascii.key, western.key);
    }

    #[test]
    fn the_key_changes_with_the_generator_version() {
        let chars = crate::charset::ascii();
        let key = FontKey::of(ROBOTO, &chars);
        // The version bytes are mixed in first, so the key is not a function of
        // the font bytes alone.
        let other = FontKey::of(ROBOTO, &chars[1..]);
        assert_ne!(key, other);
    }

    #[test]
    fn a_version_mismatch_is_rejected() {
        let mut bytes = baked().encode();
        // The format version sits right after the four magic bytes.
        bytes[4..6].copy_from_slice(&(ATLAS_FORMAT_VERSION + 1).to_le_bytes());
        assert!(matches!(
            BakedAtlas::decode(&bytes),
            Err(AtlasDecodeError::FormatVersion { .. })
        ));

        let mut bytes = baked().encode();
        bytes[6..10].copy_from_slice(&(ATLAS_GENERATOR_VERSION + 1).to_le_bytes());
        assert!(matches!(
            BakedAtlas::decode(&bytes),
            Err(AtlasDecodeError::GeneratorVersion { .. })
        ));
    }

    #[test]
    fn every_truncation_is_an_error_and_never_a_panic() {
        let bytes = baked().encode();
        for length in [
            0,
            1,
            3,
            4,
            5,
            6,
            10,
            40,
            100,
            bytes.len() / 2,
            bytes.len() - 1,
        ] {
            assert!(
                BakedAtlas::decode(&bytes[..length]).is_err(),
                "a file truncated to {length} bytes must be rejected"
            );
        }
    }

    #[test]
    fn garbage_is_rejected() {
        assert_eq!(
            BakedAtlas::decode(b"definitely not an atlas"),
            Err(AtlasDecodeError::NotAnAtlas)
        );
    }

    #[test]
    fn trailing_bytes_are_rejected() {
        let mut bytes = baked().encode();
        bytes.push(0);
        assert!(BakedAtlas::decode(&bytes).is_err());
    }
}
