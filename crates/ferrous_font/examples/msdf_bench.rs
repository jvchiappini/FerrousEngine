//! Times the MSDF bake for the bundled Roboto over `latin_western`.
//!
//! ```text
//! cargo run --release -p ferrous_font --example msdf_bench
//! cargo run --release -p ferrous_font --example msdf_bench -- 20 64
//! ```
//!
//! Arguments: `[repetitions] [cell size]`.  The reported number is the CPU
//! cost of turning every glyph outline in the character set into a 64x64 MSDF
//! cell — the part of a font registration that runs before the GPU upload.
//! Outline extraction is timed separately because it is font parsing, not
//! MSDF generation.

use std::hint::black_box;
use std::time::Instant;

use ferrous_font::msdf_gen::generate_msdf;

const ROBOTO: &[u8] = include_bytes!("../../../assets/fonts/Roboto-Regular.ttf");

fn main() {
    let mut args = std::env::args().skip(1);
    let repetitions: usize = args.next().and_then(|a| a.parse().ok()).unwrap_or(5);
    let size: u32 = args.next().and_then(|a| a.parse().ok()).unwrap_or(64);

    let parser = ferrous_font::FontParser::new(ROBOTO.to_vec()).expect("the bundled font parses");
    let chars = ferrous_font::charset::latin_western();

    let extraction_start = Instant::now();
    let outlines: Vec<_> = chars
        .iter()
        .map(|&c| parser.get_glyph_outline_by_index(parser.get_glyph_index(c)))
        .collect();
    let extraction_ms = extraction_start.elapsed().as_secs_f64() * 1000.0;

    let mut samples = Vec::with_capacity(repetitions);
    for _ in 0..repetitions {
        let start = Instant::now();
        for outline in &outlines {
            black_box(generate_msdf(black_box(outline), size));
        }
        samples.push(start.elapsed().as_secs_f64() * 1000.0);
    }
    samples.sort_by(f64::total_cmp);
    let median = samples[samples.len() / 2];

    println!("glyphs            : {}", chars.len());
    println!("cell size         : {size}x{size}");
    println!("outline extraction: {extraction_ms:.2} ms (once, parsing)");
    println!(
        "msdf generation   : {median:.2} ms median over {repetitions} runs \
         ({:.3} ms/glyph)",
        median / chars.len() as f64
    );
}
