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
use ferrous_assets::font::Font;
use ferrous_gui::{GuiBatch, Slider, TextBatch, Ui};

use crate::ui::material_inspector::PANEL_W;

const MARGIN: f32 = 10.0;
const SLIDER_H: f32 = 14.0;
const MAX_INTENSITY: f32 = 50.0;

/// Panel that controls the single global directional light.
pub struct GlobalLightPanel {
    pub slider_azimuth: Rc<RefCell<Slider>>,
    pub slider_elevation: Rc<RefCell<Slider>>,
    pub slider_intensity: Rc<RefCell<Slider>>,
}

impl GlobalLightPanel {
    pub fn new() -> Self {
        // Defaults: sun from upper-right at 45° elevation, intensity 3.5.
        // azimuth 0.125 → 45° (from the right-front diagonal)
        // elevation 0.75 → 45° above horizon  (good for PBR highlights)
        // intensity 0.875 of [0, 4] → 3.5 (bright enough to see specular)
        Self {
            slider_azimuth: Rc::new(RefCell::new(Slider::new(0.0, 0.0, 160.0, SLIDER_H, 0.125))),
            slider_elevation: Rc::new(RefCell::new(Slider::new(0.0, 0.0, 160.0, SLIDER_H, 0.75))),
            slider_intensity: Rc::new(RefCell::new(Slider::new(0.0, 0.0, 160.0, SLIDER_H, 0.875))),
        }
    }

    /// Register widgets with the [`Ui`] so they receive mouse events.
    pub fn configure_ui(&mut self, ui: &mut Ui) {
        ui.add(self.slider_azimuth.clone());
        ui.add(self.slider_elevation.clone());
        ui.add(self.slider_intensity.clone());
    }

    /// Draw the panel and push any light changes to the renderer.
    pub fn draw(
        &mut self,
        gui: &mut GuiBatch,
        text: &mut TextBatch,
        font: Option<&Font>,
        ctx: &mut AppContext,
        panel_top: f32, // y offset where the panel starts
    ) {
        let win_w = ctx.window_size.0 as f32;
        let panel_x = win_w - PANEL_W;
        let slider_x = panel_x + MARGIN;
        let slider_w = (win_w - slider_x - MARGIN).max(40.0);

        // Panel background strip.
        gui.push(ferrous_gui::GuiQuad {
            pos: [panel_x, panel_top],
            size: [PANEL_W, 130.0],
            color: [0.08, 0.10, 0.14, 0.90],
            radii: [0.0; 4],
            flags: 0,
        });

        let Some(font) = font else {
            return;
        };

        text.draw_text(
            font,
            "── Global Light ──",
            [panel_x + MARGIN, panel_top + 8.0],
            11.0,
            [1.0, 0.85, 0.4, 1.0],
        );

        // Reposition sliders.
        let set_slider = |sl: &Rc<RefCell<Slider>>, y: f32| {
            let mut s = sl.borrow_mut();
            s.rect[0] = slider_x;
            s.rect[1] = y;
            s.rect[2] = slider_w;
        };
        set_slider(&self.slider_azimuth, panel_top + 28.0);
        set_slider(&self.slider_elevation, panel_top + 62.0);
        set_slider(&self.slider_intensity, panel_top + 96.0);

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

        ctx.renderer
            .set_directional_light(dir, [1.0, 0.98, 0.95], intensity);

        // Draw rows.
        text.draw_text(
            font,
            "Azimuth",
            [slider_x, panel_top + 26.0],
            10.0,
            [0.75, 0.75, 0.75, 1.0],
        );
        self.slider_azimuth.borrow().draw(gui);
        let val_x = slider_x + self.slider_azimuth.borrow().rect[2] + 4.0;
        text.draw_text(
            font,
            &format!("{:.0}°", az_v * 360.0),
            [val_x, panel_top + 28.0],
            10.0,
            [1.0, 0.85, 0.4, 1.0],
        );

        text.draw_text(
            font,
            "Elevation",
            [slider_x, panel_top + 60.0],
            10.0,
            [0.75, 0.75, 0.75, 1.0],
        );
        self.slider_elevation.borrow().draw(gui);
        let el_deg = (el_v * 2.0 - 1.0) * 90.0;
        text.draw_text(
            font,
            &format!("{:.0}°", el_deg),
            [val_x, panel_top + 62.0],
            10.0,
            [1.0, 0.85, 0.4, 1.0],
        );

        text.draw_text(
            font,
            "Intensity",
            [slider_x, panel_top + 94.0],
            10.0,
            [0.75, 0.75, 0.75, 1.0],
        );
        self.slider_intensity.borrow().draw(gui);
        text.draw_text(
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
