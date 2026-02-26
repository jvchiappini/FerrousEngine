use crate::renderer::GuiBatch;

/// Cualquier elemento de la interfaz que pueda "dibujarse".
///
/// La implementación concreta decide qué quads añadir al `GuiBatch`.
pub trait Widget {
    fn draw(&self, batch: &mut GuiBatch);
}

/// Contenedor genérico que alberga varios widgets y los dibuja por orden.
pub struct Canvas {
    children: Vec<Box<dyn Widget>>,
}

impl Canvas {
    pub fn new() -> Self {
        Self { children: Vec::new() }
    }

    pub fn add(&mut self, widget: impl Widget + 'static) {
        self.children.push(Box::new(widget));
    }

    pub fn draw(&self, batch: &mut GuiBatch) {
        for child in &self.children {
            child.draw(batch);
        }
    }
}

// simple tests to ensure the canvas forwards draw calls
#[cfg(test)]
mod tests {
    use super::*;
    use crate::renderer::GuiQuad;

    struct Dummy {
        called: std::cell::Cell<bool>,
    }

    impl Dummy {
        fn new() -> Self {
            Self { called: std::cell::Cell::new(false) }
        }
    }

    impl Widget for Dummy {
        fn draw(&self, batch: &mut GuiBatch) {
            self.called.set(true);
            // add an empty quad just to exercise the batch
            batch.push(GuiQuad { pos: [0.0, 0.0], size: [1.0, 1.0], color: [1.0, 1.0, 1.0, 1.0] });
        }
    }

    #[test]
    fn canvas_propagates_draw() {
        let mut canvas = Canvas::new();
        let dummy = Dummy::new();
        canvas.add(dummy);
        let mut batch = GuiBatch::new();
        canvas.draw(&mut batch);
        assert!(!batch.is_empty());
    }
}
