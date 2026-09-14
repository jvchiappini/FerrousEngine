//! GPU-facing 2D rendering layer.
//!
//! Contains the POD instance types uploaded to the GPU, the wgpu pipelines and
//! bind-group layouts, the per-frame batchers, and [`Renderer2d`], the helper
//! that owns the instance buffers and issues the draw calls.

/// POD instance types for sprites, shapes and the camera uniform.
pub mod types;
/// wgpu render pipelines and bind-group layouts for sprites and shapes.
pub mod pipeline;
/// Per-frame instance batchers ([`ShapeBatcher`], [`SpriteBatcher`]) and the
/// unified [`DrawCommand2d`] command stream.
pub mod batch;

pub use types::*;
pub use pipeline::*;
pub use batch::*;
/// The [`Renderer2d`] helper and its per-frame [`Frame2dCounters`].
pub mod renderer; pub use renderer::*;
