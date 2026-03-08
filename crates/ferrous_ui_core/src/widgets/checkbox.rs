use crate::{Widget, RenderCommand, DrawContext, BuildContext, LayoutContext, EventContext, EventResponse, UiEvent, Rect, Vec2};

// ─── Checkbox ────────────────────────────────────────────────────────────────

/// Checkbox interactivo (Fase 6.1).
pub struct Checkbox<App> {
    pub checked: bool,
    pub label: String,
    on_change_cb: Option<Box<dyn Fn(&mut EventContext<App>, bool) + Send + Sync>>,
}

impl<App> Checkbox<App> {
    pub fn new(label: impl Into<String>, checked: bool) -> Self {
        Self {
            label: label.into(),
            checked,
            on_change_cb: None,
        }
    }

    pub fn on_change(mut self, f: impl Fn(&mut EventContext<App>, bool) + Send + Sync + 'static) -> Self {
        self.on_change_cb = Some(Box::new(f));
        self
    }
}

impl<App> Widget<App> for Checkbox<App> {
    fn draw(&self, ctx: &mut DrawContext, cmds: &mut Vec<RenderCommand>) {
        let theme = &ctx.theme;
        let r = &ctx.rect;
        
        // Caja del checkbox
        let size = 18.0;
        let box_rect = Rect::new(r.x, r.y + (r.height - size) * 0.5, size, size);
        
        let bg_color = if self.checked { theme.primary } else { theme.surface_variant };
        
        cmds.push(RenderCommand::Quad {
            rect: box_rect,
            color: bg_color.to_array(),
            radii: [theme.border_radius * 0.5; 4],
            flags: 0,
        });

        // Marca de verificación (simplificada como un cuadrado interno)
        if self.checked {
            let inset = 4.0;
            cmds.push(RenderCommand::Quad {
                rect: Rect::new(box_rect.x + inset, box_rect.y + inset, size - inset*2.0, size - inset*2.0),
                color: theme.on_primary.to_array(),
                radii: [1.0; 4],
                flags: 0,
            });
        }

        // Etiqueta
        cmds.push(RenderCommand::Text {
            rect: Rect::new(r.x + size + 8.0, r.y, r.width - size - 8.0, r.height),
            text: self.label.clone(),
            color: theme.on_surface.to_array(),
            font_size: theme.font_size_base,
        });
    }

    fn calculate_size(&self, _ctx: &mut LayoutContext) -> Vec2 {
        let w = 18.0 + 8.0 + self.label.len() as f32 * 8.0;
        glam::vec2(w, 24.0)
    }

    fn on_event(
        &mut self,
        ctx: &mut EventContext<App>,
        event: &UiEvent,
    ) -> EventResponse {
        match event {
            UiEvent::MouseDown { .. } => {
                self.checked = !self.checked;
                if let Some(cb) = &self.on_change_cb {
                    cb(ctx, self.checked);
                }
                EventResponse::Redraw
            }
            _ => EventResponse::Ignored,
        }
    }
}
