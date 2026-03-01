use crate::context::AppContext;
use ferrous_assets::font::Font;
use ferrous_gui::{GuiBatch, TextBatch, Ui};
use ferrous_renderer::Renderer;

/// The core trait that every FerrousApp application or game implements.
///
/// All methods have empty default implementations so you only override what
/// you need.  A minimal "hello world" needs zero methods; a full 3-D game
/// might override `setup`, `update`, and `draw_3d`.
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
    /// Called after `update`. Use `gui` for shapes/images and `text` for
    /// text rendering.
    fn draw_ui(
        &mut self,
        gui: &mut GuiBatch,
        text: &mut TextBatch,
        font: Option<&Font>,
        ctx: &mut AppContext,
    ) {
    }

    /// Emit 3-D draw commands for this frame.
    ///
    /// This is called before `draw_ui`, so the 3-D scene is rendered beneath
    /// all 2-D overlays.  The `world` is synced to the renderer automatically
    /// by the runner; you can use this method for additional imperative draw
    /// calls (particles, debug lines, etc.).
    fn draw_3d(&mut self, renderer: &mut Renderer, ctx: &mut AppContext) {}

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

