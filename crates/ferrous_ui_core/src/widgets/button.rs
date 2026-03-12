use crate::{
    Background, BuildContext, DrawContext, EventContext, EventResponse, FerrousWidget,
    LayoutContext, RenderCommand, TextAlign, UiEvent, Vec2, Widget,
};

// ─── Button ──────────────────────────────────────────────────────────────────

/// Botón interactivo con callbacks enriquecidos que acceden al estado de la aplicación.
#[derive(FerrousWidget)]
pub struct Button<App> {
    #[prop(label = "Texto", category = "Contenido")]
    pub label: String,

    /// Radios de las 4 esquinas: [top-left, top-right, bottom-left, bottom-right].
    /// Cada componente es `None` para heredar `theme.border_radius`.
    pub radii: [Option<f32>; 4],

    /// Alineación del texto del label dentro del botón. Defecto: centrado en ambos ejes.
    pub text_align: TextAlign,

    pub is_hovered: bool,
    /// Fondo personalizado. `Background::None` significa usar el color de tema estándar.
    pub background: Background,
    on_click_cb: Option<Box<dyn Fn(&mut EventContext<App>) + Send + Sync>>,

    on_hover_cb: Option<Box<dyn Fn(&mut EventContext<App>) + Send + Sync>>,
    on_hover_end_cb: Option<Box<dyn Fn(&mut EventContext<App>) + Send + Sync>>,
}

impl<App> Button<App> {
    /// Crea un botón con el texto `label`.
    pub fn new(label: impl Into<String>) -> Self {
        Self {
            label: label.into(),
            radii: [None; 4],
            text_align: TextAlign::CENTER,
            is_hovered: false,
            background: Background::None,
            on_click_cb: None,
            on_hover_cb: None,
            on_hover_end_cb: None,
        }
    }

    /// Establece el mismo radio para las 4 esquinas.
    pub fn with_border_radius(mut self, r: f32) -> Self {
        self.radii = [Some(r); 4];
        self
    }

    /// Establece los radios individuales: [top-left, top-right, bottom-left, bottom-right].
    pub fn with_radii(mut self, tl: f32, tr: f32, br: f32, bl: f32) -> Self {
        self.radii = [Some(tl), Some(tr), Some(br), Some(bl)];
        self
    }

    /// Establece la alineación del texto del label.
    pub fn with_text_align(mut self, align: TextAlign) -> Self {
        self.text_align = align;
        self
    }

    /// Atajo: alinea el label horizontalmente.
    pub fn with_h_align(mut self, h: crate::HAlign) -> Self {
        self.text_align.h = h;
        self
    }

    /// Atajo: alinea el label verticalmente.
    pub fn with_v_align(mut self, v: crate::VAlign) -> Self {
        self.text_align.v = v;
        self
    }

    /// Establece el fondo del botón (degradado, textura, procedural, sólido…).
    pub fn with_background(mut self, bg: Background) -> Self {
        self.background = bg;
        self
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
    pub fn on_hover_end(
        mut self,
        f: impl Fn(&mut EventContext<App>) + Send + Sync + 'static,
    ) -> Self {
        self.on_hover_end_cb = Some(Box::new(f));
        self
    }
}

impl<App> Widget<App> for Button<App> {
    fn build(&mut self, _ctx: &mut BuildContext<App>) {}

    fn draw(&self, ctx: &mut DrawContext, cmds: &mut Vec<RenderCommand>) {
        let bg = if self.is_hovered {
            ctx.theme.primary_variant
        } else {
            ctx.theme.primary
        };
        let def = ctx.theme.border_radius;
        let r = [
            self.radii[0].unwrap_or(def),
            self.radii[1].unwrap_or(def),
            self.radii[2].unwrap_or(def),
            self.radii[3].unwrap_or(def),
        ];

        // Fondo
        match &self.background {
            Background::None => {
                cmds.push(RenderCommand::Quad {
                    rect: ctx.rect,
                    color: bg.to_array(),
                    radii: r,
                    flags: 0,
                });
            }
            other => {
                // Base sólido primero (garantiza el radio correcto como fallback)
                cmds.push(RenderCommand::Quad {
                    rect: ctx.rect,
                    color: bg.to_array(),
                    radii: r,
                    flags: 0,
                });
                cmds.push(RenderCommand::GradientQuad {
                    rect: ctx.rect,
                    background: other.clone(),
                    radii: r,
                    raster_resolution: (0, 0),
                });
            }
        }

        // Texto
        cmds.push(RenderCommand::Text {
            rect: ctx.rect,
            text: self.label.clone(),
            color: ctx.theme.on_primary.to_array(),
            font_size: ctx.theme.font_size_base,
            align: self.text_align,
        });
    }

    fn calculate_size(&self, _ctx: &mut LayoutContext) -> Vec2 {
        let w = self.label.len() as f32 * 10.0 + 30.0;
        glam::vec2(w, 36.0)
    }

    fn on_event(&mut self, ctx: &mut EventContext<App>, event: &UiEvent) -> EventResponse {
        match event {
            UiEvent::MouseEnter => {
                self.is_hovered = true;
                if let Some(cb) = &self.on_hover_cb {
                    cb(ctx);
                }
                EventResponse::Redraw
            }
            UiEvent::MouseLeave => {
                self.is_hovered = false;
                if let Some(cb) = &self.on_hover_end_cb {
                    cb(ctx);
                }
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

    fn reflect(&self) -> Option<&dyn crate::FerrousWidgetReflect> {
        Some(self)
    }

    fn reflect_mut(&mut self) -> Option<&mut dyn crate::FerrousWidgetReflect> {
        Some(self)
    }
}
