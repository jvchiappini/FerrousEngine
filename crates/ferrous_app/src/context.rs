use ferrous_core::{InputState, Time, World};
use ferrous_renderer::Viewport;
use winit::window::Window;

/// Per-frame context passed to every [`FerrousApp`] callback.
///
/// `AppContext` bundles together all the read/write access a game or app
/// typically needs in one place, so method signatures stay simple:
///
/// ```rust,ignore
/// fn update(&mut self, ctx: &mut AppContext) {
///     ctx.world.set_position(self.player, ctx.time.delta * speed);
///     if ctx.input.just_pressed(KeyCode::Escape) {
///         ctx.request_exit();
///     }
/// }
/// ```
pub struct AppContext<'a> {
    // ── Read-only ──────────────────────────────────────────────────────────
    /// Keyboard and mouse state for this frame.
    pub input: &'a InputState,

    /// Frame timing: delta, elapsed, FPS.
    pub time: Time,

    /// Current window size in physical pixels.
    pub window_size: (u32, u32),

    /// The native window handle (for creating surfaces, grabbing cursor, etc.)
    pub window: &'a Window,

    // ── Read-write ─────────────────────────────────────────────────────────
    /// The scene graph.  Modify this in `update()` and `ferrous_app` will
    /// automatically call `renderer.sync_world` at the right moment.
    pub world: &'a mut World,

    /// Area of the window dedicated to 3-D rendering.  Set this in `update()`
    /// to control where the 3-D view appears; the runner will propagate it to
    /// the renderer and UI viewport.
    pub viewport: Viewport,

    /// Optional mutable renderer access.  Available in `setup`, `update`, and
    /// `draw_3d`; may be `None` in early lifecycle callbacks.
    pub renderer: Option<&'a mut ferrous_renderer::Renderer>,

    /// Set to `true` via [`request_exit`] to stop the event loop gracefully.
    pub(crate) exit_requested: bool,
}

impl<'a> AppContext<'a> {
    /// Signal the event loop to shut down after the current frame.
    pub fn request_exit(&mut self) {
        self.exit_requested = true;
    }

    /// Convenience accessor returning a mutable reference to the renderer if
    /// available.
    pub fn renderer(&mut self) -> Option<&mut ferrous_renderer::Renderer> {
        self.renderer.as_deref_mut()
    }

    /// Shortcut: window width in physical pixels.
    #[inline]
    pub fn width(&self) -> u32 {
        self.window_size.0
    }

    /// Shortcut: window height in physical pixels.
    #[inline]
    pub fn height(&self) -> u32 {
        self.window_size.1
    }

    /// Aspect ratio (width / height). Returns 1.0 if height is zero.
    #[inline]
    pub fn aspect(&self) -> f32 {
        let (w, h) = self.window_size;
        if h == 0 { 1.0 } else { w as f32 / h as f32 }
    }
}
