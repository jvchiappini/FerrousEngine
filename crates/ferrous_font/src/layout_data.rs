//! CPU-only, GPU-free text layout data.
//!
//! [`LayoutData`] holds everything [`crate::layout`] needs to lay text out:
//! per-character glyph metrics, glyph indices, `kern` and GPOS pair kerning and
//! the vertical font metrics.  It is built from font bytes without a device, so
//! measurement does not need a window, a GPU or an atlas.
//!
//! [`FontAtlas`](crate::FontAtlas) builds its own metrics through the same
//! [`LayoutData`] tables and then adds the atlas UV rectangles, so an atlas and
//! a `LayoutData` for the same font and character set measure text bit for bit
//! identically.  That is the property measurement callers rely on: measuring
//! with `LayoutData` can never drift from what the renderer lays out.

use std::collections::HashMap;

use crate::atlas::GlyphMetrics;
use crate::layout::{layout_block, layout_line, measure_block, measure_line};
use crate::layout::{
    GlyphLayout, GlyphQuad, LayoutFont, LineMetrics, TextAlign, TextMetrics, TextStyle,
};
use crate::msdf_gen::{CELL_MAX, CELL_MIN};
use crate::parser::{FontMetrics, FontParser, GposKerning, ParseError};
use crate::FontError;

/// Pure-data layout tables for one font and character set.
///
/// Build one with [`LayoutData::from_bytes`] (or [`LayoutData::from_parser`]
/// when the parser already exists) and measure with
/// [`LayoutData::measure_block`] / [`LayoutData::measure_line`].  The struct is
/// immutable after construction.
#[derive(Debug)]
pub struct LayoutData {
    metrics: HashMap<char, GlyphMetrics>,
    notdef: Option<GlyphMetrics>,
    glyph_indices: HashMap<char, u16>,
    kerning: HashMap<(u16, u16), f32>,
    gpos: GposKerning,
    font: FontMetrics,
}

impl LayoutData {
    /// Parses `bytes` and builds the layout tables for `charset`.
    ///
    /// An empty character set falls back to
    /// [`crate::charset::latin_western`], matching
    /// [`crate::Font::try_load_bytes`].
    ///
    /// # Errors
    ///
    /// Returns [`FontError::Parse`] for malformed bytes and
    /// [`FontError::Unsupported`] for an outline format this parser does not
    /// read (CFF/CFF2).
    pub fn from_bytes<I>(bytes: &[u8], charset: I) -> Result<Self, FontError>
    where
        I: IntoIterator<Item = char>,
    {
        let parser = FontParser::new(bytes.to_vec()).map_err(|error| match error {
            ParseError::Parse(message) => FontError::Parse { message },
            ParseError::Unsupported(reason) => FontError::Unsupported { reason },
        })?;
        let mut chars: Vec<char> = charset.into_iter().collect();
        if chars.is_empty() {
            chars = crate::charset::latin_western();
        }
        Ok(Self::from_parser(&parser, chars))
    }

    /// Builds the layout tables for an already parsed font.
    ///
    /// The atlas uses this same constructor, which is why the two cannot
    /// diverge.  Glyph metrics carry zero UV and cell size: layout does not use
    /// them (only the renderer's glyph quads do, and the atlas fills them in).
    #[must_use]
    pub fn from_parser<I>(parser: &FontParser, chars: I) -> Self
    where
        I: IntoIterator<Item = char>,
    {
        let plane_bounds = [CELL_MIN, CELL_MIN, CELL_MAX, CELL_MAX];
        let mut metrics = HashMap::new();
        let mut glyph_indices = HashMap::new();
        for c in chars {
            let glyph = parser.get_glyph_index(c);
            glyph_indices.insert(c, glyph);
            metrics.insert(c, glyph_metrics(parser, glyph, plane_bounds));
        }
        let notdef = (!parser.get_glyph_outline_by_index(0).is_empty())
            .then(|| glyph_metrics(parser, 0, plane_bounds));
        Self {
            metrics,
            notdef,
            glyph_indices,
            kerning: parser.kerning_pairs().clone(),
            gpos: parser.gpos_kerning().clone(),
            font: parser.metrics(),
        }
    }

    /// A pure-data view for [`crate::layout`] functions.
    #[must_use]
    pub fn layout_font(&self) -> LayoutFont<'_> {
        LayoutFont {
            metrics: &self.metrics,
            notdef: self.notdef.as_ref(),
            glyph_indices: &self.glyph_indices,
            kerning: &self.kerning,
            gpos: &self.gpos,
            font: self.font,
        }
    }

    /// Per-character glyph metrics (advance and ink bounds; UV is zero).
    #[must_use]
    pub fn metrics(&self) -> &HashMap<char, GlyphMetrics> {
        &self.metrics
    }

    /// Vertical font metrics in em units.
    #[must_use]
    pub fn font_metrics(&self) -> FontMetrics {
        self.font
    }

    /// Measures a single line exactly like [`crate::measure_line`].
    #[must_use]
    pub fn measure_line(&self, text: &str, size_px: f32, align: TextAlign) -> LineMetrics {
        measure_line(self.layout_font(), text, size_px, align)
    }

    /// Lays a single line out exactly like [`crate::layout_line`].
    pub fn layout_line(
        &self,
        text: &str,
        size_px: f32,
        align: TextAlign,
        out: &mut Vec<GlyphQuad>,
    ) -> LineMetrics {
        layout_line(self.layout_font(), text, size_px, align, out)
    }

    /// Measures a block exactly like [`crate::measure_block`].
    #[must_use]
    pub fn measure_block(&self, text: &str, style: TextStyle) -> TextMetrics {
        measure_block(self.layout_font(), text, style)
    }

    /// Lays a block out exactly like [`crate::layout_block`].
    pub fn layout_block(
        &self,
        text: &str,
        style: TextStyle,
        out: &mut Vec<GlyphLayout>,
    ) -> TextMetrics {
        layout_block(self.layout_font(), text, style, out)
    }

    /// Splits the tables into the pieces an atlas needs.
    pub(crate) fn into_parts(self) -> LayoutParts {
        (
            self.metrics,
            self.notdef,
            self.glyph_indices,
            self.kerning,
            self.gpos,
            self.font,
        )
    }
}

/// The owned tables of a [`LayoutData`], for [`FontAtlas`](crate::FontAtlas).
pub(crate) type LayoutParts = (
    HashMap<char, GlyphMetrics>,
    Option<GlyphMetrics>,
    HashMap<char, u16>,
    HashMap<(u16, u16), f32>,
    GposKerning,
    FontMetrics,
);

/// Builds one glyph metric from the parser, sharing the cell geometry every
/// atlas cell uses.
pub(crate) fn glyph_metrics(
    parser: &FontParser,
    glyph: u16,
    plane_bounds: [f32; 4],
) -> GlyphMetrics {
    let ink = parser.get_glyph_bounds_by_index(glyph).unwrap_or_default();
    GlyphMetrics {
        uv: [0.0; 4],
        size: [0.0; 2],
        advance: parser.get_glyph_advance_by_index(glyph),
        plane_bounds,
        ink_bounds: [ink.left, ink.bottom, ink.right, ink.top],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const ROBOTO: &[u8] = include_bytes!("../../../assets/fonts/Roboto-Regular.ttf");

    fn data() -> LayoutData {
        LayoutData::from_bytes(ROBOTO, crate::charset::latin_western()).unwrap()
    }

    #[test]
    fn from_bytes_builds_the_tables() {
        let data = data();
        assert!(data.metrics().contains_key(&'A'));
        assert!(data.glyph_indices.contains_key(&'A'));
        assert!(data.font_metrics().units_per_em > 0);
    }

    #[test]
    fn empty_charset_falls_back_to_latin_western() {
        let empty = LayoutData::from_bytes(ROBOTO, std::iter::empty()).unwrap();
        let explicit = LayoutData::from_bytes(ROBOTO, crate::charset::latin_western()).unwrap();
        assert_eq!(
            empty.measure_block("Hola", TextStyle::at(32.0)),
            explicit.measure_block("Hola", TextStyle::at(32.0))
        );
    }

    #[test]
    fn measure_matches_the_layout_functions() {
        let data = data();
        let style = TextStyle {
            size_px: 41.0,
            align: TextAlign::Center,
            tracking: 0.03,
            line_height: 1.4,
        };
        let mut glyphs = Vec::new();
        let laid_out = data.layout_block("AV\nHola", style, &mut glyphs);
        assert_eq!(data.measure_block("AV\nHola", style), laid_out);
        assert_eq!(glyphs.len(), 6);
    }

    #[test]
    fn kerning_makes_av_narrower() {
        let data = data();
        let style = TextStyle::at(100.0);
        let av = data.measure_block("AV", style).width;
        let a = data.measure_block("A", style).width;
        let v = data.measure_block("V", style).width;
        assert!(av < a + v, "kerning must reduce AV ({av} vs {})", a + v);
    }

    #[test]
    fn invalid_bytes_are_a_typed_error() {
        let error = LayoutData::from_bytes(b"not a font", ['A']).unwrap_err();
        assert!(matches!(error, FontError::Parse { .. }));
    }
}
