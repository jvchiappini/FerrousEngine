use crate::layout::RenderCommand;
use crate::widget::Widget;
use winit::keyboard::KeyCode;

/// Generic container that holds a heterogeneous collection of widgets and
/// draws them in order.  The container handles focus tracking so that a
/// widget can receive keyboard events when the user clicks on it.
pub struct Canvas {
    children: Vec<Box<dyn Widget>>,
    /// index of currently focused child (used for keyboard events). The
    /// value is `None` when no widget has focus.
    focused: Option<usize>,
}

impl Canvas {
    pub fn new() -> Self {
        Self {
            children: Vec::new(),
            focused: None,
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

    /// Returns a slice of child widgets.  This is primarily used by
    /// containers that need to inspect their children (for example to compute
    /// an adaptive bounding box).
    pub fn children(&self) -> &[Box<dyn Widget>] {
        &self.children
    }

    /// route a mouse-move event to all children; this is important for
    /// things like slider dragging or hover state. It does **not** change
    /// focus.
    pub fn mouse_move(&mut self, mx: f64, my: f64) {
        for child in &mut self.children {
            child.mouse_move(mx, my);
        }
    }

    /// handle mouse button press/release. On press we update the focus
    /// index by performing hit tests in order; the first widget hit becomes
    /// focused and all others lose focus. After focus is determined we
    /// forward the input event to every child (allowing sliders/buttons to
    /// respond even if they are not focused).
    pub fn mouse_input(&mut self, mx: f64, my: f64, pressed: bool) {
        if pressed {
            self.focused = None;
            for (i, child) in self.children.iter_mut().enumerate() {
                if child.hit(mx, my) {
                    self.focused = Some(i);
                    break;
                }
            }
        }
        for child in &mut self.children {
            child.mouse_input(mx, my, pressed);
        }
    }

    /// forward keyboard events to the currently focused widget (if any).
    pub fn keyboard_input(&mut self, text: Option<&str>, key: Option<KeyCode>, pressed: bool) {
        if let Some(idx) = self.focused {
            if let Some(child) = self.children.get_mut(idx) {
                child.keyboard_input(text, key, pressed);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;

    struct Dummy {
        called: Cell<bool>,
    }

    impl Dummy {
        fn new() -> Self {
            Self {
                called: Cell::new(false),
            }
        }
    }

    impl Widget for Dummy {
        fn collect(&self, cmds: &mut Vec<RenderCommand>) {
            self.called.set(true);
            cmds.push(RenderCommand::Quad {
                rect: crate::layout::Rect {
                    x: 0.0,
                    y: 0.0,
                    width: 1.0,
                    height: 1.0,
                },
                color: [1.0, 1.0, 1.0, 1.0],
                radii: [0.0; 4],
                flags: 0,
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
