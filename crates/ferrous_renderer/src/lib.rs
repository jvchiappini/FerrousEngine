//! `ferrous_renderer` -- modular, extensible GPU rendering for Ferrous Engine.
//!
//! This crate provides a thin re-export layer for the renderer modules.
//! All implementation code has been moved to separate modules:
//! - `renderer_api`: Public API methods for the Renderer
//! - `renderer_core`: Core Renderer struct and lifecycle methods
//! - `renderer_resource`: Resource management (textures, materials, meshes)
//! - `renderer_passes`: deprecated, unwired render-pass coordination skeleton
//!   (kept only for API compatibility; the live pipeline is `renderer_core`)
// -- Module declarations ------------------------------------------------------

pub mod exporter;
pub mod renderer_api;
pub mod renderer_core;
// A dead, unwired duplicate of the pass coordination; it references every 3D
// pass, so it is compiled only with the 3D renderer.
#[cfg(any(feature = "render-3d", not(feature = "flat2d")))]
pub mod renderer_passes;
pub mod renderer_resource;

// Headless (windowless) benchmark + integration-test harness.  Native-only
// because device creation blocks on wgpu's async requests via `pollster`.
// The Pure2D headless path does not need the `gui` feature.
#[cfg(not(target_arch = "wasm32"))]
pub mod headless;
// Generic off-screen colour target for the 2D renderer (screenshots, editor
// viewports, goldens). Native-only for the same reason as `headless`.
#[cfg(not(target_arch = "wasm32"))]
pub mod scene_target;

// Support modules located in src/.
//
// The CPU 3D modules (camera, geometry, pipeline, scene, materials,
// gizmo_system) are lightweight type/setup code the lazy slots and the 2D
// camera packet reuse, so they stay compiled in a Flat2D build. The *heavy*
// 3D surface — the render passes (world/SSAO/sky/post/AA/cel/outline/flat/
// skinning/particles) and the HDR render targets — is gated inside `passes`
// and `render_target` (Phase 9D: a Flat2D binary does not link those shaders).
pub mod camera;
pub mod camera_system;
pub mod context;
pub mod frame_builder;
pub mod geometry;
pub mod gizmo_system;
pub mod gpu_timing;
// The render-graph abstraction (`FramePacket`, `RenderPass`, `Viewport`) is a
// thin re-export of `ferrous_render_graph` and is consumed by the 2D `ui_pass`,
// so it stays available in Flat2D builds.  It owns no 3D resources.
pub mod graph;
pub mod materials;
pub mod passes;
pub mod pipeline;
pub mod render_config;
pub mod render_stats;
pub mod render_target;
pub mod resources;
pub mod scene;
pub mod static_2d;

// -- Public re-exports --------------------------------------------------------

// Re-export core renderer types
pub use renderer_core::{Renderer, RendererMode};

// Generic off-screen 2D scene target.
#[cfg(not(target_arch = "wasm32"))]
pub use scene_target::SceneTarget2d;

// Re-export all public API methods
pub use renderer_api::*;

// Re-export resource management types
pub use renderer_resource::MaterialRegistry;
#[cfg(any(feature = "render-3d", not(feature = "flat2d")))]
pub use resources::SsaoResources;

// Re-export render pass types.  `RendererPasses` is a dead, unwired duplicate
// of `Renderer`'s live pipeline; it is deprecated rather than removed because
// the crate re-exports it publicly and an external consumer cannot be ruled
// out from this workspace (see the type's deprecation note).  New code must
// use `Renderer`.
#[cfg(any(feature = "render-3d", not(feature = "flat2d")))]
#[allow(deprecated)]
pub use renderer_passes::RendererPasses;

// The UI-batch recycler is always compiled; the 3D builder and every 3D pass
// type are re-exported only when the 3D renderer is compiled.
#[cfg(any(feature = "render-3d", not(feature = "flat2d")))]
pub use renderer_core::{
    Camera, CameraSystem, CelShadedPass, Controller, FlatShadedPass, GizmoSystem, GpuCamera,
    InstanceBuffer, InstancingPipeline, OutlinePass, PostProcessPass, PrePass, SsaoBlurPass,
    SsaoPass, WorldBuilder, WorldPass,
};
pub use renderer_core::{
    FrameBuilder, RenderPass, RenderStats, RenderTarget, RenderTargetHandle, TextureUpdateError,
    Viewport,
};
// Antialiasing
#[cfg(any(feature = "render-3d", not(feature = "flat2d")))]
pub use passes::{AntialiasingMode, AntialiasingPass, FxaaParams};

// Runtime render/performance configuration (Part 1/2 of the 2D performance pass)
pub use ferrous_2d::render::types::Grid2dParams;
pub use render_config::{Antialiasing2d, Render2dConfig};

// Hardware-independent per-frame 2D counters, surfaced through
// `Renderer::frame_2d_stats()` and `ferrous_engine::RenderContext`.
pub use ferrous_2d::render::Frame2dCounters;
/// Consumer-facing alias for [`Frame2dCounters`].
pub type Frame2dStats = Frame2dCounters;

// Retained static 2D batch handle (see `Renderer::create_static_2d_batch`).
pub use static_2d::Static2dBatchId;

// Re-export geometry types
#[cfg(any(feature = "render-3d", not(feature = "flat2d")))]
pub use geometry::{Mesh, Vertex};
// Re-export scene types
#[cfg(any(feature = "render-3d", not(feature = "flat2d")))]
pub use scene::{Aabb, Frustum, GizmoDraw, SceneData};
// Re-export material types from ferrous_core
pub use ferrous_core::scene::{
    AlphaMode, MaterialDescriptor, MaterialHandle, RenderStyle, MATERIAL_DEFAULT,
};
// Re-export texture handles from resources
pub use resources::{TextureHandle, TEXTURE_BLACK, TEXTURE_NORMAL, TEXTURE_WHITE};

// Re-export glam for convenience
pub use glam;

// Re-export input types
pub use ferrous_core::input::{KeyCode, MouseButton};

// Re-export GUI types when gui feature is enabled
#[cfg(feature = "gui")]
pub use ferrous_ui_render::{GuiBatch, GuiQuad};
/// Placeholder GUI batch used when the `gui` feature is disabled.
///
/// The 3-D `FrameBuilder`/`WorldBuilder` hand a GUI batch back to the frame
/// scratch; without the GUI stack there is never one to hand back, but the
/// type must still be nameable so those paths compile unchanged.
#[cfg(not(feature = "gui"))]
#[derive(Debug, Default, Clone, Copy)]
pub struct GuiBatch;

// -- 2D Technical Rendering Re-exports ----------------------------------------
pub mod render_2d {
    pub use ferrous_2d::components::Shape2d;
    pub use ferrous_2d::render::types::{Grid2dParams, ShapeInstance};
}
