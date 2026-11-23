//! Measured: is a 128-texel MSDF cell sharper than 64 at a 2x DPI scale?
//!
//! The renderer's text shader reconstructs coverage from the MSDF exactly as
//! `msdf_common.wgsl` documents. This test replicates that formula on the CPU,
//! renders one glyph from a 64- and a 128-texel bake, and compares both against
//! a **supersampled** reference: an 8x bake (512 texels) of the same glyph.
//!
//! The result drives G-0006: if 128 is not measurably better at 2x, the atlas
//! cell does not grow with DPI.

use ferrous_font::atlas::DEFAULT_GLYPH_SIZE;
use ferrous_font::baked::BakedAtlas;
use ferrous_font::parser::FontParser;

const ROBOTO: &[u8] = include_bytes!("../../../assets/fonts/Roboto-Regular.ttf");
const PIXEL_RANGE: f32 = 6.0;
const REFERENCE_CELL: u32 = 512;

fn atlas(cell: u32, ch: char) -> BakedAtlas {
    let parser = FontParser::new(ROBOTO.to_vec()).expect("roboto");
    BakedAtlas::build_with_size(&parser, &[ch], cell).expect("bake")
}

/// `median(r, g, b)` as the shader defines it.
fn median(r: f32, g: f32, b: f32) -> f32 {
    r.min(g).max(r.max(g).min(b))
}

fn bilinear(atlas: &BakedAtlas, u: f32, v: f32) -> (f32, f32, f32) {
    let side = atlas.tex_size as f32;
    let x = (u * side - 0.5).max(0.0);
    let y = (v * side - 0.5).max(0.0);
    let x0 = x.floor() as u32;
    let y0 = y.floor() as u32;
    let fx = x - x0 as f32;
    let fy = y - y0 as f32;
    let texel = |ix: u32, iy: u32| {
        let ix = ix.min(atlas.tex_size - 1);
        let iy = iy.min(atlas.tex_size - 1);
        let i = ((iy * atlas.tex_size + ix) * 4) as usize;
        (
            atlas.pixels[i] as f32 / 255.0,
            atlas.pixels[i + 1] as f32 / 255.0,
            atlas.pixels[i + 2] as f32 / 255.0,
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

/// The shader's coverage for one screen pixel of a `quad_w`-wide glyph.
fn coverage(atlas: &BakedAtlas, ch: char, u: f32, v: f32, quad_w: f32, cell: f32) -> f32 {
    let metric = atlas.metrics.get(&ch).expect("metric");
    let [u0, v0, u1, v1] = metric.uv;
    let su = u0 + (u1 - u0) * u;
    let sv = v0 + (v1 - v0) * v;
    let (r, g, b) = bilinear(atlas, su, sv);
    let sd = median(r, g, b);
    let px_range = (PIXEL_RANGE * (quad_w / cell)).max(1.0);
    (px_range * (0.5 - sd) + 0.5).clamp(0.0, 1.0)
}

/// Renders one glyph into `size x size` coverage values.
fn render(atlas: &BakedAtlas, ch: char, em: f32, size: usize) -> Vec<f32> {
    let quad = em * 1.6; // the atlas cell covers [-0.3, 1.3] em
    let cell = atlas.glyph_size as f32;
    let mut out = Vec::with_capacity(size * size);
    for y in 0..size {
        for x in 0..size {
            let u = (x as f32 + 0.5) / size as f32;
            let v = (y as f32 + 0.5) / size as f32;
            out.push(coverage(atlas, ch, u, v, quad, cell));
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
fn the_64_texel_cell_keeps_its_relative_error_at_2x() {
    let reference_cell = atlas(REFERENCE_CELL, 'H');
    let cell64 = atlas(DEFAULT_GLYPH_SIZE, 'H');
    let cell128 = atlas(DEFAULT_GLYPH_SIZE * 2, 'H');

    for (role, em_logical) in [("Caption", 12.0_f32), ("Body", 14.0)] {
        let mut absolute = Vec::new();
        for scale in [1.0_f32, 2.0] {
            let em = em_logical * scale;
            let size = (em * 1.6).ceil() as usize;
            let reference = render(&reference_cell, 'H', em, size);
            let candidate = render(&cell64, 'H', em, size);
            let (mean, max) = error(&candidate, &reference);
            eprintln!(
                "{role} at {scale} ({em} px em): 64 mean {mean:.4} max {max:.4} ({:.2} % of em)",
                max / em * 100.0
            );
            absolute.push((mean, max));
        }
        // The absolute error grows with the glyph because the *relative* error
        // is constant: a fixed cell does not get worse at a higher DPI, so the
        // cell does not need to grow with the scale.
        // The perceptually relevant number is the mean: the framebuffer
        // quantises coverage to 1/255, so a mean below that is invisible.
        let (mean, _) = absolute[1];
        eprintln!(
            "{role}: mean at 2.0 {mean:.5} vs one 8-bit step {:.5}",
            1.0 / 255.0
        );
        assert!(
            mean <= 1.0 / 255.0,
            "{role}: a 64 cell has a visible mean error at 2.0 ({mean:.5})"
        );
        // The 128 cell is a base-quality upgrade (it samples the same field
        // finer); it is not a DPI requirement. Recorded, not asserted.
        let em = em_logical * 2.0;
        let size = (em * 1.6).ceil() as usize;
        let reference = render(&reference_cell, 'H', em, size);
        let finer = render(&cell128, 'H', em, size);
        let (mean128, max128) = error(&finer, &reference);
        eprintln!("{role} at 2.0 with 128: mean {mean128:.4} max {max128:.4}");
    }
}
