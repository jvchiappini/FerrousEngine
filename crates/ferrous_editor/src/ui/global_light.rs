//! GlobalLightPanel — Phase 13.
//!
//! A compact panel at the bottom-right of the inspector that lets the user
//! adjust the global directional light in real time.
//!
//! ## Controls
//! - **Azimuth** slider  – horizontal rotation of the light direction (0 → 360°).
//! - **Elevation** slider – vertical angle of the light (−90 → +90°).
//! - **Intensity** slider – scalar brightness multiplier (0 → 4).
//!
//! Pressing any slider calls `renderer.set_directional_light(...)` immediately.

use std::cell::RefCell;
use std::rc::Rc;

use ferrous_app::AppContext;
use ferrous_assets::Font;
use ferrous_gui::{GuiBatch, Slider, UiTree, Widget, ToBatches};
use crate::app::types::EditorApp;

use crate::ui::material_inspector::PANEL_W;

const MARGIN: f32 = 10.0;
const SLIDER_H: f32 = 14.0;
const MAX_INTENSITY: f32 = 50.0;

/// Panel that controls the single global directional light.
pub struct GlobalLightPanel {
    pub slider_azimuth: Rc<RefCell<Slider<EditorApp>>>,
    pub slider_elevation: Rc<RefCell<Slider<EditorApp>>>,
    pub slider_intensity: Rc<RefCell<Slider<EditorApp>>>,
    // NodeIds for updating layout manually
    pub azimuth_id: Option<ferrous_gui::NodeId>,
    pub elevation_id: Option<ferrous_gui::NodeId>,
    pub intensity_id: Option<ferrous_gui::NodeId>,
}

impl GlobalLightPanel {
    pub fn new() -> Self {
        // Defaults: sun from upper-right at 45° elevation, intensity 3.5.
        // azimuth 0.125 → 45° (from the right-front diagonal)
        // elevation 0.75 → 45° above horizon  (good for PBR highlights)
        // intensity 0.875 of [0, 4] → 3.5 (bright enough to see specular)
        Self {
            slider_azimuth: Rc::new(RefCell::new(Slider::new(0.0, 0.0, 1.0))),
            slider_elevation: Rc::new(RefCell::new(Slider::new(0.0, 0.0, 1.0))),
            slider_intensity: Rc::new(RefCell::new(Slider::new(0.0, 0.0, 1.0))),
            azimuth_id: None,
            elevation_id: None,
            intensity_id: None,
        }
    }

    /// Register widgets with the [`Ui`] so they receive mouse events.
    pub fn configure_ui(&mut self, ui: &mut UiTree<EditorApp>) {
        self.azimuth_id = Some(ui.add_node(Box::new(self.slider_azimuth.clone()), None));
        self.elevation_id = Some(ui.add_node(Box::new(self.slider_elevation.clone()), None));
        self.intensity_id = Some(ui.add_node(Box::new(self.slider_intensity.clone()), None));
    }

    /// Draw the panel and push any light changes to the renderer.
    pub fn draw(
        &mut self,
        gui: &mut GuiBatch,
        font: Option<&Font>,
        ctx: &mut AppContext,
        panel_top: f32, // y offset where the panel starts
    ) {
        let win_w = ctx.window_size.0 as f32;
        let panel_x = win_w - PANEL_W;
        let slider_x = panel_x + MARGIN;
        let slider_w = (win_w - slider_x - MARGIN).max(40.0);

        // Panel background strip.
        gui.push_quad(ferrous_gui::GuiQuad {
            pos: [panel_x, panel_top],
            size: [PANEL_W, 130.0],
            uv0: [0.0, 0.0],
            uv1: [1.0, 1.0],
            color: [0.08, 0.10, 0.14, 0.90],
            radii: [0.0; 4],
            tex_index: 0,
            flags: 0,
        });

        let Some(font) = font else {
            return;
        };

        gui.draw_text(
            font,
            "── Global Light ──",
            [panel_x + MARGIN, panel_top + 8.0],
            11.0,
            [1.0, 0.85, 0.4, 1.0],
        );

        // Reposition sliders and draw them.
        let mut draw_sl = |sl: &Rc<RefCell<Slider<EditorApp>>>, node_id_opt: Option<ferrous_gui::NodeId>, y: f32, label: &str| {
            let rect = ferrous_gui::Rect::new(slider_x, y, slider_w, SLIDER_H);
            
            // Manual draw
            let mut cmds: Vec<ferrous_gui::RenderCommand> = Vec::new();
            let mut dc = ferrous_gui::DrawContext {
                node_id: node_id_opt.unwrap_or_default(),
                rect,
                theme: ferrous_gui::theme::Theme::default(),
            };
            sl.borrow().draw(&mut dc, &mut cmds);
            for cmd in cmds {
                cmd.to_batches(gui, Some(font));
            }
            
            gui.draw_text(font, label, [slider_x, y - 10.0], 10.0, [0.7, 0.7, 0.7, 1.0]);
        };

        draw_sl(&self.slider_azimuth, self.azimuth_id, panel_top + 28.0, "Azimuth");
        draw_sl(&self.slider_elevation, self.elevation_id, panel_top + 62.0, "Elevation");
        draw_sl(&self.slider_intensity, self.intensity_id, panel_top + 96.0, "Intensity");

        let az_v = self.slider_azimuth.borrow().value;
        let el_v = self.slider_elevation.borrow().value;
        let int_v = self.slider_intensity.borrow().value;

        let azimuth = az_v * std::f32::consts::TAU; // 0 → 2π
        let elevation = (el_v * 2.0 - 1.0) * std::f32::consts::FRAC_PI_2; // −π/2 → +π/2
        let intensity = int_v * MAX_INTENSITY;

        // Direction vector: pointing FROM the light TOWARD the origin.
        let dir_x = elevation.cos() * azimuth.sin();
        let dir_y = elevation.sin();
        let dir_z = elevation.cos() * azimuth.cos();
        let dir = [-dir_x, -dir_y, -dir_z];

        ctx.render
            .set_directional_light(dir, [1.0, 0.98, 0.95], intensity);

        // Draw rows.
        gui.draw_text(
            font,
            "Azimuth",
            [slider_x, panel_top + 26.0],
            10.0,
            [0.75, 0.75, 0.75, 1.0],
        );
        let mut cmds: Vec<ferrous_gui::RenderCommand> = Vec::new();
        self.slider_azimuth.borrow().draw(&mut ferrous_gui::DrawContext {
            node_id: self.azimuth_id.unwrap_or_default(),
            rect: ferrous_gui::Rect::new(slider_x, panel_top + 28.0, slider_w, SLIDER_H),
            theme: ferrous_gui::theme::Theme::default(),
        }, &mut cmds);
        for cmd in cmds { cmd.to_batches(gui, Some(font)); }
        let val_x = slider_x + slider_w + 4.0;
        gui.draw_text(
            font,
            &format!("{:.0}°", az_v * 360.0),
            [val_x, panel_top + 28.0],
            10.0,
            [1.0, 0.85, 0.4, 1.0],
        );

        gui.draw_text(
            font,
            "Elevation",
            [slider_x, panel_top + 60.0],
            10.0,
            [0.75, 0.75, 0.75, 1.0],
        );
        let mut cmds_el: Vec<ferrous_gui::RenderCommand> = Vec::new();
        self.slider_elevation.borrow().draw(&mut ferrous_gui::DrawContext {
            node_id: self.elevation_id.unwrap_or_default(),
            rect: ferrous_gui::Rect::new(slider_x, panel_top + 62.0, slider_w, SLIDER_H),
            theme: ferrous_gui::theme::Theme::default(),
        }, &mut cmds_el);
        for cmd in cmds_el { cmd.to_batches(gui, Some(font)); }
        let el_deg = (el_v * 2.0 - 1.0) * 90.0;
        gui.draw_text(
            font,
            &format!("{:.0}°", el_deg),
            [val_x, panel_top + 62.0],
            10.0,
            [1.0, 0.85, 0.4, 1.0],
        );

        gui.draw_text(
            font,
            "Intensity",
            [slider_x, panel_top + 94.0],
            10.0,
            [0.75, 0.75, 0.75, 1.0],
        );
        let mut cmds_in: Vec<ferrous_gui::RenderCommand> = Vec::new();
        self.slider_intensity.borrow().draw(&mut ferrous_gui::DrawContext {
            node_id: self.intensity_id.unwrap_or_default(),
            rect: ferrous_gui::Rect::new(slider_x, panel_top + 96.0, slider_w, SLIDER_H),
            theme: ferrous_gui::theme::Theme::default(),
        }, &mut cmds_in);
        for cmd in cmds_in { cmd.to_batches(gui, Some(font)); }
        gui.draw_text(
            font,
            &format!("{:.2}", intensity),
            [val_x, panel_top + 96.0],
            10.0,
            [1.0, 0.85, 0.4, 1.0],
        );
    }
}

impl Default for GlobalLightPanel {
    fn default() -> Self {
        Self::new()
    }
}
