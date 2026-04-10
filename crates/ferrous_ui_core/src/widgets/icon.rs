use crate::{Widget, RenderCommand, DrawContext, Vec2};

/// A widget that renders an MSDF icon from the theme's icon atlas.
pub struct Icon {
    pub name: String,
    pub color: [f32; 4],
    pub size: f32,
}

impl Icon {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            color: [1.0, 1.0, 1.0, 1.0],
            size: 24.0,
        }
    }

    pub fn with_color(mut self, color: [f32; 4]) -> Self {
        self.color = color;
        self
    }

    pub fn with_size(mut self, size: f32) -> Self {
        self.size = size;
        self
    }
}

impl<App> Widget<App> for Icon {
    fn draw(&self, ctx: &mut DrawContext, cmds: &mut Vec<RenderCommand>) {
        cmds.push(RenderCommand::Icon {
            name: self.name.clone(),
            rect: ctx.rect,
            color: self.color,
        });
    }

    fn calculate_size(&self, _ctx: &mut crate::LayoutContext) -> Vec2 {
        Vec2::new(self.size, self.size)
    }
}
