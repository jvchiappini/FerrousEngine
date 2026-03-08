use crate::{Widget, RenderCommand, DrawContext, BuildContext, LayoutContext, Vec2};

// ─── ProgressBar ─────────────────────────────────────────────────────────────

/// Barra de progreso (Fase 6.3).
pub struct ProgressBar {
    /// Progreso actual [0.0 - 1.0]
    pub progress: f32,
}

impl ProgressBar {
    pub fn new(progress: f32) -> Self {
        Self { progress: progress.clamp(0.0, 1.0) }
    }
}

impl<App> Widget<App> for ProgressBar {
    fn draw(&self, ctx: &mut DrawContext, cmds: &mut Vec<RenderCommand>) {
        let theme = &ctx.theme;
        let r = &ctx.rect;
        
        // Fondo (Track)
        cmds.push(RenderCommand::Quad {
            rect: *r,
            color: theme.surface_variant.to_array(),
            radii: [theme.border_radius; 4],
            flags: 0,
        });

        // Llenado (Fill)
        let fill_width = r.width * self.progress;
        if fill_width > 0.1 {
            cmds.push(RenderCommand::Quad {
                rect: crate::Rect::new(r.x, r.y, fill_width, r.height),
                color: theme.primary.to_array(),
                radii: [theme.border_radius; 4],
                flags: 0,
            });
        }
    }

    fn calculate_size(&self, _ctx: &mut LayoutContext) -> Vec2 {
        glam::vec2(200.0, 8.0)
    }
}
