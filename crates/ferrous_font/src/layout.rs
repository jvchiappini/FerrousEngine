//! Pure-CPU single-line text layout.
//!
//! The renderer calls [`layout_line`] once per text draw call, then turns each
//! [`GlyphQuad`] into one GPU instance.  The same code is used to measure text
//! without a GPU, which is why the layout source ([`LayoutFont`]) is a plain
//! data view instead of a `FontAtlas` (that one owns a GPU texture).
//!
//! # Coordinate contract
//!
//! * The pen starts at `x = 0` on the baseline `y = 0`.
//! * `y` points up, 1 unit = 1 logical pixel.
//! * `Left` alignment keeps the origin at the pen start; `Center` and `Right`
//!   shift the finished line so the origin is its centre / right edge.
//! * Quad bounds are `[left, bottom, right, top]` in pixels, relative to the
//!   baseline origin.
//! * Multi-line layout is intentionally out of scope.  A future
//!   `layout_block` would call `layout_line` per line, advancing the baseline
//!   by `(ascender - descender + line_gap) * size_px`.

use std::collections::HashMap;

use crate::atlas::GlyphMetrics;
use crate::parser::FontMetrics;

/// Horizontal alignment of a single line relative to the pen origin.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TextAlign {
    /// The pen origin is the start of the line.
    #[default]
    Left,
    /// The pen origin is the centre of the line.
    Center,
    /// The pen origin is the end of the line.
    Right,
}

/// One laid-out glyph: a quad rectangle and the atlas UV rectangle to sample.
///
/// `bounds` is `[left, bottom, right, top]` in logical pixels, Y up, relative
/// to the baseline origin.  `uv` is `[u0, v0, u1, v1]` in atlas UV space.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct GlyphQuad {
    /// Quad rectangle `[left, bottom, right, top]` in pixels.
    pub bounds: [f32; 4],
    /// Atlas UV rectangle `[u0, v0, u1, v1]`.
    pub uv: [f32; 4],
}

/// Metrics of one laid-out line, in pixels (except the counts).
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct LineMetrics {
    /// Total advance including kerning and trailing whitespace.
    pub advance: f32,
    /// Ascender (`hhea.ascender × size_px`); positive.
    pub ascender: f32,
    /// Descender (`hhea.descender × size_px`); negative.
    pub descender: f32,
    /// Recommended extra line spacing (`hhea.lineGap × size_px`).
    pub line_gap: f32,
    /// Number of quads written to the output buffer.
    pub quad_count: u32,
    /// Characters that had no glyph and no usable `.notdef`.
    pub missing_glyphs: u32,
}

/// Pure-data source for [`layout_line`].
///
/// Obtain one from [`crate::FontAtlas::layout_font`], or build it by hand in
/// tests; it never touches the GPU.
#[derive(Clone, Copy)]
pub struct LayoutFont<'a> {
    /// Per-character glyph metrics.
    pub metrics: &'a HashMap<char, GlyphMetrics>,
    /// `.notdef` replacement glyph, if the font has one.
    pub notdef: Option<&'a GlyphMetrics>,
    /// Character → glyph index (used for kerning).
    pub glyph_indices: &'a HashMap<char, u16>,
    /// `kern` pairs in em units keyed by glyph indices.
    pub kerning: &'a HashMap<(u16, u16), f32>,
    /// Vertical font metrics in em units.
    pub font: FontMetrics,
}

/// Sink for the shared single-line layout loop.
///
/// [`layout_line`] pushes quads and shifts them for alignment; [`measure_line`]
/// only counts them, so measuring never allocates.
trait LayoutSink {
    /// Receives one laid-out glyph.
    fn glyph(&mut self, quad: GlyphQuad);
    /// Shifts every glyph already emitted by `dx` pixels (alignment).
    fn shift(&mut self, dx: f32);
    /// Number of glyphs emitted so far.
    fn quad_count(&self) -> u32;
}

impl LayoutSink for Vec<GlyphQuad> {
    fn glyph(&mut self, quad: GlyphQuad) {
        self.push(quad);
    }

    fn shift(&mut self, dx: f32) {
        for quad in self.iter_mut() {
            quad.bounds[0] += dx;
            quad.bounds[2] += dx;
        }
    }

    fn quad_count(&self) -> u32 {
        self.len() as u32
    }
}

/// Counting sink used by [`measure_line`]; it deliberately drops the quads.
struct MeasureSink {
    count: u32,
}

impl LayoutSink for MeasureSink {
    fn glyph(&mut self, _quad: GlyphQuad) {
        self.count += 1;
    }

    fn shift(&mut self, _dx: f32) {}

    fn quad_count(&self) -> u32 {
        self.count
    }
}

/// The single layout loop shared by [`layout_line`] and [`measure_line`].
fn layout_line_impl<S: LayoutSink>(
    font: LayoutFont<'_>,
    text: &str,
    size_px: f32,
    align: TextAlign,
    sink: &mut S,
) -> LineMetrics {
    let size = if size_px.is_finite() && size_px > 0.0 {
        size_px
    } else {
        0.0
    };

    let mut pen = 0.0f32;
    let mut prev_glyph: Option<u16> = None;
    let mut missing = 0u32;

    for c in text.chars() {
        if c.is_whitespace() {
            // Advance without a quad; kerning does not cross whitespace.
            let advance = font
                .metrics
                .get(&c)
                .map(|m| m.advance)
                .or_else(|| font.metrics.get(&' ').map(|m| m.advance))
                .unwrap_or(0.3);
            pen += advance * size;
            prev_glyph = None;
            continue;
        }

        let glyph_index = font.glyph_indices.get(&c).copied();

        let Some(metric) = font.metrics.get(&c) else {
            missing += 1;
            match font.notdef {
                Some(notdef) => {
                    // Draw `.notdef`, but keep the advance of glyph 0.
                    let advance = notdef.advance;
                    let bounds = notdef.plane_bounds;
                    sink.glyph(GlyphQuad {
                        bounds: [
                            pen + bounds[0] * size,
                            bounds[1] * size,
                            pen + bounds[2] * size,
                            bounds[3] * size,
                        ],
                        uv: notdef.uv,
                    });
                    pen += advance * size;
                    prev_glyph = Some(0);
                }
                None => {
                    prev_glyph = None;
                }
            }
            continue;
        };

        if let (Some(prev), Some(cur)) = (prev_glyph, glyph_index) {
            if let Some(kern) = font.kerning.get(&(prev, cur)) {
                pen += kern * size;
            }
        }

        let bounds = metric.plane_bounds;
        sink.glyph(GlyphQuad {
            bounds: [
                pen + bounds[0] * size,
                bounds[1] * size,
                pen + bounds[2] * size,
                bounds[3] * size,
            ],
            uv: metric.uv,
        });
        pen += metric.advance * size;
        prev_glyph = glyph_index;
    }

    let shift = match align {
        TextAlign::Left => 0.0,
        TextAlign::Center => -pen * 0.5,
        TextAlign::Right => -pen,
    };
    if shift != 0.0 {
        sink.shift(shift);
    }

    LineMetrics {
        advance: pen,
        ascender: font.font.ascender * size,
        descender: font.font.descender * size,
        line_gap: font.font.line_gap * size,
        quad_count: sink.quad_count(),
        missing_glyphs: missing,
    }
}

/// Lay out one line of text, reusing `out`'s allocation.
///
/// Whitespace advances the pen without emitting a quad.  Missing characters use
/// the `.notdef` glyph when available, otherwise they are skipped and counted
/// in [`LineMetrics::missing_glyphs`].  Kerning (if the font has a `kern`
/// table) is applied between adjacent non-whitespace glyphs.
///
/// The function is deterministic: the same inputs always produce the same
/// quads, and no hidden state is read.
pub fn layout_line(
    font: LayoutFont<'_>,
    text: &str,
    size_px: f32,
    align: TextAlign,
    out: &mut Vec<GlyphQuad>,
) -> LineMetrics {
    out.clear();
    layout_line_impl(font, text, size_px, align, out)
}

/// Measure one line of text without producing any quads.
///
/// Returns exactly the same [`LineMetrics`] as [`layout_line`] for the same
/// inputs (including `quad_count` and `missing_glyphs`), but runs the shared
/// layout loop through a counting sink and therefore allocates nothing.
pub fn measure_line(
    font: LayoutFont<'_>,
    text: &str,
    size_px: f32,
    align: TextAlign,
) -> LineMetrics {
    let mut sink = MeasureSink { count: 0 };
    layout_line_impl(font, text, size_px, align, &mut sink)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn metric(advance: f32, uv_x: f32) -> GlyphMetrics {
        GlyphMetrics {
            uv: [uv_x, 0.0, uv_x + 0.1, 0.1],
            size: [64.0, 64.0],
            advance,
            plane_bounds: [
                crate::msdf_gen::CELL_MIN,
                crate::msdf_gen::CELL_MIN,
                crate::msdf_gen::CELL_MAX,
                crate::msdf_gen::CELL_MAX,
            ],
        }
    }

    fn font_with<'a>(
        metrics: &'a HashMap<char, GlyphMetrics>,
        indices: &'a HashMap<char, u16>,
        kerning: &'a HashMap<(u16, u16), f32>,
        notdef: Option<&'a GlyphMetrics>,
    ) -> LayoutFont<'a> {
        LayoutFont {
            metrics,
            notdef,
            glyph_indices: indices,
            kerning,
            font: FontMetrics {
                units_per_em: 1000,
                ascender: 0.8,
                descender: -0.2,
                line_gap: 0.1,
                ..Default::default()
            },
        }
    }

    type TestMaps = (
        HashMap<char, GlyphMetrics>,
        HashMap<char, u16>,
        HashMap<(u16, u16), f32>,
    );

    fn simple_maps() -> TestMaps {
        let mut metrics = HashMap::new();
        metrics.insert('A', metric(0.6, 0.0));
        metrics.insert('V', metric(0.6, 0.1));
        metrics.insert('o', metric(0.5, 0.2));
        metrics.insert(' ', metric(0.25, 0.0));
        let mut indices = HashMap::new();
        indices.insert('A', 36);
        indices.insert('V', 57);
        indices.insert('o', 82);
        indices.insert(' ', 3);
        (metrics, indices, HashMap::new())
    }

    #[test]
    fn whitespace_advances_without_quads() {
        let (metrics, indices, kerning) = simple_maps();
        let font = font_with(&metrics, &indices, &kerning, None);
        let mut out = Vec::new();
        let m = layout_line(font, "A A", 100.0, TextAlign::Left, &mut out);
        assert_eq!(out.len(), 2, "spaces must not emit quads");
        assert!((m.advance - (0.6 + 0.25 + 0.6) * 100.0).abs() < 1e-4);
        assert_eq!(m.quad_count, 2);
        assert_eq!(m.missing_glyphs, 0);
    }

    #[test]
    fn kerning_makes_av_narrower() {
        let (metrics, indices, mut kerning) = simple_maps();
        let font = font_with(&metrics, &indices, &kerning, None);
        let mut out = Vec::new();
        let plain = layout_line(font, "AV", 100.0, TextAlign::Left, &mut out);

        kerning.insert((36, 57), -0.087);
        let font = font_with(&metrics, &indices, &kerning, None);
        let kerned = layout_line(font, "AV", 100.0, TextAlign::Left, &mut out);

        assert!(
            kerned.advance < plain.advance,
            "kerning must reduce the advance ({} vs {})",
            kerned.advance,
            plain.advance
        );
        assert!((plain.advance - kerned.advance - 8.7).abs() < 1e-4);
    }

    #[test]
    fn centre_alignment_is_symmetric() {
        let (metrics, indices, kerning) = simple_maps();
        let font = font_with(&metrics, &indices, &kerning, None);
        let mut left_out = Vec::new();
        let left = layout_line(font, "Ao", 100.0, TextAlign::Left, &mut left_out);
        let mut centre_out = Vec::new();
        let centre = layout_line(font, "Ao", 100.0, TextAlign::Center, &mut centre_out);

        assert!((left.advance - centre.advance).abs() < 1e-4);
        let half = left.advance * 0.5;
        for (l, c) in left_out.iter().zip(centre_out.iter()) {
            assert!((c.bounds[0] - (l.bounds[0] - half)).abs() < 1e-4);
            assert!((c.bounds[2] - (l.bounds[2] - half)).abs() < 1e-4);
        }
        // The centre-aligned line starts exactly half an advance left of 0.
        let first_pen = centre_out[0].bounds[0] - crate::msdf_gen::CELL_MIN * 100.0;
        assert!((first_pen + half).abs() < 1e-4);
    }

    #[test]
    fn missing_glyph_uses_notdef_or_is_counted() {
        let (metrics, indices, kerning) = simple_maps();
        let notdef = metric(0.4, 0.9);
        let font = font_with(&metrics, &indices, &kerning, Some(&notdef));
        let mut out = Vec::new();
        let m = layout_line(font, "A☃", 100.0, TextAlign::Left, &mut out);
        assert_eq!(out.len(), 2, "notdef must be drawn as a replacement");
        assert_eq!(m.missing_glyphs, 1);
        assert_eq!(out[1].uv, notdef.uv);

        let font = font_with(&metrics, &indices, &kerning, None);
        let m = layout_line(font, "A☃", 100.0, TextAlign::Left, &mut out);
        assert_eq!(out.len(), 1, "without notdef the missing glyph is skipped");
        assert_eq!(m.missing_glyphs, 1);
    }

    #[test]
    fn layout_is_deterministic_and_reuses_allocation() {
        let (metrics, indices, kerning) = simple_maps();
        let font = font_with(&metrics, &indices, &kerning, None);
        let mut out = Vec::new();
        let a = layout_line(font, "AVAVA", 32.0, TextAlign::Right, &mut out);
        let capacity = out.capacity();
        let snapshot = out.clone();
        let b = layout_line(font, "AVAVA", 32.0, TextAlign::Right, &mut out);
        assert_eq!(a, b);
        assert_eq!(snapshot, *out);
        assert_eq!(out.capacity(), capacity, "the output Vec must be reused");
    }

    #[test]
    fn line_metrics_scale_with_size() {
        let (metrics, indices, kerning) = simple_maps();
        let font = font_with(&metrics, &indices, &kerning, None);
        let mut out = Vec::new();
        let m = layout_line(font, "A", 50.0, TextAlign::Left, &mut out);
        assert!((m.ascender - 40.0).abs() < 1e-4);
        assert!((m.descender + 10.0).abs() < 1e-4);
        assert!((m.line_gap - 5.0).abs() < 1e-4);
    }

    #[test]
    fn measure_line_matches_layout_line_exactly() {
        let (metrics, indices, mut kerning) = simple_maps();
        kerning.insert((36, 57), -0.087); // A -> V
        kerning.insert((57, 36), -0.04); // V -> A
        let notdef = metric(0.4, 0.9);

        let strings = [
            "",
            "A",
            "AV",
            "AVA",
            "AVATAR",
            "o",
            "AoV",
            "hello world",
            " A",
            "A ",
            "A  ",
            "  A",
            "AV AV",
            "A V",
            "A\tA",
            "A\nA",
            "A☃",
            "☃",
            "A☃V",
            "Ferrous Engine",
        ];
        let sizes = [1.0f32, 16.0, 100.0];
        let aligns = [TextAlign::Left, TextAlign::Center, TextAlign::Right];
        let mut out = Vec::new();

        for notdef_ref in [None, Some(&notdef)] {
            let font = font_with(&metrics, &indices, &kerning, notdef_ref);
            for &text in &strings {
                for &size in &sizes {
                    for &align in &aligns {
                        let laid_out = layout_line(font, text, size, align, &mut out);
                        let measured = measure_line(font, text, size, align);
                        assert_eq!(
                            laid_out,
                            measured,
                            "text={text:?} size={size} align={align:?} \
                             notdef={}",
                            notdef_ref.is_some()
                        );
                    }
                }
            }
        }
    }
}
