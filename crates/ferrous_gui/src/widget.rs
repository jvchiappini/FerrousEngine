use crate::layout::{Node, RenderCommand};

/// Cualquier elemento de la interfaz que pueda "dibujarse".
///
/// En lugar de acoplarse al renderer, la abstracción devuelve una lista de
/// `RenderCommand` genéricos. Más adelante el código que use la UI puede
/// convertirlos a `GuiBatch` y `TextBatch` según corresponda.
pub trait Widget {
    fn collect(&self, cmds: &mut Vec<RenderCommand>);
}

/// Contenedor genérico que alberga varios widgets y los dibuja por orden.
pub struct Canvas {
    children: Vec<Box<dyn Widget>>,
}

impl Canvas {
    pub fn new() -> Self {
        Self {
            children: Vec::new(),
        }
    }

    pub fn add(&mut self, widget: impl Widget + 'static) {
        self.children.push(Box::new(widget));
    }

    pub fn collect(&self, cmds: &mut Vec<RenderCommand>) {
        for child in &self.children {
            child.collect(cmds);
        }
    }
}

// simple tests to ensure the canvas forwards draw calls
#[cfg(test)]
mod tests {
    use super::*;

    struct Dummy {
        called: std::cell::Cell<bool>,
    }

    impl Dummy {
        fn new() -> Self {
            Self {
                called: std::cell::Cell::new(false),
            }
        }
    }

    impl Widget for Dummy {
        fn collect(&self, cmds: &mut Vec<RenderCommand>) {
            self.called.set(true);
            // add a dummy quad command
            cmds.push(RenderCommand::Quad {
                rect: crate::layout::Rect {
                    x: 0.0,
                    y: 0.0,
                    width: 1.0,
                    height: 1.0,
                },
                color: [1.0, 1.0, 1.0, 1.0],
            });
        }
    }

    #[test]
    fn canvas_propagates_draw() {
        let mut canvas = Canvas::new();
        let dummy = Dummy::new();
        canvas.add(dummy);
        let mut cmds = Vec::new();
        canvas.collect(&mut cmds);
        assert!(!cmds.is_empty());
    }
}

#[cfg(test)]
mod builder_tests {
    use super::*;
    use crate::layout::DisplayMode;

    #[test]
    fn column_builder_and_layout() {
        let ui = Column::new()
            .with_padding(20.0)
            .add_child(UiButton::new("Save").with_margin(5.0))
            .add_child(Text::new("Hello"));

        // convert to Node so we can compute layout
        let mut node: Node = ui.into();
        node.compute_layout(200.0, 100.0);

        // should be flex column => each child stacked vertically
        assert_eq!(node.style.display, DisplayMode::FlexColumn);
        assert!(node.children.len() == 2);
        // the first child should have x offset equal to padding+margin
        assert!(node.children[0].rect.x > 0.0);
    }
}

// -----------------------------------------------------------------------------
// helpers for declarative builder API

/// Contenedor de columnas flexibles.
///
/// ```rust
/// use ferrous_gui::{Column, UiButton, Text};
///
/// let ui = Column::new()
///     .with_padding(20.0)
///     .add_child(UiButton::new("Guardar").with_margin(5.0))
///     .add_child(Text::new("Usuario: Admin"));
///
/// // convertir a nodos y calcular layout en pantalla de 800x600
/// let mut root: ferrous_gui::Node = ui.into();
/// root.compute_layout(800.0, 600.0);
/// ```
pub struct Column(pub Node);

impl Column {
    pub fn new() -> Self {
        Column(Node::new().with_display(crate::layout::DisplayMode::FlexColumn))
    }

    pub fn with_padding(self, v: f32) -> Self {
        Column(self.0.with_padding(v))
    }

    pub fn with_margin(self, v: f32) -> Self {
        Column(self.0.with_margin(v))
    }

    pub fn add_child<T: Into<Node>>(mut self, child: T) -> Self {
        self.0 = self.0.add_child(child.into());
        self
    }
}

impl From<Column> for Node {
    fn from(c: Column) -> Node {
        c.0
    }
}

/// Contenedor de filas flexibles (alias a Column pero con dirección horizontal).
pub struct Row(pub Node);

impl Row {
    pub fn new() -> Self {
        Row(Node::new().with_display(crate::layout::DisplayMode::FlexRow))
    }

    pub fn with_padding(self, v: f32) -> Self {
        Row(self.0.with_padding(v))
    }

    pub fn add_child<T: Into<Node>>(mut self, child: T) -> Self {
        self.0 = self.0.add_child(child.into());
        self
    }
}

impl From<Row> for Node {
    fn from(r: Row) -> Node {
        r.0
    }
}

/// Botón simple con texto y color de fondo (construido como un nodo).
/// Este tipo forma parte de la API de construcción declarativa y no debe
/// confundirse con el widget interactivo definido en `button.rs`.
pub struct UiButton(pub Node);

impl UiButton {
    pub fn new(label: &str) -> Self {
        let mut n = Node::new();
        n = n.set_text(label);
        n = n.set_background([0.2, 0.2, 0.8, 1.0]);
        n = n.set_text_color([1.0, 1.0, 1.0, 1.0]);
        n = n.with_padding(5.0);
        n = n.with_alignment(crate::layout::Alignment::Center);
        UiButton(n)
    }

    pub fn with_margin(self, v: f32) -> Self {
        UiButton(self.0.with_margin(v))
    }

    pub fn with_padding(self, v: f32) -> Self {
        UiButton(self.0.with_padding(v))
    }
}

impl From<UiButton> for Node {
    fn from(b: UiButton) -> Node {
        b.0
    }
}

/// Texto estático.
pub struct Text(pub Node);

impl Text {
    pub fn new(content: &str) -> Self {
        let mut n = Node::new();
        n = n.set_text(content);
        Text(n)
    }

    pub fn with_margin(self, v: f32) -> Self {
        Text(self.0.with_margin(v))
    }
}

impl From<Text> for Node {
    fn from(t: Text) -> Node {
        t.0
    }
}

// make Node itself a widget so containers composed of nodes can be used as widgets
impl Widget for Node {
    fn collect(&self, cmds: &mut Vec<RenderCommand>) {
        self.collect_render_commands(cmds);
    }
}
