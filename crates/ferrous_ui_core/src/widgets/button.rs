use crate::{Widget, RenderCommand, DrawContext, BuildContext, LayoutContext, EventContext, EventResponse, UiEvent, Vec2};

// ─── Button ──────────────────────────────────────────────────────────────────

/// Botón interactivo con callbacks enriquecidos que acceden al estado de la aplicación.
pub struct Button<App> {
    pub label: String,
    pub is_hovered: bool,
    on_click_cb: Option<Box<dyn Fn(&mut EventContext<App>) + Send + Sync>>,
    on_hover_cb: Option<Box<dyn Fn(&mut EventContext<App>) + Send + Sync>>,
    on_hover_end_cb: Option<Box<dyn Fn(&mut EventContext<App>) + Send + Sync>>,
}

impl<App> Button<App> {
    /// Crea un botón con el texto `label`.
    pub fn new(label: impl Into<String>) -> Self {
        Self {
            label: label.into(),
            is_hovered: false,
            on_click_cb: None,
            on_hover_cb: None,
            on_hover_end_cb: None,
        }
    }

    /// Registra un callback que se invoca al hacer clic.
    /// Recibe el `EventContext`, permitiendo mutar el estado de la aplicación o el árbol.
    pub fn on_click(mut self, f: impl Fn(&mut EventContext<App>) + Send + Sync + 'static) -> Self {
        self.on_click_cb = Some(Box::new(f));
        self
    }

    /// Registra un callback que se invoca cuando el puntero entra en el botón.
    pub fn on_hover(mut self, f: impl Fn(&mut EventContext<App>) + Send + Sync + 'static) -> Self {
        self.on_hover_cb = Some(Box::new(f));
        self
    }

    /// Registra un callback que se invoca cuando el puntero sale del botón.
    pub fn on_hover_end(mut self, f: impl Fn(&mut EventContext<App>) + Send + Sync + 'static) -> Self {
        self.on_hover_end_cb = Some(Box::new(f));
        self
    }
}

impl<App> Widget<App> for Button<App> {
    fn build(&mut self, _ctx: &mut BuildContext<App>) {}

    fn draw(&self, ctx: &mut DrawContext, cmds: &mut Vec<RenderCommand>) {
        let bg = if self.is_hovered { ctx.theme.primary_variant } else { ctx.theme.primary };

        // Fondo
        cmds.push(RenderCommand::Quad {
            rect: ctx.rect,
            color: bg.to_array(),
            radii: [ctx.theme.border_radius; 4],
            flags: 0,
        });

        // Texto
        cmds.push(RenderCommand::Text {
            rect: ctx.rect,
            text: self.label.clone(),
            color: ctx.theme.on_primary.to_array(),
            font_size: ctx.theme.font_size_base,
        });
    }

    fn calculate_size(&self, _ctx: &mut LayoutContext) -> Vec2 {
        let w = self.label.len() as f32 * 10.0 + 30.0;
        glam::vec2(w, 36.0)
    }

    fn on_event(
        &mut self,
        ctx: &mut EventContext<App>,
        event: &UiEvent,
    ) -> EventResponse {
        match event {
            UiEvent::MouseEnter => {
                self.is_hovered = true;
                if let Some(cb) = &self.on_hover_cb { cb(ctx); }
                EventResponse::Redraw
            }
            UiEvent::MouseLeave => {
                self.is_hovered = false;
                if let Some(cb) = &self.on_hover_end_cb { cb(ctx); }
                EventResponse::Redraw
            }
            UiEvent::MouseDown { .. } => {
                if let Some(cb) = &self.on_click_cb { 
                    cb(ctx); 
                }
                EventResponse::Consumed
            }
            _ => EventResponse::Ignored,
        }
    }
}
