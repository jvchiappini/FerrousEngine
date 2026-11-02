//! Pure-CPU text layout: single lines and multi-line blocks.
//!
//! The renderer calls [`layout_block`] once per text draw call, then turns each
//! visible [`GlyphLayout`] into one GPU instance.  The same code measures text
//! without a GPU, which is why the layout source ([`LayoutFont`]) is a plain
//! data view instead of a `FontAtlas` (that one owns a GPU texture).
//!
//! # Coordinate contract
//!
//! * The pen starts at `x = 0` on the baseline `y = 0`.
//! * `y` points up, 1 unit = 1 logical pixel.
//! * `Left` alignment keeps the origin at the pen start; `Center` and `Right`
//!   shift the finished line so the origin is its centre / right edge.  Every
//!   line of a block is aligned on its own.
//! * Line 0's baseline sits on the transform origin; later baselines are below
//!   it (`baseline_y < 0`), one `line_step` apart.
//! * Quad bounds are `[left, bottom, right, top]` in pixels, relative to the
//!   baseline origin.
//! * Ink bounds come from the `glyf` header, not from the MSDF cell, so they
//!   describe the drawn shape rather than the atlas quad.

use std::collections::HashMap;
use std::ops::Range;

use crate::atlas::GlyphMetrics;
use crate::parser::{FontMetrics, GposKerning};

/// Horizontal alignment of one line relative to the pen origin.
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

/// Everything that affects layout apart from the font bytes and the string.
///
/// `tracking` is extra advance per character in **em units**, inserted between
/// characters (never after the last character of a line, so centred text stays
/// centred).  `line_height` multiplies the font's natural line step
/// `ascender - descender + line_gap`.
///
/// A non-finite or negative `size_px` is treated as zero, a non-finite
/// `tracking` as zero and a non-finite or non-positive `line_height` as `1.0`;
/// layout therefore never emits NaN geometry.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TextStyle {
    /// Em size in pixels.
    pub size_px: f32,
    /// Per-line horizontal alignment.
    pub align: TextAlign,
    /// Additional advance between characters, in em units.
    pub tracking: f32,
    /// Line-step multiplier (`1.0` = the font's natural spacing).
    pub line_height: f32,
}

impl TextStyle {
    /// A style at `size_px` with the given alignment, no tracking and natural
    /// line spacing.
    #[must_use]
    pub const fn new(size_px: f32, align: TextAlign) -> Self {
        Self {
            size_px,
            align,
            tracking: 0.0,
            line_height: 1.0,
        }
    }

    /// The style used when the caller has no preferences: 16 px, left aligned.
    #[must_use]
    pub const fn at(size_px: f32) -> Self {
        Self::new(size_px, TextAlign::Left)
    }

    fn normalized(self) -> NormalizedStyle {
        let size = if self.size_px.is_finite() && self.size_px > 0.0 {
            self.size_px
        } else {
            0.0
        };
        let tracking = if self.tracking.is_finite() {
            self.tracking
        } else {
            0.0
        };
        let line_height = if self.line_height.is_finite() && self.line_height > 0.0 {
            self.line_height
        } else {
            1.0
        };
        NormalizedStyle {
            size,
            align: self.align,
            tracking,
            line_height,
        }
    }
}

impl Default for TextStyle {
    fn default() -> Self {
        Self::at(16.0)
    }
}

/// A normalized style: size already validated.
#[derive(Debug, Clone, Copy)]
struct NormalizedStyle {
    size: f32,
    align: TextAlign,
    tracking: f32,
    line_height: f32,
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

/// An axis-aligned rectangle in pixels, Y up.
///
/// An empty rectangle has `left > right` and `bottom > top`; [`Self::default`]
/// is the canonical empty box.  `TextMetrics::ink` is empty when the text has
/// no visible glyph (for example a string of spaces).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TextBounds {
    /// Left edge.
    pub left: f32,
    /// Bottom edge.
    pub bottom: f32,
    /// Right edge.
    pub right: f32,
    /// Top edge.
    pub top: f32,
}

impl TextBounds {
    /// The empty box: every `union` grows out of it.
    #[must_use]
    pub const fn empty() -> Self {
        Self {
            left: f32::INFINITY,
            bottom: f32::INFINITY,
            right: f32::NEG_INFINITY,
            top: f32::NEG_INFINITY,
        }
    }

    /// Whether the box contains no area.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        !(self.left.is_finite()
            && self.bottom.is_finite()
            && self.right.is_finite()
            && self.top.is_finite())
            || self.left >= self.right
            || self.bottom >= self.top
    }

    /// Width of the box (zero when empty).
    #[must_use]
    pub fn width(&self) -> f32 {
        if self.is_empty() {
            0.0
        } else {
            self.right - self.left
        }
    }

    /// Height of the box (zero when empty).
    #[must_use]
    pub fn height(&self) -> f32 {
        if self.is_empty() {
            0.0
        } else {
            self.top - self.bottom
        }
    }

    /// The smallest box containing both.
    #[must_use]
    pub fn union(&self, other: &Self) -> Self {
        if self.is_empty() {
            return *other;
        }
        if other.is_empty() {
            return *self;
        }
        Self {
            left: self.left.min(other.left),
            bottom: self.bottom.min(other.bottom),
            right: self.right.max(other.right),
            top: self.top.max(other.top),
        }
    }

    /// Moves the box by `dx` horizontally.
    fn shifted_x(&self, dx: f32) -> Self {
        if self.is_empty() {
            return *self;
        }
        Self {
            left: self.left + dx,
            right: self.right + dx,
            ..*self
        }
    }
}

impl Default for TextBounds {
    fn default() -> Self {
        Self::empty()
    }
}

/// One character of a laid-out block, including whitespace.
///
/// Whitespace and characters the font cannot draw still get an entry (so byte
/// ranges cover the whole string) but have `visible == false` and no quad.
#[derive(Debug, Clone, PartialEq)]
pub struct GlyphLayout {
    /// The character this entry describes.
    pub ch: char,
    /// Byte range of the character in the original string.
    pub byte_range: Range<usize>,
    /// Zero-based line index.
    pub line: u32,
    /// Pen x of the glyph's origin on its baseline, after kerning, tracking and
    /// the line's alignment shift.
    pub x: f32,
    /// Baseline y of the line (line 0 is `0.0`; later lines are negative).
    pub baseline_y: f32,
    /// The glyph's horizontal advance in pixels, excluding tracking.
    pub advance: f32,
    /// Atlas cell quad relative to the glyph origin: `[left, bottom, right,
    /// top]`, Y up.
    pub quad: [f32; 4],
    /// Atlas UV rectangle `[u0, v0, u1, v1]`.
    pub uv: [f32; 4],
    /// Ink rectangle in the block's local space, Y up; empty when not visible.
    pub ink: TextBounds,
    /// Whether this character produces a quad.  False for whitespace and for a
    /// missing glyph when the font has no `.notdef` cell.
    pub visible: bool,
}

impl GlyphLayout {
    /// The quad in block-local pixels: `[left, bottom, right, top]`.
    #[must_use]
    pub fn bounds(&self) -> [f32; 4] {
        [
            self.x + self.quad[0],
            self.baseline_y + self.quad[1],
            self.x + self.quad[2],
            self.baseline_y + self.quad[3],
        ]
    }
}

/// Metrics of one laid-out line, in pixels (except the counts).
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct LineMetrics {
    /// Total advance including kerning, tracking and trailing whitespace.
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

/// Metrics of one line of a laid-out block.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct TextLineMetrics {
    /// Total advance of the line, including kerning and tracking.
    pub advance: f32,
    /// Baseline y of the line (line 0 is `0.0`, later lines are negative).
    pub baseline_y: f32,
    /// Ink box of the line in block-local pixels; empty when it draws nothing.
    pub ink: TextBounds,
    /// Number of visible glyphs on the line.
    pub quad_count: u32,
    /// Characters on the line with no glyph and no usable `.notdef`.
    pub missing_glyphs: u32,
}

/// Metrics of a whole text block.
///
/// The width is the widest line advance.  The height spans the first line's
/// ascender down to the last line's descender; for `n` lines it is
/// `ascender - descender + (n - 1) * line_step`, where `line_step` is
/// `(ascender - descender + line_gap) * line_height * size_px`.  The ink box is
/// the union of every visible glyph's ink, and `lines` holds one entry per
/// newline-separated line (an empty string is one empty line).
#[derive(Debug, Clone, PartialEq, Default)]
pub struct TextMetrics {
    /// Width of the widest line.
    pub width: f32,
    /// Number of lines (`text.split('\n').count()`, at least 1).
    pub line_count: u32,
    /// First line's ascender through the last line's descender.
    pub height: f32,
    /// Union of every visible glyph's ink, in block-local pixels.
    pub ink: TextBounds,
    /// Per-line metrics, in top-to-bottom order.
    pub lines: Vec<TextLineMetrics>,
}

impl TextMetrics {
    /// The advance of line `index`, if it exists.
    #[must_use]
    pub fn line_advance(&self, index: usize) -> Option<f32> {
        self.lines.get(index).map(|line| line.advance)
    }
}

/// Pure-data source for the layout functions.
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
    /// GPOS pair kerning; a covered pair overrides `kerning`.
    pub gpos: &'a GposKerning,
    /// Vertical font metrics in em units.
    pub font: FontMetrics,
}

impl LayoutFont<'_> {
    fn kerning(&self, left: u16, right: u16) -> f32 {
        self.gpos
            .pair(left, right)
            .or_else(|| self.kerning.get(&(left, right)).copied())
            .unwrap_or(0.0)
    }
}

/// Consumes the laid-out glyphs of one line.
trait GlyphSink {
    /// Starts a new line; `line` is informational for sinks that keep the
    /// whole block.
    fn begin_line(&mut self, line: u32);
    /// Receives one laid-out character.
    fn glyph(&mut self, glyph: GlyphLayout);
    /// Shifts the current line by `dx` pixels for alignment.
    fn shift_line(&mut self, dx: f32);
    /// Number of visible glyphs emitted on the current line.
    fn quad_count(&self) -> u32;
}

/// Sink for the legacy single-line [`layout_line`] API.
struct QuadSink<'a> {
    out: &'a mut Vec<GlyphQuad>,
    count: u32,
}

impl GlyphSink for QuadSink<'_> {
    fn begin_line(&mut self, _line: u32) {
        self.count = 0;
    }

    fn glyph(&mut self, glyph: GlyphLayout) {
        if !glyph.visible {
            return;
        }
        self.out.push(GlyphQuad {
            bounds: glyph.bounds(),
            uv: glyph.uv,
        });
        self.count += 1;
    }

    fn shift_line(&mut self, dx: f32) {
        for quad in self.out.iter_mut() {
            quad.bounds[0] += dx;
            quad.bounds[2] += dx;
        }
    }

    fn quad_count(&self) -> u32 {
        self.count
    }
}

/// Sink for [`layout_block`]: keeps every character.
struct BlockSink<'a> {
    out: &'a mut Vec<GlyphLayout>,
    line_start: usize,
}

impl GlyphSink for BlockSink<'_> {
    fn begin_line(&mut self, _line: u32) {
        self.line_start = self.out.len();
    }

    fn glyph(&mut self, glyph: GlyphLayout) {
        self.out.push(glyph);
    }

    fn shift_line(&mut self, dx: f32) {
        for glyph in &mut self.out[self.line_start..] {
            glyph.x += dx;
            glyph.ink = glyph.ink.shifted_x(dx);
        }
    }

    fn quad_count(&self) -> u32 {
        self.out[self.line_start..]
            .iter()
            .filter(|glyph| glyph.visible)
            .count() as u32
    }
}

/// Counting sink used by [`measure_line`] and [`measure_block`]; it deliberately
/// drops the glyphs.
struct MeasureSink {
    count: u32,
    ink: TextBounds,
}

impl GlyphSink for MeasureSink {
    fn begin_line(&mut self, _line: u32) {
        self.count = 0;
    }

    fn glyph(&mut self, glyph: GlyphLayout) {
        if glyph.visible {
            self.count += 1;
            self.ink = self.ink.union(&glyph.ink);
        }
    }

    fn shift_line(&mut self, _dx: f32) {}

    fn quad_count(&self) -> u32 {
        self.count
    }
}

/// The shared layout loop for one line.
///
/// `byte_base` is the offset of `text` inside the original block; `line` and
/// `baseline_y` place the line in the block.  `tracking` is already in pixels.
#[allow(clippy::too_many_arguments)] // one linear pass: font, string, style, line, sink
fn layout_line_into<S: GlyphSink>(
    font: LayoutFont<'_>,
    text: &str,
    size: f32,
    align: TextAlign,
    tracking: f32,
    line: u32,
    baseline_y: f32,
    byte_base: usize,
    sink: &mut S,
) -> LineMetrics {
    sink.begin_line(line);

    let mut pen = 0.0f32;
    let mut prev_glyph: Option<u16> = None;
    let mut missing = 0u32;
    let mut chars = text.char_indices().peekable();

    while let Some((byte, c)) = chars.next() {
        let is_last = chars.peek().is_none();
        let byte_range = byte_base + byte..byte_base + byte + c.len_utf8();
        let after = if is_last { 0.0 } else { tracking };

        if c.is_whitespace() {
            // Advance without a quad; kerning does not cross whitespace.
            let advance = font
                .metrics
                .get(&c)
                .map(|m| m.advance)
                .or_else(|| font.metrics.get(&' ').map(|m| m.advance))
                .unwrap_or(0.3);
            sink.glyph(GlyphLayout {
                ch: c,
                byte_range,
                line,
                x: pen,
                baseline_y,
                advance: advance * size,
                quad: [0.0; 4],
                uv: [0.0; 4],
                ink: TextBounds::empty(),
                visible: false,
            });
            pen += advance * size + after;
            prev_glyph = None;
            continue;
        }

        let glyph_index = font.glyph_indices.get(&c).copied();

        let Some(metric) = font.metrics.get(&c) else {
            missing += 1;
            match font.notdef {
                Some(notdef) => {
                    // Draw `.notdef`, but keep the advance of glyph 0.
                    if let (Some(prev), Some(cur)) = (prev_glyph, glyph_index) {
                        pen += font.kerning(prev, cur) * size;
                    }
                    let advance = notdef.advance;
                    sink.glyph(glyph_from_metric(
                        c,
                        byte_range,
                        line,
                        pen,
                        baseline_y,
                        advance * size,
                        notdef,
                        size,
                    ));
                    pen += advance * size + after;
                    prev_glyph = glyph_index.or(Some(0));
                }
                None => {
                    sink.glyph(GlyphLayout {
                        ch: c,
                        byte_range,
                        line,
                        x: pen,
                        baseline_y,
                        advance: 0.0,
                        quad: [0.0; 4],
                        uv: [0.0; 4],
                        ink: TextBounds::empty(),
                        visible: false,
                    });
                    pen += after;
                    prev_glyph = None;
                }
            }
            continue;
        };

        if let (Some(prev), Some(cur)) = (prev_glyph, glyph_index) {
            pen += font.kerning(prev, cur) * size;
        }

        sink.glyph(glyph_from_metric(
            c,
            byte_range,
            line,
            pen,
            baseline_y,
            metric.advance * size,
            metric,
            size,
        ));
        pen += metric.advance * size + after;
        prev_glyph = glyph_index;
    }

    let shift = match align {
        TextAlign::Left => 0.0,
        TextAlign::Center => -pen * 0.5,
        TextAlign::Right => -pen,
    };
    if shift != 0.0 {
        sink.shift_line(shift);
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

/// Builds one visible [`GlyphLayout`] from an atlas metric.
#[allow(clippy::too_many_arguments)] // the glyph's identity, pen and metric
fn glyph_from_metric(
    ch: char,
    byte_range: Range<usize>,
    line: u32,
    x: f32,
    baseline_y: f32,
    advance: f32,
    metric: &GlyphMetrics,
    size: f32,
) -> GlyphLayout {
    let [ql, qb, qr, qt] = metric.plane_bounds;
    let [il, ib, ir, it] = metric.ink_bounds;
    GlyphLayout {
        ch,
        byte_range,
        line,
        x,
        baseline_y,
        advance,
        quad: [ql * size, qb * size, qr * size, qt * size],
        uv: metric.uv,
        ink: TextBounds {
            left: x + il * size,
            bottom: baseline_y + ib * size,
            right: x + ir * size,
            top: baseline_y + it * size,
        },
        visible: true,
    }
}

/// Lay out one line of text, reusing `out`'s allocation.
///
/// Whitespace advances the pen without emitting a quad.  Missing characters use
/// the `.notdef` glyph when available, otherwise they are skipped and counted
/// in [`LineMetrics::missing_glyphs`].  Kerning (GPOS or a `kern` table) is
/// applied between adjacent non-whitespace glyphs.  An embedded `\n` is treated
/// as whitespace: use [`layout_block`] for multi-line text.
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
    let size = if size_px.is_finite() && size_px > 0.0 {
        size_px
    } else {
        0.0
    };
    out.clear();
    let mut sink = QuadSink { out, count: 0 };
    layout_line_into(font, text, size, align, 0.0, 0, 0.0, 0, &mut sink)
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
    let size = if size_px.is_finite() && size_px > 0.0 {
        size_px
    } else {
        0.0
    };
    let mut sink = MeasureSink {
        count: 0,
        ink: TextBounds::empty(),
    };
    layout_line_into(font, text, size, align, 0.0, 0, 0.0, 0, &mut sink)
}

/// Lay out a block of `\n`-separated lines into `out`.
///
/// Each line is aligned on its own; line 0's baseline is `y = 0` and later
/// baselines step down by `(ascender - descender + line_gap) * line_height *
/// size_px`.  A trailing `\r` (Windows line ending) is not drawn as a glyph.
/// See [`TextStyle`] for tracking and line-height semantics and [`TextMetrics`]
/// for the exact metrics.
pub fn layout_block(
    font: LayoutFont<'_>,
    text: &str,
    style: TextStyle,
    out: &mut Vec<GlyphLayout>,
) -> TextMetrics {
    out.clear();
    let style = style.normalized();
    let step = line_step(font.font, &style);
    let ascender = font.font.ascender * style.size;
    let descender = font.font.descender * style.size;
    let tracking = style.tracking * style.size;

    let mut width = 0.0f32;
    let mut ink = TextBounds::empty();
    let mut lines = Vec::new();
    let mut byte_base = 0usize;
    let mut line_index = 0u32;

    for segment in text.split('\n') {
        let line_byte_len = segment.len();
        let content = segment.strip_suffix('\r').unwrap_or(segment);
        let baseline_y = -(step * line_index as f32);
        let line_start = out.len();
        let mut sink = BlockSink { out, line_start };
        let metrics = layout_line_into(
            font,
            content,
            style.size,
            style.align,
            tracking,
            line_index,
            baseline_y,
            byte_base,
            &mut sink,
        );
        let line_start = sink.line_start;
        let line_ink = out[line_start..]
            .iter()
            .filter(|glyph| glyph.visible)
            .fold(TextBounds::empty(), |acc, glyph| acc.union(&glyph.ink));
        width = width.max(metrics.advance);
        ink = ink.union(&line_ink);
        lines.push(TextLineMetrics {
            advance: metrics.advance,
            baseline_y,
            ink: line_ink,
            quad_count: metrics.quad_count,
            missing_glyphs: metrics.missing_glyphs,
        });
        byte_base += line_byte_len + 1;
        line_index += 1;
    }

    let line_count = line_index.max(1);
    let height = if style.size == 0.0 {
        0.0
    } else {
        (ascender - descender) + step * (line_count.saturating_sub(1) as f32)
    };

    TextMetrics {
        width,
        line_count,
        height,
        ink,
        lines,
    }
}

/// Measure a block without collecting glyph records.
///
/// Returns exactly the same [`TextMetrics`] as [`layout_block`] (including the
/// per-line entries and ink boxes) without allocating glyph records.
pub fn measure_block(font: LayoutFont<'_>, text: &str, style: TextStyle) -> TextMetrics {
    let style = style.normalized();
    let step = line_step(font.font, &style);
    let ascender = font.font.ascender * style.size;
    let descender = font.font.descender * style.size;
    let tracking = style.tracking * style.size;

    let mut width = 0.0f32;
    let mut ink = TextBounds::empty();
    let mut lines = Vec::new();
    let mut byte_base = 0usize;
    let mut line_index = 0u32;

    for segment in text.split('\n') {
        let line_byte_len = segment.len();
        let content = segment.strip_suffix('\r').unwrap_or(segment);
        let baseline_y = -(step * line_index as f32);
        let mut sink = MeasureSink {
            count: 0,
            ink: TextBounds::empty(),
        };
        let metrics = layout_line_into(
            font,
            content,
            style.size,
            style.align,
            tracking,
            line_index,
            baseline_y,
            byte_base,
            &mut sink,
        );
        // The counting sink does not see the alignment shift, so shift its ink
        // by the same amount the glyph sink would have applied.
        let shift = match style.align {
            TextAlign::Left => 0.0,
            TextAlign::Center => -metrics.advance * 0.5,
            TextAlign::Right => -metrics.advance,
        };
        let line_ink = sink.ink.shifted_x(shift);
        width = width.max(metrics.advance);
        ink = ink.union(&line_ink);
        lines.push(TextLineMetrics {
            advance: metrics.advance,
            baseline_y,
            ink: line_ink,
            quad_count: metrics.quad_count,
            missing_glyphs: metrics.missing_glyphs,
        });
        byte_base += line_byte_len + 1;
        line_index += 1;
    }

    let line_count = line_index.max(1);
    let height = if style.size == 0.0 {
        0.0
    } else {
        (ascender - descender) + step * (line_count.saturating_sub(1) as f32)
    };

    TextMetrics {
        width,
        line_count,
        height,
        ink,
        lines,
    }
}

/// Natural line step in pixels for `style`.
fn line_step(font: FontMetrics, style: &NormalizedStyle) -> f32 {
    let natural = font.ascender - font.descender + font.line_gap;
    natural * style.size * style.line_height
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::GposKerning;

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
            ink_bounds: [0.0, 0.0, advance, 0.7],
        }
    }

    fn font_with<'a>(
        metrics: &'a HashMap<char, GlyphMetrics>,
        indices: &'a HashMap<char, u16>,
        kerning: &'a HashMap<(u16, u16), f32>,
        gpos: &'a GposKerning,
        notdef: Option<&'a GlyphMetrics>,
    ) -> LayoutFont<'a> {
        LayoutFont {
            metrics,
            notdef,
            glyph_indices: indices,
            kerning,
            gpos,
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
        let gpos = GposKerning::default();
        let font = font_with(&metrics, &indices, &kerning, &gpos, None);
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
        let gpos = GposKerning::default();
        let font = font_with(&metrics, &indices, &kerning, &gpos, None);
        let mut out = Vec::new();
        let plain = layout_line(font, "AV", 100.0, TextAlign::Left, &mut out);

        kerning.insert((36, 57), -0.087);
        let font = font_with(&metrics, &indices, &kerning, &gpos, None);
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
        let gpos = GposKerning::default();
        let font = font_with(&metrics, &indices, &kerning, &gpos, None);
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
        let gpos = GposKerning::default();
        let notdef = metric(0.4, 0.9);
        let font = font_with(&metrics, &indices, &kerning, &gpos, Some(&notdef));
        let mut out = Vec::new();
        let m = layout_line(font, "A☃", 100.0, TextAlign::Left, &mut out);
        assert_eq!(out.len(), 2, "notdef must be drawn as a replacement");
        assert_eq!(m.missing_glyphs, 1);
        assert_eq!(out[1].uv, notdef.uv);

        let font = font_with(&metrics, &indices, &kerning, &gpos, None);
        let m = layout_line(font, "A☃", 100.0, TextAlign::Left, &mut out);
        assert_eq!(out.len(), 1, "without notdef the missing glyph is skipped");
        assert_eq!(m.missing_glyphs, 1);
    }

    #[test]
    fn layout_is_deterministic_and_reuses_allocation() {
        let (metrics, indices, kerning) = simple_maps();
        let gpos = GposKerning::default();
        let font = font_with(&metrics, &indices, &kerning, &gpos, None);
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
        let gpos = GposKerning::default();
        let font = font_with(&metrics, &indices, &kerning, &gpos, None);
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
        let gpos = GposKerning::default();
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
            let font = font_with(&metrics, &indices, &kerning, &gpos, notdef_ref);
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

    #[test]
    fn tracking_adds_exactly_n_minus_one_steps() {
        let (metrics, indices, kerning) = simple_maps();
        let gpos = GposKerning::default();
        let font = font_with(&metrics, &indices, &kerning, &gpos, None);
        let mut out = Vec::new();
        let style = TextStyle {
            size_px: 100.0,
            align: TextAlign::Left,
            tracking: 0.05,
            line_height: 1.0,
        };
        let m = layout_block(font, "AVA", style, &mut out);
        // Three characters -> two tracking gaps of 5 px.
        assert!((m.width - (0.6 * 3.0 * 100.0 + 2.0 * 5.0)).abs() < 1e-4);
        assert!((out[1].x - out[0].x - (0.6 * 100.0 + 5.0)).abs() < 1e-4);
    }

    #[test]
    fn tracking_keeps_centre_alignment() {
        let (metrics, indices, kerning) = simple_maps();
        let gpos = GposKerning::default();
        let font = font_with(&metrics, &indices, &kerning, &gpos, None);
        let mut left = Vec::new();
        let mut centre = Vec::new();
        let style = TextStyle {
            size_px: 100.0,
            align: TextAlign::Center,
            tracking: 0.1,
            line_height: 1.0,
        };
        let m = layout_block(font, "AoV", style, &mut centre);
        layout_block(
            font,
            "AoV",
            TextStyle {
                align: TextAlign::Left,
                ..style
            },
            &mut left,
        );
        let half = m.width * 0.5;
        for (l, c) in left.iter().zip(centre.iter()) {
            assert!((c.x - (l.x - half)).abs() < 1e-4);
        }
    }

    #[test]
    fn multiline_baselines_step_by_line_height() {
        let (metrics, indices, kerning) = simple_maps();
        let gpos = GposKerning::default();
        let font = font_with(&metrics, &indices, &kerning, &gpos, None);
        let mut out = Vec::new();
        let style = TextStyle {
            size_px: 100.0,
            align: TextAlign::Left,
            tracking: 0.0,
            line_height: 2.0,
        };
        let m = layout_block(font, "A\nAo", style, &mut out);
        // Natural step: (0.8 + 0.2 + 0.1) * 100 = 110; doubled = 220.
        assert_eq!(m.line_count, 2);
        assert!((m.lines[1].baseline_y + 220.0).abs() < 1e-4);
        assert!((m.height - (100.0 + 220.0)).abs() < 1e-4);
        assert_eq!(out[1].line, 1);
        assert!((out[1].baseline_y + 220.0).abs() < 1e-4);
    }

    #[test]
    fn each_line_is_aligned_on_its_own() {
        let (metrics, indices, kerning) = simple_maps();
        let gpos = GposKerning::default();
        let font = font_with(&metrics, &indices, &kerning, &gpos, None);
        let mut out = Vec::new();
        let style = TextStyle {
            size_px: 100.0,
            align: TextAlign::Center,
            tracking: 0.0,
            line_height: 1.0,
        };
        let m = layout_block(font, "A\noo", style, &mut out);
        // First line advance 60, second 100: each has its own centre.
        assert!((m.width - 100.0).abs() < 1e-4);
        let line0: Vec<&GlyphLayout> = out.iter().filter(|g| g.line == 0).collect();
        let line1: Vec<&GlyphLayout> = out.iter().filter(|g| g.line == 1).collect();
        assert!((line0[0].x + 30.0).abs() < 1e-4);
        assert!((line1[0].x + 50.0).abs() < 1e-4);
        assert!((m.lines[0].advance - 60.0).abs() < 1e-4);
        assert!((m.lines[1].advance - 100.0).abs() < 1e-4);
    }

    #[test]
    fn whitespace_has_a_layout_entry_without_a_quad() {
        let (metrics, indices, kerning) = simple_maps();
        let gpos = GposKerning::default();
        let font = font_with(&metrics, &indices, &kerning, &gpos, None);
        let mut out = Vec::new();
        let m = layout_block(font, "A o", TextStyle::at(100.0), &mut out);
        assert_eq!(out.len(), 3);
        assert!(!out[1].visible);
        assert_eq!(m.lines[0].quad_count, 2);
    }

    #[test]
    fn measure_block_matches_layout_block_exactly() {
        let (metrics, indices, mut kerning) = simple_maps();
        kerning.insert((36, 57), -0.087);
        let gpos = GposKerning::default();
        let notdef = metric(0.4, 0.9);
        let texts = ["", "A", "AV", "Ao\nAV", "AV\n\nA", "A \n A", "A\r\nA"];
        let styles = [
            TextStyle::at(16.0),
            TextStyle {
                size_px: 100.0,
                align: TextAlign::Center,
                tracking: 0.04,
                line_height: 1.5,
            },
            TextStyle {
                size_px: 5.0,
                align: TextAlign::Right,
                tracking: -0.01,
                line_height: 0.5,
            },
        ];
        let mut out = Vec::new();
        for notdef_ref in [None, Some(&notdef)] {
            let font = font_with(&metrics, &indices, &kerning, &gpos, notdef_ref);
            for &text in &texts {
                for &style in &styles {
                    let laid_out = layout_block(font, text, style, &mut out);
                    let measured = measure_block(font, text, style);
                    assert_eq!(laid_out, measured, "text={text:?} style={style:?}");
                }
            }
        }
    }

    #[test]
    fn crlf_does_not_add_a_glyph() {
        let (metrics, indices, kerning) = simple_maps();
        let gpos = GposKerning::default();
        let font = font_with(&metrics, &indices, &kerning, &gpos, None);
        let mut out = Vec::new();
        let m = layout_block(font, "A\r\nA", TextStyle::at(100.0), &mut out);
        assert_eq!(m.line_count, 2);
        assert_eq!(out.len(), 2, "the carriage return is not a glyph entry");
        assert!((m.lines[0].advance - 60.0).abs() < 1e-4);
        assert_eq!(out[1].byte_range.start, 3);
    }

    #[test]
    fn invalid_style_values_do_not_produce_nan() {
        let (metrics, indices, kerning) = simple_maps();
        let gpos = GposKerning::default();
        let font = font_with(&metrics, &indices, &kerning, &gpos, None);
        let mut out = Vec::new();
        let style = TextStyle {
            size_px: f32::NAN,
            align: TextAlign::Left,
            tracking: f32::INFINITY,
            line_height: 0.0,
        };
        let m = layout_block(font, "AV\nA", style, &mut out);
        assert!(m.width.is_finite() && m.height.is_finite());
        assert!(out
            .iter()
            .all(|g| g.x.is_finite() && g.baseline_y.is_finite()));
    }
}
