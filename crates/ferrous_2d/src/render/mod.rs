//! GPU-facing 2D rendering layer.
//!
//! Contains the POD instance types uploaded to the GPU, the wgpu pipelines and
//! bind-group layouts, the per-frame batchers, and [`Renderer2d`], the helper
//! that owns the instance buffers and issues the draw calls.

/// Per-frame instance batchers ([`ShapeBatcher`], [`SpriteBatcher`]) and the
/// unified [`DrawCommand2d`] command stream.
pub mod batch;
/// Layer effects: the Gaussian blur kernel and the drop-shadow, blur and glow
/// descriptions ([`Effect2d`]).
pub mod effects;
/// CPU-side gradient encoding for the Pure2D shaders ([GradientTable2d]).
pub mod gradient;
/// wgpu render pipelines and bind-group layouts for sprites and shapes.
pub mod pipeline;
/// Text layout types and per-glyph draw requests.
pub mod text;
/// CPU mip-chain generation and sprite texture options.
pub mod texture;
/// POD instance types for sprites, shapes and the camera uniform.
pub mod types;

pub use batch::*;
pub use effects::*;
pub use gradient::*;
pub use pipeline::*;
pub use text::*;
pub use texture::*;
pub use types::*;
/// The [`Renderer2d`] helper and its per-frame [`Frame2dCounters`].
pub mod renderer;
pub use renderer::*;
