use crate::{
    Alignment, Background, BuildContext, Color, DisplayMode, DrawContext, RectOffset,
    RenderCommand, Widget,
};

// ─── Panel ───────────────────────────────────────────────────────────────────

/// Contenedor con fondo que actua como Flexbox por defecto.
///
/// - Sin configuracion extra: apila hijos en columna y los estira (Stretch) para
///   que llenen todo el ancho del panel.
/// - `.display`, `.alignment` y `.gap` permiten personalizar el comportamiento.
pub struct Panel {
    /// Color opcional. Si es None, usa el color de superficie del tema.
    pub color: Option<Color>,
    /// Radio de borde opcional. Si es None, usa el del tema.
    pub radius: Option<f32>,
    /// Modo de layout de los hijos. Por defecto `FlexColumn`.
    pub display: DisplayMode,
    /// Alineacion de los hijos dentro del panel. Por defecto `Stretch`.
    pub alignment: Alignment,
    /// Separacion entre hijos en pixeles. Por defecto 0.
    pub gap: f32,
    /// Padding interno uniforme. Por defecto 0.
    pub pad: f32,
    /// Fondo personalizado del panel. `Background::None` usa el color de superficie del tema.
    pub background: Background,
}

impl Panel {
    pub fn new() -> Self {
        Self {
            color: None,
            radius: None,
            display: DisplayMode::FlexColumn,
            alignment: Alignment::Stretch,
            gap: 0.0,
            pad: 0.0,
            background: Background::None,
        }
    }

    pub fn with_color(mut self, color: Color) -> Self {
        self.color = Some(color);
        self
    }

    pub fn with_radius(mut self, radius: f32) -> Self {
        self.radius = Some(radius);
        self
    }

    pub fn with_display(mut self, display: DisplayMode) -> Self {
        self.display = display;
        self
    }

    pub fn with_alignment(mut self, alignment: Alignment) -> Self {
        self.alignment = alignment;
        self
    }

    pub fn with_gap(mut self, gap: f32) -> Self {
        self.gap = gap;
        self
    }

    pub fn with_padding(mut self, pad: f32) -> Self {
        self.pad = pad;
        self
    }

    /// Establece el fondo del panel (degradado, textura, procedural, sólido…).
    pub fn with_background(mut self, bg: Background) -> Self {
        self.background = bg;
        self
    }
}

impl<App> Widget<App> for Panel {
    /// Configura el Style del propio nodo para actuar como contenedor flex.
    /// Ejecutado al insertar el panel en el arbol, antes del primer layout.
    fn build(&mut self, ctx: &mut BuildContext<App>) {
        let id = ctx.node_id;
        let mut style = ctx.tree.get_node_style(id).cloned().unwrap_or_default();

        // Solo sobreescribimos si el caller no configuro display explicitamente.
        if style.display == DisplayMode::Block {
            style.display = self.display;
        }
        if style.alignment == Alignment::Start {
            style.alignment = self.alignment;
        }
        if style.gap == 0.0 && self.gap > 0.0 {
            style.gap = self.gap;
        }
        if self.pad > 0.0 {
            style.padding = RectOffset::all(self.pad);
        }
        ctx.tree.set_node_style(id, style);
    }

    fn draw(&self, ctx: &mut DrawContext, cmds: &mut Vec<RenderCommand>) {
        let color = self.color.unwrap_or(ctx.theme.surface);
        let radius = self.radius.unwrap_or(ctx.theme.border_radius);
        let radii = [radius; 4];

        // Base sólido siempre presente
        cmds.push(RenderCommand::Quad {
            rect: ctx.rect,
            color: color.to_array(),
            radii,
            flags: 0,
        });

        // Fondo personalizado encima
        if !matches!(self.background, Background::None) {
            cmds.push(RenderCommand::GradientQuad {
                rect: ctx.rect,
                background: self.background.clone(),
                radii,
                raster_resolution: (0, 0),
            });
        }
    }
}
