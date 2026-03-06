use crate::canvas::Canvas;
use crate::Widget;
use crate::GuiKey;
// winit is re-exported only when the backend feature is enabled; UI methods
// no longer depend on it directly, hence we can drop the import entirely.

/// Higher‑level UI object intended to be held by applications. It wraps a
/// [`Canvas`] and provides convenience helpers for routing events and drawing
/// the aggregated widget tree.
pub struct Ui {
    canvas: Canvas,
    /// optional handle to a viewport widget previously registered via
    /// `register_viewport`; this allows the UI to expose helpers for
    /// updating the rectangle without the caller needing to hold their own
    /// reference.
    viewport: Option<std::rc::Rc<std::cell::RefCell<crate::viewport_widget::ViewportWidget>>>,
}

impl Ui {
    pub fn new() -> Self {
        Ui {
            canvas: Canvas::new(),
            viewport: None,
        }
    }

    /// Add a widget to the UI tree.
    pub fn add(&mut self, widget: impl Widget + 'static) {
        self.canvas.add(widget);
    }

    /// Convenience helper for viewport widgets. The passed reference is stored
    /// internally and also added to the canvas; later calls to
    /// `set_viewport_rect` will update the stored widget so the application
    /// does not need to keep its own copy.
    pub fn register_viewport(
        &mut self,
        vp: std::rc::Rc<std::cell::RefCell<crate::viewport_widget::ViewportWidget>>,
    ) {
        self.viewport = Some(vp.clone());
        self.add(vp);
    }

    /// Update the dimensions of the registered viewport widget, if any. This
    /// is mostly a convenience so that applications don't need to hold a
    /// separate reference just to propagate resize events.
    pub fn set_viewport_rect(&mut self, x: f32, y: f32, w: f32, h: f32) {
        if let Some(vp) = &self.viewport {
            vp.borrow_mut().rect = [x, y, w, h];
        }
    }

    /// Returns `true` if a viewport widget has been registered and currently
    /// holds focus. Useful for applications that want to treat the viewport
    /// differently (e.g. capture mouse/keyboard for 3D camera).
    pub fn viewport_focused(&self) -> bool {
        self.viewport
            .as_ref()
            .map(|vp| vp.borrow().focused)
            .unwrap_or(false)
    }

    /// Inform the UI that the cursor has moved. Coordinates are in window
    /// space and should match whatever coordinate system the caller is using
    /// for rendering.
    pub fn mouse_move(&mut self, mx: f64, my: f64) {
        self.canvas.mouse_move(mx, my);
    }

    /// Notify the UI that a mouse button was pressed or released.
    pub fn mouse_input(&mut self, mx: f64, my: f64, pressed: bool) {
        self.canvas.mouse_input(mx, my, pressed);
    }

    /// Deliver a keyboard event to the UI. `text` is any generated Unicode
    /// string, `key` is an optional non-text key (e.g. `GuiKey::Backspace`),
    /// and `pressed` indicates whether the key went down or up.
    pub fn keyboard_input(
        &mut self,
        text: Option<&str>,
        key: Option<GuiKey>,
        pressed: bool,
    ) {
        self.canvas.keyboard_input(text, key, pressed);
    }

    /// Collect draw commands from the widget tree and convert them into the
    /// provided GUI/text batches. When the `text` feature is enabled the
    /// caller must also supply an optional font; otherwise the font parameter
    /// is omitted entirely to avoid pulling in `ferrous_assets` when it's not
    /// needed.
    #[cfg(feature = "text")]
    pub fn draw(
        &self,
        quad_batch: &mut crate::renderer::GuiBatch,
        text_batch: &mut crate::renderer::TextBatch,
        font: Option<&ferrous_assets::font::Font>,
    ) {
        let mut cmds = Vec::new();
        self.canvas.collect(&mut cmds);
        for cmd in &cmds {
            cmd.to_batches(quad_batch, text_batch, font);
        }
    }

    #[cfg(not(feature = "text"))]
    pub fn draw(
        &self,
        quad_batch: &mut crate::renderer::GuiBatch,
        text_batch: &mut crate::renderer::TextBatch,
    ) {
        let mut cmds = Vec::new();
        self.canvas.collect(&mut cmds);
        for cmd in &cmds {
            cmd.to_batches(quad_batch, text_batch);
        }
    }

    /// Mutable access to the underlying canvas in case the user needs to
    /// perform more advanced operations (e.g. remove a widget).
    pub fn canvas_mut(&mut self) -> &mut Canvas {
        &mut self.canvas
    }
}
// end of file
