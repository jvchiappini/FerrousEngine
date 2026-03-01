use ferrous_core::{InputState, RenderStats, Time, Viewport, World};
use ferrous_core::glam::Vec3;
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

    /// Per-frame renderer statistics (vertices, triangles, draw calls).
    /// Populated at the start of every `draw_3d` call; zero before the first frame.
    pub render_stats: RenderStats,

    /// World-space position of the camera eye this frame.
    /// Populated at the start of every `draw_3d` call; `Vec3::ZERO` until then.
    pub camera_eye: Vec3,

    // ── Read-write ─────────────────────────────────────────────────────────
    /// The scene graph.  Modify this in `update()` and `ferrous_app` will
    /// automatically call `renderer.sync_world` at the right moment.
    pub world: &'a mut World,

    /// Area of the window dedicated to 3-D rendering.  Set this in `update()`
    /// to control where the 3-D view appears; the runner will propagate it to
    /// the renderer and UI viewport.
    pub viewport: Viewport,

    /// Set to `true` via [`request_exit`] to stop the event loop gracefully.
    pub(crate) exit_requested: bool,
}

impl<'a> AppContext<'a> {
    /// Signal the event loop to shut down after the current frame.
    pub fn request_exit(&mut self) {
        self.exit_requested = true;
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
