//! MaterialInspector — Phase 13.
//!
//! Displays and edits every PBR parameter on the selected entity's
//! [`MaterialDescriptor`].  When the user drags a slider or changes the colour
//! picker the descriptor is written back to the [`World`] *and* pushed to the
//! renderer via `update_material_params`, so the viewport reflects the change
//! on the very same frame.
//!
//! ## Layout (right-side panel, x = win_w − PANEL_W)
//!
//! ```text
//! ┌─────────────────────────────┐  ← PANEL_W wide
//! │  ── Material Inspector ──   │
//! │  base_color   [picker]      │
//! │  metallic     [slider]      │
//! │  roughness    [slider]      │
//! │  ao_strength  [slider]      │
//! │  emissive_str [slider]      │
//! │  double_sided [checkbox]    │
//! │  alpha_mode   [◉/○/○]       │
//! └─────────────────────────────┘
//! ```
//!
//! All widget positions are recomputed on each `draw` call so that the panel
//! sticks to the right edge even when the window is resized.

use std::cell::RefCell;
use std::rc::Rc;

use ferrous_app::AppContext;
use ferrous_assets::font::Font;
use ferrous_core::scene::{AlphaMode, MaterialDescriptor};
use ferrous_core::Handle;
use ferrous_gui::{ColorPicker, GuiBatch, PickerShape, Slider, TextBatch, Ui};

// ─── Layout constants ───────────────────────────────────────────────────────

/// Width of the inspector panel in pixels.
pub const PANEL_W: f32 = 220.0;
const MARGIN: f32 = 10.0;
const LABEL_H: f32 = 16.0;
const SLIDER_H: f32 = 14.0;
const PICKER_SIZE: f32 = 44.0;

// ─── Helpers ────────────────────────────────────────────────────────────────

/// Small checkbox: returns the `GuiQuad` that should be pushed to the batch.
/// The actual *click* detection is handled by the inspector's hit-test in
/// `handle_checkbox_click`.
fn draw_checkbox(gui: &mut GuiBatch, x: f32, y: f32, size: f32, checked: bool) {
    // outer border
    gui.push(ferrous_gui::GuiQuad {
        pos: [x, y],
        size: [size, size],
        color: [0.35, 0.35, 0.35, 1.0],
        radii: [3.0; 4],
        flags: 0,
    });
    // inner fill when checked
    if checked {
        let inset = 3.0;
        gui.push(ferrous_gui::GuiQuad {
            pos: [x + inset, y + inset],
            size: [size - inset * 2.0, size - inset * 2.0],
            color: [0.3, 0.8, 0.5, 1.0],
            radii: [1.5; 4],
            flags: 0,
        });
    }
}

/// Radio button circle.
fn draw_radio(gui: &mut GuiBatch, cx: f32, cy: f32, r: f32, selected: bool) {
    // outer ring
    gui.push(ferrous_gui::GuiQuad {
        pos: [cx - r, cy - r],
        size: [r * 2.0, r * 2.0],
        color: [0.4, 0.4, 0.4, 1.0],
        radii: [r; 4],
        flags: 0,
    });
    if selected {
        let ir = r * 0.55;
        gui.push(ferrous_gui::GuiQuad {
            pos: [cx - ir, cy - ir],
            size: [ir * 2.0, ir * 2.0],
            color: [0.3, 0.75, 1.0, 1.0],
            radii: [ir; 4],
            flags: 0,
        });
    }
}

// ─── MaterialInspector ──────────────────────────────────────────────────────

/// Inspector panel for the selected entity's PBR material.
pub struct MaterialInspector {
    // ── Widgets (registered with Ui so they receive mouse events) ──────────
    pub slider_metallic: Rc<RefCell<Slider>>,
    pub slider_roughness: Rc<RefCell<Slider>>,
    pub slider_ao: Rc<RefCell<Slider>>,
    pub slider_emissive: Rc<RefCell<Slider>>,
    pub color_picker: Rc<RefCell<ColorPicker>>,

    // ── Internal state ─────────────────────────────────────────────────────
    /// Cached descriptor so we know when something changed.
    last_desc: MaterialDescriptor,
    /// Whether we already registered widgets with the Ui.
    registered: bool,
}

impl MaterialInspector {
    pub fn new() -> Self {
        let picker =
            ColorPicker::new(0.0, 0.0, PICKER_SIZE, PICKER_SIZE).with_shape(PickerShape::Rect);
        Self {
            slider_metallic: Rc::new(RefCell::new(Slider::new(0.0, 0.0, 160.0, SLIDER_H, 0.0))),
            slider_roughness: Rc::new(RefCell::new(Slider::new(0.0, 0.0, 160.0, SLIDER_H, 0.5))),
            slider_ao: Rc::new(RefCell::new(Slider::new(0.0, 0.0, 160.0, SLIDER_H, 1.0))),
            slider_emissive: Rc::new(RefCell::new(Slider::new(0.0, 0.0, 160.0, SLIDER_H, 0.0))),
            color_picker: Rc::new(RefCell::new(picker)),
            last_desc: MaterialDescriptor::default(),
            registered: false,
        }
    }

    /// Register widgets with the [`Ui`] so that they receive hit-test /
    /// drag events.  Must be called once from `EditorApp::configure_ui`.
    pub fn configure_ui(&mut self, ui: &mut Ui) {
        ui.add(self.slider_metallic.clone());
        ui.add(self.slider_roughness.clone());
        ui.add(self.slider_ao.clone());
        ui.add(self.slider_emissive.clone());
        ui.add(self.color_picker.clone());
        self.registered = true;
    }

    /// Sync slider/picker values from an external descriptor.
    ///
    /// Call this whenever the selection changes so that the widgets show the
    /// correct initial state instead of stale values.
    pub fn sync_from_descriptor(&mut self, desc: &MaterialDescriptor) {
        self.slider_metallic.borrow_mut().value = desc.metallic.clamp(0.0, 1.0);
        self.slider_roughness.borrow_mut().value = desc.roughness.clamp(0.0, 1.0);
        self.slider_ao.borrow_mut().value = desc.ao_strength.clamp(0.0, 1.0);
        // emissive_strength can be > 1 — we map [0, 5] to [0, 1]
        self.slider_emissive.borrow_mut().value = (desc.emissive_strength / 5.0).clamp(0.0, 1.0);
        self.color_picker.borrow_mut().colour = desc.base_color;
        self.last_desc = desc.clone();
    }

    /// Reposition all widgets to stick to the right edge of the window.
    fn reposition_widgets(&self, panel_x: f32, win_w: f32) {
        let slider_x = panel_x + MARGIN;
        let slider_w = win_w - slider_x - MARGIN;
        let slider_w = slider_w.max(40.0);

        // metallic  — just below header + colour picker row
        let base_y = 70.0; // header + picker row height

        let set_slider = |sl: &Rc<RefCell<Slider>>, y: f32| {
            let mut s = sl.borrow_mut();
            s.rect[0] = slider_x;
            s.rect[1] = y;
            s.rect[2] = slider_w;
        };

        set_slider(&self.slider_metallic, base_y);
        set_slider(&self.slider_roughness, base_y + 34.0);
        set_slider(&self.slider_ao, base_y + 68.0);
        set_slider(&self.slider_emissive, base_y + 102.0);

        // colour picker sits in the header area
        let mut cp = self.color_picker.borrow_mut();
        cp.rect[0] = panel_x + MARGIN;
        cp.rect[1] = 28.0;
        cp.rect[2] = PICKER_SIZE;
        cp.rect[3] = PICKER_SIZE;
    }

    // ─── Main draw method ────────────────────────────────────────────────

    /// Draw the inspector and return `true` if any value changed (so the
    /// caller can push the updated descriptor to the renderer).
    ///
    /// If `selected` is `None` draws a "Select an object" placeholder.
    pub fn draw(
        &mut self,
        selected: Option<Handle>,
        gui: &mut GuiBatch,
        text: &mut TextBatch,
        font: Option<&Font>,
        ctx: &mut AppContext,
    ) -> bool {
        let (win_w, win_h) = (ctx.window_size.0 as f32, ctx.window_size.1 as f32);
        let panel_x = win_w - PANEL_W;

        // Reposition widgets every frame (handles resize).
        self.reposition_widgets(panel_x, win_w);

        // ── Panel background ────────────────────────────────────────────────
        gui.push(ferrous_gui::GuiQuad {
            pos: [panel_x, 0.0],
            size: [PANEL_W, win_h],
            color: [0.10, 0.10, 0.12, 0.88],
            radii: [0.0; 4],
            flags: 0,
        });

        let Some(font) = font else {
            return false;
        };

        // ── Title ────────────────────────────────────────────────────────────
        text.draw_text(
            font,
            "── Material Inspector ──",
            [panel_x + MARGIN, 10.0],
            12.0,
            [0.6, 0.8, 1.0, 1.0],
        );

        // ── "No selection" guard ─────────────────────────────────────────────
        let Some(handle) = selected else {
            text.draw_text(
                font,
                "Select an object",
                [panel_x + MARGIN, 36.0],
                13.0,
                [0.5, 0.5, 0.5, 1.0],
            );
            return false;
        };

        let Some(element) = ctx.world.get(handle) else {
            return false;
        };

        // Clone the descriptor so we can compare before/after.
        let mut desc = element.material.descriptor.clone();
        let mat_handle = element.material.handle;

        let mut changed = false;

        // ── Colour picker ────────────────────────────────────────────────────
        text.draw_text(
            font,
            "Base Color",
            [panel_x + MARGIN + PICKER_SIZE + 6.0, 30.0],
            12.0,
            [0.85, 0.85, 0.85, 1.0],
        );
        // Sync colour from world in case something else changed it.
        {
            let mut cp = self.color_picker.borrow_mut();
            if cp.colour[0] != desc.base_color[0]
                || cp.colour[1] != desc.base_color[1]
                || cp.colour[2] != desc.base_color[2]
            {
                // Only override if the picker wasn't just interacted with.
                if !cp.pressed {
                    cp.colour = desc.base_color;
                }
            }
            // Read picker value into desc.
            let c = cp.colour;
            if (c[0] - desc.base_color[0]).abs() > 1e-4
                || (c[1] - desc.base_color[1]).abs() > 1e-4
                || (c[2] - desc.base_color[2]).abs() > 1e-4
            {
                desc.base_color = c;
                changed = true;
            }
        }
        // Draw the colour picker widget.
        {
            let cp = self.color_picker.borrow();
            cp.draw(gui);
        }

        // ── Sliders ──────────────────────────────────────────────────────────
        let slider_x = panel_x + MARGIN;
        let base_y = 74.0;

        // Helper: label + slider row
        let mut draw_slider_row = |label: &str, sl: &Rc<RefCell<Slider>>, y: f32| {
            text.draw_text(
                font,
                label,
                [slider_x, y - LABEL_H],
                11.0,
                [0.75, 0.75, 0.75, 1.0],
            );
            sl.borrow().draw(gui);
        };

        draw_slider_row("Metallic", &self.slider_metallic, base_y);
        draw_slider_row("Roughness", &self.slider_roughness, base_y + 34.0);
        draw_slider_row("AO Strength", &self.slider_ao, base_y + 68.0);
        draw_slider_row(
            "Emissive Strength (×5)",
            &self.slider_emissive,
            base_y + 102.0,
        );

        // Read slider values into desc.
        let new_metallic = self.slider_metallic.borrow().value;
        if (new_metallic - desc.metallic).abs() > 1e-5 {
            desc.metallic = new_metallic;
            changed = true;
        }
        let new_roughness = self.slider_roughness.borrow().value;
        if (new_roughness - desc.roughness).abs() > 1e-5 {
            desc.roughness = new_roughness;
            changed = true;
        }
        let new_ao = self.slider_ao.borrow().value;
        if (new_ao - desc.ao_strength).abs() > 1e-5 {
            desc.ao_strength = new_ao;
            changed = true;
        }
        let new_emissive = self.slider_emissive.borrow().value * 5.0;
        if (new_emissive - desc.emissive_strength).abs() > 1e-5 {
            desc.emissive_strength = new_emissive;
            // Also copy into the emissive colour channels so the shader sees it.
            desc.emissive = [1.0, 1.0, 1.0];
            changed = true;
        }

        // Draw numeric values next to sliders.
        let val_x = slider_x + self.slider_metallic.borrow().rect[2] + 4.0;
        text.draw_text(
            font,
            &format!("{:.2}", desc.metallic),
            [val_x, base_y],
            10.0,
            [0.6, 0.9, 0.6, 1.0],
        );
        text.draw_text(
            font,
            &format!("{:.2}", desc.roughness),
            [val_x, base_y + 34.0],
            10.0,
            [0.6, 0.9, 0.6, 1.0],
        );
        text.draw_text(
            font,
            &format!("{:.2}", desc.ao_strength),
            [val_x, base_y + 68.0],
            10.0,
            [0.6, 0.9, 0.6, 1.0],
        );
        text.draw_text(
            font,
            &format!("{:.2}", desc.emissive_strength),
            [val_x, base_y + 102.0],
            10.0,
            [0.6, 0.9, 0.6, 1.0],
        );

        // ── Double-sided checkbox ────────────────────────────────────────────
        let cb_y = base_y + 128.0;
        let cb_size = 14.0;
        draw_checkbox(gui, slider_x, cb_y, cb_size, desc.double_sided);
        text.draw_text(
            font,
            "Double-sided",
            [slider_x + cb_size + 6.0, cb_y + 2.0],
            12.0,
            [0.85, 0.85, 0.85, 1.0],
        );

        // Hit-test the checkbox (simple AABB on current mouse position).
        {
            let (mx, my) = ctx.input.mouse_position();
            let (mx, my) = (mx as f32, my as f32);
            if ctx
                .input
                .button_just_pressed(ferrous_app::MouseButton::Left)
            {
                if mx >= slider_x && mx <= slider_x + cb_size && my >= cb_y && my <= cb_y + cb_size
                {
                    desc.double_sided = !desc.double_sided;
                    changed = true;
                }
            }
        }

        // ── Alpha Mode radio buttons ─────────────────────────────────────────
        let alpha_y = cb_y + 28.0;
        text.draw_text(
            font,
            "Alpha Mode",
            [slider_x, alpha_y],
            11.0,
            [0.75, 0.75, 0.75, 1.0],
        );

        let modes = [
            ("Opaque", AlphaMode::Opaque),
            ("Mask", AlphaMode::Mask { cutoff: 0.5 }),
            ("Blend", AlphaMode::Blend),
        ];

        let r = 6.0;
        for (i, (label, mode)) in modes.iter().enumerate() {
            let rx = slider_x + i as f32 * 64.0 + r;
            let ry = alpha_y + 18.0 + r;
            let selected_mode =
                std::mem::discriminant(&desc.alpha_mode) == std::mem::discriminant(mode);
            draw_radio(gui, rx, ry, r, selected_mode);
            text.draw_text(
                font,
                label,
                [rx + r + 3.0, ry - r + 2.0],
                10.0,
                if selected_mode {
                    [0.4, 0.85, 1.0, 1.0]
                } else {
                    [0.65, 0.65, 0.65, 1.0]
                },
            );

            // Hit-test the radio button.
            let (mx, my) = ctx.input.mouse_position();
            let (mx, my) = (mx as f32, my as f32);
            if ctx
                .input
                .button_just_pressed(ferrous_app::MouseButton::Left)
            {
                let dx = mx - rx;
                let dy = my - ry;
                if dx * dx + dy * dy <= (r * 2.2) * (r * 2.2) {
                    if !selected_mode {
                        desc.alpha_mode = mode.clone();
                        changed = true;
                    }
                }
            }
        }

        // ── Flush changes to World + Renderer ────────────────────────────────
        if changed {
            ctx.world.set_material_descriptor(handle, desc.clone());
            ctx.render.update_material(mat_handle, &desc);
            self.last_desc = desc;
        }

        changed
    }
}

impl Default for MaterialInspector {
    fn default() -> Self {
        Self::new()
    }
}
