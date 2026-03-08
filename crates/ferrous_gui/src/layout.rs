pub use ferrous_ui_core::{Rect, RectOffset, Units, Alignment, DisplayMode, Style, RenderCommand};

/// Nodo en el árbol de layout.
#[derive(Debug, Clone, Default)]
pub struct Node {
    pub style: Style,
    pub children: Vec<Node>,
    pub rect: Rect,
    /// contenido calculado durante la pasada inferior
    desired_size: (f32, f32),
    /// opcional: texto a dibujar si este nodo es un `Text`.
    pub text: Option<String>,
    /// color de fondo (cuadro blanco por defecto si None)
    pub background: Option<[f32; 4]>,
    /// color del texto si hay texto
    pub text_color: [f32; 4],
    /// tamaño de fuente para texto
    pub font_size: f32,
}

impl Node {
    pub fn new() -> Self {
        Self {
            style: Style::default(),
            children: Vec::new(),
            rect: Rect::default(),
            desired_size: (0.0, 0.0),
            text: None,
            background: None,
            text_color: [1.0, 1.0, 1.0, 1.0],
            font_size: 16.0,
        }
    }

    pub fn with_display(mut self, display: DisplayMode) -> Self {
        self.style.display = display;
        self
    }

    pub fn with_margin(mut self, v: f32) -> Self {
        self.style.margin = RectOffset::all(v);
        self
    }

    pub fn with_padding(mut self, v: f32) -> Self {
        self.style.padding = RectOffset::all(v);
        self
    }

    pub fn with_size(mut self, width: Units, height: Units) -> Self {
        self.style.size = (width, height);
        self
    }

    pub fn with_alignment(mut self, a: Alignment) -> Self {
        self.style.alignment = a;
        self
    }

    pub fn add_child(mut self, child: Node) -> Self {
        self.children.push(child);
        self
    }

    pub fn set_text(mut self, text: impl Into<String>) -> Self {
        self.text = Some(text.into());
        self
    }

    pub fn set_background(mut self, color: [f32; 4]) -> Self {
        self.background = Some(color);
        self
    }

    pub fn set_text_color(mut self, color: [f32; 4]) -> Self {
        self.text_color = color;
        self
    }

    pub fn set_font_size(mut self, size: f32) -> Self {
        self.font_size = size;
        self
    }

    /// Realiza ambas pasadas de layout. `parent_width`/`parent_height` pueden
    /// corresponder al tamaño de la pantalla o del padre inmediato.
    pub fn compute_layout(&mut self, parent_width: f32, parent_height: f32) {
        // primera pasada, bottom-up: tamanos deseados
        self.compute_desired_size();
        // segunda pasada, top-down: asignar rects
        self.layout(0.0, 0.0, parent_width, parent_height);
    }

    fn compute_desired_size(&mut self) -> (f32, f32) {
        // primero recursivamente a hijos
        for child in &mut self.children {
            child.compute_desired_size();
        }

        // calcular en base al modo de display
        let (mut w, mut h) = match self.style.display {
            DisplayMode::Block => {
                // ancho = máximo ancho de hijos + padding
                let mut maxw: f32 = 0.0;
                let mut totalh: f32 = 0.0;
                for child in &self.children {
                    let cw =
                        child.desired_size.0 + child.style.margin.left + child.style.margin.right;
                    let ch =
                        child.desired_size.1 + child.style.margin.top + child.style.margin.bottom;
                    maxw = maxw.max(cw);
                    totalh += ch;
                }
                (maxw, totalh)
            }
            DisplayMode::FlexRow => {
                let mut totalw: f32 = 0.0;
                let mut maxh: f32 = 0.0;
                for child in &self.children {
                    let cw =
                        child.desired_size.0 + child.style.margin.left + child.style.margin.right;
                    let ch =
                        child.desired_size.1 + child.style.margin.top + child.style.margin.bottom;
                    totalw += cw;
                    maxh = maxh.max(ch);
                }
                (totalw, maxh)
            }
            DisplayMode::FlexColumn => {
                let mut totalh: f32 = 0.0;
                let mut maxw: f32 = 0.0;
                for child in &self.children {
                    let cw =
                        child.desired_size.0 + child.style.margin.left + child.style.margin.right;
                    let ch =
                        child.desired_size.1 + child.style.margin.top + child.style.margin.bottom;
                    totalh += ch;
                    maxw = maxw.max(cw);
                }
                (maxw, totalh)
            }
        };

        // si el size está fijado en px reemplazamos
        match self.style.size.0 {
            Units::Px(val) if val > 0.0 => w = val,
            Units::Auto => w = self.desired_size.0,
            _ => {}
        }
        match self.style.size.1 {
            Units::Px(val) if val > 0.0 => h = val,
            Units::Auto => h = self.desired_size.1,
            _ => {}
        }

        self.desired_size = (
            w + self.style.padding.left + self.style.padding.right,
            h + self.style.padding.top + self.style.padding.bottom,
        );
        self.desired_size
    }

    fn layout(&mut self, x: f32, y: f32, width: f32, height: f32) {
        // aplicar margen
        let x = x + self.style.margin.left;
        let y = y + self.style.margin.top;
        // ancho/alto disponibles para el contenido
        let inner_w = width - self.style.margin.left - self.style.margin.right;
        let inner_h = height - self.style.margin.top - self.style.margin.bottom;

        // tamaño final del nodo: si units en px lo respetamos, si porcentaje lo
        // calculamos en función de inner_{w,h}, si flex queda para el padre.
        let mut final_w = inner_w;
        let mut final_h = inner_h;
        match self.style.size.0 {
            Units::Px(v) if v > 0.0 => final_w = v,
            Units::Percentage(p) => final_w = inner_w * p / 100.0,
            Units::Flex(_) | Units::Auto => {
                // se asigna por el padre o se queda con inner_w por defecto
            }
            _ => {}
        }
        match self.style.size.1 {
            Units::Px(v) if v > 0.0 => final_h = v,
            Units::Percentage(p) => final_h = inner_h * p / 100.0,
            Units::Flex(_) | Units::Auto => {}
            _ => {}
        }

        self.rect = Rect {
            x,
            y,
            width: final_w,
            height: final_h,
        };

        // distribuir hijos
        match self.style.display {
            DisplayMode::Block => {
                let mut cy = y + self.style.padding.top;
                for child in &mut self.children {
                    let cw = match child.style.size.0 {
                        Units::Px(v) => v,
                        Units::Percentage(p) => final_w * p / 100.0,
                        Units::Flex(_) | Units::Auto => child.desired_size.0,
                    };
                    let ch = match child.style.size.1 {
                        Units::Px(v) => v,
                        Units::Percentage(p) => final_h * p / 100.0,
                        Units::Flex(_) | Units::Auto => child.desired_size.1,
                    };
                    child.layout(x + self.style.padding.left, cy, cw, ch);
                    cy += ch + child.style.margin.top + child.style.margin.bottom;
                }
            }
            DisplayMode::FlexRow => {
                // calcular espacio total fijo y total flex
                let mut total_fixed = 0.0;
                let mut total_flex = 0.0;
                for child in &self.children {
                    match child.style.size.0 {
                        Units::Flex(f) => total_flex += f,
                        Units::Px(v) => {
                            total_fixed += v + child.style.margin.left + child.style.margin.right
                        }
                        Units::Percentage(p) => {
                            total_fixed += (final_w * p / 100.0)
                                + child.style.margin.left
                                + child.style.margin.right
                        }
                        Units::Auto => {
                            total_fixed += child.desired_size.0 + child.style.margin.left + child.style.margin.right
                        }
                    }
                }
                let mut cx = x + self.style.padding.left;
                for child in &mut self.children {
                    let cw = match child.style.size.0 {
                        Units::Flex(f) if total_flex > 0.0 => {
                            ((final_w - total_fixed) * (f / total_flex)).max(0.0)
                        }
                        Units::Px(v) => v,
                        Units::Percentage(p) => final_w * p / 100.0,
                        Units::Flex(_) | Units::Auto => child.desired_size.0,
                    };
                    let ch = match child.style.size.1 {
                        Units::Px(v) => v,
                        Units::Percentage(p) => final_h * p / 100.0,
                        Units::Flex(_) | Units::Auto => child.desired_size.1,
                    };
                    child.layout(cx, y + self.style.padding.top, cw, ch);
                    cx += cw + child.style.margin.left + child.style.margin.right;
                }
            }
            DisplayMode::FlexColumn => {
                let mut total_fixed = 0.0;
                let mut total_flex = 0.0;
                for child in &self.children {
                    match child.style.size.1 {
                        Units::Flex(f) => total_flex += f,
                        Units::Px(v) => {
                            total_fixed += v + child.style.margin.top + child.style.margin.bottom
                        }
                        Units::Percentage(p) => {
                            total_fixed += (final_h * p / 100.0)
                                + child.style.margin.top
                                + child.style.margin.bottom
                        }
                        Units::Auto => {
                            total_fixed += child.desired_size.1 + child.style.margin.top + child.style.margin.bottom
                        }
                    }
                }
                let mut cy = y + self.style.padding.top;
                for child in &mut self.children {
                    let ch = match child.style.size.1 {
                        Units::Flex(f) if total_flex > 0.0 => {
                            ((final_h - total_fixed) * (f / total_flex)).max(0.0)
                        }
                        Units::Px(v) => v,
                        Units::Percentage(p) => final_h * p / 100.0,
                        Units::Flex(_) | Units::Auto => child.desired_size.1,
                    };
                    let cw = match child.style.size.0 {
                        Units::Px(v) => v,
                        Units::Percentage(p) => final_w * p / 100.0,
                        Units::Flex(_) | Units::Auto => child.desired_size.0,
                    };
                    child.layout(x + self.style.padding.left, cy, cw, ch);
                    cy += ch + child.style.margin.top + child.style.margin.bottom;
                }
            }
        }
        // posicionamiento de texto o cuadro de fondo no se hace aquí; solo
        // guardamos rect en self.rect
    }

    /// Recorre el árbol y genera "render commands" genéricos.
    pub fn collect_render_commands(&self, cmds: &mut Vec<RenderCommand>) {
        if let Some(bg) = self.background {
            // default background has no rounding
            cmds.push(RenderCommand::Quad {
                rect: self.rect.clone(),
                color: bg,
                radii: [0.0; 4],
                flags: 0,
            });
        }
        if let Some(text) = &self.text {
            cmds.push(RenderCommand::Text {
                rect: self.rect.clone(),
                text: text.clone(),
                color: self.text_color,
                font_size: self.font_size,
            });
        }
        for child in &self.children {
            child.collect_render_commands(cmds);
        }
    }
}

/// Puente para permitir que un `Node` (sistema antiguo) se use como un `Widget`
/// en el nuevo `UiTree` de `ferrous_ui_core`.
pub struct LegacyNodeWidget(pub Node);

impl ferrous_ui_core::Widget for LegacyNodeWidget {
    fn draw(&self, _ctx: &mut ferrous_ui_core::DrawContext, cmds: &mut Vec<RenderCommand>) {
        self.0.collect_render_commands(cmds);
    }

    fn calculate_size(&self, _ctx: &mut ferrous_ui_core::LayoutContext) -> glam::Vec2 {
        glam::Vec2::new(self.0.rect.width, self.0.rect.height)
    }
}

pub use ferrous_ui_render::ToBatches;

// tests to validate basic layout
#[cfg(test)]
mod tests {
    use super::*;
    use crate::renderer::{GuiBatch, TextBatch};

    #[test]
    fn basic_block_layout() {
        let mut root = Node::new().with_padding(10.0);
        let child = Node::new()
            .with_margin(5.0)
            .with_size(Units::Px(50.0), Units::Px(20.0));
        root = root.add_child(child);
        root.compute_layout(200.0, 100.0);
        assert_eq!(root.rect.width, 200.0);
        assert_eq!(root.children[0].rect.x, 10.0 + 5.0);
    }

    #[test]
    fn render_command_conversion() {
        let cmd = RenderCommand::Quad {
            rect: Rect {
                x: 1.0,
                y: 2.0,
                width: 3.0,
                height: 4.0,
            },
            color: [0.1, 0.2, 0.3, 0.4],
            radii: [0.0; 4],
            flags: 0,
        };
        let mut qb = GuiBatch::new();
        // no font needed for quad case
        #[cfg(feature = "text")]
        cmd.to_batches(&mut qb, None);
        #[cfg(not(feature = "text"))]
        cmd.to_batches(&mut qb);
        assert_eq!(qb.len(), 1);
    }

    #[cfg(feature = "assets")]
    #[test]
    fn render_command_image() {
        use std::sync::Arc;
        // create a dummy texture handle; the contents are never accessed by
        // the batch logic so we may safely zero them.
        let tex = Arc::new(unsafe { std::mem::zeroed::<ferrous_assets::Texture2d>() });
        let cmd = RenderCommand::Image {
            rect: Rect {
                x: 0.0,
                y: 0.0,
                width: 5.0,
                height: 5.0,
            },
            texture: tex.clone(),
            uv0: [0.0, 0.0],
            uv1: [1.0, 1.0],
            color: [1.0, 1.0, 1.0, 1.0],
        };
        let mut qb = GuiBatch::new();
        #[cfg(feature = "text")]
        cmd.to_batches(&mut qb, None);
        #[cfg(not(feature = "text"))]
        cmd.to_batches(&mut qb);
        assert_eq!(qb.len(), 1);
        // reserve the same texture again and ensure the slot index does not grow
        #[cfg(feature = "text")]
        cmd.to_batches(&mut qb, None);
        #[cfg(not(feature = "text"))]
        cmd.to_batches(&mut qb);
        assert_eq!(qb.len(), 2);
    }

    #[test]
    fn full_ui_to_batches() {
        // create a small column with a colored box and some text
        let mut root = Node::new()
            .with_padding(5.0)
            .add_child(
                Node::new()
                    .set_background([1.0, 0.0, 0.0, 1.0])
                    .with_size(Units::Px(10.0), Units::Px(10.0)),
            )
            .add_child(
                Node::new()
                    .set_text("Hi")
                    .with_size(Units::Px(50.0), Units::Px(20.0)),
            );
        root.compute_layout(100.0, 100.0);
        let mut cmds = Vec::new();
        root.collect_render_commands(&mut cmds);
        let mut qb = GuiBatch::new();
        for c in &cmds {
            #[cfg(feature = "text")]
            c.to_batches(&mut qb, None);
            #[cfg(not(feature = "text"))]
            c.to_batches(&mut qb);
        }
        // expect at least one quad and possibly text (text_batch will be empty because no font)
        assert!(qb.len() >= 1);
    }
}
