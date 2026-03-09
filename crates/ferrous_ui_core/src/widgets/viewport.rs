use crate::{Widget, RenderCommand, DrawContext, BuildContext, LayoutContext, EventContext, EventResponse, UiEvent, Rect, Vec2};

/// `ViewportWidget` reserva un espacio en el layout para que el motor 3D 
/// pueda renderizar la escena en esa zona específica.
pub struct ViewportWidget {
    pub resolved_rect: Rect,
}

impl ViewportWidget {
    pub fn new() -> Self {
        Self {
            resolved_rect: Rect::default(),
        }
    }
}

impl<App> Widget<App> for ViewportWidget {
    fn draw(&self, ctx: &mut DrawContext, _cmds: &mut Vec<RenderCommand>) {
        // El viewport no dibuja nada por sí mismo en la UI, 
        // solo sirve como marcador de posición.
    }

    fn calculate_size(&self, _ctx: &mut LayoutContext) -> Vec2 {
        // Por defecto intenta ocupar todo el espacio disponible (expandirse)
        glam::vec2(0.0, 0.0)
    }

    fn update(&mut self, ctx: &mut crate::UpdateContext) {
        self.resolved_rect = ctx.rect;
    }

    fn on_event(&mut self, _ctx: &mut EventContext<App>, _event: &UiEvent) -> EventResponse {
        EventResponse::Ignored
    }
}
