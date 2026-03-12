use crate::{Widget, RenderCommand, DrawContext, BuildContext, LayoutContext, EventContext, EventResponse, UiEvent, Vec2, TextInput};

// ─── NumberInput ─────────────────────────────────────────────────────────────

/// Input especializado en números con validación.
///
/// - `.on_change(f)` → se dispara en cada tecla válida (dígito o `.`).
/// - `.on_submit(f)` → se dispara al confirmar con Enter.
pub struct NumberInput<App> {
    pub inner: TextInput<App>,
    /// Callback por pulsación de tecla (per-keystroke).
    on_change_cb: Option<Box<dyn Fn(&mut EventContext<App>, f32) + Send + Sync + 'static>>,
}

impl<App> NumberInput<App> {
    pub fn new(placeholder: impl Into<String>) -> Self {
        Self {
            inner: TextInput::new(placeholder),
            on_change_cb: None,
        }
    }

    /// Registra una función que se invoca en **cada pulsación** de tecla numérica válida.
    ///
    /// El parámetro es el valor actual del campo parseado como `f32`.
    /// Si el texto actual no es un número válido, el callback no se invoca.
    pub fn on_change(mut self, f: impl Fn(&mut EventContext<App>, f32) + Send + Sync + 'static) -> Self {
        self.on_change_cb = Some(Box::new(f));
        self
    }

    /// Registra una función que se invoca al **confirmar** (Enter).
    ///
    /// El parámetro es el texto completo del campo.
    pub fn on_submit(mut self, f: impl Fn(&mut EventContext<App>, &str) + Send + Sync + 'static) -> Self {
        self.inner = self.inner.on_submit(f);
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
                    let resp = self.inner.on_event(ctx, event);
                    // Disparar on_change por pulsación de tecla
                    if let Some(cb) = &self.on_change_cb {
                        if let Ok(val) = self.inner.text.parse::<f32>() {
                            cb(ctx, val);
                        }
                    }
                    resp
                } else {
                    EventResponse::Ignored
                }
            }
            _ => self.inner.on_event(ctx, event),
        }
    }
}
