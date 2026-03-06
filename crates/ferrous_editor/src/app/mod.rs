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
mod types;
mod update;

pub use types::EditorApp;

use ferrous_app::{App, AppContext, Color, FerrousApp};
use ferrous_assets::Font;
use ferrous_gui::{GuiBatch, TextBatch, Ui};

impl FerrousApp for EditorApp {
    fn configure_ui(&mut self, ui: &mut Ui) {
        ui.add(self.add_button.clone());
        ui.add(self.bench_button.clone());
        ui.register_viewport(self.ui_viewport.clone());
        ui.add(self.slider_w.clone());
        ui.add(self.slider_h.clone());
        ui.add(self.slider_d.clone());
        self.inspector.configure_ui(ui);
        self.light_panel.configure_ui(ui);
    }

    fn setup(&mut self, ctx: &mut AppContext) {
        self.run_setup(ctx);
    }

    fn update(&mut self, ctx: &mut AppContext) {
        self.run_update(ctx);
    }

    fn draw_ui(
        &mut self,
        gui: &mut GuiBatch,
        text: &mut TextBatch,
        font: Option<&Font>,
        ctx: &mut AppContext,
    ) {
        self.run_draw_ui(gui, text, font, ctx);
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
