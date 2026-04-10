//! Render command system for the UI.
//!
//! Defines UI drawing operations. Commands are generated in the `draw()`
//! phase of each widget and cached in its `Node`. The backend (`ferrous_ui_render`)
//! translates them into GPU instances in the next frame if the node is marked dirty.

use crate::background::Background;
use crate::primitives::{Rect, TextAlign};

/// Simplified representation of a UI drawing operation.
/// Acts as a "Blueprint" that the rendering backend translates into GPU primitives.
#[derive(Debug, Clone)]
pub enum RenderCommand {
    // ── Basic Primitives ──────────────────────────────────────────────────

    /// Draws a solid rectangle or one with rounded corners.
    Quad {
        rect: Rect,
        color: [f32; 4],
        /// Radio of the 4 corners: [top-left, top-right, bottom-left, bottom-right].
        radii: [f32; 4],
        /// Additional shader flags.
        flags: u32,
    },

    /// Draws an outline (border) over the perimeter of the rectangle, without interior fill.
    /// More efficient than overlapping two `Quad`s. Uses BORDER_BIT in the shader.
    Border {
        rect: Rect,
        color: [f32; 4],
        radii: [f32; 4],
        /// Border thickness in pixels.
        width: f32,
    },

    /// Draws a smooth shadow (box-shadow) using Gaussian SDF.
    /// Must be emitted BEFORE the widget's `Quad` so it appears underneath.
    Shadow {
        rect: Rect,
        /// Blur radius in pixels (0 = hard shadow).
        blur_radius: f32,
        /// Expansion of the shadow beyond the rect border (can be negative).
        spread: f32,
        /// RGBA color of the shadow (typically semi-transparent black).
        color: [f32; 4],
        /// Offset [dx, dy] of the shadow relative to the rect.
        offset: [f32; 2],
    },

    // ── Text ──────────────────────────────────────────────────────────────

    /// Draws a text string using the active MSDF atlas.
    Text {
        rect: Rect,
        text: String,
        color: [f32; 4],
        font_size: f32,
        /// Text alignment within `rect`.
        align: TextAlign,
    },

    // ── Images ────────────────────────────────────────────────────────────

    /// Draws a textured image.
    #[cfg(feature = "assets")]
    Image {
        rect: Rect,
        texture: std::sync::Arc<ferrous_assets::Texture2d>,
        uv0: [f32; 2],
        uv1: [f32; 2],
        color: [f32; 4],
    },

    #[cfg(not(feature = "assets"))]
    Image {
        rect: Rect,
        texture_id: u64,
        uv0: [f32; 2],
        uv1: [f32; 2],
        color: [f32; 4],
    },

    // ── Advanced Backgrounds ──────────────────────────────────────────────

    /// Draws a gradient or procedural background.
    ///
    /// **2-stop linear/radial gradients** → 1 GPU quad (GRADIENT_BIT).
    /// **N-stop or procedural backgrounds** → Rasterized CPU strips.
    GradientQuad {
        rect: Rect,
        background: Background,
        radii: [f32; 4],
        /// Rasterization resolution for procedural backgrounds (width, height in px).
        /// `(0, 0)` uses the rect's resolution.
        raster_resolution: (u32, u32),
    },

    // ── SVGs ──────────────────────────────────────────────────────────────

    /// Draws an arbitrary mesh (tessellated SVG).
    Svg {
        rect: Rect,
        color: [f32; 4],
        /// The tessellated SVG mesh.
        mesh: ferrous_svg::SvgMesh,
    },

    // ── Clipping Control ──────────────────────────────────────────────────

    /// Starts a clipping region. Everything drawn after is limited to this rect.
    PushClip { rect: Rect },

    /// Finalizes the most recent clipping region and restores the previous one.
    PopClip,

    /// Dibujar un icono MSDF ultra-nítido desde un atlas.
    Icon { name: String, rect: Rect, color: [f32; 4] },
}



/// Render command captured with depth and identity metadata.
#[derive(Debug, Clone)]
pub struct CapturedCommand {
    pub cmd: RenderCommand,
    pub z: f32,
    pub node_id: u32,
}

impl RenderCommand {
    // ── Convenience Constructors ──────────────────────────────────────────

    /// Solid quad without rounded corners.
    #[inline]
    pub fn rect(rect: Rect, color: [f32; 4]) -> Self {
        Self::Quad { rect, color, radii: [0.0; 4], flags: 0 }
    }

    /// Quad with uniform corner radius on all 4 corners.
    #[inline]
    pub fn rect_rounded(rect: Rect, color: [f32; 4], radius: f32) -> Self {
        Self::Quad { rect, color, radii: [radius; 4], flags: 0 }
    }

    /// Shadow with standard Material Design parameters.
    #[inline]
    pub fn shadow_md(rect: Rect) -> Self {
        Self::Shadow {
            rect,
            blur_radius: 8.0,
            spread: 0.0,
            color: [0.0, 0.0, 0.0, 0.35],
            offset: [0.0, 4.0],
        }
    }

    /// Subtle shadow for slightly elevated elements.
    #[inline]
    pub fn shadow_sm(rect: Rect) -> Self {
        Self::Shadow {
            rect,
            blur_radius: 4.0,
            spread: 0.0,
            color: [0.0, 0.0, 0.0, 0.2],
            offset: [0.0, 2.0],
        }
    }

    /// Large shadow for modals and overlays.
    #[inline]
    pub fn shadow_lg(rect: Rect) -> Self {
        Self::Shadow {
            rect,
            blur_radius: 24.0,
            spread: -4.0,
            color: [0.0, 0.0, 0.0, 0.5],
            offset: [0.0, 8.0],
        }
    }
}