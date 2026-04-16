//! Conversion utilities from abstract UI commands to GPU batches.
//!
//! Converts `RenderCommand` from the UI tree into `GuiQuad`/`TextQuad` instances
//! ready for the GPU. The performance key here is using `GRADIENT_BIT`
//! for the GPU shader to interpolate gradients — **without** the 64-strip CPU loop.

use crate::gpu_types::{GuiQuad, SvgCommand};
use crate::gui_batch::GuiBatch;
use crate::{GRADIENT_BIT, GRADIENT_V_BIT, GRADIENT_RADIAL_BIT, BORDER_BIT, TEXTURED_BIT};
use ferrous_ui_core::background::Background;

/// Extension to convert abstract render commands into GPU-optimized batches.
pub trait ToBatches {
    #[cfg(feature = "text")]
    fn to_batches(&self, batch: &mut GuiBatch, font: Option<&ferrous_assets::Font>, z: f32, node_id: u32);
    #[cfg(not(feature = "text"))]
    fn to_batches(&self, quad_batch: &mut GuiBatch, z: f32, node_id: u32);
}

// ─── Helper: Background → GuiQuad(s) ────────────────────────────────────────

/// Convierte un `Background` en 1 o más `GuiQuad` optimizados.
///
/// **Degradados lineales y radiales de 2 stops** se resuelven en **1 único quad**
/// usando `GRADIENT_BIT`/`GRADIENT_RADIAL_BIT` en el shader — cero strips de CPU.
///
/// Solo fondos complejos (N stops, procedurales, texturas) usan el path de CPU rasterización.
fn background_to_quads(
    batch: &mut GuiBatch,
    bg: &Background,
    rect: ferrous_ui_core::Rect,
    radii: [f32; 4],
    z: f32,
    node_id: u32,
) {
    match bg {
        Background::None => {}

        Background::Solid(color) => {
            batch.push_quad(GuiQuad::solid(
                [rect.x, rect.y],
                [rect.width, rect.height],
                *color,
                radii,
                if color[3] >= 1.0 { crate::OPAQUE_BIT } else { 0 },
                z,
                node_id,
            ));
            batch.update_last_segment();
        }

        Background::LinearGradient { stops, angle } => {
            if stops.is_empty() {
                return;
            }
            if stops.len() == 2 {
                // Caso óptimo: 2 stops → 1 quad con interpolación GPU pura
                let color_a = stops[0].color;
                let color_b = stops[1].color;
                let [dx, _dy] = angle.direction();
                // dx ≈ 0: gradiente vertical (top→bottom); dx ≈ 1: horizontal (left→right)
                let grad_flags = if dx.abs() > 0.5 {
                    GRADIENT_BIT
                } else {
                    GRADIENT_BIT | GRADIENT_V_BIT
                };
                batch.push_quad(GuiQuad::gradient(
                    [rect.x, rect.y],
                    [rect.width, rect.height],
                    color_a,
                    color_b,
                    radii,
                    if color_a[3] >= 1.0 && color_b[3] >= 1.0 { grad_flags | crate::OPAQUE_BIT } else { grad_flags },
                    z,
                    node_id,
                ));
                batch.update_last_segment();
            } else {
                // N stops: rasterizar en CPU como tiras (calidad máxima, perf menor).
                // Se usa solo cuando hay más de 2 color stops.
                let n = (rect.width.ceil() as u32).max(2).min(512);
                let strip_w = rect.width / n as f32;
                for i in 0..n {
                    let u = (i as f32 + 0.5) / n as f32;
                    let color = bg.sample(u, 0.5);
                    batch.push_quad(GuiQuad::solid(
                        [rect.x + i as f32 * strip_w, rect.y],
                        [strip_w + 0.5, rect.height],
                        color,
                        if i == 0 || i == n - 1 { radii } else { [0.0; 4] },
                        0,
                        z,
                        0, // Strips don't need individual IDs usually, or we could pass node_id
                    ));
                }
                batch.update_last_segment();
            }
        }

        Background::RadialGradient { stops, .. } => {
            if stops.is_empty() {
                return;
            }
            if stops.len() == 2 {
                // 2 stops: 1 quad con GRADIENT_RADIAL_BIT
                let color_a = stops[0].color; // centro
                let color_b = stops[1].color; // borde
                batch.push_quad(GuiQuad::gradient(
                    [rect.x, rect.y],
                    [rect.width, rect.height],
                    color_a,
                    color_b,
                    radii,
                    GRADIENT_BIT | GRADIENT_RADIAL_BIT,
                    z,
                    node_id,
                ));
                batch.update_last_segment();
            } else {
                // N stops: tiras circulares desde el centro
                let n = 64u32;
                for i in 0..n {
                    let u = (i as f32 + 0.5) / n as f32;
                    let color = bg.sample(u, 0.5);
                    let strip_w = rect.width / n as f32;
                    batch.push_quad(GuiQuad::solid(
                        [rect.x + i as f32 * strip_w, rect.y],
                        [strip_w + 0.5, rect.height],
                        color,
                        [0.0; 4],
                        0,
                        z,
                        0,
                    ));
                }
                batch.update_last_segment();
            }
        }

        Background::ConicGradient { stops, .. } => {
            if stops.is_empty() {
                return;
            }
            // Gradiente cónico: siempre tiras angulares (no hay shortcut de 1 quad)
            let n = 128u32;
            for i in 0..n {
                let u = (i as f32 + 0.5) / n as f32;
                let color = bg.sample(u, 0.5);
                let strip_w = rect.width / n as f32;
                batch.push_quad(GuiQuad::solid(
                    [rect.x + i as f32 * strip_w, rect.y],
                    [strip_w + 0.5, rect.height],
                    color,
                    [0.0; 4],
                    0,
                    z,
                    0,
                ));
            }
            batch.update_last_segment();
        }

        Background::Procedural(f) => {
            // Rasterizado CPU con resolución adaptativa según el tamaño del rect
            let n_x = (rect.width.ceil() as u32).max(1).min(256);
            let n_y = (rect.height.ceil() as u32).max(1).min(256);
            let step_x = rect.width / n_x as f32;
            let step_y = rect.height / n_y as f32;
            for yi in 0..n_y {
                for xi in 0..n_x {
                    let u = (xi as f32 + 0.5) / n_x as f32;
                    let v = (yi as f32 + 0.5) / n_y as f32;
                    let color = f(u, v);
                    batch.push_quad(GuiQuad::solid(
                        [rect.x + xi as f32 * step_x, rect.y + yi as f32 * step_y],
                        [step_x + 0.5, step_y + 0.5],
                        color,
                        [0.0; 4],
                        0,
                        z,
                        node_id,
                    ));
                }
            }
            batch.update_last_segment();
        }

        Background::Texture { sampler, .. } => {
            // Textura procedural: 1 quad con color promedio (la textura real se cargaría como Image)
            let color = sampler(0.5, 0.5);
            batch.push_quad(GuiQuad::solid(
                [rect.x, rect.y],
                [rect.width, rect.height],
                color,
                radii,
                0,
                z,
                node_id,
            ));
            batch.update_last_segment();
        }
    }
}

// ─── ToBatches impl ──────────────────────────────────────────────────────────

impl ToBatches for ferrous_ui_core::RenderCommand {
    #[cfg(feature = "text")]
    fn to_batches(&self, batch: &mut GuiBatch, font: Option<&ferrous_assets::Font>, z: f32, node_id: u32) {
        use ferrous_ui_core::RenderCommand;
        match self {
            RenderCommand::Quad { rect, color, radii, flags } => {
                batch.push_quad(GuiQuad {
                    pos: [rect.x, rect.y],
                    size: [rect.width, rect.height],
                    uv0: [0.0, 0.0],
                    uv1: [1.0, 1.0],
                    color: *color,
                    color_b: [0.0; 4],
                    radii: *radii,
                    tex_index: 0,
                    flags: if color[3] >= 1.0 { *flags | crate::OPAQUE_BIT } else { *flags },
                    z_order: z,
                    node_id,
                });
                batch.update_last_segment();
            }

            #[cfg(feature = "assets")]
            RenderCommand::Image { rect, texture, uv0, uv1, color } => {
                let idx = batch.reserve_texture_slot(texture.clone());
                batch.push_quad(GuiQuad {
                    pos: [rect.x, rect.y],
                    size: [rect.width, rect.height],
                    uv0: *uv0,
                    uv1: *uv1,
                    color: *color,
                    color_b: [0.0; 4],
                    radii: [0.0; 4],
                    tex_index: idx,
                    flags: TEXTURED_BIT,
                    z_order: z,
                    node_id,
                });
                batch.update_last_segment();
            }

            RenderCommand::Text { rect, text, color, font_size, align } => {
                if let Some(font) = font {
                    let text_w = GuiBatch::measure_text(font, text, *font_size);
                    let text_h = *font_size;
                    let x = align.resolve_x(rect.x, rect.width, text_w, 4.0);
                    let y = align.resolve_y(rect.y, rect.height, text_h, 4.0);
                    batch.draw_text_internal(font, text, [x, y], *font_size, *color, z, node_id);
                }
            }

            RenderCommand::Icon { name, rect, color } => {
                if let Some(font) = font {
                    if let Some(icons) = &font.icons {
                        if let Some(metrics) = icons.icons.get(name) {
                            batch.push_icon(crate::gpu_types::TextQuad {
                                pos: [rect.x, rect.y],
                                size: [rect.width, rect.height],
                                uv0: [metrics.uv[0], metrics.uv[1]],
                                uv1: [metrics.uv[2], metrics.uv[3]],
                                color: *color,
                                z_order: z,
                                node_id,
                            });
                        }
                    }
                }
            }

            RenderCommand::Svg { rect, color, mesh } => {
                batch.push_svg(SvgCommand {
                    mesh: mesh.clone(),
                    pos: [rect.x, rect.y],
                    color: *color,
                    z,
                });
            }

            RenderCommand::PushClip { rect } => {
                batch.push_clip(*rect);
            }
            RenderCommand::PopClip => {
                batch.pop_clip();
            }

            // ─── GRADIENT: 1 quad GPU (linear/radial 2-stop) o strips CPU (N-stop) ───
            RenderCommand::GradientQuad { rect, background, radii, .. } => {
                background_to_quads(batch, background, *rect, *radii, z, node_id);
            }

            // ─── BORDER: 1 quad con BORDER_BIT ───────────────────────────────────────
            RenderCommand::Border { rect, color, radii, width } => {
                batch.push_quad(GuiQuad {
                    pos: [rect.x, rect.y],
                    size: [rect.width, rect.height],
                    uv0: [*width, 0.0],
                    uv1: [0.0, 0.0],
                    color: *color,
                    color_b: [0.0; 4],
                    radii: *radii,
                    tex_index: 0,
                    flags: BORDER_BIT,
                    z_order: z,
                    node_id,
                });
                batch.update_last_segment();
            }

            // ─── SHADOW: quad pre-expandido con SHADOW_BIT ───────────────────────────
            RenderCommand::Shadow { rect, blur_radius, spread, color, offset } => {
                let expand = spread + blur_radius;
                batch.push_quad(GuiQuad {
                    pos: [rect.x - expand + offset[0], rect.y - expand + offset[1]],
                    size: [rect.width + expand * 2.0, rect.height + expand * 2.0],
                    uv0: [*blur_radius, *spread],
                    uv1: [offset[0], offset[1]],
                    color: *color,
                    color_b: [rect.width, rect.height, 0.0, 0.0],
                    radii: [0.0; 4],
                    tex_index: 0,
                    flags: crate::SHADOW_BIT,
                    z_order: z,
                    node_id,
                });
                batch.update_last_segment();
            }

            #[cfg(not(feature = "assets"))]
            RenderCommand::Image { .. } => {}
        }
    }

    #[cfg(not(feature = "text"))]
    fn to_batches(&self, quad_batch: &mut GuiBatch, z: f32, node_id: u32) {
        use ferrous_ui_core::RenderCommand;
        match self {
            RenderCommand::Quad { rect, color, radii, flags } => {
                quad_batch.push_quad(GuiQuad {
                    pos: [rect.x, rect.y],
                    size: [rect.width, rect.height],
                    uv0: [0.0, 0.0],
                    uv1: [1.0, 1.0],
                    color: *color,
                    color_b: [0.0; 4],
                    radii: *radii,
                    tex_index: 0,
                    flags: *flags,
                    z_order: z,
                    node_id,
                });
            }
            #[cfg(feature = "assets")]
            RenderCommand::Image { rect, texture, uv0, uv1, color } => {
                let idx = quad_batch.reserve_texture_slot(texture.clone());
                quad_batch.push_quad(GuiQuad {
                    pos: [rect.x, rect.y],
                    size: [rect.width, rect.height],
                    uv0: *uv0,
                    uv1: *uv1,
                    color: *color,
                    color_b: [0.0; 4],
                    radii: [0.0; 4],
                    tex_index: idx,
                    flags: TEXTURED_BIT,
                    z_order: z,
                    node_id,
                });
            }
            #[cfg(not(feature = "assets"))]
            RenderCommand::Image { .. } => {}
            RenderCommand::Text { .. } => {}
            RenderCommand::PushClip { rect } => { quad_batch.push_clip(*rect); }
            RenderCommand::PopClip => { quad_batch.pop_clip(); }
            RenderCommand::Svg { rect, color, mesh } => {
                quad_batch.push_svg(SvgCommand {
                    mesh: mesh.clone(),
                    pos: [rect.x, rect.y],
                    color: *color,
                    z,
                });
            }
            RenderCommand::GradientQuad { rect, background, radii, .. } => {
                background_to_quads(quad_batch, background, *rect, *radii, z, node_id);
            }
            RenderCommand::Border { rect, color, radii, width } => {
                quad_batch.push_quad(GuiQuad {
                    pos: [rect.x, rect.y],
                    size: [rect.width, rect.height],
                    uv0: [*width, 0.0],
                    uv1: [0.0, 0.0],
                    color: *color,
                    color_b: [0.0; 4],
                    radii: *radii,
                    tex_index: 0,
                    flags: BORDER_BIT,
                    z_order: z,
                    node_id,
                });
            }
            RenderCommand::Shadow { rect, blur_radius, spread, color, offset } => {
                let expand = spread + blur_radius;
                quad_batch.push_quad(GuiQuad {
                    pos: [rect.x - expand + offset[0], rect.y - expand + offset[1]],
                    size: [rect.width + expand * 2.0, rect.height + expand * 2.0],
                    uv0: [*blur_radius, *spread],
                    uv1: [offset[0], offset[1]],
                    color: *color,
                    color_b: [rect.width, rect.height, 0.0, 0.0],
                    radii: [0.0; 4],
                    tex_index: 0,
                    flags: crate::SHADOW_BIT,
                    z_order: z,
                    node_id,
                });
            }
        }
    }
}
