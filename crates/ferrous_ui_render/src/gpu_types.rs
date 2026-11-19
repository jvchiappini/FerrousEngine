//! GPU data structures for UI rendering.
//!
//! This module contains the core GPU-visible data structures used by the UI renderer.
//! All structs are marked with `#[repr(C)]` where appropriate for C-compatible layout.

/// Exact GPU memory representation of a UI quad.
///
/// Memory layout (96 bytes total, 4-byte aligned, no padding):
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
///
/// `gui.wgsl` stops at `z_order` (location 10); `node_id` is read only by the
/// hit-test pipeline (`id_buffer.wgsl`, location 11), so the quad pipeline
/// omits that attribute while the instance buffer keeps the same 96-byte
/// stride for both passes. The flag word is defined in [`crate::COLOR_WHEEL_BIT`]
/// through [`crate::LINE_BIT`] (bits 0..=9); the remaining bits are free.
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
///
/// Memory layout (56 bytes total, 4-byte aligned, no padding):
/// - offset  0: pos      [f32; 2]  — position in px
/// - offset  8: size     [f32; 2]  — size in px
/// - offset 16: uv0      [f32; 2]  — atlas UV top-left
/// - offset 24: uv1      [f32; 2]  — atlas UV bottom-right
/// - offset 32: color    [f32; 4]  — RGBA primary color
/// - offset 48: z_order  f32       — depth (0.0=back, 1.0=front)
/// - offset 52: node_id  u32       — declared by `text.wgsl` (location 7)
///
/// The same layout carries icon glyphs; both are uploaded through the text
/// pipeline. `node_id` is part of the shader interface but `text.wgsl` never
/// reads its value, so it is kept only because that interface still declares
/// it (dropping the field needs a coordinated `assets/shaders/text.wgsl` edit,
/// which is outside this module).
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

// Both instance layouts are uploaded verbatim, so any accidental field,
// reorder or alignment change is a wire-format break. Fail to compile instead.
const _: () = {
    assert!(std::mem::size_of::<GuiQuad>() == 96);
    assert!(std::mem::align_of::<GuiQuad>() == 4);
    assert!(std::mem::offset_of!(GuiQuad, color) == 32);
    assert!(std::mem::offset_of!(GuiQuad, radii) == 64);
    assert!(std::mem::offset_of!(GuiQuad, tex_index) == 80);
    assert!(std::mem::offset_of!(GuiQuad, flags) == 84);
    assert!(std::mem::offset_of!(GuiQuad, z_order) == 88);
    assert!(std::mem::offset_of!(GuiQuad, node_id) == 92);

    assert!(std::mem::size_of::<TextQuad>() == 56);
    assert!(std::mem::align_of::<TextQuad>() == 4);
    assert!(std::mem::offset_of!(TextQuad, color) == 32);
    assert!(std::mem::offset_of!(TextQuad, z_order) == 48);
    assert!(std::mem::offset_of!(TextQuad, node_id) == 52);
};

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
