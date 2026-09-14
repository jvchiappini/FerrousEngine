use glam::Vec4;
use ferrous_ecs::component::Component;

/// A 2D Orthographic camera component.
#[derive(Debug, Clone)]
pub struct Camera2d {
    /// Zoom factor; larger values zoom in.
    pub zoom: f32,
    /// Optional background clear colour; `None` keeps the renderer's clear colour.
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
