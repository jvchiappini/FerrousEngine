// renderer types are referenced later when converting commands; bring them here
use crate::{GuiBatch, TextBatch};

/// Espacio rectilíneo con origen en (x,y) y dimensiones (w,h).
#[derive(Debug, Clone, Default)]
pub struct Rect {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

/// Margen o padding con valores por cada lado.
#[derive(Debug, Clone, Copy)]
pub struct RectOffset {
    pub left: f32,
    pub right: f32,
    pub top: f32,
    pub bottom: f32,
}

impl Default for RectOffset {
    fn default() -> Self {
        RectOffset {
            left: 0.0,
            right: 0.0,
            top: 0.0,
            bottom: 0.0,
        }
    }
}

impl RectOffset {
    pub fn all(v: f32) -> Self {
        RectOffset {
            left: v,
            right: v,
            top: v,
            bottom: v,
        }
    }
}

/// Unidad de medida para ancho/alto.
#[derive(Debug, Clone, Copy)]
pub enum Units {
    Px(f32),
    Percentage(f32),
    Flex(f32),
}

impl Default for Units {
    fn default() -> Self {
        Units::Px(0.0)
    }
}

/// Cómo alinear elementos hijos dentro de un contenedor.
#[derive(Debug, Clone, Copy)]
pub enum Alignment {
    Start,
    Center,
    End,
    Stretch,
}

impl Default for Alignment {
    fn default() -> Self {
        Alignment::Start
    }
}

/// Tipo de flujo del contenedor.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum DisplayMode {
    Block,
    FlexRow,
    FlexColumn,
}

impl Default for DisplayMode {
    fn default() -> Self {
        DisplayMode::Block
    }
}

/// Reglas de estilo de un nodo.
#[derive(Debug, Clone, Default)]
pub struct Style {
    pub margin: RectOffset,
    pub padding: RectOffset,
    pub size: (Units, Units), // (width, height)
    pub alignment: Alignment,
    pub display: DisplayMode,
}

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
            _ => {}
        }
        match self.style.size.1 {
            Units::Px(val) if val > 0.0 => h = val,
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
            Units::Flex(_) => {
                // se asigna por el padre; aquí no hacemos nada
            }
            _ => {}
        }
        match self.style.size.1 {
            Units::Px(v) if v > 0.0 => final_h = v,
            Units::Percentage(p) => final_h = inner_h * p / 100.0,
            Units::Flex(_) => {}
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
                        Units::Flex(_) => child.desired_size.0,
                    };
                    let ch = match child.style.size.1 {
                        Units::Px(v) => v,
                        Units::Percentage(p) => final_h * p / 100.0,
                        Units::Flex(_) => child.desired_size.1,
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
                        _ => child.desired_size.0,
                    };
                    let ch = match child.style.size.1 {
                        Units::Px(v) => v,
                        Units::Percentage(p) => final_h * p / 100.0,
                        Units::Flex(_) => child.desired_size.1,
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
                        _ => child.desired_size.1,
                    };
                    let cw = match child.style.size.0 {
                        Units::Px(v) => v,
                        Units::Percentage(p) => final_w * p / 100.0,
                        Units::Flex(_) => child.desired_size.0,
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

/// Representación simplificada de lo que el renderer consume; se puede
/// traducir a GuiBatch / TextBatch con facilidad.
#[derive(Debug, Clone)]
pub enum RenderCommand {
    Quad {
        rect: Rect,
        color: [f32; 4],
        /// per-corner radii in pixels: [top-left, top-right, bottom-left, bottom-right]
        radii: [f32; 4],
        /// miscellaneous flags, currently only bit 0 indicates the
        /// quad should be rendered as a colour wheel gradient instead of a
        /// flat colour.  Other bits are reserved for future enhancements.
        flags: u32,
    },
    Text {
        rect: Rect,
        text: String,
        color: [f32; 4],
        font_size: f32,
    },
}

impl RenderCommand {
    /// Convierte el comando a los lotes que entiende el renderer. Requiere
    /// una fuente para la conversión de texto a quads.
    pub fn to_batches(
        &self,
        quad_batch: &mut GuiBatch,
        text_batch: &mut TextBatch,
        font: Option<&ferrous_assets::font::Font>,
    ) {
        match self {
            RenderCommand::Quad { rect, color, radii, flags } => {
                quad_batch.push(crate::renderer::GuiQuad {
                    pos: [rect.x, rect.y],
                    size: [rect.width, rect.height],
                    color: *color,
                    radii: *radii,
                    flags: *flags,
                });
            }
            RenderCommand::Text {
                rect,
                text,
                color,
                font_size,
            } => {
                // We'll draw text at the rect origin using the supplied size
                if let Some(f) = font {
                    text_batch.draw_text(f, text, [rect.x, rect.y], *font_size, *color);
                }
            }
        }
    }
}

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
        let mut tb = TextBatch::new();
        // no font needed for quad case
        cmd.to_batches(&mut qb, &mut tb, None);
        assert_eq!(qb.len(), 1);
        assert!(tb.is_empty());
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
        let mut tb = TextBatch::new();
        for c in &cmds {
            c.to_batches(&mut qb, &mut tb, None);
        }
        // expect at least one quad and possibly text (text_batch will be empty because no font)
        assert!(qb.len() >= 1);
    }
}
