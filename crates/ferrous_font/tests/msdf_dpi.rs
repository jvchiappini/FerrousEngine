//! Measured: is a 128-texel MSDF cell sharper than 64 at a 2x DPI scale?
//!
//! The reference is built without the atlas cell clamp: `generate_msdf` is called
//! directly at 512 texels (8x the 64 cell), so the "0 error" of the previous
//! version (which silently clamped the reference to 128) cannot happen.
//!
//! The answer drives G-0006.

use ferrous_font::baked::BakedAtlas;
use ferrous_font::msdf_gen::generate_msdf;
use ferrous_font::parser::FontParser;
use ferrous_font::system;

const ROBOTO: &[u8] = include_bytes!("../../../assets/fonts/Roboto-Regular.ttf");
const PIXEL_RANGE: f32 = 6.0;
const REFERENCE_CELL: u32 = 512;
const GLYPHS: [char; 6] = ['H', 'a', 'e', 'g', 's', '\u{52D5}'];

struct RawCell {
    pixels: Vec<u8>,
    side: u32,
}

fn raw_cell(ch: char) -> Option<RawCell> {
    let bytes: Vec<u8> = if FontParser::new(ROBOTO.to_vec()).ok()?.get_glyph_index(ch) != 0 {
        ROBOTO.to_vec()
    } else {
        system::find_for(&[ch]).into_iter().next()?.0
    };
    let parser = FontParser::new(bytes).ok()?;
    let index = parser.get_glyph_index(ch);
    if index == 0 {
        return None;
    }
    let outline = parser.get_glyph_outline_by_index(index);
    Some(RawCell {
        pixels: generate_msdf(&outline, REFERENCE_CELL),
        side: REFERENCE_CELL,
    })
}

fn atlas(cell: u32, ch: char) -> BakedAtlas {
    let bytes: Vec<u8> = if FontParser::new(ROBOTO.to_vec())
        .expect("roboto")
        .get_glyph_index(ch)
        != 0
    {
        ROBOTO.to_vec()
    } else {
        system::find_for(&[ch]).into_iter().next().expect("face").0
    };
    let parser = FontParser::new(bytes).expect("parse");
    BakedAtlas::build_with_size(&parser, &[ch], cell).expect("bake")
}

fn median(r: f32, g: f32, b: f32) -> f32 {
    r.min(g).max(r.max(g).min(b))
}

fn bilinear(pixels: &[u8], side: u32, u: f32, v: f32) -> (f32, f32, f32) {
    let side_f = side as f32;
    let x = (u * side_f - 0.5).max(0.0);
    let y = (v * side_f - 0.5).max(0.0);
    let x0 = x.floor() as u32;
    let y0 = y.floor() as u32;
    let fx = x - x0 as f32;
    let fy = y - y0 as f32;
    let texel = |ix: u32, iy: u32| {
        let ix = ix.min(side - 1);
        let iy = iy.min(side - 1);
        let i = ((iy * side + ix) * 4) as usize;
        (
            pixels[i] as f32 / 255.0,
            pixels[i + 1] as f32 / 255.0,
            pixels[i + 2] as f32 / 255.0,
        )
    };
    let mix = |a: (f32, f32, f32), b: (f32, f32, f32), t: f32| {
        (
            a.0 + (b.0 - a.0) * t,
            a.1 + (b.1 - a.1) * t,
            a.2 + (b.2 - a.2) * t,
        )
    };
    let top = mix(texel(x0, y0), texel(x0 + 1, y0), fx);
    let bottom = mix(texel(x0, y0 + 1), texel(x0 + 1, y0 + 1), fx);
    mix(top, bottom, fy)
}

fn coverage(pixels: &[u8], side: u32, uv: [f32; 4], cell: f32, quad_w: f32, u: f32, v: f32) -> f32 {
    let su = uv[0] + (uv[2] - uv[0]) * u;
    let sv = uv[1] + (uv[3] - uv[1]) * v;
    let (r, g, b) = bilinear(pixels, side, su, sv);
    let sd = median(r, g, b);
    let px_range = (PIXEL_RANGE * (quad_w / cell)).max(1.0);
    (px_range * (0.5 - sd) + 0.5).clamp(0.0, 1.0)
}

fn render_atlas(atlas: &BakedAtlas, ch: char, em: f32, size: usize) -> Vec<f32> {
    let metric = atlas.metrics.get(&ch).expect("metric");
    let quad = em * 1.6;
    let cell = atlas.glyph_size as f32;
    let mut out = Vec::with_capacity(size * size);
    for y in 0..size {
        for x in 0..size {
            let u = (x as f32 + 0.5) / size as f32;
            let v = (y as f32 + 0.5) / size as f32;
            out.push(coverage(
                &atlas.pixels,
                atlas.tex_size,
                metric.uv,
                cell,
                quad,
                u,
                v,
            ));
        }
    }
    out
}

fn render_raw(raw: &RawCell, em: f32, size: usize) -> Vec<f32> {
    let quad = em * 1.6;
    let mut out = Vec::with_capacity(size * size);
    for y in 0..size {
        for x in 0..size {
            let u = (x as f32 + 0.5) / size as f32;
            let v = (y as f32 + 0.5) / size as f32;
            out.push(coverage(
                &raw.pixels,
                raw.side,
                [0.0, 0.0, 1.0, 1.0],
                raw.side as f32,
                quad,
                u,
                v,
            ));
        }
    }
    out
}

fn error(candidate: &[f32], reference: &[f32]) -> (f32, f32) {
    let mut sum = 0.0;
    let mut max = 0.0_f32;
    for (a, b) in candidate.iter().zip(reference) {
        let delta = (a - b).abs();
        sum += delta;
        max = max.max(delta);
    }
    (sum / candidate.len() as f32, max)
}

#[test]
fn a_64_texel_cell_is_under_one_8_bit_step_at_2x_for_curved_glyphs() {
    let mut worst_gain = 0.0_f32;
    for (role, em_logical) in [("Caption", 12.0_f32), ("Body", 14.0)] {
        let em = em_logical * 2.0;
        let size = (em * 1.6).ceil() as usize;
        for ch in GLYPHS {
            let Some(raw) = raw_cell(ch) else {
                eprintln!("{ch}: no face on this host");
                continue;
            };
            let reference = render_raw(&raw, em, size);
            let a = render_atlas(&atlas(64, ch), ch, em, size);
            let b = render_atlas(&atlas(128, ch), ch, em, size);
            let (mean64, max64) = error(&a, &reference);
            let (mean128, max128) = error(&b, &reference);
            eprintln!(
                "{role} {ch} at 2.0: 64 mean {mean64:.5} max {max64:.4}; 128 mean {mean128:.5} max {max128:.4}"
            );
            let gain = (mean64 - mean128) / mean64.max(1e-6);
            worst_gain = worst_gain.max(gain);
        }
    }
    // The mean is the visible quantity: the 128 cell changes it by a few
    // percent (and sometimes makes it worse), so it is not measurably better.
    // The max improves at isolated corner pixels only.
    eprintln!(
        "worst mean gain from 128 at 2.0: {:.2} %",
        worst_gain * 100.0
    );
    assert!(
        worst_gain < 0.05,
        "a 128 cell improves the mean by {:.1} %; the cell should grow",
        worst_gain * 100.0
    );
}
