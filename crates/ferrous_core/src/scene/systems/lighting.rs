//! Directional and point light ECS components.

// MaterialComponent is re-exported through scene::world; this import
// existed previously but is no longer used in this file.
#[cfg(feature = "ecs")]
// use crate::scene::world::types::MaterialComponent;

use crate::color::Color;
use ferrous_ecs::prelude::Component;

// ────────────────────────────────────────────────────────────────────────────
// DirectionalLight

/// A scene-wide directional light (sun / moon).
///
/// Spawn exactly one entity with this component; the renderer's `sync_world`
/// picks it up automatically via an ECS query.
///
/// ```rust,ignore
/// world.ecs.spawn((
///     DirectionalLight {
///         direction: Vec3::new(-0.6, -0.8, -0.4).normalize(),
///         color:     Color::WARM_WHITE,
///         intensity: 3.5,
///     },
/// ));
/// ```
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DirectionalLight {
    /// World-space direction the light is travelling **towards**.  Normalised.
    pub direction: glam::Vec3,
    /// Linear-space colour.
    pub color: Color,
    /// Intensity multiplier.
    pub intensity: f32,
}
impl Component for DirectionalLight {}

impl Default for DirectionalLight {
    fn default() -> Self {
        Self {
            direction: glam::Vec3::new(0.0, -1.0, 0.0),
            color: Color::WHITE,
            intensity: 1.0,
        }
    }
}
