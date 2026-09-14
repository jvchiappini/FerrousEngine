use glam::{Vec2, Vec4};

/// Component for rendering untextured 2D shapes (rects, lines).
#[derive(Debug, Clone, Copy)]
pub struct Shape2d {
    /// Size of the shape in world units.
    pub size: Vec2,
    /// Fill or border colour (straight RGBA, no colour-space conversion applied).
    pub color: [f32; 4],
    /// Border thickness for outlined shapes; `0.0` means solid fill.
    pub border_thickness: f32,

    /// Corner radius of the SDF rounded-rectangle; set to half the smaller
    /// dimension for a circle.
    pub corner_radius: f32,
    /// Anti-aliasing smoothing range in pixels (`2.0` is the default).
    pub smoothing: f32,
    /// `true` to fill the shape, `false` to draw only the border.
    pub is_filled: bool,
}

impl Default for Shape2d {
    fn default() -> Self {
        Self {
            size: Vec2::ONE,
            color: [1.0, 1.0, 1.0, 1.0],
            border_thickness: 0.0,

            corner_radius: 0.0,
            // fwidth multiplier for pixel-perfect anti-aliased edge.
            // 2.0 provides a slightly softer transition that works better on high-DPI.
            smoothing: 2.0,
            is_filled: true,
        }
    }
}

impl Shape2d {
    /// Filled axis-aligned rectangle of `size` with the given `color`.
    pub fn rect(size: Vec2, color: Vec4) -> Self {
        Self {
            size,
            color: color.into(),
            ..Default::default()
        }
    }

    /// Filled circle of `radius` with the given `color`.
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

