use crate::{Widget, RenderCommand, DrawContext, BuildContext};

/// Widget básico que representa un contenedor rectangular con color de fondo.
pub struct Panel {
    pub color: [f32; 4],
    pub radius: f32,
}

impl Panel {
    pub fn new(color: [f32; 4]) -> Self {
        Self { color, radius: 0.0 }
    }

    pub fn with_radius(mut self, radius: f32) -> Self {
        self.radius = radius;
        self
    }
}

impl Widget for Panel {
    fn draw(&self, ctx: &mut DrawContext, cmds: &mut Vec<RenderCommand>) {
        cmds.push(RenderCommand::Quad {
            rect: ctx.rect,
            color: self.color,
            radii: [self.radius; 4],
            flags: 0,
        });
    }
}

/// Widget para mostrar texto simple.
pub struct Label {
    pub text: String,
    pub color: [f32; 4],
    pub font_size: f32,
}

impl Label {
    pub fn new(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            color: [1.0, 1.0, 1.0, 1.0],
            font_size: 14.0,
        }
    }

    pub fn with_size(mut self, size: f32) -> Self {
        self.font_size = size;
        self
    }

    pub fn with_color(mut self, color: [f32; 4]) -> Self {
        self.color = color;
        self
    }
}

impl Widget for Label {
    fn draw(&self, ctx: &mut DrawContext, cmds: &mut Vec<RenderCommand>) {
        cmds.push(RenderCommand::Text {
            rect: ctx.rect,
            text: self.text.clone(),
            color: self.color,
            font_size: self.font_size,
        });
    }
}

/// Botón interactivo.
pub struct Button {
    pub label: String,
    pub color: [f32; 4],
    pub hover_color: [f32; 4],
    pub text_color: [f32; 4],
    pub is_hovered: bool,
}

impl Button {
    pub fn new(label: impl Into<String>) -> Self {
        Self {
            label: label.into(),
            color: [0.15, 0.15, 0.15, 1.0],
            hover_color: [0.25, 0.25, 0.25, 1.0],
            text_color: [1.0, 1.0, 1.0, 1.0],
            is_hovered: false,
        }
    }
}

impl Widget for Button {
    fn build(&mut self, _ctx: &mut BuildContext) {
        // Composición opcional aquí, pero por rendimiento un botón puede ser atómico.
    }

    fn draw(&self, ctx: &mut DrawContext, cmds: &mut Vec<RenderCommand>) {
        let bg_color = if self.is_hovered { self.hover_color } else { self.color };
        
        // Fondo
        cmds.push(RenderCommand::Quad {
            rect: ctx.rect,
            color: bg_color,
            radii: [4.0; 4], // Bordes redondeados sutiles por defecto
            flags: 0,
        });

        // Etiqueta (centrada simplificadamente por ahora)
        cmds.push(RenderCommand::Text {
            rect: ctx.rect,
            text: self.label.clone(),
            color: self.text_color,
            font_size: 14.0,
        });
    }

    fn on_event(&mut self, _ctx: &mut crate::EventContext, event: &crate::UiEvent) -> crate::EventResponse {
        match event {
            crate::UiEvent::MouseEnter => {
                self.is_hovered = true;
                crate::EventResponse::Redraw
            }
            crate::UiEvent::MouseLeave => {
                self.is_hovered = false;
                crate::EventResponse::Redraw
            }
            crate::UiEvent::MouseDown { .. } => {
                // Aquí se podría disparar un callback 'on_click'
                crate::EventResponse::Consumed
            }
            _ => crate::EventResponse::Ignored,
        }
    }
}

pub struct PlaceholderWidget;
impl Widget for PlaceholderWidget {}
