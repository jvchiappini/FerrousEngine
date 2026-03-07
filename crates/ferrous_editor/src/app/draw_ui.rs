//! `EditorApp::run_draw_ui` — HUD text, material inspector, light panel.

use ferrous_app::DrawContext;
use ferrous_core::scene::{Axis, GizmoMode};

use super::types::{BenchmarkState, EditorApp, BENCHMARK_BATCH, BENCHMARK_MIN_FPS, slider_to_size};

impl EditorApp {
    pub(super) fn run_draw_ui(&mut self, dc: &mut DrawContext<'_, '_>) {
        let font = dc.font;
        let text = &mut *dc.text;
        let gui = &mut *dc.gui;
        let ctx = &mut *dc.ctx;

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
                [1.0, 0.6, 0.2, 1.0]
            } else {
                [0.4, 1.0, 0.5, 1.0]
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
        text.draw_text(font, &verts_str, [15.0, 126.0], 13.0, [0.5, 0.9, 1.0, 1.0]);
        text.draw_text(font, &tris_str, [15.0, 142.0], 13.0, [0.5, 0.9, 1.0, 1.0]);
        text.draw_text(font, &format!("Draw calls: {}", dcs), [15.0, 158.0], 13.0, [0.5, 0.9, 1.0, 1.0]);

        if self.selected.is_some() {
            let mode_label = match self.gizmo.mode {
                GizmoMode::Translate => "Translate  [T]",
                GizmoMode::Rotate    => "Rotate     [R]",
                GizmoMode::Scale     => "Scale",
            };
            let mode_color: [f32; 4] = match self.gizmo.mode {
                GizmoMode::Translate => [0.4, 0.9, 1.0, 1.0],
                GizmoMode::Rotate    => [0.9, 0.5, 1.0, 1.0],
                GizmoMode::Scale     => [1.0, 0.8, 0.2, 1.0],
            };
            text.draw_text(font, "[ Gizmo active ]", [15.0, 178.0], 13.0, [1.0, 0.85, 0.2, 1.0]);
            text.draw_text(font, mode_label, [15.0, 194.0], 12.0, mode_color);

            let axis_str = match (self.gizmo.mode, self.gizmo.highlighted_axis) {
                (GizmoMode::Translate, Some(Axis::X)) => "Axis: X  (dragging)",
                (GizmoMode::Translate, Some(Axis::Y)) => "Axis: Y  (dragging)",
                (GizmoMode::Translate, Some(Axis::Z)) => "Axis: Z  (dragging)",
                (GizmoMode::Translate, None)          => "Click axis / plane",
                (GizmoMode::Rotate,    Some(Axis::X)) => "Ring: X  (drag to rotate)",
                (GizmoMode::Rotate,    Some(Axis::Y)) => "Ring: Y  (drag to rotate)",
                (GizmoMode::Rotate,    Some(Axis::Z)) => "Ring: Z  (drag to rotate)",
                (GizmoMode::Rotate,    None)          => "Click a ring to rotate  [P] pivot",
                _ => "",
            };
            let axis_color = match self.gizmo.highlighted_axis {
                Some(Axis::X) => [1.0, 0.3, 0.3, 1.0],
                Some(Axis::Y) => [0.3, 1.0, 0.3, 1.0],
                Some(Axis::Z) => [0.3, 0.5, 1.0, 1.0],
                None          => [0.7, 0.7, 0.7, 1.0],
            };
            text.draw_text(font, axis_str, [15.0, 208.0], 11.0, axis_color);

            if self.gizmo.mode == GizmoMode::Rotate {
                let off = self.gizmo.pivot_offset;
                let piv_str = if off == ferrous_app::Vec3::ZERO {
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
            text.draw_text(font, "Click cube to select", [15.0, 178.0], 12.0, [0.6, 0.6, 0.6, 1.0]);
        }

        match self.bench_state {
            BenchmarkState::Idle => {}
            BenchmarkState::Running => {
                let live = format!("Cubes: {}  (+{}·frame)", self.bench_cube_count, BENCHMARK_BATCH);
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

        if self.last_cube.map(|h| ctx.world.contains(h)).unwrap_or(false) {
            let w = slider_to_size(self.slider_w.borrow().value);
            let h = slider_to_size(self.slider_h.borrow().value);
            let d = slider_to_size(self.slider_d.borrow().value);
            text.draw_text(font, &format!("W: {:.2}", w), [15.0, 224.0], 13.0, [0.9, 0.9, 0.5, 1.0]);
            text.draw_text(font, &format!("H: {:.2}", h), [15.0, 252.0], 13.0, [0.9, 0.9, 0.5, 1.0]);
            text.draw_text(font, &format!("D: {:.2}", d), [15.0, 280.0], 13.0, [0.9, 0.9, 0.5, 1.0]);
        }

        // Material inspector (right panel)
        if self.selected != self.prev_selected {
            if let Some(h) = self.selected {
                if let Some(elem) = ctx.world.get(h) {
                    self.inspector.sync_from_descriptor(&elem.material.descriptor);
                }
            }
            self.prev_selected = self.selected;
        }
        self.inspector.draw(self.selected, gui, text, Some(font), ctx);

        // Global light panel
        let win_h = ctx.window_size.1 as f32;
        self.light_panel.draw(gui, text, Some(font), ctx, win_h - 140.0);
    }
}