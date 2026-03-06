//! # ferrous_core
//!
//! Foundational types shared by every layer of FerrousEngine.  This crate
//! has **zero renderer dependencies** — it is safe to use in headless tests,
//! tools, and server-side game logic.
//!
//! ## What lives here
//!
//! | Module | Purpose |
//! |--------|---------|
//! [`transform`] | `Transform` — position / rotation / scale + `matrix()` |
//! [`color`]     | `Color` — RGBA f32 with a large palette of constants |
//! [`time`]      | `Time` / `TimeClock` — frame delta, elapsed, FPS |
//! [`input`]     | `InputState` — keyboard, mouse, scroll; `just_pressed` / `just_released` |
//! [`scene`]     | `World`, `Element`, ECS systems (`TimeSystem`, `VelocitySystem`, `AnimationSystem`, `BehaviorSystem`, `TransformSystem`), hierarchy components (`Parent`, `Children`, `GlobalTransform`), `AnimationClip/Player`, `BehaviorComponent`, `Camera` |
//! [`context`]   | `EngineContext` — wgpu device + queue |
//! [`metrics`]   | CPU / RAM usage helpers |
//!
//! ## Quick start
//!
//! ```rust,ignore
//! use ferrous_core::{World, Handle, Transform, Color, Time};
//! use ferrous_core::input::{InputState, KeyCode};
//! use glam::Vec3;
//!
//! let mut world = World::new();
//! let cube = world.spawn("Player")
//!     .with_position(Vec3::new(0.0, 0.5, 0.0))
//!     .with_color(Color::CYAN)
//!     .build();
//! ```

// ─── Module declarations ───────────────────────────────────────────────────

/// World-space transform (position, rotation, scale).
pub mod transform;

/// RGBA colour type with a large palette of constants.
pub mod color;

/// Frame timing: delta, elapsed, FPS.
pub mod time;

/// Keyboard and mouse input state.
pub mod input;

/// wgpu device + queue container.
pub mod context;

/// Scene graph: `World`, `Element`, `Handle`, `ElementKind`, `Camera`, `Controller`.
///
/// **ECS systems** (register via `StagedScheduler`):
/// - `TimeSystem` (PreUpdate) — ticks `TimeClock` resource each frame.
/// - `VelocitySystem` (Update) — integrates `Velocity` into `Transform::position`.
/// - `AnimationSystem` (Update) — advances `AnimationPlayer` and applies keyframe positions.
/// - `BehaviorSystem` (Update) — calls per-entity `Behavior::update` hooks.
/// - `TransformSystem` (PostUpdate) — propagates `GlobalTransform` through the parent chain.
///
/// **New components**: `Velocity`, `Parent`, `Children`, `GlobalTransform`,
/// `AnimationClip`, `AnimationPlayer`, `BehaviorComponent`.
/// `BehaviorComponent` is non-Clone; spawn with `world.spawn_owned()`.
#[cfg(feature = "ecs")]
pub mod scene;

/// CPU / RAM usage helpers.
pub mod metrics;

/// Viewport rectangle (x, y, width, height) for 3-D rendering.
pub mod viewport;

/// Per-frame renderer statistics (vertices, triangles, draw calls).
pub mod render_stats;

// ─── Top-level re-exports ──────────────────────────────────────────────────
//
// These are the types a game author uses every day.  Import them from the
// crate root so you don't have to care about which sub-module they live in.

// Math
pub use glam;

// Core types
pub use color::Color;
pub use time::{Time, TimeClock};
pub use transform::Transform;

// Input
pub use input::{InputState, KeyCode, MouseButton};

// Scene (re-exported only when ECS support is enabled)
#[cfg(feature = "ecs")]
pub use scene::{AlphaMode, MaterialDescriptor, MaterialHandle, RenderQuality, RenderStyle};

#[cfg(feature = "ecs")]
pub use scene::{
    AnimationClip, AnimationPlayer, AnimationSystem, Behavior, BehaviorComponent, BehaviorSystem,
    Camera3D, Camera3DBuilder, Children, DirectionalLight, GlobalTransform, Keyframe, OrbitCamera,
    OrbitCameraSystem, Parent, Stage, TimeSystem, TransformSystem, Velocity, VelocitySystem,
};

#[cfg(feature = "ecs")]
pub use scene::{Camera, CameraUniform, Controller};

#[cfg(feature = "ecs")]
pub use scene::{Element, ElementKind, Handle, PointLightComponent, World};

#[cfg(feature = "ecs")]
pub use scene::{Material, MaterialBuilder};

// Context
pub use context::EngineContext;

// Metrics shortcuts (the most common two)
pub use metrics::{get_cpu_usage, get_ram_usage_mb};

// Renderer-agnostic display types
pub use render_stats::RenderStats;
pub use viewport::Viewport;
