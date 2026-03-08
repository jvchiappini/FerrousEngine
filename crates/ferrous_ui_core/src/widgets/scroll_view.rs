use crate::{Widget, RenderCommand, DrawContext, LayoutContext, EventContext, EventResponse, UiEvent, Rect, NodeId, Observable, Vec2};

// ─── ScrollView ───────────────────────────────────────────────────────────────

/// Contenedor con soporte para desplazamiento (scroll).
///
/// Permite que un subárbol de widgets exceda las dimensiones del contenedor,
/// proporcionando desplazamiento vertical y horizontal.
pub struct ScrollView {
    pub scroll_offset: Vec2,
    pub wheel_speed: f32,
    pub is_hovered: bool,
}

impl ScrollView {
    pub fn new() -> Self {
        Self {
            scroll_offset: Vec2::ZERO,
            wheel_speed: 20.0,
            is_hovered: false,
        }
    }

    pub fn with_wheel_speed(mut self, speed: f32) -> Self {
        self.wheel_speed = speed;
        self
    }
}

impl Default for ScrollView {
    fn default() -> Self {
        Self::new()
    }
}

impl Widget for ScrollView {
    fn draw(&self, _ctx: &mut DrawContext, _cmds: &mut Vec<RenderCommand>) {
        // ScrollView no pinta nada por sí mismo por defecto,
        // pero actúa como ancla para el recorte y desplazamiento de hijos.
        // Los hijos son dibujados por el UiTree.
    }

    fn calculate_size(&self, _ctx: &mut LayoutContext) -> Vec2 {
        // Por defecto intenta llenar el espacio, pero reportamos algo mínimo
        Vec2::ZERO
    }

    fn on_event(
        &mut self,
        _ctx: &mut EventContext,
        event: &UiEvent,
    ) -> EventResponse {
        match event {
            UiEvent::MouseEnter => {
                self.is_hovered = true;
                EventResponse::Ignored
            }
            UiEvent::MouseLeave => {
                self.is_hovered = false;
                EventResponse::Ignored
            }
            UiEvent::MouseWheel { delta_x, delta_y } if self.is_hovered => {
                self.scroll_offset.x += delta_x * self.wheel_speed;
                self.scroll_offset.y += delta_y * self.wheel_speed;
                
                // Aseguramos que no se salga de límites negativos 
                // (el límite superior depende del tamaño del contenido, 
                // que aún no conocemos fácilmente aquí sin consultar el tree).
                self.scroll_offset.x = self.scroll_offset.x.max(0.0);
                self.scroll_offset.y = self.scroll_offset.y.max(0.0);
                
                EventResponse::Redraw
            }
            _ => EventResponse::Ignored,
        }
    }

    fn scroll_offset(&self) -> Vec2 {
        self.scroll_offset
    }
}
