//! Framework modular para crear aplicaciones y juegos con FerrousEngine.
//!
//! ## Sistema de ejecución por etapas
//!
//! El runner utiliza un `StagedScheduler` con etapas fijas:
//!
//! | Etapa | Sistemas registrados |
//! |-------|---------------------|
//! | `PreUpdate`  | `TimeSystem` — actualiza el reloj de frame |
//! | `Update`     | `VelocitySystem`, `AnimationSystem`, `BehaviorSystem` |
//! | `PostUpdate` | `TransformSystem` — propaga `GlobalTransform` por la jerarquía |
//!
//! Para añadir sistemas propios usa `AppContext::scheduler` (si está expuesto)
//! o implementa un [`FerrousApp::setup`] que inserte componentes ECS.
//!
//! # Quick-start
//!
//! ```rust,ignore
//! use ferrous_app::{App, AppContext, FerrousApp, Color, Vec3};
//!
//! struct MyGame;
//!
//! impl FerrousApp for MyGame {
//!     fn setup(&mut self, ctx: &mut AppContext) {
//!         ctx.world.spawn_cube("Ground", Vec3::ZERO);
//!     }
//!
//!     fn update(&mut self, ctx: &mut AppContext) {
//!         if ctx.input.just_pressed(ferrous_app::KeyCode::Escape) {
//!             ctx.request_exit();
//!         }
//!     }
//! }
//!
//! fn main() {
//!     App::new(MyGame)
//!         .with_title("My Game")
//!         .with_background_color(Color::SKY_BLUE)
//!         .run();
//! }
//! ```

mod asset_bridge;
pub mod builder;
pub mod context;
mod graphics;
mod runner;
pub mod traits;

pub use builder::{App, AppConfig};
pub use context::AppContext;
pub use traits::FerrousApp;

// ── Render style ───────────────────────────────────────────────────────────
pub use ferrous_renderer::RenderStyle;

// ── Re-export the most-used ferrous_core primitives ────────────────────────
// Users can do `use ferrous_app::{Color, Time, World, Handle, Vec3};` without
// adding ferrous_core as a direct dependency.
pub use ferrous_core::{
    Color, Handle, InputState, KeyCode, MouseButton, Time, TimeClock, Transform, World,
};

// glam math types — re-exported for convenience
pub use ferrous_core::glam::{Mat4, Quat, Vec2, Vec3, Vec4};

// Renderer-agnostic display types
pub use ferrous_core::{RenderStats, Viewport};

// Gizmo types — re-exported so app code doesn't need ferrous_renderer directly.
pub use ferrous_renderer::scene::GizmoDraw;

// ECS stage / system types — game code can register custom systems
pub use ferrous_core::{AnimationClip, AnimationPlayer, Keyframe};
pub use ferrous_core::{Behavior, BehaviorComponent, Stage, Velocity};
pub use ferrous_core::{Children, GlobalTransform, Parent};
pub use ferrous_ecs::prelude::{Entity, StagedScheduler};

// helpers
pub use crate::asset_bridge::{spawn_gltf, spawn_gltf_async, GltfSpawnTask};
