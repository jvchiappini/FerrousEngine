use std::cell::RefCell;
use std::rc::Rc;

use ferrous_app::{App, AppContext, FerrousApp};
use ferrous_assets::font::Font;
use ferrous_gui::{
    GuiBatch, GuiQuad, InteractiveButton, Slider, TextBatch, TextInput, Ui, ViewportWidget,
};
use ferrous_renderer::{Renderer, Viewport};

struct EditorApp {
    ui_button: Rc<RefCell<InteractiveButton>>,
    ui_slider: Rc<RefCell<Slider>>,
    ui_text_input: Rc<RefCell<TextInput>>,
    ui_viewport: Rc<RefCell<ViewportWidget>>,

    // Tamaños de paneles dinámicos
    panel_left_w: u32,
    panel_bottom_h: u32,
    // estado auxiliar para detectar clics en el botón
    button_was_pressed: bool,
    // petición de agregar cubo, consumida en draw_3d
    add_cube: bool,
}

impl Default for EditorApp {
    fn default() -> Self {
        Self {
            ui_button: Rc::new(RefCell::new(InteractiveButton::new(
                50.0, 50.0, 100.0, 100.0,
            ))),
            ui_slider: Rc::new(RefCell::new(Slider::new(50.0, 200.0, 200.0, 20.0, 0.5))),
            ui_text_input: Rc::new(RefCell::new(TextInput::new(50.0, 240.0, 200.0, 24.0))),
            ui_viewport: Rc::new(RefCell::new(ViewportWidget::new(0.0, 0.0, 0.0, 0.0))),
            panel_left_w: 300,
            panel_bottom_h: 200,
            button_was_pressed: false,
            add_cube: false,
        }
    }
}

impl FerrousApp for EditorApp {
    fn configure_ui(&mut self, ui: &mut Ui) {
        ui.add(self.ui_button.clone());
        ui.add(self.ui_slider.clone());
        ui.add(self.ui_text_input.clone());
        ui.register_viewport(self.ui_viewport.clone());
    }

    fn update(&mut self, ctx: &mut AppContext) {
        let (win_w, win_h) = ctx.window_size;

        // Aquí definimos DÓNDE va a estar el 3D.
        // El motor leerá esto y ajustará la cámara wgpu automáticamente.
        ctx.viewport = Viewport {
            x: self.panel_left_w,
            y: 0,
            width: win_w.saturating_sub(self.panel_left_w),
            height: win_h.saturating_sub(self.panel_bottom_h),
        };

        // detectamos un clic completo en el botón (pressed -> released)
        let pressed = self.ui_button.borrow().pressed;
        if !pressed && self.button_was_pressed {
            // el botón fue soltado, generamos la solicitud de cubo
            self.add_cube = true;
        }
        self.button_was_pressed = pressed;
    }

    fn draw_ui(
        &mut self,
        gui: &mut GuiBatch,
        text: &mut TextBatch,
        font: Option<&Font>,
        ctx: &mut AppContext,
    ) {
        let (win_w, win_h) = ctx.window_size;

        // Panel Izquierdo
        gui.push(GuiQuad {
            pos: [0.0, 0.0],
            size: [self.panel_left_w as f32, win_h as f32],
            color: [0.12, 0.12, 0.12, 1.0],
        });

        // Panel Inferior
        gui.push(GuiQuad {
            pos: [
                self.panel_left_w as f32,
                (win_h.saturating_sub(self.panel_bottom_h)) as f32,
            ],
            size: [win_w as f32, self.panel_bottom_h as f32],
            color: [0.15, 0.15, 0.15, 1.0],
        });

        // Textos
        if let Some(font) = font {
            text.draw_text(
                font,
                "Ferrous Editor",
                [10.0, 10.0],
                24.0,
                [1.0, 1.0, 1.0, 1.0],
            );
            // dibujo del texto del botón
            text.draw_text(
                font,
                "Add cube",
                [55.0, 80.0],
                18.0,
                [1.0, 1.0, 1.0, 1.0],
            );
            let slider_val = self.ui_slider.borrow().value;
            text.draw_text(
                font,
                &format!("Slider: {:.2}", slider_val),
                [10.0, 90.0],
                18.0,
                [0.8, 0.8, 0.8, 1.0],
            );
        }
    }

    fn draw_3d(&mut self, renderer: &mut Renderer, _ctx: &mut AppContext) {
        // Cuando se haya solicitado mediante el botón, añadimos un cubo
        if self.add_cube {
            // el cubo se crea usando el dispositivo que tiene el renderer
            let mesh = ferrous_renderer::mesh::Mesh::cube(&renderer.context.device);
            renderer.add_mesh(mesh);
            self.add_cube = false;
        }

        // otras cargas 3D irían aquí
    }
}

fn main() {
    App::new(EditorApp::default())
        .with_title("Ferrous Engine - Editor")
        .with_size(1280, 720)
        .with_font("assets/fonts/Roboto-Regular.ttf")
        .run();
}
