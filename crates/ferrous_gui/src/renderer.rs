//! `renderer` module — Bridge to the GPU UI renderer.
//! 
//! This module has been refactored to use `ferrous_ui_render`. 
//! Primitives and the main renderer now live in that crate.

pub use ferrous_ui_render::{
    GuiBatch, GuiQuad, GuiRenderer, TextBatch, TextQuad, MAX_TEXTURE_SLOTS, TEXTURED_BIT,
};

/// Dibuja solo el borde (stroke) de un rectángulo, con radio opcional.
/// El borde es *inset* (dibujado hacia adentro del rect original).
/// `stroke_px`: grosor del borde en píxeles.
/// `radius`: radio de las esquinas (0.0 = esquinas rectas).
///
/// TODO: Move this helper to `ferrous_ui_render` if it's generally useful.
pub fn rect_stroke(
    batch: &mut GuiBatch,
    x: f32,
    y: f32,
    w: f32,
    h: f32,
    color: [f32; 4],
    radius: f32,
    stroke_px: f32,
) {
    if radius == 0.0 {
        // top
        batch.rect(x, y, w, stroke_px, color);
        // bottom
        batch.rect(x, y + h - stroke_px, w, stroke_px, color);
        // left
        batch.rect(x, y + stroke_px, stroke_px, h - 2.0 * stroke_px, color);
        // right
        batch.rect(
            x + w - stroke_px,
            y + stroke_px,
            stroke_px,
            h - 2.0 * stroke_px,
            color,
        );
    } else {
        batch.rect_r(x, y, w, stroke_px, color, radius);
        batch.rect_r(x, y + h - stroke_px, w, stroke_px, color, radius);
        batch.rect(x, y + stroke_px, stroke_px, h - 2.0 * stroke_px, color);
        batch.rect(
            x + w - stroke_px,
            y + stroke_px,
            stroke_px,
            h - 2.0 * stroke_px,
            color,
        );
    }
}

/// Dibuja una línea de (x1,y1) a (x2,y2).
pub fn line(
    batch: &mut GuiBatch,
    x1: f32,
    y1: f32,
    x2: f32,
    y2: f32,
    thickness: f32,
    color: [f32; 4],
) {
    let width = thickness.max(1.0);
    let delta_x = x2 - x1;
    let delta_y = y2 - y1;
    let length = (delta_x * delta_x + delta_y * delta_y).sqrt();
    if length <= f32::EPSILON {
        batch.rect_r(
            x1 - width * 0.5,
            y1 - width * 0.5,
            width,
            width,
            color,
            width * 0.5,
        );
        return;
    }

    let step = (width * 0.5).max(1.0);
    let segments = (length / step).ceil() as u32;
    for segment_index in 0..=segments {
        let t = segment_index as f32 / segments as f32;
        let x = x1 + delta_x * t;
        let y = y1 + delta_y * t;
        batch.rect_r(
            x - width * 0.5,
            y - width * 0.5,
            width,
            width,
            color,
            width * 0.5,
        );
    }
}
