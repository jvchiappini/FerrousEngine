use crate::{Widget, RenderCommand, DrawContext, LayoutContext, UpdateContext, EventContext, EventResponse, UiEvent, Vec2};
use crate::primitives::Rect;

pub struct InfiniteCanvas<App = ()> {
    pub pan: Vec2,
    pub zoom: f32,
    pub is_dragging: bool,
    _marker: std::marker::PhantomData<App>,
}

impl<App> InfiniteCanvas<App> {
    pub fn new() -> Self {
        Self {
            pan: Vec2::ZERO,
            zoom: 1.0,
            is_dragging: false,
            _marker: std::marker::PhantomData,
        }
    }
}

impl<App: Send + Sync> Widget<App> for InfiniteCanvas<App> {
    fn draw(&self, ctx: &mut DrawContext, _cmds: &mut Vec<RenderCommand>) {
        // Draw the background grid or pattern using procedurals or commands
    }

    fn update(&mut self, ctx: &mut UpdateContext) {
        // ...
    }

    fn calculate_size(&self, ctx: &mut LayoutContext) -> Vec2 {
        ctx.available_space
    }

    fn on_event(
        &mut self,
        ctx: &mut EventContext<App>,
        event: &UiEvent,
    ) -> EventResponse {
        match event {
            UiEvent::MouseDown { button: crate::MouseButton::Middle, .. } | 
            UiEvent::MouseDown { button: crate::MouseButton::Right, .. } => {
                self.is_dragging = true;
                EventResponse::Consumed
            }
            UiEvent::MouseUp { button: crate::MouseButton::Middle, .. } |
            UiEvent::MouseUp { button: crate::MouseButton::Right, .. } => {
                self.is_dragging = false;
                EventResponse::Consumed
            }
            UiEvent::MouseMove { pos } => {
                // We need previous pos to calculate delta.
                // In a real impl, EventManager passes delta, or we store last_pos.
                EventResponse::Ignored
            }
            UiEvent::MouseWheel { delta_y, .. } => {
                // Zooming
                self.zoom += delta_y * 0.1;
                self.zoom = self.zoom.clamp(0.1, 5.0);
                ctx.request_redraw();
                EventResponse::Redraw
            }
            _ => EventResponse::Ignored,
        }
    }
}
