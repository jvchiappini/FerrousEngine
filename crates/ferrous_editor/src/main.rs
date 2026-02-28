// header imports
use std::cell::RefCell;
use std::rc::Rc;

use ferrous_app::{App, AppContext, FerrousApp};
use ferrous_assets::font::Font;
use ferrous_gui::Widget;
use ferrous_gui::{GuiBatch, GuiQuad, InteractiveButton, Slider, TextBatch, Ui, ViewportWidget};
use ferrous_renderer::{Renderer, Viewport};

// application state
struct EditorApp {
    /// four helper buttons used to verify corner rounding; ordered
    /// [top-left, top-right, bottom-left, bottom-right]
    corner_buttons: [Rc<RefCell<InteractiveButton>>; 4],
    // sliders and text input removed (legacy)
    ui_viewport: Rc<RefCell<ViewportWidget>>,
    /// example button that rounds multiple corners at once
    combo_button: Rc<RefCell<InteractiveButton>>,

    // Tamaños de paneles dinámicos
    panel_left_w: u32,
    panel_bottom_h: u32,
    // estado auxiliar para detectar clics en el botón
    button_was_pressed: bool,
    // petición de agregar cubo, consumida en draw_3d
    add_cube: bool,
    // informacion de objetos añadidos
    objects: Vec<(String, usize)>, // (name, renderer index)
    // sliders for each objects' x,y,z position
    object_sliders: Vec<[Slider; 3]>,
        // colour picker widget used for demonstration
        color_picker: Rc<RefCell<ferrous_gui::ColorPicker>>,
}

impl Default for EditorApp {
    fn default() -> Self {
        Self {
            corner_buttons: [
                // top-left
                Rc::new(RefCell::new(
                    InteractiveButton::new(50.0, 50.0, 80.0, 80.0).round_tl(20.0),
                )),
                // top-right
                Rc::new(RefCell::new(
                    InteractiveButton::new(150.0, 50.0, 80.0, 80.0).round_tr(20.0),
                )),
                // bottom-left
                Rc::new(RefCell::new(
                    InteractiveButton::new(50.0, 150.0, 80.0, 80.0).round_bl(20.0),
                )),
                // bottom-right
                Rc::new(RefCell::new(
                    InteractiveButton::new(150.0, 150.0, 80.0, 80.0).round_br(20.0),
                )),
            ],
            combo_button: Rc::new(RefCell::new(
                // round both top-left and bottom-right simultaneously
                InteractiveButton::new(250.0, 100.0, 80.0, 80.0).with_radii([20.0, 0.0, 0.0, 20.0]),
            )),
            // sliders/text input not used anymore
            ui_viewport: Rc::new(RefCell::new(ViewportWidget::new(0.0, 0.0, 0.0, 0.0))),
            panel_left_w: 300,
            panel_bottom_h: 200,
            button_was_pressed: false,
            add_cube: false,
            objects: Vec::new(),
            object_sliders: Vec::new(),
                // place colour picker below the corner buttons
                color_picker: Rc::new(RefCell::new(
                    ferrous_gui::ColorPicker::new(50.0, 250.0, 100.0, 100.0)
                        .with_colour([1.0, 0.0, 0.0, 1.0]),
                )),
        }
    }
}

impl FerrousApp for EditorApp {
    fn configure_ui(&mut self, ui: &mut Ui) {
        for btn in &self.corner_buttons {
            ui.add(btn.clone());
        }
        ui.add(self.combo_button.clone());
            ui.add(self.color_picker.clone());
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

        // detect clicks on any of the corner buttons; if the top-left
        // button was released we add a cube (rest are just for testing).
        let pressed = self.corner_buttons[0].borrow().pressed;
        if !pressed && self.button_was_pressed {
            self.add_cube = true;
        }
        self.button_was_pressed = pressed;

        // process slider input manually for each object slider
        let (mx, my) = ctx.input.mouse_position();
        let down = ctx
            .input
            .is_button_down(ferrous_core::input::MouseButton::Left);
        for sliders in &mut self.object_sliders {
            for s in sliders.iter_mut() {
                s.mouse_move(mx, my);
                s.mouse_input(mx, my, down);
            }
        }
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
            radii: [0.0; 4],
        });

        // Panel Inferior
        gui.push(GuiQuad {
            pos: [
                self.panel_left_w as f32,
                (win_h.saturating_sub(self.panel_bottom_h)) as f32,
            ],
            size: [win_w as f32, self.panel_bottom_h as f32],
            color: [0.15, 0.15, 0.15, 1.0],
            radii: [0.0; 4],
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

            // etiquetas de los cuatro botones de esquina para facilitar las pruebas
            text.draw_text(font, "TL", [60.0, 70.0], 16.0, [1.0, 1.0, 1.0, 1.0]);
            text.draw_text(font, "TR", [160.0, 70.0], 16.0, [1.0, 1.0, 1.0, 1.0]);
            text.draw_text(font, "BL", [60.0, 170.0], 16.0, [1.0, 1.0, 1.0, 1.0]);
            text.draw_text(font, "BR", [160.0, 170.0], 16.0, [1.0, 1.0, 1.0, 1.0]);

            // ahora dibujamos el panel de objetos con sus sliders
            // label for combined button
            text.draw_text(font, "TL+BR", [250.0, 90.0], 12.0, [1.0, 1.0, 1.0, 1.0]);
            // also show current colour chosen by the picker just below its square
            let cp = self.color_picker.borrow();
            gui.push(GuiQuad {
                pos: [50.0, 360.0], // place below the circular picker area
                size: [100.0, 20.0],
                color: cp.colour,
                radii: [2.0; 4],
            });

            let mut y_offset = 140.0;
            for (i, (name, _idx)) in self.objects.iter().enumerate() {
                text.draw_text(font, name, [10.0, y_offset], 16.0, [1.0, 1.0, 0.0, 1.0]);
                y_offset += 20.0;
                if let Some(sliders) = self.object_sliders.get_mut(i) {
                    // update slider positions before drawing
                    sliders[0].rect[0] = 10.0;
                    sliders[0].rect[1] = y_offset;
                    sliders[1].rect[0] = 10.0;
                    sliders[1].rect[1] = y_offset + 20.0;
                    sliders[2].rect[0] = 10.0;
                    sliders[2].rect[1] = y_offset + 40.0;
                    sliders[0].draw(gui);
                    sliders[1].draw(gui);
                    sliders[2].draw(gui);
                }
                y_offset += 30.0;
            }
            // diagnostics: show CPU / memory usage of the editor process.
            // we draw a semi‑opaque box first so the text doesn't leave
            // behind artifacts when it changes length.
            let box_width = 300.0;
            let box_height = 22.0;
            gui.push(ferrous_gui::GuiQuad {
                pos: [10.0, (win_h as f32 - box_height)],
                size: [box_width, box_height],
                color: [0.0, 0.0, 0.0, 0.6],
                radii: [0.0; 4],
            });

            let cpu = ferrous_core::get_cpu_usage();
            let ram_mb = ferrous_core::get_ram_usage_mb();
            let virt_mb = ferrous_core::get_virtual_memory_mb();
            // log to console periodically for debugging.
            static mut LAST_LOG: Option<std::time::Instant> = None;
            let now = std::time::Instant::now();
            let should_log = unsafe {
                if let Some(prev) = LAST_LOG {
                    now.duration_since(prev) > std::time::Duration::from_secs(1)
                } else {
                    true
                }
            };
            if should_log {
                unsafe {
                    LAST_LOG = Some(now);
                }
                println!(
                    "[metrics] cpu={}%, ram={}MB, virt={}MB",
                    cpu, ram_mb, virt_mb
                );
            }

            let info = format!("cpu: {cpu:.1}%   ram: {ram_mb:.1} MB   virt: {virt_mb:.1} MB",);
            text.draw_text(
                font,
                &info,
                [10.0, (win_h - 20) as f32],
                12.0,
                [0.8, 0.8, 0.8, 1.0],
            );
        }
                // show current colour selected by picker
                let cp = self.color_picker.borrow();
                let col_rect = GuiQuad {
                    pos: [50.0, 250.0],
                    size: [100.0, 20.0],
                    color: cp.colour,
                    radii: [2.0; 4],
                };
                gui.push(col_rect);
    }

    fn draw_3d(&mut self, renderer: &mut Renderer, _ctx: &mut AppContext) {
        // Cuando se haya solicitado mediante el botón, añadimos un cubo
        if self.add_cube {
            let mesh = ferrous_renderer::mesh::Mesh::cube(&renderer.context.device);
            let idx = renderer.add_object(mesh, ferrous_renderer::glam::Vec3::ZERO);
            let name = format!("Cube {}", self.objects.len() + 1);
            self.objects.push((name, idx));
            // crear sliders con valores iniciales basados en la posición (0)
            let zero = 0.5; // (0 +10)/20
            self.object_sliders.push([
                Slider::new(10.0, 0.0, 150.0, 16.0, zero),
                Slider::new(10.0, 0.0, 150.0, 16.0, zero),
                Slider::new(10.0, 0.0, 150.0, 16.0, zero),
            ]);
            self.add_cube = false;
        }

        // sincronizamos posiciones de sliders con el renderer
        for (i, sliders) in self.object_sliders.iter().enumerate() {
            if i < self.objects.len() {
                let idx = self.objects[i].1;
                let x = sliders[0].value * 20.0 - 10.0;
                let y = sliders[1].value * 20.0 - 10.0;
                let z = sliders[2].value * 20.0 - 10.0;
                renderer.set_object_position(idx, ferrous_renderer::glam::Vec3::new(x, y, z));
            }
        }
    }
}

fn main() {
    App::new(EditorApp::default())
        .with_title("Ferrous Engine - Editor")
        .with_size(1280, 720)
        .with_font("assets/fonts/Roboto-Regular.ttf")
        .run();
}
