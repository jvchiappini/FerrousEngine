//! GPU-facing 2D rendering layer.
//!
//! Contains the POD instance types uploaded to the GPU, the wgpu pipelines and
//! bind-group layouts, the per-frame batchers, and [`Renderer2d`], the helper
//! that owns the instance buffers and issues the draw calls.

/// Per-frame instance batchers ([`ShapeBatcher`], [`SpriteBatcher`]) and the
/// unified [`DrawCommand2d`] command stream.
pub mod batch;
/// CPU-side gradient encoding for the Pure2D shaders ([GradientTable2d]).
pub mod gradient;
/// wgpu render pipelines and bind-group layouts for sprites and shapes.
pub mod pipeline;
/// POD instance types for sprites, shapes and the camera uniform.
pub mod types;

pub use batch::*;
pub use gradient::*;
pub use pipeline::*;
pub use types::*;
/// The [`Renderer2d`] helper and its per-frame [`Frame2dCounters`].
pub mod renderer;
pub use renderer::*;
