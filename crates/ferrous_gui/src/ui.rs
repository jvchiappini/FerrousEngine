use crate::canvas::Canvas;
use ferrous_core::InputState;
use crate::Widget;
use winit::event::WindowEvent;

/// Higher‑level UI object intended to be held by applications. It wraps a
/// [`Canvas`] and provides convenience helpers for routing `winit` events and
/// drawing the aggregated widget tree.
///
/// The goal is to let users write very little boilerplate: they create one
/// `Ui`, add widgets to it, and then in their event loop call
/// `ui.handle_window_event(&event, &mut input_state)` followed by
/// `ui.draw(...)` when rendering. The `Canvas` still does all the hard work
/// (focus, hit tests, etc.), but the application no longer needs to know the
/// details of which methods to call for each type of event.
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
    pub fn register_viewport(&mut self, vp: std::rc::Rc<std::cell::RefCell<crate::viewport_widget::ViewportWidget>>) {
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

    /// Handle a winit window event, updating both the provided `InputState`
    /// (so that the rest of the engine sees the mouse position/keys) and
    /// dispatching the event to the widget tree.
    pub fn handle_window_event(&mut self, event: &WindowEvent, input: &mut InputState) {
        match event {
            WindowEvent::CursorMoved { position, .. } => {
                input.set_mouse_position(position.x, position.y);
                self.canvas.mouse_move(position.x, position.y);
            }
            WindowEvent::MouseInput { state, button, .. } => {
                let pressed = *state == winit::event::ElementState::Pressed;
                input.update_mouse_button(*button, pressed);
                let (mx, my) = input.mouse_position();
                self.canvas.mouse_input(mx, my, pressed);
            }
            WindowEvent::KeyboardInput { event, .. } => {
                let winit::event::KeyEvent { physical_key, state, text, .. } = event;
                if let winit::keyboard::PhysicalKey::Code(code) = physical_key {
                    input.update_key(*code, *state == winit::event::ElementState::Pressed);
                }
                self.canvas.keyboard_input(
                    text.as_deref(),
                    if let winit::keyboard::PhysicalKey::Code(k) = physical_key { Some(*k) } else { None },
                    *state == winit::event::ElementState::Pressed,
                );
            }
            _ => {}
        }
    }

    /// Collect draw commands from the widget tree and convert them into the
    /// provided GUI/text batches. The `font` parameter is passed through to
    /// `RenderCommand::to_batches`, so `None` is valid when no font is
    /// available yet.
    pub fn draw(&self, quad_batch: &mut crate::renderer::GuiBatch, text_batch: &mut crate::renderer::TextBatch, font: Option<&ferrous_assets::font::Font>) {
        let mut cmds = Vec::new();
        self.canvas.collect(&mut cmds);
        for cmd in &cmds {
            cmd.to_batches(quad_batch, text_batch, font);
        }
    }

    /// Mutable access to the underlying canvas in case the user needs to
    /// perform more advanced operations (e.g. remove a widget).
    pub fn canvas_mut(&mut self) -> &mut Canvas {
        &mut self.canvas
    }
}
