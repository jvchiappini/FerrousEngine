use crate::{Widget, RenderCommand, DrawContext, BuildContext, LayoutContext, EventContext, EventResponse, UiEvent, Rect, Vec2};

// ─── Slider ──────────────────────────────────────────────────────────────────

/// Slider interactivo para valores numéricos con soporte de reactividad y el nuevo sistema de eventos.
pub struct Slider<App> {
    pub value: f32,
    pub min: f32,
    pub max: f32,
    pub is_dragging: bool,
    pub binding: Option<std::sync::Arc<crate::Observable<f32>>>,
    on_change_cb: Option<Box<dyn Fn(&mut EventContext<App>, f32) + Send + Sync>>,
}

impl<App> Slider<App> {
    pub fn new(value: f32, min: f32, max: f32) -> Self {
        Self {
            value,
            min,
            max,
            is_dragging: false,
            binding: None,
            on_change_cb: None,
        }
    }

    /// Vincula el slider a un `Observable<f32>`.
    pub fn with_binding(
        mut self,
        observable: std::sync::Arc<crate::Observable<f32>>,
        node_id: crate::NodeId,
    ) -> Self {
        observable.subscribe(node_id);
        self.binding = Some(observable);
        self
    }

    /// Registra un callback invocado cada vez que el valor cambia, con acceso al contexto.
    pub fn on_change(mut self, f: impl Fn(&mut EventContext<App>, f32) + Send + Sync + 'static) -> Self {
        self.on_change_cb = Some(Box::new(f));
        self
    }

    fn update_value(&mut self, ctx: &mut EventContext<App>, new_val: f32) {
        let clamped = new_val.clamp(self.min, self.max);
        
        if let Some(o) = &self.binding {
            let dirty = o.set(clamped);
            ctx.tree.reactivity.notify_change(dirty);
        } else {
            self.value = clamped;
        }

        if let Some(cb) = &self.on_change_cb { 
            cb(ctx, clamped); 
        }
    }
}

impl<App> Widget<App> for Slider<App> {
    fn draw(&self, ctx: &mut DrawContext, cmds: &mut Vec<RenderCommand>) {
        let val = self.binding.as_ref().map(|o| o.get()).unwrap_or(self.value);
        let n = ((val - self.min) / (self.max - self.min)).clamp(0.0, 1.0);
        let r = &ctx.rect;
        let theme = &ctx.theme;

        // Track
        cmds.push(RenderCommand::Quad {
            rect: Rect::new(r.x, r.y + r.height * 0.45, r.width, r.height * 0.1),
            color: theme.on_surface_muted.with_alpha(0.2).to_array(),
            radii: [2.0; 4],
            flags: 0,
        });

        // Fill
        cmds.push(RenderCommand::Quad {
            rect: Rect::new(r.x, r.y + r.height * 0.45, r.width * n, r.height * 0.1),
            color: theme.primary.to_array(),
            radii: [2.0; 4],
            flags: 0,
        });

        // Knob
        let ks = r.height * 0.6;
        cmds.push(RenderCommand::Quad {
            rect: Rect::new(r.x + r.width * n - ks * 0.5, r.y + r.height * 0.2, ks, ks),
            color: theme.on_primary.to_array(),
            radii: [ks * 0.5; 4],
            flags: 0,
        });
    }

    fn calculate_size(&self, _ctx: &mut LayoutContext) -> Vec2 {
        glam::vec2(150.0, 25.0)
    }

    fn on_event(
        &mut self,
        ctx: &mut EventContext<App>,
        event: &UiEvent,
    ) -> EventResponse {
        match event {
            UiEvent::MouseDown { pos, .. } => {
                self.is_dragging = true;
                let n = (pos.x - ctx.rect.x) / ctx.rect.width;
                let val = self.min + n.clamp(0.0, 1.0) * (self.max - self.min);
                self.update_value(ctx, val);
                EventResponse::Redraw
            }
            UiEvent::MouseUp { .. } => {
                self.is_dragging = false;
                EventResponse::Consumed
            }
            UiEvent::MouseMove { pos } if self.is_dragging => {
                let n = (pos.x - ctx.rect.x) / ctx.rect.width;
                let val = self.min + n.clamp(0.0, 1.0) * (self.max - self.min);
                self.update_value(ctx, val);
                EventResponse::Redraw
            }
            _ => EventResponse::Ignored,
        }
    }
}
