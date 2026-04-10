//! GPU data structures for UI rendering.
//!
//! This module contains the core GPU-visible data structures used by the UI renderer.
//! All structs are marked with `#[repr(C)]` where appropriate for C-compatible layout.

/// Exact GPU memory representation of a UI quad.
///
/// Memory layout (96 bytes total, 16-aligned):
/// - offset  0: pos      [f32; 2]  — position in px
/// - offset  8: size     [f32; 2]  — size in px
/// - offset 16: uv0      [f32; 2]  — UV top-left
/// - offset 24: uv1      [f32; 2]  — UV bottom-right
/// - offset 32: color    [f32; 4]  — RGBA primary color (16 bytes)
/// - offset 48: color_b  [f32; 4]  — RGBA secondary (gradient end, shadow color)
/// - offset 64: radii    [f32; 4]  — corner radii (TL, TR, BL, BR)
/// - offset 80: tex_idx  u32       — texture array index
/// - offset 84: flags    u32       — control bits (textured, gradient, borders, etc.)
/// - offset 88: z_order  f32       — depth (0.0=back, 1.0=front)
/// - offset 92: node_id  u32       — widget ID for GPU hit-testing
#[repr(C)]
#[derive(Copy, Clone, bytemuck::Pod, bytemuck::Zeroable, Debug)]
pub struct GuiQuad {
    pub pos: [f32; 2],
    pub size: [f32; 2],
    pub uv0: [f32; 2],
    pub uv1: [f32; 2],
    pub color: [f32; 4],
    pub color_b: [f32; 4],
    pub radii: [f32; 4],
    pub tex_index: u32,
    pub flags: u32,
    pub z_order: f32,
    pub node_id: u32,
}

impl GuiQuad {
    /// Creates a standard solid Quad.
    #[inline]
    pub fn solid(
        pos: [f32; 2],
        size: [f32; 2],
        color: [f32; 4],
        radii: [f32; 4],
        flags: u32,
        z_order: f32,
        node_id: u32,
    ) -> Self {
        Self {
            pos,
            size,
            uv0: [0.0, 0.0],
            uv1: [1.0, 1.0],
            color,
            color_b: [0.0; 4],
            radii,
            tex_index: 0,
            flags,
            z_order,
            node_id,
        }
    }

    /// Creates a GPU-interpolated gradient Quad.
    #[inline]
    pub fn gradient(
        pos: [f32; 2],
        size: [f32; 2],
        color_a: [f32; 4],
        color_b: [f32; 4],
        radii: [f32; 4],
        gradient_flags: u32,
        z_order: f32,
        node_id: u32,
    ) -> Self {
        Self {
            pos,
            size,
            uv0: [0.0, 0.0],
            uv1: [1.0, 1.0],
            color: color_a,
            color_b,
            radii,
            tex_index: 0,
            flags: gradient_flags,
            z_order,
            node_id,
        }
    }
}

/// GPU memory representation of a text glyph.
#[repr(C)]
#[derive(Copy, Clone, bytemuck::Pod, bytemuck::Zeroable, Debug)]
pub struct TextQuad {
    pub pos: [f32; 2],
    pub size: [f32; 2],
    pub uv0: [f32; 2],
    pub uv1: [f32; 2],
    pub color: [f32; 4],
    pub z_order: f32,
    pub node_id: u32,
}

/// Abstract representation of an SVG draw command.
#[derive(Clone, Debug)]
pub struct SvgCommand {
    pub mesh: ferrous_svg::SvgMesh,
    pub pos: [f32; 2],
    pub color: [f32; 4],
    pub z: f32,
}

/// Draw segment defining ranges for various primitives and an optional scissor.
#[derive(Clone, Debug)]
pub struct DrawSegment {
    pub quad_range: std::ops::Range<u32>,
    pub text_range: std::ops::Range<u32>,
    pub icon_range: std::ops::Range<u32>,
    pub svg_range: std::ops::Range<u32>,
    pub scissor: Option<ferrous_ui_core::Rect>,
}