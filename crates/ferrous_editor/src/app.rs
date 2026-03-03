use std::cell::RefCell;
use std::rc::Rc;

use ferrous_app::{
    App, AppContext, Color, FerrousApp, Handle, MouseButton, RenderStats, Vec3, Viewport,
};
use ferrous_assets::font::Font;
use ferrous_core::scene::MaterialDescriptor;
use ferrous_core::scene::{Axis, GizmoMode};
use ferrous_gui::{GuiBatch, InteractiveButton, Slider, TextBatch, Ui, ViewportWidget};
use glam::Quat;
use rand::Rng;

use crate::ui::{GlobalLightPanel, MaterialInspector};

/// How many cubos se spawnean por frame durante el benchmark.
const BENCHMARK_BATCH: u32 = 200;
/// FPS mínimo antes de parar el benchmark (se evalúa sobre la media).
const BENCHMARK_MIN_FPS: f32 = 60.0;
/// Número de frames sobre los que se calcula la media deslizante de FPS.
const FPS_WINDOW: usize = 60;

// ─── Slider helpers ────────────────────────────────────────────────────────

const SLIDER_MIN: f32 = 0.1;
const SLIDER_MAX: f32 = 5.0;

fn slider_norm(size: f32) -> f32 {
    ((size - SLIDER_MIN) / (SLIDER_MAX - SLIDER_MIN)).clamp(0.0, 1.0)
}

fn slider_to_size(v: f32) -> f32 {
    SLIDER_MIN + v * (SLIDER_MAX - SLIDER_MIN)
}

#[derive(Debug, Clone, PartialEq)]
enum BenchmarkState {
    Idle,
    Running,
    Finished,
}

pub struct EditorApp {
    add_button: Rc<RefCell<InteractiveButton>>,
    bench_button: Rc<RefCell<InteractiveButton>>,
    ui_viewport: Rc<RefCell<ViewportWidget>>,
    button_was_pressed: bool,
    bench_button_was_pressed: bool,
    add_cube: bool,
    last_cube: Option<Handle>,
    last_quad: Option<Handle>,
    bench_state: BenchmarkState,
    bench_cube_count: u32,
    bench_peak_cubes: u32,
    bench_stopped_fps: f32,
    fps_history: Vec<f32>,
    fps_history_idx: usize,
    fps_avg: f32,
    cached_render_stats: RenderStats,
    slider_w: Rc<RefCell<Slider>>,
    slider_h: Rc<RefCell<Slider>>,
    slider_d: Rc<RefCell<Slider>>,
    /// Backend GPU activo (WebGPU, WebGL2, Vulkan, etc.), detectado en setup.
    gpu_backend: String,
    selected: Option<Handle>,
    /// Previously selected handle — used to detect selection changes.
    prev_selected: Option<Handle>,
    /// Phase-13: PBR material inspector panel.
    inspector: MaterialInspector,
    /// Phase-13: Global directional light control panel.
    light_panel: GlobalLightPanel,
    gizmo: ferrous_core::scene::GizmoState,
    /// Secondary gizmo used to reposition the rotation pivot point.
    pivot_gizmo: ferrous_core::scene::GizmoState,
    /// Whether the pivot-move gizmo is visible / active.
    show_pivot_gizmo: bool,
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
            last_quad: None,
            bench_state: BenchmarkState::Idle,
            bench_cube_count: 0,
            bench_peak_cubes: 0,
            bench_stopped_fps: 0.0,
            selected: None,
            prev_selected: None,
            inspector: MaterialInspector::new(),
            light_panel: GlobalLightPanel::new(),
            gizmo: ferrous_core::scene::GizmoState::default(),
            pivot_gizmo: {
                let mut g = ferrous_core::scene::GizmoState::default();
                // Slightly smaller + white/grey palette so it's visually distinct.
                g.style.arm_length = 0.8;
                g.style.show_planes = false;
                g.style.x_axis =
                    ferrous_core::scene::AxisColors::new([1.0, 0.6, 0.6], [1.0, 1.0, 0.0]);
                g.style.y_axis =
                    ferrous_core::scene::AxisColors::new([0.6, 1.0, 0.6], [1.0, 1.0, 0.0]);
                g.style.z_axis =
                    ferrous_core::scene::AxisColors::new([0.6, 0.8, 1.0], [1.0, 1.0, 0.0]);
                g
            },
            show_pivot_gizmo: false,
            fps_history: vec![0.0; FPS_WINDOW],
            fps_history_idx: 0,
            fps_avg: 0.0,
            cached_render_stats: RenderStats::default(),
            slider_w: Rc::new(RefCell::new(Slider::new(
                10.0,
                234.0,
                160.0,
                16.0,
                slider_norm(1.0),
            ))),
            slider_h: Rc::new(RefCell::new(Slider::new(
                10.0,
                262.0,
                160.0,
                16.0,
                slider_norm(1.0),
            ))),
            slider_d: Rc::new(RefCell::new(Slider::new(
                10.0,
                290.0,
                160.0,
                16.0,
                slider_norm(1.0),
            ))),
            gpu_backend: String::new(),
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
        // Phase-13 panels.
        self.inspector.configure_ui(ui);
        self.light_panel.configure_ui(ui);
    }

    fn setup(&mut self, ctx: &mut AppContext) {
        // ── PBR test scene ────────────────────────────────────────────────────
        // A row of 7 cubes covering the full PBR parameter space:
        //
        //  0  Dielectric matte      (metallic=0, roughness=1.0)  grey
        //  1  Dielectric semi-rough (metallic=0, roughness=0.5)  grey
        //  2  Dielectric smooth     (metallic=0, roughness=0.1)  grey
        //  3  Metal rough           (metallic=1, roughness=0.8)  gold
        //  4  Metal semi-rough      (metallic=1, roughness=0.4)  gold
        //  5  Metal smooth          (metallic=1, roughness=0.1)  gold
        //  6  Metal mirror          (metallic=1, roughness=0.0)  gold
        //
        // A red and blue colored dielectric pair sit below the row for
        // quick color-tinting checks.

        struct Preset {
            name: &'static str,
            base_color: [f32; 4],
            metallic: f32,
            roughness: f32,
            emissive_strength: f32,
        }

        let presets: &[Preset] = &[
            // ── Dielectrics (metallic = 0) ────────────────────────────────
            Preset {
                name: "Dielectric Matte",
                base_color: [0.8, 0.8, 0.8, 1.0],
                metallic: 0.0,
                roughness: 1.0,
                emissive_strength: 0.0,
            },
            Preset {
                name: "Dielectric Semi-rough",
                base_color: [0.8, 0.8, 0.8, 1.0],
                metallic: 0.0,
                roughness: 0.5,
                emissive_strength: 0.0,
            },
            Preset {
                name: "Dielectric Smooth",
                base_color: [0.8, 0.8, 0.8, 1.0],
                metallic: 0.0,
                roughness: 0.1,
                emissive_strength: 0.0,
            },
            // ── Metals (metallic = 1, gold-ish albedo) ────────────────────
            Preset {
                name: "Metal Rough",
                base_color: [1.0, 0.76, 0.33, 1.0],
                metallic: 1.0,
                roughness: 0.8,
                emissive_strength: 0.0,
            },
            Preset {
                name: "Metal Semi-rough",
                base_color: [1.0, 0.76, 0.33, 1.0],
                metallic: 1.0,
                roughness: 0.4,
                emissive_strength: 0.0,
            },
            Preset {
                name: "Metal Smooth",
                base_color: [1.0, 0.76, 0.33, 1.0],
                metallic: 1.0,
                roughness: 0.1,
                emissive_strength: 0.0,
            },
            Preset {
                name: "Metal Mirror",
                base_color: [1.0, 0.76, 0.33, 1.0],
                metallic: 1.0,
                roughness: 0.0,
                emissive_strength: 0.0,
            },
        ];

        // Helper to convert typical sRGB hex/colors to Linear for the PBR uniform buffer
        fn to_linear(c: [f32; 4]) -> [f32; 4] {
            [
                c[0].powf(2.2),
                c[1].powf(2.2),
                c[2].powf(2.2),
                c[3], // alpha is linear
            ]
        }

        let spacing = 1.6_f32;
        let total = presets.len() as f32;
        let x_start = -(total - 1.0) * spacing * 0.5;

        for (i, preset) in presets.iter().enumerate() {
            let x = x_start + i as f32 * spacing;
            let mut desc = MaterialDescriptor::default();
            desc.base_color = to_linear(preset.base_color);
            desc.metallic = preset.metallic;
            desc.roughness = preset.roughness;
            desc.emissive_strength = preset.emissive_strength;
            if preset.emissive_strength > 0.0 {
                desc.emissive = [1.0, 0.4, 0.1];
            }
            let mat = ctx.renderer.create_material(&desc);
            let h = ctx
                .world
                .spawn(preset.name)
                .with_position(Vec3::new(x, 0.0, 0.0))
                .with_kind(ferrous_core::scene::ElementKind::Cube {
                    half_extents: Vec3::splat(0.5),
                })
                .with_scale(Vec3::splat(0.5))
                .with_material_handle(mat)
                .build();
            ctx.world.set_material_descriptor(h, desc);
            // keep last cube selectable
            self.last_cube = Some(h);
        }

        // ── Color pair (row below) ─────────────────────────────────────────
        // Red plastic (dielectric smooth) and blue plastic (dielectric smooth)
        // to verify that colored dielectrics show correct colored diffuse +
        // white specular highlight.
        for (name, col, x_off) in &[
            ("Red Plastic", [0.9_f32, 0.1, 0.1, 1.0], -1.0_f32),
            ("Blue Plastic", [0.1_f32, 0.3, 0.9, 1.0], 1.0_f32),
        ] {
            let mut desc = MaterialDescriptor::default();
            desc.base_color = to_linear(*col);
            desc.metallic = 0.0;
            desc.roughness = 0.3;
            let mat = ctx.renderer.create_material(&desc);
            let h = ctx
                .world
                .spawn(*name)
                .with_position(Vec3::new(*x_off, -1.6, 0.0))
                .with_kind(ferrous_core::scene::ElementKind::Cube {
                    half_extents: Vec3::splat(0.5),
                })
                .with_scale(Vec3::splat(0.5))
                .with_material_handle(mat)
                .build();
            ctx.world.set_material_descriptor(h, desc);
        }

        // ── Add a single sphere to exercise the UV-sphere primitive and
        // verify reflections.  Place it to the right of the cube row.
        {
            let mut desc = MaterialDescriptor::default();
            desc.metallic = 1.0; // ensure a mirror-like appearance
            let mat = ctx.renderer.create_material(&desc);
            let hs = ctx
                .world
                .spawn_sphere("Sphere", Vec3::new(2.0, 0.0, 0.0), 1.0, 32);
            ctx.world.set_material_handle(hs, mat);
            ctx.world.set_material_descriptor(hs, desc);
        }

        // ── Directional light at a 45° angle for clear shading ────────────
        // Direction = from upper-right-front toward scene center.
        let ldir = Vec3::new(-0.6, -0.8, -0.4).normalize();
        ctx.renderer.set_directional_light(
            [ldir.x, ldir.y, ldir.z],
            [1.0, 0.97, 0.90], // warm white
            3.5,
        );

        // ── Camera: start at a slight 3/4 angle so cubes look 3-D ─────────
        // yaw=-30°, pitch=20° — shows top/right faces clearly.
        ctx.renderer.orbit.yaw = -0.52; // ≈ -30°
        ctx.renderer.orbit.pitch = 0.35; // ≈  20°
        {
            let yaw = ctx.renderer.orbit.yaw;
            let pitch = ctx.renderer.orbit.pitch;
            let dist = ctx.renderer.camera.controller.orbit_distance;
            // Manually compute orbit eye from yaw/pitch angles:
            //   forward = (sin(yaw)*cos(pitch), sin(pitch), cos(yaw)*cos(pitch))
            // eye = target + dist * forward
            let cy = pitch.cos();
            let sy = pitch.sin();
            let cx = yaw.cos();
            let sx = yaw.sin();
            let offset = Vec3::new(sx * cy, sy, cx * cy) * dist;
            ctx.renderer.camera.eye = ctx.renderer.camera.target + offset;
        }

        self.gpu_backend = ctx.gpu_backend().to_string();

        // ------------------------------------------------------------------
        // Load a known test model if available.  the engine repository ships
        // `DamagedHelmet.glb` under `assets/models`; when running from the
        // workspace root this path is valid, so we simply try it first.  the
        // previous demo logic (looking for `model.gltf`/`.glb` in the
        // current directory) is still retained for ad-hoc tests.
        let test_model = r"C:\Users\jvchi\CARPETAS\FerrousEngine\assets\models\DamagedHelmet.glb";
        if std::path::Path::new(test_model).exists() {
            if let Ok(handles) = ferrous_app::spawn_gltf(&mut ctx.world, &mut ctx.renderer, test_model) {
                log::info!("spawned {} meshes from {}", handles.len(), test_model);
                // Position the helmet above the ground and rotate it to face
                // the camera (90° around Y).
                for h in &handles {
                    ctx.world.set_position(*h, Vec3::new(0.0, 2.5, 0.0));
                    ctx.world.set_rotation(*h, Quat::from_rotation_y(
                        std::f32::consts::FRAC_PI_2,
                    ));
                }
            } else {
                log::warn!("failed to load glTF from {}", test_model);
            }
        } else {
            let demo_paths = ["model.gltf", "model.glb"];
            for p in &demo_paths {
                if std::path::Path::new(p).exists() {
                    if let Ok(handles) = ferrous_app::spawn_gltf(&mut ctx.world, &mut ctx.renderer, p) {
                        log::info!("spawned {} meshes from {}", handles.len(), p);
                    } else {
                        log::warn!("failed to load glTF from {}", p);
                    }
                    break;
                }
            }
        }
    }

    fn update(&mut self, ctx: &mut AppContext) {
        self.fps_history[self.fps_history_idx] = ctx.time.fps;
        self.fps_history_idx = (self.fps_history_idx + 1) % FPS_WINDOW;
        self.fps_avg = self.fps_history.iter().sum::<f32>() / FPS_WINDOW as f32;

        let (win_w, win_h) = ctx.window_size;
        ctx.viewport = Viewport {
            x: 0,
            y: 0,
            width: win_w,
            height: win_h,
        };

        let pressed = self.add_button.borrow().pressed;
        if !pressed && self.button_was_pressed {
            self.add_cube = true;
        }
        self.button_was_pressed = pressed;

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
                    self.bench_state = BenchmarkState::Finished;
                    self.bench_stopped_fps = ctx.time.fps;
                }
            }
        }
        self.bench_button_was_pressed = bench_pressed;

        if self.bench_state == BenchmarkState::Running {
            if self.fps_avg >= BENCHMARK_MIN_FPS {
                self.bench_cube_count += BENCHMARK_BATCH;
            } else {
                self.bench_stopped_fps = self.fps_avg;
                self.bench_peak_cubes = self.bench_cube_count;
                self.bench_state = BenchmarkState::Finished;
            }
        }

        // ------------------------------------------------------------------
        // Selection — invalidate if entity was despawned.
        // Gizmo interaction (pick axis, drag-translate) runs in draw_3d
        // where camera_eye / camera_target are valid.
        // ------------------------------------------------------------------
        if let Some(h) = self.selected {
            if !ctx.world.contains(h) {
                self.selected = None;
                self.gizmo.dragging = false;
                self.gizmo.highlighted_axis = None;
            }
        }

        // Left-click with no gizmo drag active → select last spawned cube.
        if ctx.input.button_just_pressed(MouseButton::Left)
            && !self.gizmo.dragging
            && !self.pivot_gizmo.dragging
        {
            if let Some(h) = self.last_cube {
                if ctx.world.contains(h) {
                    self.selected = Some(h);
                }
            }
        }

        // ── Gizmo mode hotkeys ─────────────────────────────────────────────
        // T  → Translate mode
        // R  → Rotate mode  (shows arc rings)
        // P  → Toggle pivot-move gizmo (only active in Rotate mode)
        if ctx.input.just_pressed(ferrous_app::KeyCode::KeyT) {
            self.gizmo.mode = GizmoMode::Translate;
            self.gizmo.dragging = false;
            self.gizmo.highlighted_axis = None;
            self.show_pivot_gizmo = false;
        }
        if ctx.input.just_pressed(ferrous_app::KeyCode::KeyR) {
            self.gizmo.mode = GizmoMode::Rotate;
            self.gizmo.dragging = false;
            self.gizmo.highlighted_axis = None;
        }
        if ctx.input.just_pressed(ferrous_app::KeyCode::KeyP) {
            if self.gizmo.mode == GizmoMode::Rotate {
                self.show_pivot_gizmo = !self.show_pivot_gizmo;
                // Reset pivot offset to entity origin when toggling on.
                if self.show_pivot_gizmo {
                    if let Some(sel) = self.selected {
                        if let Some(pos) = ctx.world.position(sel) {
                            // Reset to local offset zero = pivot sits on entity.
                            self.gizmo.pivot_offset = Vec3::ZERO;
                            self.pivot_gizmo.world_transform.position = pos;
                        }
                    }
                }
            }
        }

        // Escape only makes sense on desktop (browser has no app exit).
        #[cfg(not(target_arch = "wasm32"))]
        if ctx.input.just_pressed(ferrous_app::KeyCode::Escape) {
            ctx.request_exit();
        }
    }

    fn draw_ui(
        &mut self,
        gui: &mut GuiBatch,
        text: &mut TextBatch,
        font: Option<&Font>,
        ctx: &mut AppContext,
    ) {
        if let Some(font) = font {
            text.draw_text(font, "Add Cube", [15.0, 16.0], 16.0, [1.0, 1.0, 1.0, 1.0]);

            let bench_label = match self.bench_state {
                BenchmarkState::Idle | BenchmarkState::Finished => "Benchmark",
                BenchmarkState::Running => "Stop Bench",
            };
            text.draw_text(font, bench_label, [15.0, 57.0], 16.0, [1.0, 1.0, 1.0, 1.0]);

            let fps_str = format!("FPS: {:.0}  avg: {:.0}", ctx.time.fps, self.fps_avg);
            text.draw_text(font, &fps_str, [15.0, 92.0], 14.0, [0.8, 0.8, 0.8, 1.0]);

            if !self.gpu_backend.is_empty() {
                let backend_color = if self.gpu_backend == "WebGL2" {
                    [1.0, 0.6, 0.2, 1.0] // naranja = backend lento
                } else {
                    [0.4, 1.0, 0.5, 1.0] // verde = backend nativo/WebGPU
                };
                text.draw_text(
                    font,
                    &format!("GPU: {}", self.gpu_backend),
                    [15.0, 108.0],
                    12.0,
                    backend_color,
                );
            }

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
            text.draw_text(font, &verts_str, [15.0, 126.0], 13.0, [0.5, 0.9, 1.0, 1.0]);
            text.draw_text(font, &tris_str, [15.0, 142.0], 13.0, [0.5, 0.9, 1.0, 1.0]);
            text.draw_text(font, &dc_str, [15.0, 158.0], 13.0, [0.5, 0.9, 1.0, 1.0]);

            // Gizmo status
            if self.selected.is_some() {
                let mode_label = match self.gizmo.mode {
                    GizmoMode::Translate => "Translate  [T]",
                    GizmoMode::Rotate => "Rotate     [R]",
                    GizmoMode::Scale => "Scale",
                };
                let mode_color: [f32; 4] = match self.gizmo.mode {
                    GizmoMode::Translate => [0.4, 0.9, 1.0, 1.0],
                    GizmoMode::Rotate => [0.9, 0.5, 1.0, 1.0],
                    GizmoMode::Scale => [1.0, 0.8, 0.2, 1.0],
                };
                text.draw_text(
                    font,
                    "[ Gizmo active ]",
                    [15.0, 178.0],
                    13.0,
                    [1.0, 0.85, 0.2, 1.0],
                );
                text.draw_text(font, mode_label, [15.0, 194.0], 12.0, mode_color);

                let axis_str = match (self.gizmo.mode, self.gizmo.highlighted_axis) {
                    (GizmoMode::Translate, Some(Axis::X)) => "Axis: X  (dragging)",
                    (GizmoMode::Translate, Some(Axis::Y)) => "Axis: Y  (dragging)",
                    (GizmoMode::Translate, Some(Axis::Z)) => "Axis: Z  (dragging)",
                    (GizmoMode::Translate, None) => "Click axis / plane",
                    (GizmoMode::Rotate, Some(Axis::X)) => "Ring: X  (drag to rotate)",
                    (GizmoMode::Rotate, Some(Axis::Y)) => "Ring: Y  (drag to rotate)",
                    (GizmoMode::Rotate, Some(Axis::Z)) => "Ring: Z  (drag to rotate)",
                    (GizmoMode::Rotate, None) => "Click a ring to rotate  [P] pivot",
                    _ => "",
                };
                let axis_color = match self.gizmo.highlighted_axis {
                    Some(Axis::X) => [1.0, 0.3, 0.3, 1.0],
                    Some(Axis::Y) => [0.3, 1.0, 0.3, 1.0],
                    Some(Axis::Z) => [0.3, 0.5, 1.0, 1.0],
                    None => [0.7, 0.7, 0.7, 1.0],
                };
                text.draw_text(font, axis_str, [15.0, 208.0], 11.0, axis_color);

                // Show pivot info when in rotate mode.
                if self.gizmo.mode == GizmoMode::Rotate {
                    let off = self.gizmo.pivot_offset;
                    let piv_str = if off == Vec3::ZERO {
                        "Pivot: origin  [P] move".to_string()
                    } else {
                        format!("Pivot offset ({:.2},{:.2},{:.2})  [P]", off.x, off.y, off.z)
                    };
                    let piv_color: [f32; 4] = if self.show_pivot_gizmo {
                        [1.0, 1.0, 0.4, 1.0]
                    } else {
                        [0.6, 0.6, 0.6, 1.0]
                    };
                    text.draw_text(font, &piv_str, [15.0, 221.0], 10.0, piv_color);
                }
            } else {
                text.draw_text(
                    font,
                    "Click cube to select",
                    [15.0, 178.0],
                    12.0,
                    [0.6, 0.6, 0.6, 1.0],
                );
            }

            match self.bench_state {
                BenchmarkState::Idle => {}
                BenchmarkState::Running => {
                    let live = format!(
                        "Cubes: {}  (+{}·frame)",
                        self.bench_cube_count, BENCHMARK_BATCH
                    );
                    text.draw_text(font, &live, [15.0, 178.0], 14.0, [0.4, 1.0, 0.4, 1.0]);
                    let threshold = format!("Stops at avg < {:.0} FPS", BENCHMARK_MIN_FPS);
                    text.draw_text(font, &threshold, [15.0, 196.0], 12.0, [0.6, 0.6, 0.6, 1.0]);
                }
                BenchmarkState::Finished => {
                    let result = format!("Peak cubes: {}", self.bench_peak_cubes);
                    text.draw_text(font, &result, [15.0, 178.0], 14.0, [1.0, 0.8, 0.2, 1.0]);
                    let fps_drop = format!("Avg FPS at stop: {:.1}", self.bench_stopped_fps);
                    text.draw_text(font, &fps_drop, [15.0, 196.0], 12.0, [1.0, 0.5, 0.3, 1.0]);
                }
            }

            if self
                .last_cube
                .map(|h| ctx.world.contains(h))
                .unwrap_or(false)
            {
                let w = slider_to_size(self.slider_w.borrow().value);
                let h = slider_to_size(self.slider_h.borrow().value);
                let d = slider_to_size(self.slider_d.borrow().value);
                text.draw_text(
                    font,
                    &format!("W: {:.2}", w),
                    [15.0, 224.0],
                    13.0,
                    [0.9, 0.9, 0.5, 1.0],
                );
                text.draw_text(
                    font,
                    &format!("H: {:.2}", h),
                    [15.0, 252.0],
                    13.0,
                    [0.9, 0.9, 0.5, 1.0],
                );
                text.draw_text(
                    font,
                    &format!("D: {:.2}", d),
                    [15.0, 280.0],
                    13.0,
                    [0.9, 0.9, 0.5, 1.0],
                );
            }
        }

        // ── Phase-13: Material Inspector (right panel) ─────────────────────
        // Sync widgets when the selection changes.
        if self.selected != self.prev_selected {
            if let Some(h) = self.selected {
                if let Some(elem) = ctx.world.get(h) {
                    self.inspector
                        .sync_from_descriptor(&elem.material.descriptor);
                }
            }
            self.prev_selected = self.selected;
        }

        self.inspector.draw(self.selected, gui, text, font, ctx);

        // ── Phase-13: Global Light Panel (below the inspector) ─────────────
        let win_h = ctx.window_size.1 as f32;
        self.light_panel.draw(gui, text, font, ctx, win_h - 140.0);
    }

    fn draw_3d(&mut self, ctx: &mut AppContext) {
        self.cached_render_stats = ctx.render_stats;

        if let Some(handle) = self.last_cube {
            if ctx.world.contains(handle) {
                let w = slider_to_size(self.slider_w.borrow().value);
                let h = slider_to_size(self.slider_h.borrow().value);
                let d = slider_to_size(self.slider_d.borrow().value);
                ctx.world.set_cube_size(handle, Vec3::new(w, h, d));
            }
        }

        let mut rng = rand::thread_rng();

        if self.add_cube {
            let base = ctx.camera_eye;
            let pos = Vec3::new(
                base.x + (rng.gen::<f32>() - 0.5) * 2.0,
                base.y + (rng.gen::<f32>() - 0.5) * 2.0,
                base.z - 5.0 + (rng.gen::<f32>() - 0.5),
            );
            let handle = ctx.world.spawn_cube("Cube", pos);
            let color = Color::from_rgb8(
                rng.gen_range(100..=255),
                rng.gen_range(100..=255),
                rng.gen_range(100..=255),
            );
            // create a dedicated material for this object and assign it
            let mut desc = MaterialDescriptor::default();
            desc.base_color = color.to_array();
            let mat = ctx.renderer.create_material(&desc);
            ctx.world.set_material_handle(handle, mat);
            ctx.world.set_material_descriptor(handle, desc.clone());
            // no need to call update_material_params because create_material
            // already populated the GPU state, but the descriptor lives in the
            // world so editors can still tweak it later.
            // also spawn a small quad at the same location
            let qh = ctx
                .world
                .spawn_quad("Quad", pos + Vec3::new(0.0, 0.0, 1.0), 0.5, 0.5, true);
            ctx.world.set_material_handle(qh, mat);
            self.last_quad = Some(qh);
            self.last_cube = Some(handle);
            self.add_cube = false;
        }

        // Gizmo: pick axis / ring, drag, queue draw.
        // Must be in draw_3d (not update) so camera_eye/camera_target are valid.
        if let Some(sel) = self.selected {
            // When the pivot gizmo is active and dragging, give it priority
            // so its mouse-pick wins over the rotation gizmo.
            let pivot_active = self.show_pivot_gizmo && self.gizmo.mode == GizmoMode::Rotate;

            if pivot_active {
                // Borrow split: update_pivot_gizmo needs both &mut pivot_gizmo
                // and &mut rotation_gizmo simultaneously.  We use a small trick:
                // call the helper manually here and update the fields directly.
                // Safer: clone the pivot state for the call, then write back.
                let mut pg = self.pivot_gizmo.clone();
                ctx.update_pivot_gizmo(sel, &mut pg, &mut self.gizmo);
                self.pivot_gizmo = pg;
            }

            ctx.update_gizmo(sel, &mut self.gizmo);
        }
    }

    fn on_resize(&mut self, new_size: (u32, u32), ctx: &mut AppContext) {
        ctx.viewport = Viewport {
            x: 0,
            y: 0,
            width: new_size.0,
            height: new_size.1,
        };
    }
}

/// Builds and returns the configured [`App`] for this editor.
///
/// - **Desktop**: called by `main()` with `with_font` (loads from disk).
/// - **wasm32**: called by `run()` with `with_font_bytes` (embedded bytes).
#[allow(dead_code)]
pub fn build_app() -> App<EditorApp> {
    let base = App::new(EditorApp::default())
        .with_msaa(1)
        .with_title("Ferrous Engine — Editor")
        .with_size(1280, 720)
        .with_background_color(Color::rgb(0.08, 0.08, 0.10));

    #[cfg(not(target_arch = "wasm32"))]
    let base = base
        .with_target_fps(Some(240))
        .with_vsync(false)
        .with_idle_timeout(None)
        .with_font("assets/fonts/Roboto-Regular.ttf");

    #[cfg(target_arch = "wasm32")]
    let base = base
        .with_target_fps(None) // rAF already rate-limits to monitor refresh; no extra cap needed
        .with_vsync(false) // rAF is always vsynced; this is just for clarity
        .with_font_bytes(include_bytes!("../../../assets/fonts/Roboto-Regular.ttf"));

    base
}
