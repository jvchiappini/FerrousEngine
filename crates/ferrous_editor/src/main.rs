// header imports
use std::cell::RefCell;
use std::rc::Rc;

use ferrous_app::{App, AppContext, FerrousApp};
use ferrous_assets::font::Font;
use ferrous_core::scene::World;
// `Widget` not needed in minimal UI
use ferrous_gui::{GuiBatch, InteractiveButton, TextBatch, Ui, ViewportWidget};
// random positioning
use ferrous_renderer::{Renderer, Viewport};
use rand::Rng;

// application state
struct EditorApp {
    /// a single button that requests a cube be added
    add_button: Rc<RefCell<InteractiveButton>>,
    /// viewport widget where the 3D scene is rendered
    ui_viewport: Rc<RefCell<ViewportWidget>>,
    // auxiliary state for detecting button clicks
    button_was_pressed: bool,
    // request to add cube, consumed in draw_3d
    add_cube: bool,
    // the ECS world; entities that have a cube component live here
    world: World,
}

impl Default for EditorApp {
    fn default() -> Self {
        Self {
            // simple rectangular button placed near top-left
            add_button: Rc::new(RefCell::new(InteractiveButton::new(
                10.0, 10.0, 100.0, 30.0,
            ))),
            ui_viewport: Rc::new(RefCell::new(ViewportWidget::new(0.0, 0.0, 0.0, 0.0))),
            button_was_pressed: false,
            add_cube: false,
            world: World::new(),
        }
    }
}

impl FerrousApp for EditorApp {
    fn configure_ui(&mut self, ui: &mut Ui) {
        ui.add(self.add_button.clone());
        ui.register_viewport(self.ui_viewport.clone());
    }

    fn update(&mut self, ctx: &mut AppContext) {
        let (win_w, win_h) = ctx.window_size;

        // Aquí definimos DÓNDE va a estar el 3D.
        // El motor leerá esto y ajustará la cámara wgpu automáticamente.
        // use entire window for 3d viewport
        ctx.viewport = Viewport {
            x: 0,
            y: 0,
            width: win_w,
            height: win_h,
        };

        // detect click on our single "add cube" button
        let pressed = self.add_button.borrow().pressed;
        if !pressed && self.button_was_pressed {
            self.add_cube = true;
        }
        self.button_was_pressed = pressed;
    }

    fn draw_ui(
        &mut self,
        _gui: &mut GuiBatch,
        text: &mut TextBatch,
        font: Option<&Font>,
        _ctx: &mut AppContext,
    ) {
        // draw only a simple label for the button
        if let Some(font) = font {
            text.draw_text(font, "Add Cube", [15.0, 15.0], 16.0, [1.0, 1.0, 1.0, 1.0]);
        }
    }

    fn draw_3d(&mut self, renderer: &mut Renderer, _ctx: &mut AppContext) {
        // Cuando se haya solicitado mediante el botón, añadimos un cubo
        if self.add_cube {
            // create element (cube) in the scene world; renderer will be
            // updated below via `sync_world`.
            let handle = self
                .world
                .add_cube(ferrous_core::elements::cube::Cube::default());
            // place it randomly within a small region in front of the camera
            let mut rng = rand::thread_rng();
            let offset_x = (rng.gen::<f32>() - 0.5) * 2.0; // +/-1
            let offset_y = (rng.gen::<f32>() - 0.5) * 2.0; // +/-1
                                                           // assume camera looks down -Z; place cubes roughly 4..6 units ahead
            let offset_z = -5.0 + (rng.gen::<f32>() - 0.5) * 1.0;
            // use renderer's camera if available; otherwise default to origin
            let base = renderer.camera.eye;
            self.world.set_position(
                handle,
                ferrous_renderer::glam::Vec3::new(
                    base.x + offset_x,
                    base.y + offset_y,
                    base.z + offset_z,
                ),
            );
            self.add_cube = false;
        }

        // make sure the renderer knows about new/changed cubes
        renderer.sync_world(&mut self.world);
    }
}

fn main() {
    App::new(EditorApp::default())
        .with_title("Ferrous Engine - Editor")
        .with_size(1280, 720)
        .with_font("assets/fonts/Roboto-Regular.ttf")
        .run();
}
