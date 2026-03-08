use crate::{Widget, RenderCommand, DrawContext, BuildContext, LayoutContext, EventContext, EventResponse, UiEvent, Vec2, TextInput};

// ─── NumberInput ─────────────────────────────────────────────────────────────

/// Input especializado en números con validación.
pub struct NumberInput<App> {
    pub inner: TextInput<App>,
}

impl<App> NumberInput<App> {
    pub fn new(placeholder: impl Into<String>) -> Self {
        Self {
            inner: TextInput::new(placeholder),
        }
    }

    pub fn on_change(mut self, f: impl Fn(&mut EventContext<App>, f32) + Send + Sync + 'static) -> Self {
        self.inner = self.inner.on_submit(move |ctx, text| {
            if let Ok(val) = text.parse::<f32>() {
                f(ctx, val);
            }
        });
        self
    }
}

impl<App> Widget<App> for NumberInput<App> {
    fn build(&mut self, ctx: &mut BuildContext<App>) {
        self.inner.build(ctx);
    }

    fn draw(&self, ctx: &mut DrawContext, cmds: &mut Vec<RenderCommand>) {
        self.inner.draw(ctx, cmds);
    }

    fn calculate_size(&self, ctx: &mut LayoutContext) -> Vec2 {
        self.inner.calculate_size(ctx)
    }

    fn on_event(&mut self, ctx: &mut EventContext<App>, event: &UiEvent) -> EventResponse {
        // Filtrar solo números y puntos antes de pasar al inner
        match event {
            UiEvent::Char { c } => {
                if c.is_ascii_digit() || *c == '.' {
                    self.inner.on_event(ctx, event)
                } else {
                    EventResponse::Ignored
                }
            }
            _ => self.inner.on_event(ctx, event),
        }
    }
}
