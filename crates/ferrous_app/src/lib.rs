//! Framework modular para crear aplicaciones y juegos con FerrousEngine.
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

pub mod builder;
pub mod context;
mod graphics;
mod runner;
pub mod traits;

pub use builder::{App, AppConfig};
pub use context::AppContext;
pub use traits::FerrousApp;

// ── Re-export the most-used ferrous_core primitives ────────────────────────
// Users can do `use ferrous_app::{Color, Time, World, Handle, Vec3};` without
// adding ferrous_core as a direct dependency.
pub use ferrous_core::{
    Color,
    Handle,
    InputState,
    KeyCode,
    MouseButton,
    Time,
    TimeClock,
    Transform,
    World,
};

// glam math types — re-exported for convenience
pub use ferrous_core::glam::{Mat4, Quat, Vec2, Vec3, Vec4};
