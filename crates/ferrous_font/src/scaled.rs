//! One glyph atlas, re-rasterized at the current DPI scale.
//!
//! The MSDF cell is a fixed 64 texels by default. On a 1.5x or 2x monitor the
//! same cell is stretched over more device pixels, so the signed distance field
//! loses resolution exactly where it matters (thin stems, hairlines). The fix
//! is to bake the atlas at `round(base * scale)` texels per cell and **replace**
//! the previous scale's atlas: keeping both would double the GPU bytes for no
//! benefit, and the old scale is never sampled again.
//!
//! [`ScaledAtlases`] owns that one active atlas. A host asks for a scale on
//! every scale-change event; a different cell size rebuilds and drops the old
//! `BakedAtlas`. The caller uploads it (`BakedAtlas::upload`) and its GPU bytes
//! are `tex_size^2 * 4`, never the sum of scales.

#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    clippy::cast_sign_loss
)] // reason: the cell size is a small clamped pixel count

use crate::atlas::DEFAULT_GLYPH_SIZE;
use crate::baked::BakedAtlas;
use crate::FontError;

/// The cell size for a DPI scale, clamped to the rasterizer's range.
#[must_use]
#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
// reason: `scale` is a small positive DPI factor and the result is clamped.
pub fn cell_size_for_scale(scale: f32) -> u32 {
    let base = DEFAULT_GLYPH_SIZE as f32;
    (base * scale.max(0.25)).round().clamp(8.0, 128.0) as u32
}

/// The one glyph atlas active at the current DPI scale.
#[derive(Debug, Default)]
pub struct ScaledAtlases {
    active: Option<(u32, BakedAtlas)>,
}

impl ScaledAtlases {
    /// Builds an empty cache; the first [`Self::select`] bakes.
    #[must_use]
    pub fn new() -> Self {
        Self { active: None }
    }

    /// The active cell size, if one was baked.
    #[must_use]
    pub fn cell_size(&self) -> Option<u32> {
        self.active.as_ref().map(|(cell, _)| *cell)
    }

    /// The active atlas.
    #[must_use]
    pub fn atlas(&self) -> Option<&BakedAtlas> {
        self.active.as_ref().map(|(_, atlas)| atlas)
    }

    /// The active atlas's GPU bytes (`tex_size^2 * 4`).
    #[must_use]
    pub fn gpu_bytes(&self) -> u64 {
        self.active.as_ref().map_or(0, |(_, atlas)| {
            u64::from(atlas.tex_size) * u64::from(atlas.tex_size) * 4
        })
    }

    /// The bytes a fresh bake at `scale` would use, without touching the cache.
    ///
    /// # Errors
    ///
    /// Same as [`BakedAtlas::build_with_size`].
    pub fn fresh_bytes(scale: f32, bytes: &[u8], chars: &[char]) -> Result<u64, FontError> {
        let cell = cell_size_for_scale(scale);
        let fresh = BakedAtlas::build_cached_with_size(None, bytes, chars, cell)?;
        Ok(u64::from(fresh.tex_size) * u64::from(fresh.tex_size) * 4)
    }

    /// Returns the atlas for `scale`, baking and **evicting** the previous
    /// scale when the cell size changes.
    ///
    /// # Errors
    ///
    /// Same as [`BakedAtlas::build_with_size`].
    pub fn select(
        &mut self,
        scale: f32,
        bytes: &[u8],
        chars: &[char],
    ) -> Result<&BakedAtlas, FontError> {
        let cell = cell_size_for_scale(scale);
        let rebuild = self
            .active
            .as_ref()
            .is_none_or(|(active, _)| *active != cell);
        if rebuild {
            let atlas = BakedAtlas::build_cached_with_size(None, bytes, chars, cell)?;
            // The previous scale is dropped here: never keep two.
            self.active = Some((cell, atlas));
        }
        Ok(&self.active.as_ref().expect("just baked").1)
    }
}

#[cfg(test)]
mod tests {
    use super::{cell_size_for_scale, ScaledAtlases};
    use crate::charset;

    const ROBOTO: &[u8] = include_bytes!("../../../assets/fonts/Roboto-Regular.ttf");

    #[test]
    fn the_cell_grows_with_the_scale_and_is_clamped() {
        assert_eq!(cell_size_for_scale(1.0), 64);
        assert_eq!(cell_size_for_scale(1.5), 96);
        assert_eq!(cell_size_for_scale(2.0), 128);
        assert_eq!(cell_size_for_scale(0.01), 16);
        assert_eq!(cell_size_for_scale(100.0), 128);
    }

    #[test]
    fn switching_scale_evicts_the_old_atlas() {
        let chars = charset::latin_western();
        let mut cache = ScaledAtlases::new();
        let mut per_scale = Vec::new();
        for scale in [1.0_f32, 1.5, 2.0] {
            let atlas = cache.select(scale, ROBOTO, &chars).expect("bake");
            assert_eq!(Some(atlas.glyph_size), cache.cell_size());
            let bytes = cache.gpu_bytes();
            let fresh = ScaledAtlases::fresh_bytes(scale, ROBOTO, &chars).expect("fresh");
            assert!(
                bytes <= fresh,
                "scale {scale}: switched atlas {bytes} bytes > fresh {fresh}"
            );
            eprintln!(
                "atlas scale {scale}: cell {} bytes {bytes}",
                cache.cell_size().unwrap_or(0)
            );
            per_scale.push(bytes);
        }
        // After 1.0 -> 1.5 -> 2.0 only the 2.0 atlas is alive: the total is the
        // last scale's bytes, not the sum of the three.
        assert_eq!(cache.gpu_bytes(), *per_scale.last().expect("a scale"));
        assert_eq!(
            cache.gpu_bytes(),
            ScaledAtlases::fresh_bytes(2.0, ROBOTO, &chars).expect("fresh 2.0")
        );
        assert_eq!(cache.cell_size(), Some(128));
    }
}
