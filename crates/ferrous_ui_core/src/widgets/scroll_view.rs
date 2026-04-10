use crate::{Widget, RenderCommand, DrawContext, LayoutContext, UpdateContext, EventContext, EventResponse, UiEvent, Vec2};

// ─── ScrollView ─────────────────────────────────────────────────────

/// Container with scrolling support.
///
/// Allows a subtree of widgets to exceed the dimensions of the container,
/// providing vertical and horizontal scrolling.
pub struct ScrollView<App = ()> {
    pub scroll_offset: Vec2,
    pub velocity: Vec2,
    pub friction: f32,
    pub wheel_speed: f32,
    pub is_hovered: bool,
    _marker: std::marker::PhantomData<App>,

    on_scroll_cb: Option<Box<dyn Fn(&mut EventContext<App>, Vec2) + Send + Sync + 'static>>,
}

impl<App> ScrollView<App> {
    pub fn new() -> Self {
        Self {
            scroll_offset: Vec2::ZERO,
            velocity: Vec2::ZERO,
            friction: 0.92, // 92% of velocity remains per frame (approx)
            wheel_speed: 20.0,
            is_hovered: false,
            _marker: std::marker::PhantomData,
            on_scroll_cb: None,
        }
    }

    pub fn with_wheel_speed(mut self, speed: f32) -> Self {
        self.wheel_speed = speed;
        self
    }

    pub fn with_friction(mut self, friction: f32) -> Self {
        self.friction = friction.clamp(0.0, 1.0);
        self
    }

    /// Registers a function to be called when the user scrolls.
    pub fn on_scroll(mut self, f: impl Fn(&mut EventContext<App>, Vec2) + Send + Sync + 'static) -> Self {
        self.on_scroll_cb = Some(Box::new(f));
        self
    }
}

impl<App> Default for ScrollView<App> {
    fn default() -> Self {
        Self::new()
    }
}

impl<App: Send + Sync> Widget<App> for ScrollView<App> {
    fn draw(&self, _ctx: &mut DrawContext, _cmds: &mut Vec<RenderCommand>) {
        // ScrollView doesn't draw anything by itself by default,
        // but acts as an anchor for clipping and scrolling children.
    }

    fn update(&mut self, ctx: &mut UpdateContext) {
        // Apply inertia
        if self.velocity.length_squared() > 0.01 {
            self.scroll_offset.x += self.velocity.x * ctx.delta_time * 60.0;
            self.scroll_offset.y += self.velocity.y * ctx.delta_time * 60.0;
            
            // Decelerate
            self.velocity.x *= self.friction;
            self.velocity.y *= self.friction;
            
            // Bounds check
            let max_scroll_x = (ctx.content_size.x - ctx.rect.width).max(0.0);
            let max_scroll_y = (ctx.content_size.y - ctx.rect.height).max(0.0);

            if self.scroll_offset.x < 0.0 {
                self.scroll_offset.x = 0.0;
                self.velocity.x = 0.0;
            } else if self.scroll_offset.x > max_scroll_x {
                self.scroll_offset.x = max_scroll_x;
                self.velocity.x = 0.0;
            }

            if self.scroll_offset.y < 0.0 {
                self.scroll_offset.y = 0.0;
                self.velocity.y = 0.0;
            } else if self.scroll_offset.y > max_scroll_y {
                self.scroll_offset.y = max_scroll_y;
                self.velocity.y = 0.0;
            }
            
            // Request redraw while moving
            ctx.needs_redraw = true;
        } else {
            self.velocity = Vec2::ZERO;
            
            // Still ensure bounds are correct if content size changed without velocity
            let max_scroll_x = (ctx.content_size.x - ctx.rect.width).max(0.0);
            let max_scroll_y = (ctx.content_size.y - ctx.rect.height).max(0.0);
            self.scroll_offset.x = self.scroll_offset.x.clamp(0.0, max_scroll_x);
            self.scroll_offset.y = self.scroll_offset.y.clamp(0.0, max_scroll_y);
        }
    }


    fn calculate_size(&self, _ctx: &mut LayoutContext) -> Vec2 {
        // By default tries to fill space, report zero to avoid influencing parent too much
        Vec2::ZERO
    }

    fn on_event(
        &mut self,
        ctx: &mut EventContext<App>,
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
                // Add to velocity instead of position for inertia
                self.velocity.x += delta_x * self.wheel_speed;
                self.velocity.y += delta_y * self.wheel_speed;

                // Notify user immediately of the start of scroll
                if let Some(cb) = &self.on_scroll_cb {
                    cb(ctx, self.scroll_offset);
                }

                EventResponse::Redraw
            }
            _ => EventResponse::Ignored,
        }
    }

    fn scroll_offset(&self) -> Vec2 {
        self.scroll_offset
    }
}

