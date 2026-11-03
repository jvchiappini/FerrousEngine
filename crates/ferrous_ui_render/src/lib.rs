//! `ferrous_ui_render` — GPU rendering backend for FerrousEngine UI using WGPU.
//!
//! This crate acts as the final translator between the abstract UI tree and graphics APIs.

pub mod gadgets;
pub mod gpu_types;
pub mod gui_batch;
pub mod gui_renderer;
pub mod pipelines;
pub mod text_utils;
pub mod to_batches;

// Re-exports
pub use gpu_types::{DrawSegment, GuiQuad, SvgCommand, TextQuad};
pub use gui_batch::GuiBatch;
pub use gui_renderer::GuiRenderer;
pub use to_batches::ToBatches;

/// Maximum number of distinct textures that can be referenced in a single draw batch.
pub const MAX_TEXTURE_SLOTS: u32 = 16;

// ─── Fragment shader flags (gui.wgsl) ────────────────────────────────────

/// Flag: draws the color picker wheel color (HSV wheel). Historical value = 1.
pub const COLOR_WHEEL_BIT: u32 = 1 << 0;

/// Flag: the quad samples a texture from the binding_array.
pub const TEXTURED_BIT: u32 = 1 << 1;

/// Flag: 2-color gradient resolved on GPU. `color` = start, `color_b` = end.
/// Default direction: left → right (raw_uv.x).
pub const GRADIENT_BIT: u32 = 1 << 2;

/// Flag: thin gradient strip (radial/conic, legacy — many strips per rect).
pub const GRADIENT_STRIP_BIT: u32 = 1 << 3;

/// Flag: vertical gradient (top → bottom). Used in combination with GRADIENT_BIT.
pub const GRADIENT_V_BIT: u32 = 1 << 4;

/// Flag: radial gradient (from center outwards). Used in combination with GRADIENT_BIT.
pub const GRADIENT_RADIAL_BIT: u32 = 1 << 5;

/// Flag: border (outline) without fill. `color` = border color, `color_b.r` = width in px.
pub const BORDER_BIT: u32 = 1 << 6;

/// Flag: smooth shadow (box-shadow). `color_b` = shadow color, `uv0` = offset, `uv1.x` = blur.
pub const SHADOW_BIT: u32 = 1 << 7;

/// Flag: indicates the quad is fully opaque and can write to the depth buffer (Early-Z).
pub const OPAQUE_BIT: u32 = 1 << 8;

/// Flag: the quad is an anti-aliased line segment (see [`GuiBatch::line`]).
///
/// `uv0` and `uv1` hold the segment endpoints in pixels relative to the
/// quad's top-left corner, and `radii.x` is the half line width. The quad is
/// the segment's bounds expanded by the half width plus one anti-aliasing
/// pixel, so the fragment shader can round-cap both ends.
///
/// Segments are independent quads. A translucent [`GuiBatch::polyline`]
/// therefore composites every shared endpoint twice (coverage
/// `1 - (1 - a)^2` instead of `a`) where two round caps overlap. This is a
/// documented limitation of the primitive; see that method for the guidance
/// and the test that pins the overlap.
pub const LINE_BIT: u32 = 1 << 9;
