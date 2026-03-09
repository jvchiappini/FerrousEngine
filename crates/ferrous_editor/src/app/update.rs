//! `EditorApp::run_update` — per-frame logic, input handling and gizmo hotkeys.

use ferrous_app::{AppContext, Vec3, Viewport};
use ferrous_core::scene::GizmoMode;

use super::types::{BenchmarkState, EditorApp, BENCHMARK_BATCH, BENCHMARK_MIN_FPS, FPS_WINDOW};

impl EditorApp {
    pub(super) fn run_update(&mut self, ctx: &mut AppContext) {
        self.fps_history[self.fps_history_idx] = ctx.time.fps;
        self.fps_history_idx = (self.fps_history_idx + 1) % FPS_WINDOW;
        self.fps_avg = self.fps_history.iter().sum::<f32>() / FPS_WINDOW as f32;

        let (win_w, win_h) = ctx.window_size;
        ctx.viewport = Viewport { x: 0, y: 0, width: win_w, height: win_h };

        // La lógica de los botones Add y Benchmark ahora se maneja vía callbacks 
        // registrados en configure_ui().

        if self.bench_state == BenchmarkState::Running {
            if self.fps_avg >= BENCHMARK_MIN_FPS {
                self.bench_cube_count += BENCHMARK_BATCH;
            } else {
                self.bench_stopped_fps = self.fps_avg;
                self.bench_peak_cubes = self.bench_cube_count;
                self.bench_state = BenchmarkState::Finished;
            }
        }

        // Selection validity check
        if let Some(h) = self.selected {
            if !ctx.world.contains(h) {
                self.selected = None;
                self.gizmo.dragging = false;
                self.gizmo.highlighted_axis = None;
            }
        }

        // Left-click selects last spawned cube when no gizmo is dragging
        if ctx.input.button_just_pressed(ferrous_app::MouseButton::Left)
            && !self.gizmo.dragging
            && !self.pivot_gizmo.dragging
        {
            if let Some(h) = self.last_cube {
                if ctx.world.contains(h) {
                    self.selected = Some(h);
                }
            }
        }

        // Gizmo mode hotkeys
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
                if self.show_pivot_gizmo {
                    if let Some(sel) = self.selected {
                        if let Some(pos) = ctx.world.position(sel) {
                            self.gizmo.pivot_offset = Vec3::ZERO;
                            self.pivot_gizmo.world_transform.position = pos;
                        }
                    }
                }
            }
        }

        #[cfg(not(target_arch = "wasm32"))]
        if ctx.input.just_pressed(ferrous_app::KeyCode::Escape) {
            ctx.request_exit();
        }
    }
}
