//! Real text layout over `ferrous_font`.
//!
//! [`FerrousTextSystem`] implements [`guimaker::text::TextSystem`] with the
//! CPU-only `ferrous_font` layout tables: real advances, `kern` and `GPOS` pair
//! kerning, letter tracking and line wrapping. It never creates a GPU atlas, so
//! measurement works in headless tests and in a windowless gallery.
//!
//! # Faces
//!
//! A font face is selected by `(FontFamily, FontWeight)`. A [`GlyphSource`]
//! supplies the bytes for a face; [`BundledGlyphs`] returns the engine's
//! `Roboto-Regular` for every request, so a host that does not register Inter
//! still gets deterministic metrics. Register your own source with
//! [`FerrousTextSystem::with_source`] to supply the real D-0002 faces. The
//! bundled font has one weight; a requested weight that the source cannot
//! provide falls back to the source's `Sans` regular face.

use std::collections::HashMap;

use ferrous_font::layout::{layout_block, TextAlign, TextStyle as FontTextStyle};
use ferrous_font::{charset, GlyphLayout, LayoutData};
use guimaker::text::{Ellipsis, Glyph, Line, TextShape, TextStyle as UiTextStyle, TextSystem};
use guimaker::theme::{FontFamily, FontWeight};

/// The font embedded in the engine repository, used when no source provides a
/// face. It is Roboto Regular, the engine's UI fallback font.
const BUNDLED_BYTES: &[u8] = include_bytes!("../../../assets/fonts/Roboto-Regular.ttf");

/// Advance used for a character the face has no glyph for, in em units.
///
/// It matches the core's `MetricsTextSystem` fallback so a missing glyph is
/// visible in layout rather than silently ignored.
const MISSING_ADVANCE_EM: f32 = 0.5;

/// Supplies the font bytes for a `GUIMaker` text face.
///
/// Implement this to hand [`FerrousTextSystem`] the real Inter and `JetBrains`
/// Mono files (per weight). Returning `None` lets the system try its bundled
/// fallback.
pub trait GlyphSource {
    /// The TTF/OTF bytes for `family` at `weight`, if available.
    fn bytes(&self, family: FontFamily, weight: FontWeight) -> Option<&[u8]>;
}

/// A [`GlyphSource`] that returns the bundled engine font for every face.
///
/// The bundled file has a single weight, so `Medium`, `SemiBold` and `Bold`
/// measure with the regular outlines until a host installs a richer source.
#[derive(Clone, Copy, Debug, Default)]
pub struct BundledGlyphs;

impl GlyphSource for BundledGlyphs {
    fn bytes(&self, _family: FontFamily, _weight: FontWeight) -> Option<&[u8]> {
        Some(BUNDLED_BYTES)
    }
}

/// The `ferrous_font`-backed [`TextSystem`] for the `FerrousEngine` backend.
///
/// The system is deterministic: the same text, style and width always produce
/// the same lines, advances and width. It is GPU-free and safe to keep for the
/// life of a UI.
pub struct FerrousTextSystem {
    source: Box<dyn GlyphSource>,
    faces: HashMap<(u8, u16), LayoutData>,
}

impl Default for FerrousTextSystem {
    fn default() -> Self {
        Self::new()
    }
}

impl FerrousTextSystem {
    /// Builds a text system over the bundled engine font.
    #[must_use]
    pub fn new() -> Self {
        Self::with_source(Box::new(BundledGlyphs))
    }

    /// Builds a text system over an application-provided face source.
    #[must_use]
    pub fn with_source(source: Box<dyn GlyphSource>) -> Self {
        Self {
            source,
            faces: HashMap::new(),
        }
    }

    /// The number of face tables built so far (diagnostics and tests).
    #[must_use]
    pub fn face_count(&self) -> usize {
        self.faces.len()
    }

    fn key(family: FontFamily, weight: FontWeight) -> (u8, u16) {
        (family_byte(family), weight.value())
    }

    /// Builds the face on first use. Returns `false` when the source and the
    /// bundled font both fail to parse.
    fn ensure_face(&mut self, family: FontFamily, weight: FontWeight) -> bool {
        let key = Self::key(family, weight);
        if self.faces.contains_key(&key) {
            return true;
        }
        let bytes = self
            .source
            .bytes(family, weight)
            .or_else(|| self.source.bytes(FontFamily::Sans, FontWeight::Regular))
            .unwrap_or(BUNDLED_BYTES);
        match LayoutData::from_bytes(bytes, charset::latin_western()) {
            Ok(data) => {
                self.faces.insert(key, data);
                true
            }
            Err(_) => false,
        }
    }
}

impl TextSystem for FerrousTextSystem {
    fn shape(&mut self, text: &str, style: UiTextStyle, max_width: Option<f32>) -> TextShape {
        let weight = weight_enum(style.weight);
        if !self.ensure_face(style.family, weight) {
            return guimaker::text::MetricsTextSystem::new().shape(text, style, max_width);
        }
        let key = Self::key(style.family, weight);
        let font = &self.faces[&key];
        shape_wrapped(font, text, style, max_width)
    }

    fn shape_clamped(
        &mut self,
        text: &str,
        style: UiTextStyle,
        max_width: Option<f32>,
        max_lines: Option<u16>,
        ellipsis: Ellipsis,
    ) -> TextShape {
        let weight = weight_enum(style.weight);
        if !self.ensure_face(style.family, weight) {
            return guimaker::text::MetricsTextSystem::new()
                .shape_clamped(text, style, max_width, max_lines, ellipsis);
        }
        let key = Self::key(style.family, weight);
        let font = &self.faces[&key];
        let mut shape = shape_wrapped(font, text, style, max_width);
        clamp_shape(
            font, &mut shape, style, max_width, max_lines, ellipsis, text,
        );
        shape
    }
}

fn family_byte(family: FontFamily) -> u8 {
    match family {
        FontFamily::Sans => 0,
        FontFamily::Mono => 1,
    }
}

/// Converts a line count to `f32`.
///
/// A shaped run has at most one line per input byte, and UI strings are far
/// shorter than `2^24` bytes, so the conversion is exact for every real input.
#[allow(clippy::cast_precision_loss)]
fn count_f32(count: usize) -> f32 {
    count as f32
}

/// Maps a numeric weight to the nearest token weight.
fn weight_enum(weight: u16) -> FontWeight {
    match weight {
        0..=450 => FontWeight::Regular,
        451..=550 => FontWeight::Medium,
        551..=650 => FontWeight::SemiBold,
        _ => FontWeight::Bold,
    }
}

/// Width of `text` at `style`, including `kern`/`GPOS` and tracking, in logical
/// pixels.
///
/// Tracking is inserted between characters, never after the last one, matching
/// `ferrous_font::layout::layout_block`.
fn line_width(font: &LayoutData, text: &str, style: UiTextStyle) -> f32 {
    let layout = font.layout_font();
    let mut width = 0.0;
    let mut previous: Option<u16> = None;
    for ch in text.chars() {
        let glyph = layout.glyph_indices.get(&ch).copied().unwrap_or(0);
        if let Some(left) = previous {
            let kerning = layout
                .gpos
                .pair(left, glyph)
                .or_else(|| layout.kerning.get(&(left, glyph)).copied())
                .unwrap_or(0.0);
            width += kerning + style.tracking;
        }
        width += layout
            .metrics
            .get(&ch)
            .map_or(MISSING_ADVANCE_EM, |metric| metric.advance);
        previous = Some(glyph);
    }
    width * style.size
}

/// Wraps one paragraph (no newlines) and returns its `(start, end)` byte ranges.
fn wrap_paragraph(
    font: &LayoutData,
    text: &str,
    style: UiTextStyle,
    limit: Option<f32>,
) -> Vec<(usize, usize)> {
    if text.is_empty() {
        return vec![(0, 0)];
    }
    let Some(limit) = limit else {
        return vec![(0, text.len())];
    };
    let layout = font.layout_font();
    let mut lines = Vec::new();
    let mut start = 0usize;
    while start < text.len() {
        let mut cursor = start;
        let mut end = start;
        let mut last_space_end = None;
        let mut width = 0.0f32;
        let mut previous: Option<u16> = None;
        let mut first = true;
        let mut overflowed = false;
        while cursor < text.len() {
            let ch = text[cursor..].chars().next().expect("char boundary");
            let next = cursor + ch.len_utf8();
            let glyph = layout.glyph_indices.get(&ch).copied().unwrap_or(0);
            let add = if let Some(left) = previous {
                let kerning = layout
                    .gpos
                    .pair(left, glyph)
                    .or_else(|| layout.kerning.get(&(left, glyph)).copied())
                    .unwrap_or(0.0);
                kerning
                    + style.tracking
                    + layout
                        .metrics
                        .get(&ch)
                        .map_or(MISSING_ADVANCE_EM, |metric| metric.advance)
            } else {
                layout
                    .metrics
                    .get(&ch)
                    .map_or(MISSING_ADVANCE_EM, |metric| metric.advance)
            };
            let add = add * style.size;
            if width + add > limit + 0.01 && !first {
                overflowed = true;
                break;
            }
            width += add;
            first = false;
            previous = Some(glyph);
            end = next;
            if ch == ' ' {
                last_space_end = Some(next);
            }
            cursor = next;
        }
        if overflowed {
            if let Some(space_end) = last_space_end {
                if space_end > start {
                    end = space_end;
                }
            }
        }
        if end <= start {
            let ch = text[start..].chars().next().expect("char boundary");
            end = start + ch.len_utf8();
        }
        lines.push((start, end));
        start = end;
    }
    lines
}

/// Shapes a whole run: wraps each newline-separated paragraph and lays every
/// resulting line out with `ferrous_font`.
fn shape_wrapped(
    font: &LayoutData,
    text: &str,
    style: UiTextStyle,
    max_width: Option<f32>,
) -> TextShape {
    if text.is_empty() {
        return TextShape::empty(style);
    }
    let font_style = FontTextStyle {
        size_px: style.size,
        align: TextAlign::Left,
        tracking: style.tracking,
        line_height: 1.0,
    };
    let limit = max_width.filter(|width| width.is_finite() && *width > 0.0);
    let mut shape = TextShape::default();
    let mut scratch: Vec<GlyphLayout> = Vec::new();
    let mut line_index = 0usize;
    let mut base = 0usize;
    for paragraph in text.split('\n') {
        for (start, end) in wrap_paragraph(font, paragraph, style, limit) {
            let line_text = &paragraph[start..end];
            let metrics = layout_block(font.layout_font(), line_text, font_style, &mut scratch);
            let glyph_start = shape.glyphs.len();
            for glyph in &scratch {
                shape.glyphs.push(Glyph {
                    ch: glyph.ch,
                    x: glyph.x,
                    advance: glyph.advance,
                    byte_start: base + start + glyph.byte_range.start,
                    byte_end: base + start + glyph.byte_range.end,
                });
            }
            shape.lines.push(Line {
                start: base + start,
                end: base + end,
                width: metrics.width,
                baseline: count_f32(line_index) * style.line_height + style.ascent(),
                glyph_start,
                glyph_end: shape.glyphs.len(),
            });
            line_index += 1;
        }
        base += paragraph.len() + 1;
    }
    shape.width = shape
        .lines
        .iter()
        .map(|line| line.width)
        .fold(0.0_f32, f32::max);
    shape.height = count_f32(shape.lines.len()) * style.line_height;
    shape
}

/// Applies a line clamp and ellipsis to an already shaped run.
///
/// `Ellipsis::Middle` degrades to `End`: the display list does not carry the
/// shortened string, so a middle marker could not be drawn by the adapter
/// without new core data. The shaped width is still clamped so layout is
/// correct.
fn clamp_shape(
    font: &LayoutData,
    shape: &mut TextShape,
    style: UiTextStyle,
    max_width: Option<f32>,
    max_lines: Option<u16>,
    ellipsis: Ellipsis,
    text: &str,
) {
    let limit = max_lines.map_or(usize::MAX, |lines| usize::from(lines.max(1)));
    if shape.lines.len() > limit {
        shape.truncated = true;
        shape.lines.truncate(limit);
        let last_glyph = shape.lines.last().map_or(0, |line| line.glyph_end);
        shape.glyphs.truncate(last_glyph);
        if ellipsis != Ellipsis::None {
            append_ellipsis(font, shape, style, max_width, text);
        }
        shape.height = count_f32(shape.lines.len()) * style.line_height;
        shape.width = shape
            .lines
            .iter()
            .map(|line| line.width)
            .fold(0.0_f32, f32::max);
    } else if ellipsis != Ellipsis::None && max_width.is_some_and(|width| width > 0.0) {
        shape.truncated = shape.width > max_width.unwrap_or(f32::INFINITY) + 0.01;
    }
}

/// Shortens the last line until the end ellipsis fits, then appends it to the
/// shape's glyph list.
fn append_ellipsis(
    font: &LayoutData,
    shape: &mut TextShape,
    style: UiTextStyle,
    max_width: Option<f32>,
    text: &str,
) {
    let ellipsis = '\u{2026}';
    let ellipsis_width = line_width(font, "\u{2026}", style);
    let Some(line) = shape.lines.last_mut() else {
        return;
    };
    let width_limit = max_width.filter(|width| width.is_finite() && *width > 0.0);
    let mut kept_end = line.end;
    loop {
        let too_wide = width_limit.is_some_and(|limit| {
            let kept = text
                .get(line.start.min(text.len())..kept_end.min(text.len()))
                .unwrap_or("");
            line_width(font, kept, style) + ellipsis_width > limit + 0.01
        });
        if !too_wide || shape.glyphs.len() <= line.glyph_start {
            break;
        }
        let removed = shape.glyphs.pop().expect("checked non-empty");
        kept_end = removed.byte_start;
    }
    let kept = text
        .get(line.start.min(text.len())..kept_end.min(text.len()))
        .unwrap_or("");
    line.end = kept_end;
    line.glyph_end = shape.glyphs.len();
    line.width = line_width(font, kept, style) + ellipsis_width;
    shape.glyphs.push(Glyph {
        ch: ellipsis,
        x: line.width - ellipsis_width,
        advance: ellipsis_width,
        byte_start: line.end,
        byte_end: line.end,
    });
    line.glyph_end = shape.glyphs.len();
}

#[cfg(test)]
mod tests {
    use super::{count_f32, FerrousTextSystem, GlyphSource};
    use guimaker::text::{Ellipsis, TextStyle, TextSystem};
    use guimaker::theme::{FontFamily, FontWeight};

    fn style(size: f32) -> TextStyle {
        TextStyle {
            size,
            weight: FontWeight::Regular.value(),
            line_height: size * 1.4,
            tracking: 0.0,
            family: FontFamily::Sans,
            tabular: false,
        }
    }

    #[test]
    fn shapes_with_real_advances_and_kerning() {
        let mut system = FerrousTextSystem::new();
        let shape = system.shape("AV", style(14.0), None);
        assert_eq!(shape.lines.len(), 1);
        assert_eq!(shape.glyphs.len(), 2);
        let separate =
            system.shape("A", style(14.0), None).width + system.shape("V", style(14.0), None).width;
        assert!(
            shape.width < separate,
            "kerned AV ({}) should be narrower than A + V ({})",
            shape.width,
            separate
        );
        assert_eq!(system.face_count(), 1);
    }

    #[test]
    fn wraps_words_at_a_width() {
        let mut system = FerrousTextSystem::new();
        let shape = system.shape("The quick brown fox", style(14.0), Some(60.0));
        assert!(shape.lines.len() > 1, "{shape:?}");
        assert!(shape.lines.iter().all(|line| line.width <= 60.1));
        assert!(shape.width <= 60.1);
        assert!((shape.height - count_f32(shape.lines.len()) * 19.6).abs() < 1e-4);
    }

    #[test]
    fn clamps_with_an_end_marker() {
        let mut system = FerrousTextSystem::new();
        let shape = system.shape_clamped(
            "one two three four five six",
            style(14.0),
            Some(60.0),
            Some(2),
            Ellipsis::End,
        );
        assert_eq!(shape.lines.len(), 2);
        assert!(shape.truncated);
        assert_eq!(shape.glyphs.last().map(|glyph| glyph.ch), Some('\u{2026}'));
    }

    #[test]
    fn tracks_between_characters() {
        let mut system = FerrousTextSystem::new();
        let plain = system.shape("xxxx", style(14.0), None).width;
        let mut tracked = style(14.0);
        tracked.tracking = 0.1;
        let spaced = system.shape("xxxx", tracked, None).width;
        assert!(
            (spaced - (plain + 3.0 * 1.4)).abs() < 0.01,
            "{spaced} vs {plain}"
        );
    }

    #[test]
    fn falls_back_to_the_bundled_face_for_an_unknown_weight() {
        struct NoBold;
        impl GlyphSource for NoBold {
            fn bytes(&self, _family: FontFamily, weight: FontWeight) -> Option<&[u8]> {
                (weight == FontWeight::Regular).then_some(super::BUNDLED_BYTES)
            }
        }
        let mut system = FerrousTextSystem::with_source(Box::new(NoBold));
        let mut bold = style(14.0);
        bold.weight = FontWeight::Bold.value();
        let shape = system.shape("Hello", bold, None);
        assert_eq!(shape.lines.len(), 1);
        assert!(!shape.glyphs.is_empty());
    }
}
