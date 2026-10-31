//! Simplified MSDF/SDF generator used by the atlas packer.
//!
//! Implements the essential parts of Chlumsky's multi-channel signed distance
//! field algorithm (https://github.com/Chlumsky/msdfgen):
//!
//! * Bézier outlines are flattened into line segments for distance evaluation.
//! * Each original outline edge is assigned an [`EdgeColor`] that enables **two**
//!   of the three channels (`CYAN`, `MAGENTA`, `YELLOW`) and switches at sharp
//!   corners, so adjacent corner edges never share both channels.
//! * Each channel stores the signed distance to the nearest edge that enables
//!   it; the shader recovers a single smooth distance with `median(R,G,B)`.
//!
//! Because every edge is present in at least two channels, a straight edge
//! keeps a gradient in the median (two equal channels vs. a saturated third),
//! which is what `fwidth`/`screenPxRange` anti-aliasing needs. A naive
//! one-channel-per-edge assignment loses that gradient and produces hard edges.

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

/// Width of the encoded distance band, in atlas texels.  The generator stores a
/// linear signed-distance ramp over roughly `PIXEL_RANGE` texels around the
/// outline; a wider band keeps the reconstructed edge smooth when the glyph is
/// rasterised at the small pixel sizes the UI uses (9–22 px).  The shader's
/// `screenPxRange` must use the same constant.
const PIXEL_RANGE: f32 = 6.0;

// ── Edge colouring (bit flags) ───────────────────────────────────────────────
const RED: u8 = 1;
const GREEN: u8 = 2;
const BLUE: u8 = 4;
const WHITE: u8 = RED | GREEN | BLUE;
const CYAN: u8 = GREEN | BLUE;
const MAGENTA: u8 = RED | BLUE;
const YELLOW: u8 = RED | GREEN;

/// Rotates the enabled channels.  With the deterministic seed used here this
/// cycles `CYAN → MAGENTA → YELLOW → CYAN`, i.e. every edge always enables
/// exactly two channels (Chlumsky's `switchColor`).
fn switch_color(color: u8) -> u8 {
    let shifted = color << 1;
    (shifted | (shifted >> 3)) & WHITE
}

/// Like [`switch_color`] but avoids producing `banned` when the result would be
/// a single channel (`RED`/`GREEN`/`BLUE`).
fn switch_color_banned(color: u8, banned: u8) -> u8 {
    let combined = color & banned;
    if combined == RED || combined == GREEN || combined == BLUE {
        combined ^ WHITE
    } else {
        switch_color(color)
    }
}

struct Edge {
    seg_start: usize,
    seg_end: usize,
    start_dir: (f32, f32),
    end_dir: (f32, f32),
    color: u8,
}

fn unit(dx: f32, dy: f32) -> (f32, f32) {
    let len = (dx * dx + dy * dy).sqrt();
    if len > 1e-9 {
        (dx / len, dy / len)
    } else {
        (0.0, 0.0)
    }
}

fn is_corner(a: (f32, f32), b: (f32, f32)) -> bool {
    // angleThreshold = 3.0 rad (msdfgen default) → sin(3.0) ≈ 0.1411.
    const CROSS_THRESHOLD: f32 = 0.14112001;
    let dot = a.0 * b.0 + a.1 * b.1;
    let cross = (a.0 * b.1 - a.1 * b.0).abs();
    dot <= 0.0 || cross > CROSS_THRESHOLD
}

/// Flattens the outline into segments (one or more per original edge) and
/// assigns an MSDF [`EdgeColor`] to every segment.
fn build_colored_segments(commands: &[GlyphCommand]) -> Vec<(Segment, u8)> {
    let mut segments: Vec<Segment> = Vec::new();
    let mut edges: Vec<Edge> = Vec::new();
    let mut contours: Vec<Vec<usize>> = Vec::new();
    let mut cursor = (0.0f32, 0.0f32);

    for cmd in commands {
        match cmd {
            GlyphCommand::MoveTo(x, y) => {
                contours.push(Vec::new());
                cursor = (*x, *y);
            }
            GlyphCommand::LineTo(x, y) => {
                if contours.is_empty() {
                    contours.push(Vec::new());
                }
                let seg_start = segments.len();
                segments.push(Segment { x0: cursor.0, y0: cursor.1, x1: *x, y1: *y });
                let dir = unit(x - cursor.0, y - cursor.1);
                edges.push(Edge { seg_start, seg_end: segments.len(), start_dir: dir, end_dir: dir, color: 0 });
                contours.last_mut().unwrap().push(edges.len() - 1);
                cursor = (*x, *y);
            }
            GlyphCommand::QuadTo { ctrl_x, ctrl_y, to_x, to_y } => {
                if contours.is_empty() {
                    contours.push(Vec::new());
                }
                let start_dir = {
                    let d = unit(ctrl_x - cursor.0, ctrl_y - cursor.1);
                    if d == (0.0, 0.0) { unit(to_x - cursor.0, to_y - cursor.1) } else { d }
                };
                let end_dir = {
                    let d = unit(to_x - ctrl_x, to_y - ctrl_y);
                    if d == (0.0, 0.0) { unit(to_x - cursor.0, to_y - cursor.1) } else { d }
                };
                let seg_start = segments.len();
                let steps = 16;
                let (mut prev_x, mut prev_y) = cursor;
                for i in 1..=steps {
                    let t = i as f32 / steps as f32;
                    let mt = 1.0 - t;
                    let nx = mt * mt * cursor.0 + 2.0 * mt * t * ctrl_x + t * t * to_x;
                    let ny = mt * mt * cursor.1 + 2.0 * mt * t * ctrl_y + t * t * to_y;
                    segments.push(Segment { x0: prev_x, y0: prev_y, x1: nx, y1: ny });
                    prev_x = nx;
                    prev_y = ny;
                }
                edges.push(Edge { seg_start, seg_end: segments.len(), start_dir, end_dir, color: 0 });
                contours.last_mut().unwrap().push(edges.len() - 1);
                cursor = (*to_x, *to_y);
            }
        }
    }

    color_edges(&mut edges, &contours);

    // Fan the per-edge colour out to each of its flattened segments.
    let mut out = Vec::with_capacity(segments.len());
    for edge in &edges {
        for seg_idx in edge.seg_start..edge.seg_end {
            out.push((segments[seg_idx], edge.color));
        }
    }
    out
}

fn color_edges(edges: &mut [Edge], contours: &[Vec<usize>]) {
    let mut color = CYAN; // initColor(seed = 0)
    for contour in contours {
        if contour.is_empty() {
            continue;
        }
        // Corners: an edge whose start turns sharply from the previous edge.
        let m = contour.len();
        let mut corners: Vec<usize> = Vec::new();
        for i in 0..m {
            let prev = &edges[contour[(i + m - 1) % m]];
            let cur = &edges[contour[i]];
            if is_corner(prev.end_dir, cur.start_dir) {
                corners.push(i);
            }
        }

        if corners.is_empty() {
            color = switch_color(color);
            for &e in contour {
                edges[e].color = color;
            }
        } else {
            let corner_count = corners.len();
            let start = corners[0];
            color = switch_color(color);
            let initial = color;
            let mut spline = 0usize;
            for i in 0..m {
                let idx = contour[(start + i) % m];
                if spline + 1 < corner_count && corners[spline + 1] == i {
                    spline += 1;
                    let banned = if spline == corner_count - 1 { initial } else { 0 };
                    color = switch_color_banned(color, banned);
                }
                edges[idx].color = color;
            }
        }
    }
}

pub fn generate_msdf(commands: &[GlyphCommand], size: u32) -> Vec<u8> {
    let colored = build_colored_segments(commands);

    if colored.is_empty() {
        return vec![255u8; (size * size * 4) as usize];
    }

    // Global EM box: most fonts live between Y = -0.3 (descenders) and
    // Y = 1.3 (ascenders), X in the same range.
    let cell_minx = -0.3f32;
    let cell_maxx = 1.3f32;
    let cell_miny = -0.3f32;
    let cell_maxy = 1.3f32;

    let inv_size = 1.0 / size as f32;
    let units_per_pixel = (cell_maxx - cell_minx) * inv_size;
    let dist_scale = 1.0 / (PIXEL_RANGE * units_per_pixel);

    let mut out = vec![0u8; (size * size * 4) as usize];
    for iy in 0..size {
        for ix in 0..size {
            let nx = (ix as f32 + 0.5) * inv_size;
            let ny = ((size - 1 - iy) as f32 + 0.5) * inv_size;
            let gx = cell_minx + nx * (cell_maxx - cell_minx);
            let gy = cell_miny + ny * (cell_maxy - cell_miny);

            let mut min_dist = [f32::MAX; 3];
            let mut winding = 0;

            for (s, color) in &colored {
                let d = point_seg_dist(gx, gy, s);
                if color & RED != 0 && d < min_dist[0] {
                    min_dist[0] = d;
                }
                if color & GREEN != 0 && d < min_dist[1] {
                    min_dist[1] = d;
                }
                if color & BLUE != 0 && d < min_dist[2] {
                    min_dist[2] = d;
                }
                winding += line_winding(gx, gy, s);
            }

            // Inside is negative, outside is positive.
            let sign = if winding != 0 { -1.0 } else { 1.0 };

            let offset = ((iy * size + ix) * 4) as usize;
            for ch in 0..3 {
                let sd = sign * min_dist[ch] * dist_scale;
                out[offset + ch] = ((0.5 + sd).clamp(0.0, 1.0) * 255.0) as u8;
            }
            out[offset + 3] = 255; // A
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn square_outline() -> Vec<GlyphCommand> {
        vec![
            GlyphCommand::MoveTo(0.1, 0.1),
            GlyphCommand::LineTo(0.9, 0.1),
            GlyphCommand::LineTo(0.9, 0.9),
            GlyphCommand::LineTo(0.1, 0.9),
            GlyphCommand::LineTo(0.1, 0.1),
        ]
    }

    fn pixel_at(bmp: &[u8], size: u32, ix: u32, iy: u32) -> [u8; 4] {
        let o = ((iy * size + ix) * 4) as usize;
        [bmp[o], bmp[o + 1], bmp[o + 2], bmp[o + 3]]
    }

    #[test]
    fn every_edge_enables_at_least_two_channels() {
        for (_, color) in build_colored_segments(&square_outline()) {
            assert!(
                color.count_ones() >= 2,
                "edge colour {color:#05b} must enable >= 2 channels"
            );
        }
    }

    #[test]
    fn interior_is_inside_and_exterior_outside() {
        let size = 64u32;
        let bmp = generate_msdf(&square_outline(), size);
        let center = pixel_at(&bmp, size, 32, 31);
        let corner = pixel_at(&bmp, size, 1, 1);
        // Median of the three channels is the reconstructed distance.
        let med = |p: [u8; 4]| {
            let mut c = [p[0], p[1], p[2]];
            c.sort_unstable();
            c[1]
        };
        assert!(med(center) < 128, "centre should be inside the glyph");
        assert!(med(corner) > 128, "corner should be outside the glyph");
    }

    /// Regression for anti-aliasing quality: across a straight edge the median
    /// of the three MSDF channels must ramp through intermediate values so the
    /// shader can anti-alias. A one-channel-per-edge heuristic saturated two
    /// channels and made the median jump from `0` to `255`.
    #[test]
    fn gradient_across_a_straight_edge() {
        let size = 128u32;
        let bmp = generate_msdf(&square_outline(), size);
        let iy = size / 2;
        let intermediates = (0..size)
            .map(|ix| {
                let p = pixel_at(&bmp, size, ix, iy);
                let mut c = [p[0], p[1], p[2]];
                c.sort_unstable();
                c[1]
            })
            .filter(|&v| v > 20 && v < 235)
            .count();
        assert!(
            intermediates >= 2,
            "edge should anti-alias through intermediate values, got {intermediates}"
        );
    }

    /// Guards the sign convention shared with `assets/shaders/text.wgsl`: the
    /// generator encodes inside as `sd < 0.5`, and the shader computes
    /// `opacity = clamp(range*(0.5 - sd) + 0.5)`.  Using `sd - 0.5` would make
    /// the whole quad opaque and punch the glyph out (solid squares).
    #[test]
    fn shader_sign_makes_interior_opaque_and_exterior_transparent() {
        let size = 64u32;
        let bmp = generate_msdf(&square_outline(), size);
        let opacity = |p: [u8; 4]| -> f32 {
            let mut c = [p[0] as f32 / 255.0, p[1] as f32 / 255.0, p[2] as f32 / 255.0];
            c.sort_by(|a, b| a.partial_cmp(b).unwrap());
            (4.0 * (0.5 - c[1]) + 0.5).clamp(0.0, 1.0)
        };
        assert!(opacity(pixel_at(&bmp, size, 32, 31)) > 0.9, "centre must be opaque");
        assert!(opacity(pixel_at(&bmp, size, 1, 1)) < 0.1, "corner must be transparent");
    }

    #[test]
    fn alpha_is_opaque_everywhere() {
        let bmp = generate_msdf(&square_outline(), 32);
        assert!(bmp.chunks(4).all(|px| px[3] == 255));
    }
}
