use std::cell::RefCell;
use std::rc::Rc;

use ferrous_app::{App, AppContext, Color, FerrousApp, Handle, RenderStats, Vec3, Viewport};
use ferrous_assets::font::Font;
use ferrous_gui::{GuiBatch, InteractiveButton, Slider, TextBatch, Ui, ViewportWidget};
use rand::Rng;

/// How many cubos se spawnean por frame durante el benchmark.
const BENCHMARK_BATCH: u32 = 200;
/// FPS mínimo antes de parar el benchmark (se evalúa sobre la media).
const BENCHMARK_MIN_FPS: f32 = 60.0;
/// Número de frames sobre los que se calcula la media deslizante de FPS.
const FPS_WINDOW: usize = 60;

// ─── Slider helpers ────────────────────────────────────────────────────────

/// Minimum and maximum cube dimension (world units) for the size sliders.
const SLIDER_MIN: f32 = 0.1;
const SLIDER_MAX: f32 = 5.0;

/// Convert a world-unit size to a normalized slider value [0, 1].
fn slider_norm(size: f32) -> f32 {
    ((size - SLIDER_MIN) / (SLIDER_MAX - SLIDER_MIN)).clamp(0.0, 1.0)
}

/// Convert a normalized slider value [0, 1] to a world-unit size.
fn slider_to_size(v: f32) -> f32 {
    SLIDER_MIN + v * (SLIDER_MAX - SLIDER_MIN)
}

/// Estado del benchmark.
#[derive(Debug, Clone, PartialEq)]
enum BenchmarkState {
    Idle,
    Running,
    Finished,
}

/// Application state for the Ferrous Engine editor.
struct EditorApp {
    /// Button that requests a new cube be added to the scene.
    add_button: Rc<RefCell<InteractiveButton>>,
    /// Button that starts the benchmark.
    bench_button: Rc<RefCell<InteractiveButton>>,
    /// 3-D viewport widget embedded in the GUI.
    ui_viewport: Rc<RefCell<ViewportWidget>>,
    /// Tracks previous pressed state so we trigger on *release*.
    button_was_pressed: bool,
    bench_button_was_pressed: bool,
    /// Set in `update`, consumed in `draw_3d`.
    add_cube: bool,
    /// Handle of the most recently added cube (for demonstration).
    last_cube: Option<Handle>,

    // --- benchmark ---
    bench_state: BenchmarkState,
    bench_cube_count: u32,
    bench_peak_cubes: u32,
    bench_stopped_fps: f32,

    // --- rolling FPS average ---
    /// Circular buffer de muestras de FPS.
    fps_history: Vec<f32>,
    /// Índice de escritura en el buffer circular.
    fps_history_idx: usize,
    /// Media calculada cada frame.
    fps_avg: f32,

    // --- render stats (captured in draw_3d, displayed in draw_ui) ---
    cached_render_stats: RenderStats,

    // --- cube size sliders (W / H / D) ---
    /// Width slider for the selected cube (normalized 0..1 → 0.1..5.0)
    slider_w: Rc<RefCell<Slider>>,
    /// Height slider for the selected cube.
    slider_h: Rc<RefCell<Slider>>,
    /// Depth slider for the selected cube.
    slider_d: Rc<RefCell<Slider>>,
}

impl Default for EditorApp {
    fn default() -> Self {
        Self {
            add_button: Rc::new(RefCell::new(InteractiveButton::new(
                10.0, 10.0, 120.0, 32.0,
            ))),
            bench_button: Rc::new(RefCell::new(InteractiveButton::new(
                10.0, 50.0, 150.0, 32.0,
            ))),
            ui_viewport: Rc::new(RefCell::new(ViewportWidget::new(0.0, 0.0, 0.0, 0.0))),
            button_was_pressed: false,
            bench_button_was_pressed: false,
            add_cube: false,
            last_cube: None,
            bench_state: BenchmarkState::Idle,
            bench_cube_count: 0,
            bench_peak_cubes: 0,
            bench_stopped_fps: 0.0,
            fps_history: vec![0.0; FPS_WINDOW],
            fps_history_idx: 0,
            fps_avg: 0.0,
            cached_render_stats: RenderStats::default(),
            // Start all three sliders at mid-range (maps to 1.0 world unit).
            slider_w: Rc::new(RefCell::new(Slider::new(10.0, 220.0, 160.0, 16.0, slider_norm(1.0)))),
            slider_h: Rc::new(RefCell::new(Slider::new(10.0, 248.0, 160.0, 16.0, slider_norm(1.0)))),
            slider_d: Rc::new(RefCell::new(Slider::new(10.0, 276.0, 160.0, 16.0, slider_norm(1.0)))),
        }
    }
}

impl FerrousApp for EditorApp {
    fn configure_ui(&mut self, ui: &mut Ui) {
        ui.add(self.add_button.clone());
        ui.add(self.bench_button.clone());
        ui.register_viewport(self.ui_viewport.clone());
        ui.add(self.slider_w.clone());
        ui.add(self.slider_h.clone());
        ui.add(self.slider_d.clone());
    }

    fn setup(&mut self, ctx: &mut AppContext) {
        // Spawn a default cube so the viewport isn't empty at start.
        ctx.world.spawn_cube("Default Cube", Vec3::ZERO);
    }

    fn update(&mut self, ctx: &mut AppContext) {
        // Actualizar la media deslizante de FPS.
        self.fps_history[self.fps_history_idx] = ctx.time.fps;
        self.fps_history_idx = (self.fps_history_idx + 1) % FPS_WINDOW;
        self.fps_avg = self.fps_history.iter().sum::<f32>() / FPS_WINDOW as f32;

        let (win_w, win_h) = ctx.window_size;

        // Use the whole window for the 3-D viewport.
        ctx.viewport = Viewport {
            x: 0,
            y: 0,
            width: win_w,
            height: win_h,
        };

        // Detect "Add Cube" button click (trigger on release).
        let pressed = self.add_button.borrow().pressed;
        if !pressed && self.button_was_pressed {
            self.add_cube = true;
        }
        self.button_was_pressed = pressed;

        // Detect "Benchmark" button click.
        let bench_pressed = self.bench_button.borrow().pressed;
        if !bench_pressed && self.bench_button_was_pressed {
            match self.bench_state {
                BenchmarkState::Idle | BenchmarkState::Finished => {
                    self.bench_state = BenchmarkState::Running;
                    self.bench_cube_count = 0;
                    self.bench_peak_cubes = 0;
                    self.bench_stopped_fps = 0.0;
                }
                BenchmarkState::Running => {
                    // Allow manual stop too.
                    self.bench_state = BenchmarkState::Finished;
                    self.bench_stopped_fps = ctx.time.fps;
                }
            }
        }
        self.bench_button_was_pressed = bench_pressed;

        // If benchmark is running and avg FPS is still above threshold, queue more cubes.
        // Usamos la media para evitar paradas prematuras por spikes puntuales.
        if self.bench_state == BenchmarkState::Running {
            if self.fps_avg >= BENCHMARK_MIN_FPS {
                self.bench_cube_count += BENCHMARK_BATCH;
            } else {
                self.bench_stopped_fps = self.fps_avg;
                self.bench_peak_cubes = self.bench_cube_count;
                self.bench_state = BenchmarkState::Finished;
            }
        }

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

            // Benchmark button label.
            let bench_label = match self.bench_state {
                BenchmarkState::Idle | BenchmarkState::Finished => "Benchmark",
                BenchmarkState::Running => "Stop Bench",
            };
            text.draw_text(font, bench_label, [15.0, 57.0], 16.0, [1.0, 1.0, 1.0, 1.0]);

            // HUD — FPS instantáneo + media deslizante.
            let fps_str = format!("FPS: {:.0}  avg: {:.0}", ctx.time.fps, self.fps_avg);
            text.draw_text(font, &fps_str, [15.0, 92.0], 14.0, [0.8, 0.8, 0.8, 1.0]);

            // Render stats panel.
            let stats = &self.cached_render_stats;
            let verts = stats.vertex_count;
            let tris = stats.triangle_count;
            let dcs = stats.draw_calls;
            let verts_str = if verts >= 1_000_000 {
                format!("Verts: {:.1}M", verts as f32 / 1_000_000.0)
            } else if verts >= 1_000 {
                format!("Verts: {:.1}K", verts as f32 / 1_000.0)
            } else {
                format!("Verts: {}", verts)
            };
            let tris_str = if tris >= 1_000_000 {
                format!("Tris: {:.1}M", tris as f32 / 1_000_000.0)
            } else if tris >= 1_000 {
                format!("Tris: {:.1}K", tris as f32 / 1_000.0)
            } else {
                format!("Tris: {}", tris)
            };
            let dc_str = format!("Draw calls: {}", dcs);
            text.draw_text(font, &verts_str, [15.0, 112.0], 13.0, [0.5, 0.9, 1.0, 1.0]);
            text.draw_text(font, &tris_str, [15.0, 128.0], 13.0, [0.5, 0.9, 1.0, 1.0]);
            text.draw_text(font, &dc_str, [15.0, 144.0], 13.0, [0.5, 0.9, 1.0, 1.0]);

            // Benchmark status HUD.
            match self.bench_state {
                BenchmarkState::Idle => {}
                BenchmarkState::Running => {
                    let live = format!(
                        "Cubes: {}  (+{}·frame)",
                        self.bench_cube_count, BENCHMARK_BATCH
                    );
                    text.draw_text(font, &live, [15.0, 164.0], 14.0, [0.4, 1.0, 0.4, 1.0]);
                    let threshold = format!("Stops at avg < {:.0} FPS", BENCHMARK_MIN_FPS);
                    text.draw_text(font, &threshold, [15.0, 182.0], 12.0, [0.6, 0.6, 0.6, 1.0]);
                }
                BenchmarkState::Finished => {
                    let result = format!("Peak cubes: {}", self.bench_peak_cubes);
                    text.draw_text(font, &result, [15.0, 164.0], 14.0, [1.0, 0.8, 0.2, 1.0]);
                    let fps_drop = format!("Avg FPS at stop: {:.1}", self.bench_stopped_fps);
                    text.draw_text(font, &fps_drop, [15.0, 182.0], 12.0, [1.0, 0.5, 0.3, 1.0]);
                }
            }

            // Cube size sliders — only displayed when a cube is selected.
            if self.last_cube.map(|h| ctx.world.contains(h)).unwrap_or(false) {
                let w = slider_to_size(self.slider_w.borrow().value);
                let h = slider_to_size(self.slider_h.borrow().value);
                let d = slider_to_size(self.slider_d.borrow().value);
                text.draw_text(font, &format!("W: {:.2}", w), [15.0, 210.0], 13.0, [0.9, 0.9, 0.5, 1.0]);
                text.draw_text(font, &format!("H: {:.2}", h), [15.0, 238.0], 13.0, [0.9, 0.9, 0.5, 1.0]);
                text.draw_text(font, &format!("D: {:.2}", d), [15.0, 266.0], 13.0, [0.9, 0.9, 0.5, 1.0]);
            }
        }
    }

    fn draw_3d(&mut self, ctx: &mut AppContext) {
        // Capture render stats from the previous frame.
        self.cached_render_stats = ctx.render_stats;

        // Apply size sliders to the selected cube.
        if let Some(handle) = self.last_cube {
            if ctx.world.contains(handle) {
                let w = slider_to_size(self.slider_w.borrow().value);
                let h = slider_to_size(self.slider_h.borrow().value);
                let d = slider_to_size(self.slider_d.borrow().value);
                ctx.world.set_cube_size(handle, Vec3::new(w, h, d));
            }
        }

        let mut rng = rand::thread_rng();

        // Manual "Add Cube" button.
        if self.add_cube {
            let base = ctx.camera_eye;
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

        // Benchmark: spawn a batch of cubes this frame.
        if self.bench_state == BenchmarkState::Running {
            for _ in 0..BENCHMARK_BATCH {
                let pos = Vec3::new(
                    (rng.gen::<f32>() - 0.5) * 5.0,
                    (rng.gen::<f32>() - 0.5) * 5.0,
                    -(rng.gen::<f32>() * 10.0) - 5.0,
                );
                let handle = ctx.world.spawn_cube("BenchCube", pos);
                let color = Color::from_rgb8(
                    rng.gen_range(80..=255),
                    rng.gen_range(80..=255),
                    rng.gen_range(80..=255),
                );
                ctx.world.set_color(handle, color);
            }
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
        .with_target_fps(Some(240))
        .with_idle_timeout(Some(0.5))
        .with_msaa(1)
        .with_title("Ferrous Engine — Editor")
        .with_size(1280, 720)
        .with_font("assets/fonts/Roboto-Regular.ttf")
        .with_background_color(Color::rgb(0.08, 0.08, 0.10))
        .run();
}
