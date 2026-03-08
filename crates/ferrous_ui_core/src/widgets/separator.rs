use crate::{Widget, RenderCommand, DrawContext, LayoutContext, Color, Vec2, Rect};

// ─── Separator ───────────────────────────────────────────────────────────────

/// Línea divisoria tenue para separar secciones.
pub struct Separator {
    pub color: Option<Color>,
}

impl Separator {
    pub fn new() -> Self {
        Self { color: None }
    }

    pub fn with_color(mut self, color: Color) -> Self {
        self.color = Some(color);
        self
    }
}

impl<App> Widget<App> for Separator {
    fn draw(&self, ctx: &mut DrawContext, cmds: &mut Vec<RenderCommand>) {
        let theme = &ctx.theme;
        let color = self.color.unwrap_or(theme.on_surface_muted.with_alpha(0.1));
        
        // Línea central de 1px
        cmds.push(RenderCommand::Quad {
            rect: Rect::new(ctx.rect.x, ctx.rect.y + ctx.rect.height * 0.5, ctx.rect.width, 1.0),
            color: color.to_array(),
            radii: [0.0; 4],
            flags: 0,
        });
    }

    fn calculate_size(&self, _ctx: &mut LayoutContext) -> Vec2 {
        glam::vec2(0.0, 1.0) // Altura mínima de 1px, ancho flexible
    }
}
