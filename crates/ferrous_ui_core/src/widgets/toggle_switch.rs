use crate::{Widget, RenderCommand, DrawContext, BuildContext, LayoutContext, EventContext, EventResponse, UiEvent, Rect, Vec2};

// ─── ToggleSwitch ────────────────────────────────────────────────────────────

/// Interruptor de alternancia (Fase 6.1).
pub struct ToggleSwitch<App> {
    pub is_on: bool,
    pub binding: Option<std::sync::Arc<crate::Observable<bool>>>,
    on_change_cb: Option<Box<dyn Fn(&mut EventContext<App>, bool) + Send + Sync>>,
}

impl<App> ToggleSwitch<App> {
    pub fn new(is_on: bool) -> Self {
        Self {
            is_on,
            binding: None,
            on_change_cb: None,
        }
    }

    /// Vincula el switch a un `Observable<bool>`.
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
            self.is_on = new_val;
        }

        if let Some(cb) = &self.on_change_cb {
            cb(ctx, new_val);
        }
    }
}

impl<App> Widget<App> for ToggleSwitch<App> {
    fn draw(&self, ctx: &mut DrawContext, cmds: &mut Vec<RenderCommand>) {
        let theme = &ctx.theme;
        let r = &ctx.rect;
        let is_on = self.binding.as_ref().map(|o| o.get()).unwrap_or(self.is_on);
        
        let width = 40.0;
        let height = 20.0;
        let track_rect = Rect::new(r.x, r.y + (r.height - height) * 0.5, width, height);
        
        // Track
        let track_color = if is_on { theme.primary } else { theme.surface_elevated };
        cmds.push(RenderCommand::Quad {
            rect: track_rect,
            color: track_color.to_array(),
            radii: [height * 0.5; 4],
            flags: 0,
        });

        // Knob
        let knob_size = height - 4.0;
        let knob_x = if is_on {
            track_rect.x + width - knob_size - 2.0
        } else {
            track_rect.x + 2.0
        };
        
        cmds.push(RenderCommand::Quad {
            rect: Rect::new(knob_x, track_rect.y + 2.0, knob_size, knob_size),
            color: theme.on_primary.to_array(),
            radii: [knob_size * 0.5; 4],
            flags: 0,
        });
    }

    fn calculate_size(&self, _ctx: &mut LayoutContext) -> Vec2 {
        glam::vec2(40.0, 20.0)
    }

    fn on_event(
        &mut self,
        ctx: &mut EventContext<App>,
        event: &UiEvent,
    ) -> EventResponse {
        match event {
            UiEvent::MouseDown { .. } => {
                let current = self.binding.as_ref().map(|o| o.get()).unwrap_or(self.is_on);
                self.update_value(ctx, !current);
                EventResponse::Redraw
            }
            _ => EventResponse::Ignored,
        }
    }
}
