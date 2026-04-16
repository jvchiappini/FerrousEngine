use glam::{Vec2, Vec4};

/// Component for rendering untextured 2D shapes (rects, lines).
#[derive(Debug, Clone, Copy)]
pub struct Shape2d {
    pub size: Vec2,
    pub color: [f32; 4],
    pub border_thickness: f32,

    pub corner_radius: f32,
    pub smoothing: f32,
    pub is_filled: bool,
}

impl Default for Shape2d {
    fn default() -> Self {
        Self {
            size: Vec2::ONE,
            color: [1.0, 1.0, 1.0, 1.0],
            border_thickness: 0.0,

            corner_radius: 0.0,
            // fwidth multiplier for pixel-perfect anti-aliased edge. ~1.5 covers physical distance cleanly.
            smoothing: 1.5,
            is_filled: true,
        }
    }
}

impl Shape2d {
    pub fn rect(size: Vec2, color: Vec4) -> Self {
        Self {
            size,
            color: color.into(),
            ..Default::default()
        }
    }

    pub fn circle(radius: f32, color: Vec4) -> Self {
        Self {
            size: Vec2::new(radius * 2.0, radius * 2.0),
            color: color.into(),
            corner_radius: radius,
            ..Default::default()
        }
    }
}


impl ferrous_ecs::component::Component for Shape2d {}

