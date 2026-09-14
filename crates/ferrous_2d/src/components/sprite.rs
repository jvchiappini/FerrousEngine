use glam::{Vec2, Vec4};
use ferrous_ecs::component::Component;

/// A Sprite component to bind textures, colors, and region data to a 2D entity.
#[derive(Debug, Clone, PartialEq)]
pub struct Sprite {
    /// Multiplicative tint applied to the sampled texel.
    pub color: Vec4,
    /// Explicit sprite size in world units; `None` falls back to `100×100`.
    pub custom_size: Option<Vec2>,
    /// Mirror the sprite horizontally.
    pub flip_x: bool,
    /// Mirror the sprite vertically.
    pub flip_y: bool,
    /// Renderer texture id from `Renderer::register_sprite_texture`; `None` uses id 0.
    pub texture_id: Option<u32>, // ID reference to a texture asset
    /// Source sub-rectangle `[x, y, width, height]` for spritesheets; `None` is the full texture.
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
