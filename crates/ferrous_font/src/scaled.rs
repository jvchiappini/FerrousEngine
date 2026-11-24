//! The raster cell policy: the atlas cell does **not** grow with the DPI scale.
//!
//! MSDF quality is about texels per em, not about device pixels: at 2x the
//! glyph and its distance field are both 2x larger on screen, so a fixed 64-texel
//! cell keeps the same relative edge quality. The measured numbers are in
//! `docs/decisions/G-0006.md`: at 2.0 a 128-texel cell changes the mean
//! coverage error against a true 8x supersampled reference by at most 2.6 %
//! (and sometimes increases it); the residual is the MSDF reconstruction bias,
//! common to both cells. A 128 cell costs 4x the GPU bytes (5.3 -> 21.2 MB) for
//! a mean the framebuffer cannot show.
//!
//! The cell therefore stays at [`DEFAULT_GLYPH_SIZE`] unless the **drawn em** is
//! larger than [`GROW_MULTIPLE`] cells; only then does the distance field lack
//! texels relative to the glyph, and the cell grows to the em size (clamped).

use crate::atlas::DEFAULT_GLYPH_SIZE;

/// How many cells of drawn em fit in the base cell before it grows.
///
/// At two cells the field has 64 texels over a 256 px em; below that the
/// measured error is under the 8-bit quantisation (G-0006).
pub const GROW_MULTIPLE: f32 = 2.0;

/// The cell size to bake for a glyph drawn at `em_px` device pixels.
///
/// Returns the base cell (64) unless the drawn em exceeds
/// `DEFAULT_GLYPH_SIZE * GROW_MULTIPLE` (128 px), in which case it grows to the
/// em (clamped to the rasterizer's range).
#[must_use]
#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
// reason: the em is a small positive pixel count and the result is clamped
pub fn cell_for_em(em_px: f32) -> u32 {
    let threshold = DEFAULT_GLYPH_SIZE as f32 * GROW_MULTIPLE;
    if em_px > threshold {
        (em_px.ceil() as u32).clamp(DEFAULT_GLYPH_SIZE, 128)
    } else {
        DEFAULT_GLYPH_SIZE
    }
}

#[cfg(test)]
mod tests {
    use super::{cell_for_em, DEFAULT_GLYPH_SIZE, GROW_MULTIPLE};

    #[test]
    fn caption_and_body_keep_the_base_cell_at_every_scale() {
        for role_size in [12.0_f32, 14.0] {
            for scale in [1.0_f32, 1.5, 2.0] {
                assert_eq!(
                    cell_for_em(role_size * scale),
                    DEFAULT_GLYPH_SIZE,
                    "{role_size} px at {scale}"
                );
            }
        }
    }

    #[test]
    fn the_cell_grows_only_for_a_large_drawn_em() {
        let threshold = DEFAULT_GLYPH_SIZE as f32 * GROW_MULTIPLE;
        assert_eq!(cell_for_em(threshold), DEFAULT_GLYPH_SIZE);
        assert!(cell_for_em(threshold + 1.0) > DEFAULT_GLYPH_SIZE);
        assert_eq!(
            cell_for_em(10_000.0),
            128,
            "clamped to the rasterizer range"
        );
    }
}
