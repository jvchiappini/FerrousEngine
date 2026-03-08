use crate::{Widget, RenderCommand, DrawContext, BuildContext, LayoutContext, Color, Vec2};

// ─── Panel ───────────────────────────────────────────────────────────────────

/// Contenedor básico con fondo y bordes (Phase 1.0).
pub struct Panel {
    /// Color opcional. Si es None, usa el color de superficie del tema.
    pub color: Option<Color>,
    /// Radio de borde opcional. Si es None, usa el del tema.
    pub radius: Option<f32>,
}

impl Panel {
    pub fn new() -> Self {
        Self { color: None, radius: None }
    }

    pub fn with_color(mut self, color: Color) -> Self {
        self.color = Some(color);
        self
    }

    pub fn with_radius(mut self, radius: f32) -> Self {
        self.radius = Some(radius);
        self
    }
}

impl<App> Widget<App> for Panel {
    fn draw(&self, ctx: &mut DrawContext, cmds: &mut Vec<RenderCommand>) {
        let color = self.color.unwrap_or(ctx.theme.surface);
        let radius = self.radius.unwrap_or(ctx.theme.border_radius);

        cmds.push(RenderCommand::Quad {
            rect: ctx.rect,
            color: color.to_array(),
            radii: [radius; 4],
            flags: 0,
        });
    }
}


