//! Simplified MSDF generator used by the atlas packer.

use crate::path::GlyphCommand;

/// Generate a very basic multi-channel signed distance field for a glyph.
/// `pixel_range` is the distance scaling factor controlling the width of the
/// transition band; typical values are 2.0–4.0.
pub fn generate_msdf(commands: &[GlyphCommand], size: u32) -> Vec<u8> {
    #[derive(Clone, Copy)]
    struct Segment {
        x0: f32,
        y0: f32,
        x1: f32,
        y1: f32,
    }

    let mut segs: Vec<Segment> = Vec::new();
    let mut cur_x = 0.0f32;
    let mut cur_y = 0.0f32;
    for cmd in commands {
        match *cmd {
            GlyphCommand::MoveTo(x, y) => {
                cur_x = x;
                cur_y = y;
            }
            GlyphCommand::LineTo(x, y) => {
                segs.push(Segment { x0: cur_x, y0: cur_y, x1: x, y1: y });
                cur_x = x;
                cur_y = y;
            }
            GlyphCommand::QuadTo { ctrl_x, ctrl_y, to_x, to_y } => {
                let steps = 8;
                let mut px = cur_x;
                let mut py = cur_y;
                for i in 1..=steps {
                    let t = i as f32 / steps as f32;
                    let mt = 1.0 - t;
                    let x = mt * mt * cur_x + 2.0 * mt * t * ctrl_x + t * t * to_x;
                    let y = mt * mt * cur_y + 2.0 * mt * t * ctrl_y + t * t * to_y;
                    segs.push(Segment { x0: px, y0: py, x1: x, y1: y });
                    px = x;
                    py = y;
                }
                cur_x = to_x;
                cur_y = to_y;
            }
        }
    }

    if segs.is_empty() {
        return vec![255u8; (size * size * 4) as usize];
    }

    // compute bbox
    let (mut minx, mut maxx, mut miny, mut maxy) = (f32::INFINITY, f32::NEG_INFINITY, f32::INFINITY, f32::NEG_INFINITY);
    for s in &segs {
        minx = minx.min(s.x0).min(s.x1);
        maxx = maxx.max(s.x0).max(s.x1);
        miny = miny.min(s.y0).min(s.y1);
        maxy = maxy.max(s.y0).max(s.y1);
    }
    let pad = 0.1;
    minx -= pad;
    miny -= pad;
    maxx += pad;
    maxy += pad;

    fn point_seg_dist(px: f32, py: f32, s: &Segment) -> (f32, f32) {
        let vx = s.x1 - s.x0;
        let vy = s.y1 - s.y0;
        let wx = px - s.x0;
        let wy = py - s.y0;
        let c1 = vx * wx + vy * wy;
        let c2 = vx * vx + vy * vy;
        let b = if c2 > 0.0 { (c1 / c2).clamp(0.0, 1.0) } else { 0.0 };
        let projx = s.x0 + b * vx;
        let projy = s.y0 + b * vy;
        let dx = px - projx;
        let dy = py - projy;
        let dist = (dx * dx + dy * dy).sqrt();
        let angle = vy.atan2(vx);
        (dist, angle)
    }

    let mut out = vec![0u8; (size * size * 4) as usize];
    let pixel_range = 4.0;
    for iy in 0..size {
        for ix in 0..size {
            let nx = ix as f32 / (size as f32);
            let ny = iy as f32 / (size as f32);
            let gx = minx + nx * (maxx - minx);
            let gy = miny + ny * (maxy - miny);
            let mut best_r = f32::MAX;
            let mut best_g = f32::MAX;
            let mut best_b = f32::MAX;
            for seg in &segs {
                let (d, ang) = point_seg_dist(gx, gy, seg);
                let cos = ang.cos().abs();
                let sin = ang.sin().abs();
                if cos > 0.8 {
                    best_r = best_r.min(d);
                } else if sin > 0.8 {
                    best_g = best_g.min(d);
                } else {
                    best_b = best_b.min(d);
                }
            }
            if best_r == f32::MAX { best_r = best_g.min(best_b); }
            if best_g == f32::MAX { best_g = best_r.min(best_b); }
            if best_b == f32::MAX { best_b = best_r.min(best_g); }
            let er = (0.5 + best_r * pixel_range).clamp(0.0, 1.0);
            let eg = (0.5 + best_g * pixel_range).clamp(0.0, 1.0);
            let eb = (0.5 + best_b * pixel_range).clamp(0.0, 1.0);
            let offset = ((iy * size + ix) * 4) as usize;
            out[offset + 0] = (er * 255.0) as u8;
            out[offset + 1] = (eg * 255.0) as u8;
            out[offset + 2] = (eb * 255.0) as u8;
            out[offset + 3] = 255;
        }
    }
    out
}
