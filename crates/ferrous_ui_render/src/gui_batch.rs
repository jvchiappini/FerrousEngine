//! GPU batching system for UI rendering.
//!
//! This module contains GuiBatch, which aggregates UI primitives into
//! rendering segments optimized for GPU execution.

use crate::gpu_types::{DrawSegment, GuiQuad, SvgCommand, TextQuad};
use crate::TEXTURED_BIT;

/// Grouping of UI primitives organized by rendering segments.
#[derive(Clone)]
pub struct GuiBatch {
    pub quads: Vec<GuiQuad>,
    pub text_quads: Vec<TextQuad>,
    pub icon_quads: Vec<TextQuad>,
    pub svg_commands: Vec<SvgCommand>,
    pub segments: Vec<DrawSegment>,
    pub current_scissor: Option<ferrous_ui_core::Rect>,
    pub scissor_stack: Vec<ferrous_ui_core::Rect>,
    #[cfg(feature = "assets")]
    pub textures: Vec<std::sync::Arc<ferrous_assets::Texture2d>>,
    pub damage_union: Option<ferrous_ui_core::Rect>,
}

impl Default for GuiBatch {
    fn default() -> Self {
        Self::new()
    }
}

impl GuiBatch {
    fn add_damage_rect(&mut self, mut rect: ferrous_ui_core::Rect) {
        if rect.width <= 0.0 || rect.height <= 0.0 {
            return;
        }

        if let Some(scissor) = self.current_scissor {
            rect = rect.intersect(&scissor);
            if rect.width <= 0.0 || rect.height <= 0.0 {
                return;
            }
        }

        self.damage_union = match self.damage_union {
            Some(current) => Some(current.union(rect)),
            None => Some(rect),
        };
    }

    pub fn new() -> Self {
        Self {
            quads: Vec::new(),
            text_quads: Vec::new(),
            icon_quads: Vec::new(),
            svg_commands: Vec::new(),
            segments: Vec::new(),
            current_scissor: None,
            scissor_stack: Vec::new(),
            #[cfg(feature = "assets")]
            textures: Vec::new(),
            damage_union: None,
        }
    }

    pub fn clear(&mut self) {
        self.quads.clear();
        self.text_quads.clear();
        self.icon_quads.clear();
        self.svg_commands.clear();
        self.segments.clear();
        self.current_scissor = None;
        self.scissor_stack.clear();
        self.damage_union = None;
        #[cfg(feature = "assets")]
        {
            self.textures.clear();
        }
    }

    pub fn push_quad(&mut self, quad: GuiQuad) {
        self.add_damage_rect(ferrous_ui_core::Rect {
            x: quad.pos[0],
            y: quad.pos[1],
            width: quad.size[0],
            height: quad.size[1],
        });
        self.ensure_segment();
        self.quads.push(quad);
        self.update_last_segment();
    }

    pub fn push_text(&mut self, quad: TextQuad) {
        self.add_damage_rect(ferrous_ui_core::Rect {
            x: quad.pos[0],
            y: quad.pos[1],
            width: quad.size[0],
            height: quad.size[1],
        });
        self.ensure_segment();
        self.text_quads.push(quad);
        self.update_last_segment();
    }

    pub fn push_icon(&mut self, quad: TextQuad) {
        self.add_damage_rect(ferrous_ui_core::Rect {
            x: quad.pos[0],
            y: quad.pos[1],
            width: quad.size[0],
            height: quad.size[1],
        });
        self.ensure_segment();
        self.icon_quads.push(quad);
        self.update_last_segment();
    }

    pub fn push_svg(&mut self, cmd: SvgCommand) {
        if !cmd.mesh.vertices.is_empty() {
            let mut min_x = f32::INFINITY;
            let mut min_y = f32::INFINITY;
            let mut max_x = f32::NEG_INFINITY;
            let mut max_y = f32::NEG_INFINITY;

            for v in &cmd.mesh.vertices {
                let x = v.position[0] + cmd.pos[0];
                let y = v.position[1] + cmd.pos[1];
                min_x = min_x.min(x);
                min_y = min_y.min(y);
                max_x = max_x.max(x);
                max_y = max_y.max(y);
            }

            self.add_damage_rect(ferrous_ui_core::Rect {
                x: min_x,
                y: min_y,
                width: (max_x - min_x).max(0.0),
                height: (max_y - min_y).max(0.0),
            });
        }
        self.ensure_segment();
        self.svg_commands.push(cmd);
        self.update_last_segment();
    }

    pub fn extend(&mut self, mut other: GuiBatch) {
        let q_offset = self.quads.len() as u32;
        let t_offset = self.text_quads.len() as u32;
        let i_offset = self.icon_quads.len() as u32;
        let s_offset = self.svg_commands.len() as u32;

        for seg in &mut other.segments {
            seg.quad_range.start += q_offset;
            seg.quad_range.end += q_offset;
            seg.text_range.start += t_offset;
            seg.text_range.end += t_offset;
            seg.icon_range.start += i_offset;
            seg.icon_range.end += i_offset;
            seg.svg_range.start += s_offset;
            seg.svg_range.end += s_offset;
        }

        self.quads.extend(other.quads);
        self.text_quads.extend(other.text_quads);
        self.icon_quads.extend(other.icon_quads);
        self.svg_commands.extend(other.svg_commands);
        self.segments.extend(other.segments);

        if let Some(other_damage) = other.damage_union {
            self.damage_union = match self.damage_union {
                Some(current) => Some(current.union(other_damage)),
                None => Some(other_damage),
            };
        }

        #[cfg(feature = "assets")]
        {
            self.textures.extend(other.textures);
        }
    }

    /// Starts a new **layer**: everything drawn after this call is painted
    /// above everything drawn before it, text included.
    ///
    /// Inside one segment the renderer draws all quads, then all text, then
    /// icons and SVG, so text drawn early (e.g. canvas labels) would otherwise
    /// end up on top of panels drawn later. A layer is simply a new segment
    /// with the same scissor. No-op when the current segment is empty.
    pub fn begin_layer(&mut self) {
        let empty = self.segments.last().map_or(true, |s| {
            s.quad_range.is_empty()
                && s.text_range.is_empty()
                && s.icon_range.is_empty()
                && s.svg_range.is_empty()
        });
        if empty {
            return;
        }
        let q = self.quads.len() as u32;
        let t = self.text_quads.len() as u32;
        let i = self.icon_quads.len() as u32;
        let s = self.svg_commands.len() as u32;
        self.segments.push(DrawSegment {
            quad_range: q..q,
            text_range: t..t,
            icon_range: i..i,
            svg_range: s..s,
            scissor: self.current_scissor,
        });
    }

    /// Ensures there is an active segment for the current scissor.
    pub fn ensure_segment(&mut self) {
        let needs_new = match self.segments.last() {
            Some(last) => last.scissor != self.current_scissor,
            None => true,
        };

        if needs_new {
            let q_start = self.quads.len() as u32;
            let t_start = self.text_quads.len() as u32;
            let i_start = self.icon_quads.len() as u32;
            let s_start = self.svg_commands.len() as u32;
            self.segments.push(DrawSegment {
                quad_range: q_start..q_start,
                text_range: t_start..t_start,
                icon_range: i_start..i_start,
                svg_range: s_start..s_start,
                scissor: self.current_scissor,
            });
        }
    }

    pub fn update_last_segment(&mut self) {
        self.ensure_segment();
        if let Some(last) = self.segments.last_mut() {
            last.quad_range.end = self.quads.len() as u32;
            last.text_range.end = self.text_quads.len() as u32;
            last.icon_range.end = self.icon_quads.len() as u32;
            last.svg_range.end = self.svg_commands.len() as u32;
        }
    }

    /// Registers a texture in the current batch and returns its slot index.
    #[cfg(feature = "assets")]
    pub fn reserve_texture_slot(
        &mut self,
        texture: std::sync::Arc<ferrous_assets::Texture2d>,
    ) -> u32 {
        if let Some(pos) = self
            .textures
            .iter()
            .position(|t| std::sync::Arc::ptr_eq(t, &texture))
        {
            return pos as u32;
        }
        if self.textures.len() as u32 >= crate::MAX_TEXTURE_SLOTS {
            panic!(
                "Exceeded texture limit per UI batch (max={})",
                crate::MAX_TEXTURE_SLOTS
            );
        }
        let idx = self.textures.len() as u32;
        self.textures.push(texture);
        idx
    }

    pub fn rect(&mut self, x: f32, y: f32, w: f32, h: f32, color: [f32; 4]) {
        self.rect_radii(x, y, w, h, color, [0.0; 4]);
    }

    pub fn rect_r(&mut self, x: f32, y: f32, w: f32, h: f32, color: [f32; 4], radius: f32) {
        self.rect_radii(x, y, w, h, color, [radius; 4]);
    }

    pub fn rect_radii(&mut self, x: f32, y: f32, w: f32, h: f32, color: [f32; 4], radii: [f32; 4]) {
        self.push_quad(GuiQuad {
            pos: [x, y],
            size: [w, h],
            uv0: [0.0, 0.0],
            uv1: [1.0, 1.0],
            color,
            color_b: [0.0; 4],
            radii,
            tex_index: 0,
            flags: 0,
            z_order: 0.0,
            node_id: 0,
        });
        self.update_last_segment();
    }

    /// Pushes an outline quad: the rounded-rect border is drawn inside `w`×`h`
    /// with `width` physical pixels of stroke.
    ///
    /// Packing follows `assets/shaders/gui.wgsl`, which reads the border width
    /// from `uv0.x`; `uv1` is unused by the border path (it is set to
    /// `[0, 0]` so the interpolated `uv` cannot leak gradient data).
    pub fn border(
        &mut self,
        x: f32,
        y: f32,
        w: f32,
        h: f32,
        color: [f32; 4],
        radii: [f32; 4],
        width: f32,
    ) {
        self.push_quad(GuiQuad {
            pos: [x, y],
            size: [w, h],
            uv0: [width, 0.0], // gui.wgsl: border width (uv0.x)
            uv1: [0.0, 0.0],
            color,
            color_b: [0.0; 4],
            radii,
            tex_index: 0,
            flags: crate::BORDER_BIT,
            z_order: 0.0,
            node_id: 0,
        });
        self.update_last_segment();
    }

    /// Pushes a soft drop shadow for the rectangle at `(x, y, w, h)`.
    ///
    /// `gui.wgsl` computes the shadow SDF from the **unexpanded** rectangle
    /// size in `color_b.xy` and the blur radius in `uv0.x`, centred on the
    /// quad.  The quad is therefore expanded by `blur * 2 + |spread| + the
    /// larger offset` around the *offset* rectangle, so the unexpanded rect
    /// lands where the shadow belongs.  `uv1` keeps the offset for debugging;
    /// the shader does not read it.
    pub fn shadow(
        &mut self,
        x: f32,
        y: f32,
        w: f32,
        h: f32,
        color: [f32; 4],
        blur_radius: f32,
        spread: f32,
        offset: [f32; 2],
        radii: [f32; 4],
    ) {
        // To include the shadow in the SDF, the quad must be expanded around
        // the offset rectangle.
        let expand = blur_radius * 2.0 + spread.abs() + offset[0].abs().max(offset[1].abs());

        self.push_quad(GuiQuad {
            pos: [x + offset[0] - expand, y + offset[1] - expand],
            size: [w + expand * 2.0, h + expand * 2.0],
            uv0: [blur_radius, 0.0],     // gui.wgsl: blur radius (uv0.x)
            uv1: [offset[0], offset[1]], // informational only
            color,
            color_b: [w, h, 0.0, 0.0], // gui.wgsl: unexpanded size (color_b.xy)
            radii,
            tex_index: 0,
            flags: crate::SHADOW_BIT,
            z_order: 0.0,
            node_id: 0,
        });
        self.update_last_segment();
    }

    pub fn shadow_rect(&mut self, x: f32, y: f32, w: f32, h: f32) {
        // Helper MD default shadow
        self.shadow(
            x,
            y,
            w,
            h,
            [0.0, 0.0, 0.0, 0.35],
            8.0,
            0.0,
            [0.0, 4.0],
            [0.0; 4],
        );
    }

    pub fn rect_textured(
        &mut self,
        x: f32,
        y: f32,
        w: f32,
        h: f32,
        color: [f32; 4],
        uv0: [f32; 2],
        uv1: [f32; 2],
        tex_index: u32,
    ) {
        self.push_quad(GuiQuad {
            pos: [x, y],
            size: [w, h],
            uv0,
            uv1,
            color,
            color_b: [0.0; 4],
            radii: [0.0; 4],
            tex_index,
            flags: TEXTURED_BIT,
            z_order: 0.0,
            node_id: 0,
        });
        self.update_last_segment();
    }

    #[cfg(feature = "assets")]
    pub fn image(
        &mut self,
        x: f32,
        y: f32,
        w: f32,
        h: f32,
        texture: std::sync::Arc<ferrous_assets::Texture2d>,
        uv0: [f32; 2],
        uv1: [f32; 2],
        color: [f32; 4],
    ) {
        let idx = self.reserve_texture_slot(texture);
        self.rect_textured(x, y, w, h, color, uv0, uv1, idx);
    }

    pub fn len(&self) -> usize {
        self.quads.len()
    }

    pub fn is_empty(&self) -> bool {
        self.quads.is_empty()
            && self.text_quads.is_empty()
            && self.icon_quads.is_empty()
            && self.svg_commands.is_empty()
    }

    pub fn as_quad_bytes(&self) -> &[u8] {
        bytemuck::cast_slice(&self.quads)
    }

    pub fn as_text_bytes(&self) -> &[u8] {
        bytemuck::cast_slice(&self.text_quads)
    }

    pub fn as_icon_bytes(&self) -> &[u8] {
        bytemuck::cast_slice(&self.icon_quads)
    }

    pub fn push_clip(&mut self, rect: ferrous_ui_core::Rect) {
        let new_rect = if let Some(current) = self.current_scissor {
            current.intersect(&rect)
        } else {
            rect
        };
        self.scissor_stack.push(new_rect);
        self.current_scissor = Some(new_rect);
        self.ensure_segment();
    }

    pub fn pop_clip(&mut self) {
        self.scissor_stack.pop();
        self.current_scissor = self.scissor_stack.last().copied();
        self.ensure_segment();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn quad_of(build: impl FnOnce(&mut GuiBatch)) -> GuiQuad {
        let mut batch = GuiBatch::new();
        build(&mut batch);
        assert_eq!(batch.quads.len(), 1);
        batch.quads[0]
    }

    #[test]
    fn border_puts_width_in_uv0_where_the_shader_reads_it() {
        let quad = quad_of(|b| {
            b.border(10.0, 20.0, 100.0, 40.0, [1.0; 4], [4.0; 4], 2.5);
        });
        assert_eq!(
            quad.uv0,
            [2.5, 0.0],
            "gui.wgsl reads border_width from uv0.x"
        );
        assert_eq!(quad.uv1, [0.0, 0.0], "uv1 must not carry the width");
        assert_eq!(quad.flags, crate::BORDER_BIT);
        assert_eq!(quad.pos, [10.0, 20.0]);
        assert_eq!(quad.size, [100.0, 40.0]);
    }

    #[test]
    fn shadow_packs_blur_and_unexpanded_size_where_the_shader_reads_them() {
        let quad = quad_of(|b| {
            b.shadow(
                100.0,
                200.0,
                300.0,
                150.0,
                [0.0, 0.0, 0.0, 0.5],
                8.0,
                0.0,
                [0.0, 4.0],
                [6.0; 4],
            );
        });
        assert_eq!(quad.uv0[0], 8.0, "gui.wgsl reads the blur from uv0.x");
        assert_eq!(quad.color_b[0], 300.0, "unexpanded width in color_b.x");
        assert_eq!(quad.color_b[1], 150.0, "unexpanded height in color_b.y");
        // The quad is expanded by blur * 2 + |offset| and moved by the offset,
        // so the shader's centred, unexpanded rect lands on the offset rect.
        let expand = 8.0 * 2.0 + 4.0;
        assert!((quad.pos[0] - (100.0 + 0.0 - expand)).abs() < 1e-4);
        assert!((quad.pos[1] - (200.0 + 4.0 - expand)).abs() < 1e-4);
        assert!((quad.size[0] - (300.0 + expand * 2.0)).abs() < 1e-4);
        assert_eq!(quad.flags, crate::SHADOW_BIT);
    }
}
