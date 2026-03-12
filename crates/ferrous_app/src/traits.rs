use crate::context::AppContext;
use ferrous_assets::Font;
use ferrous_gui::GuiBatch;
use ferrous_ui_core::UiTree;

/// Aggregates all mutable drawing resources passed to [`FerrousApp::draw_ui`].
pub struct DrawContext<'a, 'b> {
    pub gui: &'a mut GuiBatch,
    pub font: &'a Font,
    pub ctx: &'a mut AppContext<'b>,
}

// ---------------------------------------------------------------------------
// Convenience drawing helpers attached to `DrawContext`.
// ---------------------------------------------------------------------------

impl<'a, 'b> DrawContext<'a, 'b> {
    /// Draws a labelled text-input row similar to the property panel in
    /// GUIMaker.  This is just a thin wrapper around `GuiBatch::draw_text_field`
    /// that handles drawing the label and computing the correct textbox size.
    ///
    /// The caller is responsible for updating the cursor/selection state
    /// (for example by using `TextFieldState` from `ferrous_ui_core`).
    pub fn draw_text_input_row(
        &mut self,
        x: f32,
        y: f32,
        right_w: f32,
        row_pad_x: f32,
        label_w: f32,
        row_h: f32,
        label: &str,
        value: &str,
        focused: bool,
        cursor_visible: bool,
        cursor_pos: usize,
        selection: Option<(usize, usize)>,
        all_selected: bool,
        text_color: [f32; 4],
        bg_color: [f32; 4],
        border_color: Option<[f32; 4]>,
        sel_color: [f32; 4],
    ) {
        use crate::Color as AppColor;

        // draw label text
        self.gui.draw_text(
            self.font,
            label,
            [x + row_pad_x, y + (row_h - 10.0) * 0.5],
            10.0,
            AppColor::hex("#CCCCCC").to_linear_f32(),
        );

        let val_x = x + row_pad_x + label_w;
        let val_w = right_w - row_pad_x * 2.0 - label_w;
        let effective_sel = if all_selected && !value.is_empty() {
            Some((0, value.len()))
        } else {
            selection
        };

        // use colors provided by caller rather than hardcoding
        let bg = bg_color;
        let text_col = text_color;
        let border = border_color;
        let sel_col = sel_color;

        self.gui.draw_text_field(
            self.font,
            val_x,
            y + 3.0,
            val_w,
            row_h - 6.0,
            value,
            10.0,
            focused,
            cursor_visible,
            cursor_pos,
            effective_sel,
            text_col,
            bg,
            border,
            [0.0f32, 0.47, 0.83, 0.35],
            4.0,
        );
    }
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
    fn configure_ui(&mut self, ui: &mut UiTree<Self>) where Self: Sized {}

    /// Emit 2-D draw commands for this frame.
    ///
    /// Called after `update`. Use `dc.gui` for shapes, images and text rendering.
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
