use glam::{Vec2, Vec4};
use ferrous_ecs::component::Component;

/// A Sprite component to bind textures, colors, and region data to a 2D entity.
#[derive(Debug, Clone, PartialEq)]
pub struct Sprite {
    pub color: Vec4,
    pub custom_size: Option<Vec2>,
    pub flip_x: bool,
    pub flip_y: bool,
    pub texture_id: Option<u32>, // ID reference to a texture asset
    pub rect: Option<[f32; 4]>,  // [x, y, width, height] for spritesheets
}
impl Component for Sprite {}

impl Default for Sprite {
    fn default() -> Self {
        Self {
            color: Vec4::ONE,
            custom_size: None,
            flip_x: false,
            flip_y: false,
            texture_id: None,
            rect: None,
        }
    }
}
