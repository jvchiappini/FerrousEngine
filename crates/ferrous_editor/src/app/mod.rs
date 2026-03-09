//! `EditorApp` — main editor application type and `FerrousApp` implementation.
//!
//! Heavy per-callback logic is in sub-modules:
//! - [`setup`]    — PBR test scene, lights, camera
//! - [`update`]   — input, benchmark, gizmo hotkeys
//! - [`draw_ui`]  — HUD text, material inspector, light panel
//! - [`draw_3d`]  — gizmo interaction, cube spawning

mod draw_3d;
mod draw_ui;
mod setup;
pub mod types;

mod update;

pub use types::EditorApp;

use ferrous_app::{App, AppContext, Color, DrawContext, FerrousApp};
use ferrous_gui::{Button, RectOffset, Slider, Style, Units, ViewportWidget};

impl FerrousApp for EditorApp {
    fn configure_ui(&mut self, ui: &mut ferrous_gui::UiTree<Self>) {
        use ferrous_gui::{Button, Slider, Style, Units, ViewportWidget};

        // Botón "Add Cube"
        let add_btn = Button::new("Add Cube").on_click(
            |ctx: &mut ferrous_gui::EventContext<'_, EditorApp>| {
                ctx.app.add_cube = true;
                ctx.app.button_was_pressed = true;
            },
        );
        let add_id = ui.add_node(Box::new(add_btn), None);
        ui.set_node_style(
            add_id,
            Style {
                margin: RectOffset {
                    left: 10.0,
                    right: 10.0,
                    top: 10.0,
                    bottom: 10.0,
                },
                size: (Units::Px(120.0), Units::Px(32.0)),
                ..Default::default()
            },
        );
        self.add_button = Some(add_id);

        // Botón "Benchmark"
        let bench_btn = Button::new("Run Benchmark").on_click(
            |ctx: &mut ferrous_gui::EventContext<'_, EditorApp>| {
                use crate::app::types::BenchmarkState;
                match ctx.app.bench_state {
                    BenchmarkState::Idle | BenchmarkState::Finished => {
                        ctx.app.bench_state = BenchmarkState::Running;
                        ctx.app.bench_cube_count = 0;
                        ctx.app.bench_peak_cubes = 0;
                        ctx.app.bench_stopped_fps = 0.0;
                    }
                    BenchmarkState::Running => {
                        ctx.app.bench_state = BenchmarkState::Finished;
                    }
                }
            },
        );
        let bench_id = ui.add_node(Box::new(bench_btn), None);
        ui.set_node_style(
            bench_id,
            Style {
                margin: RectOffset {
                    left: 10.0,
                    right: 10.0,
                    top: 50.0,
                    bottom: 10.0,
                },
                size: (Units::Px(150.0), Units::Px(32.0)),
                ..Default::default()
            },
        );
        self.bench_button = Some(bench_id);

        // Viewport 3D
        let viewport = ViewportWidget::new();
        let viewport_id = ui.add_node(Box::new(viewport), None);
        ui.set_node_style(
            viewport_id,
            Style {
                size: (Units::Percentage(100.0), Units::Percentage(100.0)),
                position: ferrous_gui::Position::Absolute,
                ..Default::default()
            },
        );
        self.ui_viewport = Some(viewport_id);

        // Sliders de dimensiones
        let slider_w = Slider::new(1.0, 0.1, 5.0).on_change(
            |ctx: &mut ferrous_gui::EventContext<'_, EditorApp>, val| {
                ctx.app.cube_size.x = val;
            },
        );
        self.slider_w = Some(ui.add_node(Box::new(slider_w), None));

        let slider_h = Slider::new(1.0, 0.1, 5.0).on_change(
            |ctx: &mut ferrous_gui::EventContext<'_, EditorApp>, val| {
                ctx.app.cube_size.y = val;
            },
        );
        self.slider_h = Some(ui.add_node(Box::new(slider_h), None));

        let slider_d = Slider::new(1.0, 0.1, 5.0).on_change(
            |ctx: &mut ferrous_gui::EventContext<'_, EditorApp>, val| {
                ctx.app.cube_size.z = val;
            },
        );
        self.slider_d = Some(ui.add_node(Box::new(slider_d), None));

        // ... etc (simplificado para el ejemplo)
        self.inspector.configure_ui(ui);
        self.light_panel.configure_ui(ui);
    }

    fn setup(&mut self, ctx: &mut AppContext) {
        self.run_setup(ctx);
    }

    fn update(&mut self, ctx: &mut AppContext) {
        self.run_update(ctx);
    }

    fn draw_ui(&mut self, dc: &mut DrawContext<'_, '_>) {
        self.run_draw_ui(dc);
    }

    fn draw_3d(&mut self, ctx: &mut AppContext) {
        self.run_draw_3d(ctx);
    }

    fn on_resize(&mut self, new_size: (u32, u32), ctx: &mut AppContext) {
        self.run_on_resize(new_size, ctx);
    }
}

// ─── App builder ─────────────────────────────────────────────────────────────

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
        .with_background_color(Color::rgb(0.08, 0.08, 0.10))
        .with_hdri("assets/skybox/citrus_orchard_road_puresky_4k.exr");

    #[cfg(not(target_arch = "wasm32"))]
    let base = base
        .with_target_fps(Some(240))
        .with_vsync(false)
        .with_idle_timeout(None)
        .with_font("assets/fonts/Roboto-Regular.ttf");

    #[cfg(target_arch = "wasm32")]
    let base = base
        .with_target_fps(None)
        .with_vsync(false)
        .with_font_bytes(include_bytes!("../../../assets/fonts/Roboto-Regular.ttf"));

    base
}
