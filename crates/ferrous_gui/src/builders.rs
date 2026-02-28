use crate::layout::{Alignment, DisplayMode, Node};

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
        Column(Node::new().with_display(DisplayMode::FlexColumn))
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
        Row(Node::new().with_display(DisplayMode::FlexRow))
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
        n = n.with_alignment(Alignment::Center);
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

#[cfg(test)]
mod tests {
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
