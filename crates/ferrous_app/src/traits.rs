use crate::context::AppContext;
use ferrous_assets::Font;
use ferrous_gui::{GuiBatch, TextBatch, Ui};

/// Aggregates all mutable drawing resources passed to [`FerrousApp::draw_ui`].
pub struct DrawContext<'a, 'b> {
    pub gui: &'a mut GuiBatch,
    pub text: &'a mut TextBatch,
    pub font: &'a Font,
    pub ctx: &'a mut AppContext<'b>,
}

/// The core trait that every FerrousApp application or game implements.
///
/// All methods have empty default implementations so you only override what
/// you need.  The runner only calls the callbacks that make sense for the
/// active [`crate::AppMode`]:
///
/// | Callback | `Desktop2D` | `Game2D` | `Game3D` |
/// |----------|:-----------:|:--------:|:--------:|
/// | `setup` / `update` / `draw_ui` | ✓ | ✓ | ✓ |
/// | `draw_3d` | ✗ | ✗ | ✓ |
/// | ECS world sync | ✗ | ✗ | ✓ |
///
/// A minimal "hello world" GUI tool needs zero methods and
/// `App::new(MyTool).with_mode(AppMode::Desktop2D).run()`.
/// A full 3-D game uses the default [`AppMode::Game3D`] and overrides
/// `setup`, `update`, and optionally `draw_3d`.
///
/// ```rust,ignore
/// struct MyGame { speed: f32 }
///
/// impl FerrousApp for MyGame {
///     fn setup(&mut self, ctx: &mut AppContext) {
///         ctx.world.spawn_cube("Player", Vec3::ZERO);
///     }
///
///     fn update(&mut self, ctx: &mut AppContext) {
///         if ctx.input.key_held(KeyCode::Escape) {
///             ctx.request_exit();
///         }
///     }
/// }
/// ```
#[allow(unused_variables)]
pub trait FerrousApp {
    /// Called once after the window and GPU are ready.
    ///
    /// Use this to spawn initial entities, load assets, or configure the
    /// camera.
    fn setup(&mut self, ctx: &mut AppContext) {}

    /// Called every frame before rendering.
    ///
    /// This is where game logic and scene mutations go.  The `world` on `ctx`
    /// is already populated and will be synced to the renderer automatically
    /// after this call returns.
    fn update(&mut self, ctx: &mut AppContext) {}

    /// Register persistent GUI widgets (called once, during `resumed`).
    ///
    /// Use `ui.add(widget)` / `ui.register_viewport(widget)` here.
    fn configure_ui(&mut self, ui: &mut Ui) {}

    /// Emit 2-D draw commands for this frame.
    ///
    /// Called after `update`. Use `dc.gui` for shapes/images and `dc.text`
    /// for text rendering.
    fn draw_ui(&mut self, dc: &mut DrawContext<'_, '_>) {}

    /// Emit 3-D draw commands for this frame.
    ///
    /// This is called before `draw_ui`, so the 3-D scene is rendered beneath
    /// all 2-D overlays.  The world is synced to the renderer automatically
    /// by the runner before this call.
    ///
    /// `ctx.render_stats` contains statistics from the previous frame.
    /// `ctx.camera_eye` contains the world-space camera position this frame.
    fn draw_3d(&mut self, ctx: &mut AppContext) {}

    /// Called whenever the window is resized.
    ///
    /// The new physical pixel dimensions are in `new_size`.  The runner
    /// already updates the swap-chain and camera aspect before calling this.
    fn on_resize(&mut self, new_size: (u32, u32), ctx: &mut AppContext) {}

    /// Called for every raw winit `WindowEvent` (after it has been processed
    /// by the GUI system).
    ///
    /// Use this for drag-and-drop, IME, or any event not covered by the
    /// helpers on `AppContext`.
    fn on_window_event(&mut self, event: &winit::event::WindowEvent, ctx: &mut AppContext) {}
}
