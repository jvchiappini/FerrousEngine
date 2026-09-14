//! 2D ECS components.
//!
//! These components describe 2D entities but are currently only consumed by
//! the (unregistered) [`crate::systems`] preparation functions. The engine's
//! live 2D path uses the immediate-mode renderer instead.

/// Position, scale, rotation and z-order for a 2D entity.
pub mod transform;
/// Sprite component: texture id, tint, size, flip flags and source rect.
pub mod sprite;
/// Orthographic 2D camera component.
pub mod camera;
/// Simple untextured shape component (rectangle/circle SDF parameters).
pub mod shape;
/// Polyline/path component with cap and join styles.
pub mod path;

pub use transform::*;
pub use sprite::*;
pub use camera::*;
pub use shape::*;
pub use path::*;

