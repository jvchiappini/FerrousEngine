/// `ferrous_renderer` -- modular, extensible GPU rendering for Ferrous Engine.
///
/// This crate provides a thin re-export layer for the renderer modules.
/// All implementation code has been moved to separate modules:
/// - `renderer_api`: Public API methods for the Renderer
/// - `renderer_core`: Core Renderer struct and lifecycle methods
/// - `renderer_resource`: Resource management (textures, materials, meshes)
/// - `renderer_passes`: Render pass coordination and execution pipeline

// -- Module declarations ------------------------------------------------------

pub mod renderer_api;
pub mod renderer_core;
pub mod renderer_resource;
pub mod renderer_passes;
pub mod exporter;

// Support modules located in src/
pub mod camera;
pub mod camera_system;
pub mod frame_builder;
pub mod geometry;
pub mod gizmo_system;
pub mod graph;
pub mod materials;
pub mod passes;
pub mod pipeline;
pub mod render_stats;
pub mod render_target;
pub mod resources;
pub mod scene;
pub mod context;

// -- Public re-exports --------------------------------------------------------

// Re-export core renderer types
pub use renderer_core::{Renderer, RendererMode};

// Re-export all public API methods
pub use renderer_api::*;

// Re-export resource management types
pub use renderer_resource::MaterialRegistry;
pub use resources::SsaoResources;

// Re-export render pass types
pub use renderer_passes::RendererPasses;

// Re-export commonly used types from renderer_core
pub use renderer_core::{
    Camera, Controller, GpuCamera, CameraSystem, Viewport, RenderPass, RenderStats,
    RenderTarget, InstanceBuffer, FrameBuilder, GizmoSystem, InstancingPipeline,
    CelShadedPass, FlatShadedPass, OutlinePass, PostProcessPass, PrePass, SsaoBlurPass,
    SsaoPass, WorldPass,
};
// Antialiasing
pub use passes::{AntialiasingMode, AntialiasingPass, FxaaParams};

// Re-export geometry types
pub use geometry::{Mesh, Vertex};
// Re-export scene types
pub use scene::{Aabb, Frustum, SceneData, GizmoDraw};
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

// -- 2D Technical Rendering Re-exports ----------------------------------------
pub mod render_2d {
    pub use ferrous_2d::render::types::ShapeInstance;
    pub use ferrous_2d::components::Shape2d;
}