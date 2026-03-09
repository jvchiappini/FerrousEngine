use crate::{Widget, RenderCommand, DrawContext, BuildContext, LayoutContext, Vec2};

// ─── ProgressBar ─────────────────────────────────────────────────────────────

/// Barra de progreso (Fase 6.3).
pub struct ProgressBar {
    /// Progreso actual [0.0 - 1.0] (fallback si no hay binding)
    pub progress: f32,
    pub binding: Option<std::sync::Arc<crate::Observable<f32>>>,
}

impl ProgressBar {
    /// Crea una nueva barra de progreso con el valor inicial dado.
    pub fn new(progress: f32) -> Self {
        Self { 
            progress: progress.clamp(0.0, 1.0),
            binding: None,
        }
    }

    /// Vincula la barra a un `Observable<f32>`.
    pub fn with_binding(mut self, observable: std::sync::Arc<crate::Observable<f32>>, node_id: crate::NodeId) -> Self {
        observable.subscribe(node_id);
        self.binding = Some(observable);
        self
    }
}

impl<App> Widget<App> for ProgressBar {
    fn draw(&self, ctx: &mut DrawContext, cmds: &mut Vec<RenderCommand>) {
        let theme = &ctx.theme;
        let r = &ctx.rect;
        let progress = self.binding.as_ref().map(|o| o.get()).unwrap_or(self.progress).clamp(0.0, 1.0);
        
        // Fondo (Track)
        cmds.push(RenderCommand::Quad {
            rect: *r,
            color: theme.surface_elevated.to_array(),
            radii: [theme.border_radius; 4],
            flags: 0,
        });

        // Llenado (Fill)
        let fill_width = r.width * progress;
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
