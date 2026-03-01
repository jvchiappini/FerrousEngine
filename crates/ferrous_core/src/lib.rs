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
//! [`scene`]     | `World`, `Element`, `Handle`, `ElementKind`, `Camera`, `Controller` |
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

/// Scene graph: World, Element, Handle, ElementKind, Camera, Controller.
/// Shape kinds (Cube, Sphere, Mesh, etc.) are defined as `ElementKind` variants.
pub mod scene;

/// CPU / RAM usage helpers.
pub mod metrics;

// ─── Top-level re-exports ──────────────────────────────────────────────────
//
// These are the types a game author uses every day.  Import them from the
// crate root so you don't have to care about which sub-module they live in.

// Math
pub use glam;

// Core types
pub use transform::Transform;
pub use color::Color;
pub use time::{Time, TimeClock};

// Input
pub use input::{InputState, KeyCode, MouseButton};

// Scene
pub use scene::{World, Element, Handle, ElementKind};
pub use scene::{Camera, CameraUniform, Controller};

// Context
pub use context::EngineContext;

// Metrics shortcuts (the most common two)
pub use metrics::{get_cpu_usage, get_ram_usage_mb};
