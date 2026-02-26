//! Simplified MSDF/SDF generator used by the atlas packer.

use super::path::GlyphCommand;

#[derive(Clone, Copy, Debug)]
pub(crate) struct Segment {
    pub x0: f32,
    pub y0: f32,
    pub x1: f32,
    pub y1: f32,
}

pub(crate) fn point_seg_dist(px: f32, py: f32, s: &Segment) -> f32 {
    let vx = s.x1 - s.x0;
    let vy = s.y1 - s.y0;
    let wx = px - s.x0;
    let wy = py - s.y0;
    let c1 = vx * wx + vy * wy;
    let c2 = vx * vx + vy * vy;
    let b = if c2 > 0.0 {
        (c1 / c2).clamp(0.0, 1.0)
    } else {
        0.0
    };
    let projx = s.x0 + b * vx;
    let projy = s.y0 + b * vy;
    let dx = px - projx;
    let dy = py - projy;
    (dx * dx + dy * dy).sqrt()
}

fn line_winding(px: f32, py: f32, s: &Segment) -> i32 {
    if (s.y0 <= py && s.y1 > py) || (s.y1 <= py && s.y0 > py) {
        let t = (py - s.y0) / (s.y1 - s.y0);
        let ix = s.x0 + t * (s.x1 - s.x0);
        if ix > px {
            return if s.y1 > s.y0 { 1 } else { -1 };
        }
    }
    0
}

pub fn generate_msdf(commands: &[GlyphCommand], size: u32) -> Vec<u8> {
    let mut segments: Vec<Segment> = Vec::new();
    let mut cursor = (0.0, 0.0);

    for cmd in commands {
        match cmd {
            GlyphCommand::MoveTo(x, y) => cursor = (*x, *y),
            GlyphCommand::LineTo(x, y) => {
                segments.push(Segment { x0: cursor.0, y0: cursor.1, x1: *x, y1: *y });
                cursor = (*x, *y);
            }
            GlyphCommand::QuadTo { ctrl_x, ctrl_y, to_x, to_y } => {
                let steps = 16;
                let mut prev_x = cursor.0;
                let mut prev_y = cursor.1;
                for i in 1..=steps {
                    let t = i as f32 / steps as f32;
                    let mt = 1.0 - t;
                    let nx = mt * mt * cursor.0 + 2.0 * mt * t * ctrl_x + t * t * to_x;
                    let ny = mt * mt * cursor.1 + 2.0 * mt * t * ctrl_y + t * t * to_y;
                    segments.push(Segment { x0: prev_x, y0: prev_y, x1: nx, y1: ny });
                    prev_x = nx;
                    prev_y = ny;
                }
                cursor = (*to_x, *to_y);
            }
        }
    }

   if segments.is_empty() {
        return vec![255u8; (size * size * 4) as usize]; // ¡Cambiado a 255!
    }

    // CORRECCIÓN DE ESCALA: Usamos una caja global de EM (EM Box) en lugar de una individual
    // La mayoría de las fuentes existen entre Y = -0.3 (descendentes) y 1.3 (ascendentes)
    let cell_minx = -0.3;
    let cell_maxx =  1.3;
    let cell_miny = -0.3;
    let cell_maxy =  1.3;

    let pixel_range = 4.0;
    let inv_size = 1.0 / size as f32;
    let units_per_pixel = (cell_maxx - cell_minx) * inv_size;
    let dist_scale = 1.0 / (pixel_range * units_per_pixel);

    let mut out = vec![0u8; (size * size * 4) as usize];
    for iy in 0..size {
        for ix in 0..size {
            let nx = (ix as f32 + 0.5) * inv_size;
            let ny = ((size - 1 - iy) as f32 + 0.5) * inv_size;
            let gx = cell_minx + nx * (cell_maxx - cell_minx);
            let gy = cell_miny + ny * (cell_maxy - cell_miny);

            let mut min_dist = f32::MAX;
            let mut winding = 0;

            for s in &segments {
                let d = point_seg_dist(gx, gy, s);
                if d < min_dist { min_dist = d; }
                winding += line_winding(gx, gy, s);
            }

            let inside = winding != 0;
            // CORRECCIÓN SIGNO SDF: Adentro es Negativo, Afuera es Positivo
            let sign = if inside { -1.0 } else { 1.0 };
            
            let sd = sign * min_dist * dist_scale;
            let encoded = (0.5 + sd).clamp(0.0, 1.0);
            let val = (encoded * 255.0) as u8;
            
            let offset = ((iy * size + ix) * 4) as usize;
            out[offset] = val;     // R
            out[offset + 1] = val; // G
            out[offset + 2] = val; // B
            out[offset + 3] = 255; // A
        }
    }
    out
}