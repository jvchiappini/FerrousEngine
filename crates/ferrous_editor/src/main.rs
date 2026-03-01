use std::cell::RefCell;
use std::rc::Rc;

use ferrous_app::{App, AppContext, Color, FerrousApp, Handle, Vec3};
use ferrous_assets::font::Font;
use ferrous_gui::{GuiBatch, InteractiveButton, TextBatch, Ui, ViewportWidget};
use ferrous_renderer::{Renderer, Viewport};
use rand::Rng;

/// Application state for the Ferrous Engine editor.
struct EditorApp {
    /// Button that requests a new cube be added to the scene.
    add_button: Rc<RefCell<InteractiveButton>>,
    /// 3-D viewport widget embedded in the GUI.
    ui_viewport: Rc<RefCell<ViewportWidget>>,
    /// Tracks previous pressed state so we trigger on *release*.
    button_was_pressed: bool,
    /// Set in `update`, consumed in `draw_3d`.
    add_cube: bool,
    /// Handle of the most recently added cube (for demonstration).
    last_cube: Option<Handle>,
}

impl Default for EditorApp {
    fn default() -> Self {
        Self {
            add_button: Rc::new(RefCell::new(InteractiveButton::new(
                10.0, 10.0, 120.0, 32.0,
            ))),
            ui_viewport: Rc::new(RefCell::new(ViewportWidget::new(0.0, 0.0, 0.0, 0.0))),
            button_was_pressed: false,
            add_cube: false,
            last_cube: None,
        }
    }
}

impl FerrousApp for EditorApp {
    fn configure_ui(&mut self, ui: &mut Ui) {
        ui.add(self.add_button.clone());
        ui.register_viewport(self.ui_viewport.clone());
    }

    fn setup(&mut self, ctx: &mut AppContext) {
        // Spawn a default cube so the viewport isn't empty at start.
        ctx.world.spawn_cube("Default Cube", Vec3::ZERO);
    }

    fn update(&mut self, ctx: &mut AppContext) {
        let (win_w, win_h) = ctx.window_size;

        // Use the whole window for the 3-D viewport.
        ctx.viewport = Viewport {
            x: 0,
            y: 0,
            width: win_w,
            height: win_h,
        };

        // Detect button click (trigger on release).
        let pressed = self.add_button.borrow().pressed;
        if !pressed && self.button_was_pressed {
            self.add_cube = true;
        }
        self.button_was_pressed = pressed;

        // Press Escape to quit.
        if ctx.input.just_pressed(ferrous_app::KeyCode::Escape) {
            ctx.request_exit();
        }
    }

    fn draw_ui(
        &mut self,
        _gui: &mut GuiBatch,
        text: &mut TextBatch,
        font: Option<&Font>,
        ctx: &mut AppContext,
    ) {
        if let Some(font) = font {
            text.draw_text(font, "Add Cube", [15.0, 16.0], 16.0, [1.0, 1.0, 1.0, 1.0]);

            // Show a small HUD with frame timing.
            let fps_str = format!("FPS: {:.0}", ctx.time.fps);
            let elapsed_str = format!("t = {:.1}s", ctx.time.elapsed);
            text.draw_text(font, &fps_str, [15.0, 52.0], 14.0, [0.8, 0.8, 0.8, 1.0]);
            text.draw_text(font, &elapsed_str, [15.0, 70.0], 14.0, [0.6, 0.6, 0.6, 1.0]);
        }
    }

    fn draw_3d(&mut self, renderer: &mut Renderer, ctx: &mut AppContext) {
        if self.add_cube {
            let mut rng = rand::thread_rng();
            let base = renderer.camera.eye;
            let pos = Vec3::new(
                base.x + (rng.gen::<f32>() - 0.5) * 2.0,
                base.y + (rng.gen::<f32>() - 0.5) * 2.0,
                base.z - 5.0 + (rng.gen::<f32>() - 0.5),
            );

            // Spawn via the world in AppContext — the runner syncs automatically.
            let handle = ctx.world.spawn_cube("Cube", pos);

            // Give each new cube a random tint.
            let color = Color::from_rgb8(
                rng.gen_range(100..=255),
                rng.gen_range(100..=255),
                rng.gen_range(100..=255),
            );
            ctx.world.set_color(handle, color);

            self.last_cube = Some(handle);
            self.add_cube = false;
        }
    }

    fn on_resize(&mut self, new_size: (u32, u32), ctx: &mut AppContext) {
        // Keep the 3-D viewport covering the whole window after resize.
        ctx.viewport = Viewport {
            x: 0,
            y: 0,
            width: new_size.0,
            height: new_size.1,
        };
    }
}

fn main() {
    App::new(EditorApp::default())
        .with_target_fps(60)
        .with_msaa(1)
        .with_title("Ferrous Engine — Editor")
        .with_size(1280, 720)
        .with_font("assets/fonts/Roboto-Regular.ttf")
        .with_background_color(Color::rgb(0.08, 0.08, 0.10))
        .run();
}
