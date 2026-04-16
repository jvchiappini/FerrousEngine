use glam::Vec2;
use ferrous_ecs::component::Component;

/// Core 2D transform component.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Transform2d {
    pub position: Vec2,
    pub scale: Vec2,
    pub rotation: f32, // In radians
    pub z_index: f32,  // Custom depth for Z-sorting
}
impl Component for Transform2d {}

impl Default for Transform2d {
    fn default() -> Self {
        Self {
            position: Vec2::ZERO,
            scale: Vec2::ONE,
            rotation: 0.0,
            z_index: 0.0,
        }
    }
}
