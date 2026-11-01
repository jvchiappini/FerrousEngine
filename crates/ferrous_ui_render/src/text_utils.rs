//! Text rendering utilities for the UI.

use crate::gpu_types::TextQuad;
use crate::gui_batch::GuiBatch;

/// Glyph data needed to lay a run of text out on a baseline, in em units.
///
/// Implemented for the engine font and for test fixtures, so placement (and
/// kerning) can be verified without a GPU or an atlas texture.
pub trait BaselineGlyphs {
    /// Horizontal advance of `c`, or `None` when the font has no glyph.
    fn advance(&self, c: char) -> Option<f32>;
    /// Kerning adjustment between `left` and `right`, in em.
    fn kerning(&self, left: char, right: char) -> f32;
    /// Em-space rectangle covered by one atlas cell: `[left, bottom, right,
    /// top]`, Y up, relative to the pen position on the baseline.
    fn cell_bounds(&self) -> [f32; 4];
    /// Atlas UV rectangle `[u0, v0, u1, v1]` of `c`.
    fn uv(&self, c: char) -> Option<[f32; 4]>;
}

impl BaselineGlyphs for ferrous_assets::Font {
    fn advance(&self, c: char) -> Option<f32> {
        self.atlas.metrics.get(&c).map(|m| m.advance)
    }

    fn kerning(&self, left: char, right: char) -> f32 {
        let atlas = &self.atlas;
        match (
            atlas.glyph_indices.get(&left),
            atlas.glyph_indices.get(&right),
        ) {
            (Some(a), Some(b)) => atlas.kerning.get(&(*a, *b)).copied().unwrap_or(0.0),
            _ => 0.0,
        }
    }

    fn cell_bounds(&self) -> [f32; 4] {
        self.atlas.cell_bounds
    }

    fn uv(&self, c: char) -> Option<[f32; 4]> {
        self.atlas.metrics.get(&c).map(|m| m.uv)
    }
}

/// Advance used for characters the font cannot draw, in em.
const MISSING_ADVANCE_EM: f32 = 0.5;

/// Calls `emit(pos, size, uv)` once per drawable glyph, laying `text` out on
/// the baseline at `origin[1]` with its pen starting at `origin[0]`.
///
/// Each glyph quad is the whole atlas cell (from
/// [`BaselineGlyphs::cell_bounds`]) positioned so the cell's origin sits on
/// the pen position on the baseline.  Kerning is applied between consecutive
/// characters.  Whitespace advances the pen without emitting a quad.  The
/// returned value is the total advance drawn, in pixels.
pub fn for_each_baseline_glyph(
    glyphs: &impl BaselineGlyphs,
    text: &str,
    origin: [f32; 2],
    size: f32,
    mut emit: impl FnMut([f32; 2], [f32; 2], [f32; 4]),
) -> f32 {
    let [left, bottom, right, top] = glyphs.cell_bounds();
    let quad_w = (right - left) * size;
    let quad_h = (top - bottom) * size;
    let mut pen = origin[0];
    let mut previous = None;
    for c in text.chars() {
        if let Some(p) = previous {
            pen += glyphs.kerning(p, c) * size;
        }
        previous = Some(c);
        let Some(advance) = glyphs.advance(c) else {
            pen += MISSING_ADVANCE_EM * size;
            continue;
        };
        if !c.is_whitespace() {
            if let Some(uv) = glyphs.uv(c) {
                emit(
                    [pen + left * size, origin[1] - top * size],
                    [quad_w, quad_h],
                    uv,
                );
            }
        }
        pen += advance * size;
    }
    pen - origin[0]
}

impl GuiBatch {
    /// Draws `text` on the baseline at `position[1]`, with the pen starting
    /// at `position[0]`, using the atlas cell bounds and kerning.
    ///
    /// Unlike [`GuiBatch::draw_text`], `position[1]` **is** the baseline: the
    /// glyph cells hang above it exactly as the font designed them.
    #[cfg(feature = "text")]
    pub fn draw_text_baseline(
        &mut self,
        font: &ferrous_assets::Font,
        text: &str,
        position: [f32; 2],
        size: f32,
        color: [f32; 4],
    ) {
        let mut quads: Vec<([f32; 2], [f32; 2], [f32; 4])> = Vec::new();
        for_each_baseline_glyph(font, text, position, size, |pos, quad, uv| {
            quads.push((pos, quad, uv));
        });
        self.push_baseline_quads(&quads, color);
    }

    fn push_baseline_quads(&mut self, quads: &[([f32; 2], [f32; 2], [f32; 4])], color: [f32; 4]) {
        for &(pos, quad, uv) in quads {
            self.push_text(TextQuad {
                pos,
                size: quad,
                uv0: [uv[0], uv[1]],
                uv1: [uv[2], uv[3]],
                color,
                z_order: 0.0,
                node_id: 0,
            });
        }
        self.update_last_segment();
    }
}

impl GuiBatch {
    /// Draws `text` with its **top-left em-box corner** at `position`.
    ///
    /// The baseline lands at `position[1] + size`: the top of the em box is
    /// taken as `y`, so a caller that wants a specific baseline must subtract
    /// the em size.  Kerning is not applied and the glyph cell is a fixed
    /// `1.6 × size` box.  This is the historical convention every existing
    /// caller relies on; [`GuiBatch::draw_text_baseline`] is the alternative
    /// that takes a baseline and kerns.
    #[cfg(feature = "text")]
    pub fn draw_text(
        &mut self,
        font: &ferrous_assets::Font,
        text: &str,
        position: [f32; 2],
        size: f32,
        color: [f32; 4],
    ) {
        self.draw_text_internal(font, text, position, size, color, 0.0, 0);
    }

    #[cfg(feature = "text")]
    pub fn draw_text_internal(
        &mut self,
        font: &ferrous_assets::Font,
        text: &str,
        position: [f32; 2],
        size: f32,
        color: [f32; 4],
        z: f32,
        node_id: u32,
    ) {
        let atlas = &font.atlas;
        let mut x = position[0];
        let y = position[1];
        let box_scale = 1.6;
        let quad_size = size * box_scale;

        for c in text.chars() {
            if let Some(metric) = atlas.metrics.get(&c) {
                let qx = x - (0.3 * size);
                let qy = y - (0.3 * size);

                self.push_text(TextQuad {
                    pos: [qx, qy],
                    size: [quad_size, quad_size],
                    uv0: [metric.uv[0], metric.uv[1]],
                    uv1: [metric.uv[2], metric.uv[3]],
                    color,
                    z_order: z,
                    node_id,
                });
                x += metric.advance * size;
            }
        }
        self.update_last_segment();
    }

    #[cfg(feature = "text")]
    pub fn measure_text(font: &ferrous_assets::Font, text: &str, size: f32) -> f32 {
        text.chars()
            .map(|c| {
                font.atlas
                    .metrics
                    .get(&c)
                    .map(|m| m.advance * size)
                    .unwrap_or(size * 0.6)
            })
            .sum()
    }

    #[cfg(feature = "text")]
    pub fn char_at_px(font: &ferrous_assets::Font, text: &str, size: f32, target_px: f32) -> usize {
        let mut x = 0.0f32;
        for (byte_idx, c) in text.char_indices() {
            let adv = font
                .atlas
                .metrics
                .get(&c)
                .map(|m| m.advance * size)
                .unwrap_or(size * 0.6);
            if x + adv * 0.5 >= target_px {
                return byte_idx;
            }
            x += adv;
        }
        text.len()
    }

    #[cfg(feature = "text")]
    #[allow(clippy::too_many_arguments)]
    pub fn draw_text_field(
        &mut self,
        font: &ferrous_assets::Font,
        x: f32,
        y: f32,
        w: f32,
        h: f32,
        text: &str,
        size: f32,
        focused: bool,
        cursor_visible: bool,
        cursor_pos: usize,
        selection: Option<(usize, usize)>,
        text_color: [f32; 4],
        bg_color: [f32; 4],
        border_color: Option<[f32; 4]>,
        sel_color: [f32; 4],
        pad: f32,
    ) {
        self.draw_text_field_internal(
            font,
            x,
            y,
            w,
            h,
            text,
            size,
            focused,
            cursor_visible,
            cursor_pos,
            selection,
            text_color,
            bg_color,
            sel_color,
            pad,
            0.0,
            0,
            border_color,
        );
    }

    #[cfg(feature = "text")]
    #[allow(clippy::too_many_arguments)]
    pub fn draw_text_field_internal(
        &mut self,
        font: &ferrous_assets::Font,
        x: f32,
        y: f32,
        w: f32,
        h: f32,
        text: &str,
        size: f32,
        focused: bool,
        cursor_visible: bool,
        cursor_pos: usize,
        selection: Option<(usize, usize)>,
        text_color: [f32; 4],
        bg_color: [f32; 4],
        sel_color: [f32; 4],
        pad: f32,
        z: f32,
        node_id: u32,
        border_color: Option<[f32; 4]>,
    ) {
        let inner_w = w - pad * 2.0;
        self.rect(x, y, w, h, bg_color);
        if focused {
            if let Some(bc) = border_color {
                self.rect(x, y, w, 1.0, bc);
                self.rect(x, y + h - 1.0, w, 1.0, bc);
                self.rect(x, y, 1.0, h, bc);
                self.rect(x + w - 1.0, y, 1.0, h, bc);
            }
        }
        let cursor_byte = cursor_pos.min(text.len());
        let cursor_px_from_start = Self::measure_text(font, &text[..cursor_byte], size);
        let scroll_px = if cursor_px_from_start > inner_w {
            cursor_px_from_start - inner_w
        } else {
            0.0
        };
        let scroll_byte: usize = {
            let mut acc = 0.0f32;
            let mut result = 0usize;
            for (b, c) in text.char_indices() {
                let adv = font
                    .atlas
                    .metrics
                    .get(&c)
                    .map(|m| m.advance * size)
                    .unwrap_or(size * 0.6);
                if acc + adv > scroll_px {
                    result = b;
                    break;
                }
                acc += adv;
                result = b + c.len_utf8();
            }
            result
        };
        let visible_str = {
            let after = &text[scroll_byte..];
            let mut end = after.len();
            let mut acc = 0.0f32;
            for (b, c) in after.char_indices() {
                let adv = font
                    .atlas
                    .metrics
                    .get(&c)
                    .map(|m| m.advance * size)
                    .unwrap_or(size * 0.6);
                if acc + adv > inner_w + 1.0 {
                    end = b;
                    break;
                }
                acc += adv;
            }
            &after[..end]
        };
        let px_of_byte = |byte: usize| -> f32 {
            let b = byte.min(text.len());
            Self::measure_text(font, &text[..b], size) - scroll_px
        };
        self.push_clip(ferrous_ui_core::Rect {
            x,
            y,
            width: w,
            height: h,
        });
        if focused {
            if let Some((sel_start, sel_end)) = selection {
                let vis_start = px_of_byte(sel_start).max(0.0);
                let vis_end = px_of_byte(sel_end).min(inner_w);
                let sx = x + pad + vis_start;
                let sw = vis_end - vis_start;
                if sw > 0.0 {
                    self.rect(sx, y + 1.0, sw, h - 2.0, sel_color);
                }
            }
        }
        let text_y = y + (h - size) * 0.5;
        self.draw_text_internal(
            font,
            visible_str,
            [x + pad, text_y],
            size,
            text_color,
            z,
            node_id,
        );
        if focused && cursor_visible && selection.is_none() {
            let cur_x = x + pad + px_of_byte(cursor_byte);
            self.rect(cur_x, y + 2.0, 1.5, h - 4.0, [1.0, 1.0, 1.0, 0.9]);
        }
        self.pop_clip();
    }
}

#[cfg(test)]
mod baseline_tests {
    use super::*;

    /// Fixture atlas: every glyph advances 0.5 em, "AV" kerns by -0.1 em, the
    /// cell spans em [-0.3, 1.3] on both axes.
    struct Fixture;

    impl BaselineGlyphs for Fixture {
        fn advance(&self, c: char) -> Option<f32> {
            (c != '\u{1}').then_some(0.5)
        }

        fn kerning(&self, left: char, right: char) -> f32 {
            if (left, right) == ('A', 'V') {
                -0.1
            } else {
                0.0
            }
        }

        fn cell_bounds(&self) -> [f32; 4] {
            [-0.3, -0.3, 1.3, 1.3]
        }

        fn uv(&self, _c: char) -> Option<[f32; 4]> {
            Some([0.0, 0.0, 0.25, 0.25])
        }
    }

    #[test]
    fn second_glyph_of_av_moves_left_by_the_kerning_amount() {
        let mut positions = Vec::new();
        let advance = for_each_baseline_glyph(&Fixture, "AV", [10.0, 100.0], 10.0, |pos, _, _| {
            positions.push(pos)
        });
        assert_eq!(positions.len(), 2);
        // Unkerned the second cell would start at 10 + 5 - 3 = 12.
        let unkerned = 10.0 + 0.5 * 10.0 - 0.3 * 10.0;
        let kerning_px = 0.1 * 10.0;
        assert!(
            (positions[1][0] - (unkerned - kerning_px)).abs() < 1e-4,
            "{positions:?}"
        );
        // The cell hangs above the baseline, never below it.
        assert!((positions[0][1] - (100.0 - 1.3 * 10.0)).abs() < 1e-4);
        // Advance includes the kern.
        assert!((advance - (10.0 - kerning_px)).abs() < 1e-4, "{advance}");
    }

    #[test]
    fn whitespace_advances_without_emitting_a_glyph() {
        let mut count = 0;
        let advance =
            for_each_baseline_glyph(&Fixture, "a b", [0.0, 0.0], 10.0, |_, _, _| count += 1);
        assert_eq!(count, 2, "the space emits nothing");
        assert!((advance - 15.0).abs() < 1e-4);
    }

    #[test]
    fn cell_size_comes_from_the_atlas_cell_bounds() {
        let mut size = [0.0; 2];
        for_each_baseline_glyph(&Fixture, "a", [0.0, 0.0], 10.0, |_, quad, _| size = quad);
        assert!((size[0] - 16.0).abs() < 1e-4, "{size:?}");
        assert!((size[1] - 16.0).abs() < 1e-4, "{size:?}");
    }
}
