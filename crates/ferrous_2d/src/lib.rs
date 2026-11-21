//! 2D subsystem for FerrousEngine.
//!
//! This crate provides the data types and GPU pipelines for the engine's 2D
//! render path:
//!
//! - [`components`] — ECS components describing 2D entities: [`Transform2d`],
//!   [`Sprite`], [`Shape2d`], [`Path2d`] and [`Camera2d`].
//! - [`render`] — the GPU-facing layer: [`ShapeInstance`]/[`SpriteInstance`],
//!   the instance batchers ([`ShapeBatcher`], [`SpriteBatcher`]), the wgpu
//!   pipelines and [`Renderer2d`].
//! - [`systems`] — **deprecated** ECS→batcher preparation systems. The engine
//!   does **not** register these in any scheduler, so they are dead code; the
//!   live path is the immediate-mode API on `ferrous_renderer::Renderer`.
//!
//! Most applications never name these types directly. They draw through the
//! user-facing façade on `ferrous_engine::RenderContext`; see the crate
//! `README.md` for the immediate-mode/ECS distinction and known caveats.

/// 2D ECS components (`Transform2d`, `Sprite`, `Shape2d`, `Path2d`, `Camera2d`).
pub mod components;
/// Generation-tracked handles ([`handle::Handle`]) and a reusable [`handle::SlotRegistry`].
pub mod handle;
/// GPU pipelines, instance types, batchers and the [`Renderer2d`] helper.
pub mod render;
/// Deprecated ECS preparation systems that convert 2D components into batcher
/// instances. Not registered in any scheduler; use the immediate-mode renderer.
pub mod systems;

// Re-exports
pub use components::*;
// To be implemented:
pub use render::*;
pub use systems::*;
