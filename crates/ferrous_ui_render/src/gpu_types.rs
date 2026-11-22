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
/// - offset 48: `color_b` [f32; 4]  — RGBA secondary (gradient end, shadow color)
/// - offset 64: `radii`   [f32; 4]  — corner radii (TL, TR, BL, BR)
/// - offset 80: `tex_idx` `u32`     — texture array index
/// - offset 84: `flags`   `u32`     — control bits (textured, gradient, borders, etc.)
/// - offset 88: `z_order` `f32`     — depth (0.0=back, 1.0=front)
/// - offset 92: `node_id` `u32`     — widget ID for GPU hit-testing
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
    #[must_use]
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
    ///
    /// `colors` is `[start, end]`: the shader mixes `colors[0]` into
    /// `colors[1]` according to the gradient flags.
    #[must_use]
    #[inline]
    pub fn gradient(
        pos: [f32; 2],
        size: [f32; 2],
        colors: [[f32; 4]; 2],
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
            color: colors[0],
            color_b: colors[1],
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

/// A generic clip region: a rectangle with optional per-corner radii.
///
/// The GPU scissor is always axis-aligned, so a rounded clip cannot be
/// expressed by `set_scissor_rect` alone. This type carries both the
/// rectangular bounds and the precise shape:
///
/// - [`Self::rect`] is the scissor (the smallest rectangle that contains the
///   shape). The renderer applies it first as a cheap early reject.
/// - [`Self::radii`] follows [`GuiQuad::radii`] order: top-left, top-right,
///   bottom-left, bottom-right. All zero means a plain rectangle.
///
/// The renderer delivers `rect` + `radii` to the fragment shader as a
/// per-segment rounded-rectangle SDF, so quads, text and images inside the
/// clip are alpha-masked at the rounded corners as well as scissored.
///
/// Nested clips are intersected on the CPU. When the intersection can be
/// expressed exactly (the inner shape is fully inside the outer one) the inner
/// radii survive; otherwise the result is not a rounded rectangle and the
/// result is the rectangular intersection with zero radii.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ClipShape {
    pub rect: ferrous_ui_core::Rect,
    pub radii: [f32; 4],
}

impl ClipShape {
    /// A rectangular clip (no rounded corners).
    #[inline]
    pub fn rectangular(rect: ferrous_ui_core::Rect) -> Self {
        Self {
            rect,
            radii: [0.0; 4],
        }
    }

    /// A clip with per-corner radii (top-left, top-right, bottom-left, bottom-right).
    #[inline]
    pub fn rounded(rect: ferrous_ui_core::Rect, radii: [f32; 4]) -> Self {
        Self {
            rect,
            radii: clamp_radii(rect, radii),
        }
    }

    /// Whether any corner has a positive radius.
    #[inline]
    pub fn is_rounded(&self) -> bool {
        self.radii.iter().any(|radius| *radius > 0.0)
    }

    /// The rectangular scissor that approximates this shape.
    #[inline]
    pub fn scissor(&self) -> ferrous_ui_core::Rect {
        self.rect
    }

    /// Intersects this clip with `other`, keeping the shape exact only when the
    /// result is still a rectangle or when `other` is fully inside `self`.
    ///
    /// A partial overlap between two rounded shapes is not a rounded rectangle,
    /// so it degrades to the rectangular intersection (documented fallback).
    pub fn intersect(&self, other: &Self) -> Self {
        let rect = self.rect.intersect(&other.rect);
        // The tighter clip wins when one shape fully contains the other.
        if contains(&other.rect, &self.rect) {
            return *self;
        }
        if contains(&self.rect, &other.rect) {
            return other.clamped_to(rect);
        }
        // A partial overlap between two rounded shapes is not a rounded
        // rectangle: degrade to the rectangular intersection.
        Self::rectangular(rect)
    }

    fn clamped_to(self, rect: ferrous_ui_core::Rect) -> Self {
        Self {
            rect,
            radii: clamp_radii(rect, self.radii),
        }
    }
}

/// Clamps per-corner radii to the rectangle and to half its smaller side so
/// adjacent corners cannot overlap.
#[inline]
fn clamp_radii(rect: ferrous_ui_core::Rect, radii: [f32; 4]) -> [f32; 4] {
    let half = (rect.width.min(rect.height) * 0.5).max(0.0);
    radii.map(|radius| radius.max(0.0).min(half))
}

#[inline]
fn contains(outer: &ferrous_ui_core::Rect, inner: &ferrous_ui_core::Rect) -> bool {
    inner.x >= outer.x
        && inner.y >= outer.y
        && inner.x + inner.width <= outer.x + outer.width
        && inner.y + inner.height <= outer.y + outer.height
}

/// Draw segment defining ranges for various primitives and an optional scissor.
///
/// A segment is also the unit of painter order: the renderer draws all quads,
/// then all text, then icons and SVG *within one segment* before moving to the
/// next. [`GuiBatch::extend`](crate::GuiBatch::extend) may therefore coalesce
/// contiguous same-scissor segments, which is only pixel-identical when the
/// merged segments do not overlap on screen (Motioner's virtualised rows are
/// disjoint). A layer barrier (`layer_start`) always stays a separate segment.
#[derive(Clone, Debug, PartialEq)]
pub struct DrawSegment {
    pub quad_range: std::ops::Range<u32>,
    pub text_range: std::ops::Range<u32>,
    pub icon_range: std::ops::Range<u32>,
    pub svg_range: std::ops::Range<u32>,
    pub scissor: Option<ferrous_ui_core::Rect>,
    /// Precise clip shape when `scissor` cannot express it (rounded corners).
    ///
    /// `None` for the default rectangular scissor. When present, `scissor` is
    /// still its rectangular bounds and the renderer uploads the rect + radii
    /// to the fragment shader's rounded-rectangle SDF mask.
    pub clip: Option<ClipShape>,
    /// Segment flags, see [`crate::ROUNDED_CLIP_BIT`].
    pub flags: u32,
    /// `true` when this segment starts a [`GuiBatch::begin_layer`](crate::GuiBatch::begin_layer)
    /// barrier and must never be merged with the segment before it.
    pub layer_start: bool,
}

impl DrawSegment {
    /// Whether this segment carries a rounded clip mask.
    #[inline]
    pub fn has_rounded_clip(&self) -> bool {
        self.flags & crate::ROUNDED_CLIP_BIT != 0
    }
}
