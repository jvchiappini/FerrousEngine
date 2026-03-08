use crate::{Widget, RenderCommand, DrawContext, LayoutContext, Color, Vec2};

// ─── Label ───────────────────────────────────────────────────────────────────

/// Widget para mostrar texto estático o dinámico.
pub struct Label {
    pub text: String,
    pub color: Option<Color>,
    pub font_size: Option<f32>,
}

impl Label {
    pub fn new(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            color: None,
            font_size: None,
        }
    }

    pub fn with_color(mut self, color: Color) -> Self {
        self.color = Some(color);
        self
    }

    pub fn with_size(mut self, size: f32) -> Self {
        self.font_size = Some(size);
        self
    }
}

impl<App> Widget<App> for Label {
    fn draw(&self, ctx: &mut DrawContext, cmds: &mut Vec<RenderCommand>) {
        let color = self.color.unwrap_or(ctx.theme.on_surface);
        let font_size = self.font_size.unwrap_or(ctx.theme.font_size_base);

        cmds.push(RenderCommand::Text {
            rect: ctx.rect,
            text: self.text.clone(),
            color: color.to_array(),
            font_size,
        });
    }

    fn calculate_size(&self, ctx: &mut LayoutContext) -> Vec2 {
        let fs = self.font_size.unwrap_or(ctx.theme.font_size_base);
        let w = self.text.len() as f32 * fs * 0.55;
        glam::vec2(w, fs * 1.2)
    }
}
