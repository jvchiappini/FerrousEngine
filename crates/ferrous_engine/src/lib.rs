//! # Ferrous Engine SDK
//!
//! Librería de Renderizado Profesional (SDK) de código abierto.
//!
//! Este crate es el punto de entrada principal para trabajar con el motor de renderizado Ferrous.
//! Permite inicializar el contexto gráfico, gestionar la escena y ejecutar el pipeline
//! de renderizado de forma determinista y profesional.
//!
//! ## Ejemplo de uso (SDK)
//!
//! ```rust,ignore
//! use ferrous_engine::Renderer;
//!
//! fn main() -> anyhow::Result<()> {
//!     let mut renderer = Renderer::builder()
//!         .with_dimensions(1920, 1080)
//!         .with_headless_mode(false)
//!         .build()?;
//!
//!     let mesh = renderer.spawn_mesh("player", Transform::identity());
//!     renderer.update_color(mesh, [0.8, 0.2, 0.2, 1.0]);
//!
//!     Ok(())
//! }
//! ```

mod asset_bridge;
mod builder;
pub mod config;
pub mod context;
mod graphics;
pub mod plugin;
pub mod renderer;
pub mod render_context;
mod runner;
pub mod traits;

// --- Re-exports Principales (SDK Facade) ---
pub use renderer::{Renderer, RendererBuilder, RendererConfig, EngineError};

// --- Tipos de Escena y Transformación ---
pub use ferrous_core::api::types::NodeId;
pub use ferrous_core::{Color, Transform, World};
pub use ferrous_core::scene::{
    Camera3D, Camera3DBuilder, DirectionalLight, OrbitCamera,
    Material, MaterialBuilder, BillboardMode,
    AnimJob, Animator, EasingType, MaterialComponent
};

// --- Sistema de Exportación ---
pub use ferrous_renderer::exporter::{FrameExporter, FFmpegExporter};

// --- Compatibilidad con Framework (Opcional) ---
pub use traits::{DrawContext, FerrousApp};
pub use builder::{App, AppConfig, AppMode};
pub use context::{AppContext, WindowResizeDirection};
pub use render_context::RenderContext;
pub use ferrous_core::scene::camera::Projection as ProjectionType;

// --- Re-exports de Utilidad (ferrous_core / glam) ---
pub use ferrous_core::glam;
pub use ferrous_core::glam::{Mat4, Quat, Vec2, Vec3, Vec4};
pub use ferrous_core::{Time, TimeClock, Handle, InputState, KeyCode, MouseButton, RenderQuality, RenderStats, Viewport};
pub use ferrous_core::scene::world::types::{PathData, PathCommand};
pub use ferrous_ecs::prelude::Entity;

// --- Estilos de Renderizado ---
pub use ferrous_renderer::{RenderStyle, RendererMode};
pub use ferrous_renderer::{AntialiasingMode, FxaaParams};

// --- Helpers de Assets ---
pub use crate::asset_bridge::{spawn_gltf, spawn_gltf_async, GltfSpawnTask};

// Gizmo types
pub use ferrous_renderer::scene::GizmoDraw;

// Export wgpu and winit types for consumers
pub use wgpu;
pub use winit::window::CursorIcon;
