use crate::{Widget, RenderCommand, DrawContext, BuildContext, LayoutContext, EventContext, EventResponse, UiEvent, Rect, Vec2};

// ─── Checkbox ────────────────────────────────────────────────────────────────

/// Checkbox interactivo (Fase 6.1).
pub struct Checkbox<App> {
    pub checked: bool,
    pub label: String,
    pub binding: Option<std::sync::Arc<crate::Observable<bool>>>,
    on_change_cb: Option<Box<dyn Fn(&mut EventContext<App>, bool) + Send + Sync>>,
}

impl<App> Checkbox<App> {
    pub fn new(label: impl Into<String>, checked: bool) -> Self {
        Self {
            label: label.into(),
            checked,
            binding: None,
            on_change_cb: None,
        }
    }

    /// Vincula el checkbox a un `Observable<bool>`.
    pub fn with_binding(mut self, observable: std::sync::Arc<crate::Observable<bool>>, node_id: crate::NodeId) -> Self {
        observable.subscribe(node_id);
        self.binding = Some(observable);
        self
    }

    pub fn on_change(mut self, f: impl Fn(&mut EventContext<App>, bool) + Send + Sync + 'static) -> Self {
        self.on_change_cb = Some(Box::new(f));
        self
    }

    fn update_value(&mut self, ctx: &mut EventContext<App>, new_val: bool) {
        if let Some(o) = &self.binding {
            let dirty = o.set(new_val);
            ctx.tree.reactivity.notify_change(dirty);
        } else {
            self.checked = new_val;
        }

        if let Some(cb) = &self.on_change_cb {
            cb(ctx, new_val);
        }
    }
}

impl<App> Widget<App> for Checkbox<App> {
    fn draw(&self, ctx: &mut DrawContext, cmds: &mut Vec<RenderCommand>) {
        let theme = &ctx.theme;
        let r = &ctx.rect;
        let checked = self.binding.as_ref().map(|o| o.get()).unwrap_or(self.checked);
        
        // Caja del checkbox
        let size = 18.0;
        let box_rect = Rect::new(r.x, r.y + (r.height - size) * 0.5, size, size);
        
        let bg_color = if checked { theme.primary } else { theme.surface_elevated };
        
        cmds.push(RenderCommand::Quad {
            rect: box_rect,
            color: bg_color.to_array(),
            radii: [theme.border_radius * 0.5; 4],
            flags: 0,
        });

        // Marca de verificación
        if checked {
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
            align: crate::TextAlign::TOP_LEFT,
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
                let current = self.binding.as_ref().map(|o| o.get()).unwrap_or(self.checked);
                self.update_value(ctx, !current);
                EventResponse::Redraw
            }
            _ => EventResponse::Ignored,
        }
    }
}
