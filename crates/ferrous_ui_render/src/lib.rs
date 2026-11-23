//! `ferrous_ui_render` — GPU rendering backend for FerrousEngine UI using WGPU.
//!
//! This crate acts as the final translator between the abstract UI tree and graphics APIs.

pub mod gadgets;
pub mod gpu_types;
pub mod gui_batch;
pub mod gui_renderer;
pub mod overdraw;
pub mod persist;
pub mod pipelines;
pub mod text_utils;
pub mod to_batches;
pub mod ui_present;

// Re-exports
pub use gpu_types::{ClipShape, DrawSegment, GuiQuad, SvgCommand, TextQuad};
pub use gui_batch::{
    BackdropBlurSpec, BorderSpec, GradientBorderSpec, GuiBatch, LineSpec, ShadowSpec,
    TexturedRectSpec,
};
pub use gui_renderer::{GuiRenderer, UiFrameCounters};
pub use persist::PersistentUiTarget;
pub use to_batches::ToBatches;
pub use ui_present::UiPresenter;
// Re-exported for the GUIMaker backend so it can name the batch types without
// depending on `ferrous_ui_core` or `ferrous_assets` directly (G-0001 boundary).
#[cfg(feature = "text")]
pub use ferrous_assets::Font;
#[cfg(feature = "assets")]
pub use ferrous_assets::Texture2d;
pub use ferrous_ui_core::Rect;

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

/// Flag: the quad asks for a backdrop blur behind its rounded rect.
///
/// `color` is the translucent tint composited over the blurred backdrop,
/// `uv0.x` is the requested blur radius in pixels, and `radii` are the
/// per-corner radii of the blurred region.
///
/// [`GuiRenderer::set_backdrop_source`](crate::GuiRenderer::set_backdrop_source)
/// supplies the scene view; the renderer downsamples it, runs a separable
/// horizontal + vertical blur at quarter resolution and binds the result at
/// group 2. `gui.wgsl` samples that blurred texture and composites the tint on
/// top. Without a backdrop source the shader falls back to the translucent
/// tint masked by the rounded-rect SDF (the documented degraded path).
pub const BACKDROP_BLUR_BIT: u32 = 1 << 10;

/// Flag: the border quad carries an independent width per edge.
///
/// Packing follows `assets/shaders/gui.wgsl`: `uv0 = [left, top]` and
/// `uv1 = [right, bottom]`, in physical pixels. Without this bit the border
/// keeps the legacy packing (`uv0.x` is one width for every edge). Gradient
/// borders use the legacy packing, so the bit is never combined with
/// [`GRADIENT_BIT`].
pub const PER_SIDE_BORDER_BIT: u32 = 1 << 11;

/// Convenience mask for a gradient border: [`BORDER_BIT`] | [`GRADIENT_BIT`].
///
/// `gui.wgsl` resolves the base colour from the two-stop gradient before it
/// applies the outline mask, so a border with this flag pair is a gradient
/// border. [`GuiBatch::gradient_border`] packs it with `color` = start and
/// `color_b` = end.
pub const GRADIENT_BORDER_BITS: u32 = BORDER_BIT | GRADIENT_BIT;

// ─── DrawSegment flags (segment-level, not instance flags) ───────────────

/// Flag: the segment's clip is a rounded rectangle with per-corner radii.
///
/// Stored in [`DrawSegment::flags`](crate::DrawSegment), not in
/// [`GuiQuad::flags`](crate::GuiQuad). The precise shape travels in
/// [`DrawSegment::clip`](crate::DrawSegment); the renderer keeps the
/// rectangular `scissor` as an early reject and uploads the rect + radii to
/// the fragment shader's rounded-rectangle SDF mask, so quads, text and images
/// are also alpha-masked at the corners.
pub const ROUNDED_CLIP_BIT: u32 = 1 << 0;
