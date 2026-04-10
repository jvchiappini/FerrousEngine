//! Text rendering utilities for the UI.

use crate::gpu_types::TextQuad;
use crate::gui_batch::GuiBatch;

impl GuiBatch {
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
            font, x, y, w, h, text, size, focused, cursor_visible, cursor_pos, selection,
            text_color, bg_color, sel_color, pad, 0.0, 0, border_color
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
                let adv = font.atlas.metrics.get(&c).map(|m| m.advance * size).unwrap_or(size * 0.6);
                if acc + adv > scroll_px { result = b; break; }
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
                let adv = font.atlas.metrics.get(&c).map(|m| m.advance * size).unwrap_or(size * 0.6);
                if acc + adv > inner_w + 1.0 { end = b; break; }
                acc += adv;
            }
            &after[..end]
        };
        let px_of_byte = |byte: usize| -> f32 {
            let b = byte.min(text.len());
            Self::measure_text(font, &text[..b], size) - scroll_px
        };
        self.push_clip(ferrous_ui_core::Rect { x, y, width: w, height: h });
        if focused {
            if let Some((sel_start, sel_end)) = selection {
                let vis_start = px_of_byte(sel_start).max(0.0);
                let vis_end = px_of_byte(sel_end).min(inner_w);
                let sx = x + pad + vis_start;
                let sw = vis_end - vis_start;
                if sw > 0.0 { self.rect(sx, y + 1.0, sw, h - 2.0, sel_color); }
            }
        }
        let text_y = y + (h - size) * 0.5;
        self.draw_text_internal(font, visible_str, [x + pad, text_y], size, text_color, z, node_id);
        if focused && cursor_visible && selection.is_none() {
            let cur_x = x + pad + px_of_byte(cursor_byte);
            self.rect(cur_x, y + 2.0, 1.5, h - 4.0, [1.0, 1.0, 1.0, 0.9]);
        }
        self.pop_clip();
    }
}
