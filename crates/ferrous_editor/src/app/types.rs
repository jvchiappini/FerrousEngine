use ferrous_app::{Handle, RenderStats};
use ferrous_gui::NodeId;

use crate::ui::{GlobalLightPanel, MaterialInspector};

// ─── Constants ────────────────────────────────────────────────────────────────

/// Cubes spawned per frame during the benchmark.
pub(super) const BENCHMARK_BATCH: u32 = 200;
/// Minimum average FPS before the benchmark stops automatically.
pub(super) const BENCHMARK_MIN_FPS: f32 = 60.0;
/// Sliding window size for FPS averaging.
pub(super) const FPS_WINDOW: usize = 60;

pub(super) const SLIDER_MIN: f32 = 0.1;
pub(super) const SLIDER_MAX: f32 = 5.0;

pub(super) fn slider_norm(size: f32) -> f32 {
    ((size - SLIDER_MIN) / (SLIDER_MAX - SLIDER_MIN)).clamp(0.0, 1.0)
}

pub(super) fn slider_to_size(v: f32) -> f32 {
    SLIDER_MIN + v * (SLIDER_MAX - SLIDER_MIN)
}

// ─── BenchmarkState ──────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq)]
pub(super) enum BenchmarkState {
    Idle,
    Running,
    Finished,
}

// ─── EditorApp ───────────────────────────────────────────────────────────────

pub struct EditorApp {
    pub(super) add_button: Option<NodeId>,
    pub(super) bench_button: Option<NodeId>,
    pub(super) ui_viewport: Option<NodeId>,
    pub(super) button_was_pressed: bool,
    pub(super) bench_button_was_pressed: bool,
    pub(super) add_cube: bool,
    pub(super) last_cube: Option<Handle>,
    pub(super) last_quad: Option<Handle>,
    pub(super) bench_state: BenchmarkState,
    pub(super) bench_cube_count: u32,
    pub(super) bench_peak_cubes: u32,
    pub(super) bench_stopped_fps: f32,
    pub(super) fps_history: Vec<f32>,
    pub(super) fps_history_idx: usize,
    pub(super) fps_avg: f32,
    pub(super) cached_render_stats: RenderStats,
    pub(super) slider_w: Option<NodeId>,
    pub(super) slider_h: Option<NodeId>,
    pub(super) slider_d: Option<NodeId>,
    pub(super) cube_size: ferrous_app::Vec3,
    /// Active GPU backend name, detected in `setup`.
    pub(super) gpu_backend: String,
    pub(super) selected: Option<Handle>,
    /// Previously selected handle — used to detect selection changes.
    pub(super) prev_selected: Option<Handle>,
    pub(super) inspector: MaterialInspector,
    pub(super) light_panel: GlobalLightPanel,
    pub(super) gizmo: ferrous_core::scene::GizmoState,
    /// Secondary gizmo used to reposition the rotation pivot point.
    pub(super) pivot_gizmo: ferrous_core::scene::GizmoState,
    /// Whether the pivot-move gizmo is visible / active.
    pub(super) show_pivot_gizmo: bool,
}

impl Default for EditorApp {
    fn default() -> Self {
        Self {
            add_button: None,
            bench_button: None,
            ui_viewport: None,
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
            slider_w: None,
            slider_h: None,
            slider_d: None,
            gpu_backend: String::new(),
            cube_size: ferrous_app::Vec3::ONE,
        }
    }
}
