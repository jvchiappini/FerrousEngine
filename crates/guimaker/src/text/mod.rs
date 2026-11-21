//! Text measurement: the core's only view into a font.
//!
//! The core never rasterises, opens a font file or touches a glyph atlas. It
//! asks a [`TextSystem`] for a [`TextShape`] — line boxes, advances, the shaped
//! width and height — and lays the node out from that. `guimaker_ferrous`
//! implements the trait over `ferrous_font` in phase 5; [`MetricsTextSystem`]
//! is the deterministic, GPU-free implementation used by tests and benches and
//! built from real `Inter` advances (G-0001 §3).

#![allow(
    clippy::cast_precision_loss,
    clippy::too_many_lines,
    clippy::unused_self,
    clippy::trivially_copy_pass_by_ref
)] // reason: font-unit arithmetic and one shaping pass per text run
mod metrics;

use crate::theme::{FontFamily, TypeStyle};

/// How a line that does not fit is shortened.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Ellipsis {
    /// Clip the overflow without a marker.
    #[default]
    None,
    /// Replace the tail with `…`.
    End,
    /// Replace the middle with `…`, keeping the start and the end (paths).
    Middle,
}

/// The fully resolved text style passed to a [`TextSystem`].
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TextStyle {
    /// Font size in logical pixels.
    pub size: f32,
    /// Font weight (400/500/600/700).
    pub weight: u16,
    /// Absolute line height in logical pixels.
    pub line_height: f32,
    /// Letter spacing in em units.
    pub tracking: f32,
    /// Font family.
    pub family: FontFamily,
    /// Whether digits use a fixed tabular advance.
    pub tabular: bool,
}

impl TextStyle {
    /// Builds a text style from a theme type style.
    #[must_use]
    pub fn from_type(style: TypeStyle, tabular: bool) -> Self {
        Self {
            size: style.size,
            weight: style.weight.value(),
            line_height: style.line_height,
            tracking: style.tracking,
            family: style.family,
            tabular,
        }
    }

    /// The ascender above the baseline, in logical pixels.
    #[must_use]
    pub fn ascent(self) -> f32 {
        self.size * (metrics::ASCENT / metrics::UPEM)
    }

    /// The descender below the baseline, in logical pixels (positive).
    #[must_use]
    pub fn descent(self) -> f32 {
        self.size * (-metrics::DESCENT / metrics::UPEM)
    }
}

/// One shaped character.
#[derive(Clone, Debug, PartialEq)]
pub struct Glyph {
    /// The character.
    pub ch: char,
    /// Horizontal offset from the line start.
    pub x: f32,
    /// Advance width.
    pub advance: f32,
    /// Byte range in the shaped string.
    pub byte_start: usize,
    /// Byte range end (exclusive).
    pub byte_end: usize,
}

/// One shaped line.
#[derive(Clone, Debug, PartialEq)]
pub struct Line {
    /// Byte range in the shaped string.
    pub start: usize,
    /// Byte range end (exclusive).
    pub end: usize,
    /// Line width in logical pixels.
    pub width: f32,
    /// Baseline offset from the top of the text box.
    pub baseline: f32,
    /// Index of the first glyph in [`TextShape::glyphs`].
    pub glyph_start: usize,
    /// Index one past the last glyph.
    pub glyph_end: usize,
}

/// The result of shaping one text run.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct TextShape {
    /// Shaped lines.
    pub lines: Vec<Line>,
    /// Shaped glyphs.
    pub glyphs: Vec<Glyph>,
    /// The widest line, in logical pixels.
    pub width: f32,
    /// `lines * line_height`.
    pub height: f32,
    /// Whether the shape was truncated (by `max_lines` or the width).
    pub truncated: bool,
}

impl TextShape {
    /// The text box for an empty run: one empty line, so labels keep rhythm.
    #[must_use]
    pub fn empty(style: TextStyle) -> Self {
        Self {
            lines: vec![Line {
                start: 0,
                end: 0,
                width: 0.0,
                baseline: style.ascent(),
                glyph_start: 0,
                glyph_end: 0,
            }],
            glyphs: Vec::new(),
            width: 0.0,
            height: style.line_height,
            truncated: false,
        }
    }
}

/// Measures and wraps text for the layout engine.
pub trait TextSystem {
    /// Shapes `text` at `style`, wrapping at `max_width` when it is positive.
    fn shape(&mut self, text: &str, style: TextStyle, max_width: Option<f32>) -> TextShape;

    /// Shapes with a maximum line count and an ellipsis mode.
    fn shape_clamped(
        &mut self,
        text: &str,
        style: TextStyle,
        max_width: Option<f32>,
        max_lines: Option<u16>,
        ellipsis: Ellipsis,
    ) -> TextShape;
}

/// Shapes text from the committed `Inter` advance table.
///
/// Deterministic, allocation-light and GPU-free: the same text, style and
/// width always produce the same lines and the same width. Kerning and the
/// real font's `GPOS` are the adapter's job; layout only needs advances.
#[derive(Clone, Copy, Debug, Default)]
pub struct MetricsTextSystem;

impl MetricsTextSystem {
    /// Builds a metrics text system.
    #[must_use]
    pub const fn new() -> Self {
        Self
    }

    fn advance(&self, ch: char, style: TextStyle) -> f32 {
        let units = metrics::advance_units(ch, style.tabular);
        f32::from(units) / metrics::UPEM * style.size + style.tracking * style.size
    }

    /// Shapes with an explicit maximum line count and ellipsis mode.
    ///
    /// [`TextSystem::shape`] calls this with one line and [`Ellipsis::None`];
    /// the layout engine calls it with the node's clamp.
    #[must_use]
    pub fn shape_clamped_impl(
        &mut self,
        text: &str,
        style: TextStyle,
        max_width: Option<f32>,
        max_lines: Option<u16>,
        ellipsis: Ellipsis,
    ) -> TextShape {
        let mut shape = self.shape_wrapped(text, style, max_width);
        let limit = max_lines.map_or(usize::MAX, |lines| usize::from(lines.max(1)));
        if shape.lines.len() > limit {
            shape.truncated = true;
            if limit == 1 && ellipsis == Ellipsis::Middle {
                shape = self.shape_middle(text, style, max_width);
            } else {
                truncate_lines(&mut shape, text, style, self, limit, ellipsis, max_width);
            }
        } else if ellipsis != Ellipsis::None && max_width.is_some_and(|width| width > 0.0) {
            // A single long line can still overflow when it has no break
            // opportunity; mark it so the adapter can decide.
            shape.truncated = shape.width > max_width.unwrap_or(f32::INFINITY) + 0.01;
        }
        shape
    }

    fn shape_wrapped(&mut self, text: &str, style: TextStyle, max_width: Option<f32>) -> TextShape {
        if text.is_empty() {
            return TextShape::empty(style);
        }
        let wrap = max_width.filter(|width| width.is_finite() && *width > 0.0);
        let mut shape = TextShape::default();
        let mut line_start = 0usize;
        let mut line_width = 0.0f32;
        let mut line_glyph_start = 0usize;
        let mut x = 0.0f32;
        let mut byte = 0usize;

        while byte < text.len() {
            let ch = text[byte..].chars().next().expect("valid char boundary");
            if ch == '\n' {
                push_line(
                    &mut shape,
                    line_start,
                    byte,
                    line_width,
                    line_glyph_start,
                    style,
                );
                byte += 1;
                line_start = byte;
                line_glyph_start = shape.glyphs.len();
                line_width = 0.0;
                x = 0.0;
                continue;
            }

            // One word with its trailing space, so breaks fall after a space
            // rather than in the middle of a word.
            let mut end = byte;
            while end < text.len() {
                let next = text[end..].chars().next().expect("valid char boundary");
                if next == '\n' {
                    break;
                }
                end += next.len_utf8();
                if next == ' ' {
                    break;
                }
            }
            let word = &text[byte..end];
            let word_width = word.chars().map(|c| self.advance(c, style)).sum::<f32>();
            if let Some(limit) = wrap {
                if line_width > 0.0 && line_width + word_width > limit + 0.01 {
                    push_line(
                        &mut shape,
                        line_start,
                        byte,
                        line_width,
                        line_glyph_start,
                        style,
                    );
                    line_start = byte;
                    line_glyph_start = shape.glyphs.len();
                    line_width = 0.0;
                    x = 0.0;
                }
                // A single word wider than the line breaks by character.
                if word_width > limit + 0.01 {
                    for (offset, ch) in word.char_indices() {
                        let advance = self.advance(ch, style);
                        if line_width > 0.0 && line_width + advance > limit + 0.01 {
                            push_line(
                                &mut shape,
                                line_start,
                                byte + offset,
                                line_width,
                                line_glyph_start,
                                style,
                            );
                            line_start = byte + offset;
                            line_glyph_start = shape.glyphs.len();
                            line_width = 0.0;
                            x = 0.0;
                        }
                        shape.glyphs.push(Glyph {
                            ch,
                            x,
                            advance,
                            byte_start: byte + offset,
                            byte_end: byte + offset + ch.len_utf8(),
                        });
                        x += advance;
                        line_width += advance;
                    }
                    byte = end;
                    continue;
                }
            }
            for (offset, ch) in word.char_indices() {
                let advance = self.advance(ch, style);
                shape.glyphs.push(Glyph {
                    ch,
                    x,
                    advance,
                    byte_start: byte + offset,
                    byte_end: byte + offset + ch.len_utf8(),
                });
                x += advance;
                line_width += advance;
            }
            byte = end;
        }
        push_line(
            &mut shape,
            line_start,
            text.len(),
            line_width,
            line_glyph_start,
            style,
        );
        shape.width = shape
            .lines
            .iter()
            .map(|line| line.width)
            .fold(0.0_f32, f32::max);
        shape.height = shape.lines.len() as f32 * style.line_height;
        shape
    }

    fn shape_middle(&mut self, text: &str, style: TextStyle, max_width: Option<f32>) -> TextShape {
        let Some(width) = max_width.filter(|width| width.is_finite() && *width > 0.0) else {
            return self.shape_wrapped(text, style, None);
        };
        let ellipsis_advance = self.advance('\u{2026}', style);
        let mut left = String::new();
        let mut right = String::new();
        let mut left_width = 0.0;
        let mut right_width = 0.0;
        let chars: Vec<char> = text.chars().collect();
        let mut start = 0;
        let mut end = chars.len();
        while start < end {
            let next_left = chars[start];
            let next_right = chars[end - 1];
            let left_advance = self.advance(next_left, style);
            let right_advance = self.advance(next_right, style);
            if left_width + right_width + left_advance + right_advance + ellipsis_advance > width {
                break;
            }
            left.push(next_left);
            left_width += left_advance;
            start += 1;
            if start >= end {
                break;
            }
            right.insert(0, next_right);
            right_width += right_advance;
            end -= 1;
        }
        let joined = format!("{left}\u{2026}{right}");
        let mut shape = TextShape::default();
        let mut x = 0.0;
        for (byte, ch) in joined.char_indices() {
            let advance = self.advance(ch, style);
            shape.glyphs.push(Glyph {
                ch,
                x,
                advance,
                byte_start: byte,
                byte_end: byte + ch.len_utf8(),
            });
            x += advance;
        }
        shape.lines.push(Line {
            start: 0,
            end: joined.len(),
            width: x,
            baseline: style.ascent(),
            glyph_start: 0,
            glyph_end: shape.glyphs.len(),
        });
        shape.width = x;
        shape.height = style.line_height;
        shape.truncated = true;
        shape
    }
}

impl TextSystem for MetricsTextSystem {
    fn shape(&mut self, text: &str, style: TextStyle, max_width: Option<f32>) -> TextShape {
        self.shape_wrapped(text, style, max_width)
    }

    fn shape_clamped(
        &mut self,
        text: &str,
        style: TextStyle,
        max_width: Option<f32>,
        max_lines: Option<u16>,
        ellipsis: Ellipsis,
    ) -> TextShape {
        self.shape_clamped_impl(text, style, max_width, max_lines, ellipsis)
    }
}

fn push_line(
    shape: &mut TextShape,
    start: usize,
    end: usize,
    width: f32,
    glyph_start: usize,
    style: TextStyle,
) {
    let baseline = shape.lines.len() as f32 * style.line_height + style.ascent();
    shape.lines.push(Line {
        start,
        end,
        width,
        baseline,
        glyph_start,
        glyph_end: shape.glyphs.len(),
    });
}

fn truncate_lines(
    shape: &mut TextShape,
    text: &str,
    style: TextStyle,
    metrics: &mut MetricsTextSystem,
    limit: usize,
    ellipsis: Ellipsis,
    max_width: Option<f32>,
) {
    shape.lines.truncate(limit);
    let keep_glyphs = shape.lines.last().map_or(0, |line| line.glyph_end);
    shape.glyphs.truncate(keep_glyphs);
    if ellipsis == Ellipsis::None {
        shape.height = shape.lines.len() as f32 * style.line_height;
        shape.width = shape
            .lines
            .iter()
            .map(|line| line.width)
            .fold(0.0_f32, f32::max);
        return;
    }
    let ellipsis_advance = metrics.advance('\u{2026}', style);
    let width_limit = max_width.filter(|width| width.is_finite() && *width > 0.0);
    let Some(last) = shape.lines.last_mut() else {
        return;
    };
    let mut x = last.width;
    while width_limit.is_some_and(|limit| x + ellipsis_advance > limit + 0.01)
        && shape.glyphs.len() > last.glyph_start
    {
        if let Some(removed) = shape.glyphs.pop() {
            x -= removed.advance;
        }
    }
    let byte_start = last.end.saturating_sub(1);
    shape.glyphs.push(Glyph {
        ch: '\u{2026}',
        x,
        advance: ellipsis_advance,
        byte_start,
        byte_end: byte_start,
    });
    last.glyph_end = shape.glyphs.len();
    last.width = x + ellipsis_advance;
    last.end = text.len();
    shape.width = shape
        .lines
        .iter()
        .map(|line| line.width)
        .fold(0.0_f32, f32::max);
    shape.height = shape.lines.len() as f32 * style.line_height;
}

#[cfg(test)]
mod tests {
    use super::{Ellipsis, MetricsTextSystem, TextStyle, TextSystem};
    use crate::theme::{FontFamily, FontWeight};

    fn style() -> TextStyle {
        TextStyle {
            size: 14.0,
            weight: FontWeight::Regular.value(),
            line_height: 22.0,
            tracking: 0.0,
            family: FontFamily::Sans,
            tabular: false,
        }
    }

    #[test]
    fn measures_real_advances() {
        let mut metrics = MetricsTextSystem::new();
        let shape = metrics.shape("AV", style(), None);
        assert_eq!(shape.lines.len(), 1);
        let expected = (1413.0 + 1413.0) / 2048.0 * 14.0;
        assert!((shape.width - expected).abs() < 0.05, "{}", shape.width);
        assert!((shape.height - 22.0).abs() < 1e-6);
        assert_eq!(shape.glyphs.len(), 2);
    }

    #[test]
    fn wraps_at_words() {
        let mut metrics = MetricsTextSystem::new();
        let shape = metrics.shape("The quick brown fox", style(), Some(40.0));
        assert!(shape.lines.len() > 1, "{shape:?}");
        assert!(shape.width <= 40.0 + 0.1);
        assert!(shape.lines.iter().all(|line| line.width <= 40.0 + 0.1));
        assert!((shape.height - shape.lines.len() as f32 * 22.0).abs() < 1e-6);
    }

    #[test]
    fn clamps_lines_with_end_ellipsis() {
        let mut metrics = MetricsTextSystem::new();
        let shape = metrics.shape_clamped(
            "one two three four five six",
            style(),
            Some(50.0),
            Some(2),
            Ellipsis::End,
        );
        assert_eq!(shape.lines.len(), 2);
        assert!(shape.truncated);
        assert_eq!(shape.glyphs.last().map(|glyph| glyph.ch), Some('\u{2026}'));
    }

    #[test]
    fn midellipsis_keeps_both_ends() {
        let mut metrics = MetricsTextSystem::new();
        let shape = metrics.shape_clamped(
            "/Users/somebody/projects/motioner/examples/demo.motioner",
            style(),
            Some(120.0),
            Some(1),
            Ellipsis::Middle,
        );
        assert_eq!(shape.lines.len(), 1);
        assert!(shape.width <= 120.0 + 0.1);
        assert!(shape.glyphs.iter().any(|glyph| glyph.ch == '\u{2026}'));
    }

    #[test]
    fn tabular_digits_share_one_advance() {
        let mut metrics = MetricsTextSystem::new();
        let mut tabular = style();
        tabular.tabular = true;
        let shape = metrics.shape("1111", tabular, None);
        let width = shape.width;
        let proportional = metrics.shape("1111", style(), None).width;
        assert!((width - 4.0 * 1270.0 / 2048.0 * 14.0).abs() < 0.05);
        assert!(width > proportional);
    }

    #[test]
    #[allow(clippy::float_cmp)] // reason: the value is an exact literal
    fn empty_text_keeps_one_line() {
        let mut metrics = MetricsTextSystem::new();
        let shape = metrics.shape("", style(), None);
        assert_eq!(shape.lines.len(), 1);
        assert!((shape.height - 22.0).abs() < 1e-6);
        assert_eq!(shape.width, 0.0);
    }
}
