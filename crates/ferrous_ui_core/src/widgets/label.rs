use crate::{
    Color, DrawContext, HAlign, LayoutContext, RenderCommand, TextAlign, VAlign, Vec2, Widget,
};

// ─── Label ───────────────────────────────────────────────────────────────────

/// Widget para mostrar texto estático o dinámico.
pub struct Label {
    pub text: String,
    pub color: Option<Color>,
    pub font_size: Option<f32>,
    pub binding: Option<std::sync::Arc<crate::Observable<String>>>,
    /// Alineación del texto dentro del bounding-box del widget. Defecto: centrado en Y, izquierda en X.
    pub text_align: TextAlign,
}

impl Label {
    pub fn new(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            color: None,
            font_size: None,
            binding: None,
            text_align: TextAlign::new(HAlign::Left, VAlign::Center),
        }
    }

    /// Vincula el label a un `Observable<String>`.
    pub fn with_binding(
        mut self,
        observable: std::sync::Arc<crate::Observable<String>>,
        node_id: crate::NodeId,
    ) -> Self {
        observable.subscribe(node_id);
        self.binding = Some(observable);
        self
    }

    pub fn with_color(mut self, color: Color) -> Self {
        self.color = Some(color);
        self
    }

    pub fn with_size(mut self, size: f32) -> Self {
        self.font_size = Some(size);
        self
    }

    /// Establece la alineación del texto.
    pub fn with_text_align(mut self, align: TextAlign) -> Self {
        self.text_align = align;
        self
    }

    /// Atajo: alinea el texto horizontalmente.
    pub fn with_h_align(mut self, h: HAlign) -> Self {
        self.text_align.h = h;
        self
    }

    /// Atajo: alinea el texto verticalmente.
    pub fn with_v_align(mut self, v: VAlign) -> Self {
        self.text_align.v = v;
        self
    }
}

impl<App> Widget<App> for Label {
    fn draw(&self, ctx: &mut DrawContext, cmds: &mut Vec<RenderCommand>) {
        let color = self.color.unwrap_or(ctx.theme.on_surface);
        let font_size = self.font_size.unwrap_or(ctx.theme.font_size_base);
        let text = self
            .binding
            .as_ref()
            .map(|o| o.get())
            .unwrap_or_else(|| self.text.clone());

        cmds.push(RenderCommand::Text {
            rect: ctx.rect,
            text,
            color: color.to_array(),
            font_size,
            align: self.text_align,
        });
    }

    fn calculate_size(&self, ctx: &mut LayoutContext) -> Vec2 {
        let fs = self.font_size.unwrap_or(ctx.theme.font_size_base);
        let text = self
            .binding
            .as_ref()
            .map(|o| o.get())
            .unwrap_or_else(|| self.text.clone());
        let w = text.len() as f32 * fs * 0.55;
        glam::vec2(w, fs * 1.2)
    }
}
