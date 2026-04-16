use glam::Vec4;
use ferrous_ecs::component::Component;

/// A 2D Orthographic camera component.
#[derive(Debug, Clone)]
pub struct Camera2d {
    pub zoom: f32,
    pub clear_color: Option<Vec4>,
}
impl Component for Camera2d {}

impl Default for Camera2d {
    fn default() -> Self {
        Self {
            zoom: 1.0,
            clear_color: Some(Vec4::new(0.05, 0.05, 0.05, 1.0)),
        }
    }
}
