//! Text layout and per-glyph drawing types for the Pure2D renderer.
//!
//! Layout itself lives in [`ferrous_font::layout`] and is pure CPU code, so a
//! caller can measure text without a GPU.  This module re-exports the layout
//! types under their 2D names and adds [`GlyphDraw2d`], the per-glyph draw
//! request accepted by the renderer.
//!
//! Shaping is deliberately out of scope: there is no HarfBuzz, no ligature
//! substitution and no complex-script reordering.  Kerning comes from the
//! font's `GPOS` pair adjustment or its legacy `kern` table.

pub use ferrous_font::layout::{
    GlyphLayout as GlyphLayout2d, TextAlign, TextBounds, TextLineMetrics, TextMetrics,
    TextStyle as TextStyle2d,
};

/// One pre-laid-out glyph with its own transform and paint.
///
/// `transform` maps the glyph's local space (pen origin on the baseline, Y up)
/// to world space.  [`crate::Renderer::draw_glyphs_2d`] places the atlas quad
/// at the glyph's `x`/`baseline_y` inside that local space, so an identity
/// transform reproduces [`crate::Renderer::draw_text_2d`] exactly.
///
/// A glyph with `glyph.visible == false` (whitespace, or a missing character
/// without `.notdef`) produces no quad and is skipped.
#[derive(Debug, Clone, PartialEq)]
pub struct GlyphDraw2d {
    /// The laid-out glyph, including its quad and atlas UV rectangle.
    pub glyph: GlyphLayout2d,
    /// Local-to-world transform for this glyph.
    pub transform: glam::Mat4,
    /// Straight linear RGBA colour of the glyph.
    pub color: [f32; 4],
    /// Optional gradient base slot (see [`crate::render::GradientTable2d`]).
    pub gradient: Option<u32>,
}
