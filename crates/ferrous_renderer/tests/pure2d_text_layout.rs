//! Exact-value and behaviour tests for the styled text layout API.
//!
//! The tests split into two groups:
//!
//! * CPU metrics (`tracking`, multi-line baselines, per-line alignment, GPOS
//!   kerning) — these run without a device and check the arithmetic exactly.
//! * GPU pixels (`layout_text_2d` + `draw_glyphs_2d` versus `draw_text_2d`) —
//!   these read the frame back and require an adapter.
//!
//! GPU tests take [`common::lock`] for their whole body; creating many devices
//! at once exhausts the adapter (see `tests/common/mod.rs`).

use ferrous_2d::render::{GlyphDraw2d, TextStyle2d};
use ferrous_font::{charset, TextAlign, TextStyle};
use ferrous_renderer::headless::{Pure2dHarness, DEFAULT_FORMAT};
use glam::Mat4;

mod common;

const WHITE: [f32; 4] = [1.0, 1.0, 1.0, 1.0];

fn roboto_bytes() -> Option<Vec<u8>> {
    let manifest = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let path = manifest.join("../../assets/fonts/Roboto-Regular.ttf");
    match std::fs::read(&path) {
        Ok(bytes) => Some(bytes),
        Err(err) => {
            println!("[skip] Roboto-Regular.ttf unavailable at {path:?}: {err}");
            None
        }
    }
}

fn harness(width: u32, height: u32) -> Option<Pure2dHarness> {
    match Pure2dHarness::new(width, height, DEFAULT_FORMAT) {
        Ok(h) => Some(h),
        Err(err) => {
            println!("[skip] headless GPU unavailable: {err}");
            None
        }
    }
}

fn style(size_px: f32, align: TextAlign, tracking: f32, line_height: f32) -> TextStyle2d {
    TextStyle2d {
        size_px,
        align,
        tracking,
        line_height,
    }
}

fn render(
    h: &mut Pure2dHarness,
    draw: impl FnOnce(&mut Pure2dHarness, ferrous_2d::render::FontId2d),
    font: ferrous_2d::render::FontId2d,
) -> Vec<u8> {
    h.set_clear_color(wgpu::Color::TRANSPARENT);
    h.begin_frame();
    draw(h, font);
    h.render_frame_blocking();
    h.read_pixels()
}

#[test]
fn tracking_adds_n_minus_one_steps_and_stays_centred() {
    let _gpu = common::lock();
    let Some(bytes) = roboto_bytes() else { return };
    let Some(mut h) = harness(256, 96) else {
        return;
    };
    let Ok(font) = h.register_font(&bytes, charset::ascii()) else {
        return;
    };

    let plain = style(48.0, TextAlign::Left, 0.0, 1.0);
    let tracked = style(48.0, TextAlign::Left, 0.1, 1.0);
    let width = |h: &Pure2dHarness, style: TextStyle2d| {
        h.measure_text_block(font, "AVATAR", style)
            .expect("live font")
            .width
    };
    let plain_width = width(&h, plain);
    let tracked_width = width(&h, tracked);
    let expected = plain_width + 5.0 * 0.1 * 48.0;
    assert!(
        (tracked_width - expected).abs() < 1e-3,
        "tracking must add (n-1)·tracking·size: {tracked_width} vs {expected}"
    );

    // Centre alignment subtracts half of the *tracked* advance from every pen,
    // so the run stays centred instead of drifting right.
    let mut left = Vec::new();
    let mut centre = Vec::new();
    let left_metrics = h
        .layout_text(font, "AVATAR", tracked, &mut left)
        .expect("live font");
    let centre_metrics = h
        .layout_text(
            font,
            "AVATAR",
            style(48.0, TextAlign::Center, 0.1, 1.0),
            &mut centre,
        )
        .expect("live font");
    assert!((left_metrics.width - tracked_width).abs() < 1e-3);
    assert!((centre_metrics.width - tracked_width).abs() < 1e-3);
    let shift = tracked_width * 0.5;
    for (l, c) in left.iter().zip(&centre) {
        assert!((c.x - (l.x - shift)).abs() < 1e-3, "left x: {}", l.x);
    }
}

#[test]
fn multiline_baselines_step_by_line_height() {
    let _gpu = common::lock();
    let Some(bytes) = roboto_bytes() else { return };
    let Some(mut h) = harness(256, 256) else {
        return;
    };
    let Ok(font) = h.register_font(&bytes, charset::ascii()) else {
        return;
    };

    let single = h
        .measure_text_block(font, "A\nA", style(40.0, TextAlign::Left, 0.0, 1.0))
        .expect("live font");
    let doubled = h
        .measure_text_block(font, "A\nA", style(40.0, TextAlign::Left, 0.0, 2.0))
        .expect("live font");

    assert_eq!(single.line_count, 2);
    assert!((single.lines[0].baseline_y).abs() < 1e-6);
    assert!(
        (doubled.lines[1].baseline_y - 2.0 * single.lines[1].baseline_y).abs() < 1e-3,
        "baseline step must scale with line_height: {} vs {}",
        doubled.lines[1].baseline_y,
        single.lines[1].baseline_y
    );
    let grew = doubled.height - single.height;
    assert!(
        (grew + single.lines[1].baseline_y).abs() < 1e-3,
        "height must grow by exactly one natural step"
    );
}

#[test]
fn each_line_is_aligned_on_its_own() {
    let _gpu = common::lock();
    let Some(bytes) = roboto_bytes() else { return };
    let Some(mut h) = harness(256, 256) else {
        return;
    };
    let Ok(font) = h.register_font(&bytes, charset::ascii()) else {
        return;
    };

    let mut left = Vec::new();
    let mut centre = Vec::new();
    let left_metrics = h
        .layout_text(
            font,
            "A\nAVATAR",
            style(40.0, TextAlign::Left, 0.0, 1.0),
            &mut left,
        )
        .expect("live font");
    let centre_metrics = h
        .layout_text(
            font,
            "A\nAVATAR",
            style(40.0, TextAlign::Center, 0.0, 1.0),
            &mut centre,
        )
        .expect("live font");
    assert!((left_metrics.lines[0].advance - left_metrics.lines[1].advance).abs() > 1.0);

    let line0_left = left.iter().find(|g| g.line == 0).expect("line 0");
    let line0_centre = centre.iter().find(|g| g.line == 0).expect("line 0");
    let line1_left = left.iter().find(|g| g.line == 1).expect("line 1");
    let line1_centre = centre.iter().find(|g| g.line == 1).expect("line 1");
    assert!((line0_centre.x - (line0_left.x - left_metrics.lines[0].advance * 0.5)).abs() < 1e-3);
    assert!((line1_centre.x - (line1_left.x - left_metrics.lines[1].advance * 0.5)).abs() < 1e-3);
    assert!((centre_metrics.width - left_metrics.width).abs() < 1e-3);
}

#[test]
fn gpos_kerning_shrinks_av() {
    let _gpu = common::lock();
    let Some(bytes) = roboto_bytes() else { return };
    let Some(mut h) = harness(64, 64) else { return };
    let Ok(font) = h.register_font(&bytes, charset::ascii()) else {
        return;
    };
    let width = |h: &Pure2dHarness, text: &str| {
        h.measure_text_block(font, text, style(100.0, TextAlign::Left, 0.0, 1.0))
            .expect("live font")
            .width
    };
    let a = width(&h, "A");
    let v = width(&h, "V");
    let av = width(&h, "AV");
    assert!(
        av < a + v,
        "GPOS kerning must shrink A/V: {av} vs {} + {}",
        a,
        v
    );
    let to = width(&h, "To");
    let t = width(&h, "T");
    let o = width(&h, "o");
    assert!(
        to < t + o,
        "GPOS kerning must shrink T/o: {to} vs {t} + {o}"
    );
}

#[test]
fn identity_glyph_draw_matches_draw_text_exactly() {
    let _gpu = common::lock();
    let Some(bytes) = roboto_bytes() else { return };
    let Some(mut h) = harness(320, 160) else {
        return;
    };
    let Ok(font) = h.register_font(&bytes, charset::ascii()) else {
        return;
    };

    let transform = Mat4::IDENTITY;
    let text = "AV";
    let text_style = style(72.0, TextAlign::Center, 0.04, 1.0);
    let direct = render(
        &mut h,
        |h, font| h.draw_text_styled(font, text, transform, text_style, WHITE),
        font,
    );

    let mut layout = Vec::new();
    h.layout_text(font, text, text_style, &mut layout)
        .expect("live font");
    let draws: Vec<GlyphDraw2d> = layout
        .iter()
        .map(|glyph| GlyphDraw2d {
            glyph: glyph.clone(),
            transform,
            color: WHITE,
            gradient: None,
        })
        .collect();
    let via_glyphs = render(&mut h, |h, font| h.draw_glyphs(font, &draws), font);
    assert_eq!(
        direct, via_glyphs,
        "identity draw_glyphs_2d must be pixel-identical to draw_text_2d"
    );
}

#[test]
fn layout_positions_equal_what_draw_text_renders() {
    let _gpu = common::lock();
    let Some(bytes) = roboto_bytes() else { return };
    let Some(mut h) = harness(320, 160) else {
        return;
    };
    let Ok(font) = h.register_font(&bytes, charset::ascii()) else {
        return;
    };

    let transform = Mat4::IDENTITY;
    let text_style = style(72.0, TextAlign::Left, 0.0, 1.0);
    // Draw only the first glyph through the text path, then draw the first
    // laid-out glyph through the per-glyph path.
    let direct = render(
        &mut h,
        |h, font| h.draw_text_styled(font, "A", transform, text_style, WHITE),
        font,
    );
    let mut layout = Vec::new();
    h.layout_text(font, "A", text_style, &mut layout)
        .expect("live font");
    let first = layout.first().expect("one glyph").clone();
    assert!(first.visible);
    let draw = GlyphDraw2d {
        glyph: first,
        transform,
        color: WHITE,
        gradient: None,
    };
    let via_glyph = render(&mut h, |h, font| h.draw_glyphs(font, &[draw]), font);
    assert_eq!(
        direct, via_glyph,
        "the laid-out pen must equal the rendered pen for one glyph"
    );
}

#[test]
fn a_second_line_renders_below_the_first() {
    let _gpu = common::lock();
    let Some(bytes) = roboto_bytes() else { return };
    let Some(mut h) = harness(200, 300) else {
        return;
    };
    let Ok(font) = h.register_font(&bytes, charset::ascii()) else {
        return;
    };

    let style = style(40.0, TextAlign::Center, 0.0, 2.0);
    let pixels = render(
        &mut h,
        |h, font| h.draw_text_styled(font, "A\nA", Mat4::IDENTITY, style, WHITE),
        font,
    );
    // Target centre is (100, 150); world y = 150 - row - 0.5.  Line 0's ink
    // sits above the centre, line 1's below it.
    let lit_rows: Vec<u32> = (0..300)
        .filter(|&row| {
            (0..200).any(|x| {
                let pixel = pixels[((row * 200 + x) * 4) as usize + 3];
                pixel > 128
            })
        })
        .collect();
    assert!(
        lit_rows.iter().any(|&row| row < 150),
        "the first line must render above the origin: {lit_rows:?}"
    );
    assert!(
        lit_rows.iter().any(|&row| row > 150),
        "the second line must render below the origin: {lit_rows:?}"
    );
    let gap = lit_rows.windows(2).any(|pair| pair[1] > pair[0] + 20);
    assert!(gap, "the two baselines must be visibly apart: {lit_rows:?}");
}

#[test]
fn unsupported_fonts_are_a_typed_error() {
    let _gpu = common::lock();
    let Some(mut h) = harness(16, 16) else { return };
    let mut data = vec![0, 1, 0, 0];
    data.extend(&1u16.to_be_bytes()); // numTables
    data.extend(&[0; 6]);
    data.extend(b"CFF ");
    data.extend(&0u32.to_be_bytes());
    data.extend(&28u32.to_be_bytes());
    data.extend(&0u32.to_be_bytes());
    match h.register_font(&data, charset::ascii()) {
        Err(ferrous_font::FontError::Unsupported { reason }) => {
            assert!(reason.contains("CFF"), "reason: {reason}");
        }
        other => panic!("expected FontError::Unsupported, got {other:?}"),
    }
}

/// Removes the TrueType `kern` table so a font's pair kerning can only come
/// from GPOS.  Roboto ships both tables; stripping `kern` produces a
/// GPOS-only face without needing a second font file.
fn without_kern_table(bytes: &[u8]) -> Vec<u8> {
    let num_tables = u16::from_be_bytes([bytes[4], bytes[5]]) as usize;
    let mut records: Vec<([u8; 4], usize, usize)> = Vec::new();
    for index in 0..num_tables {
        let start = 12 + index * 16;
        let tag: [u8; 4] = bytes[start..start + 4].try_into().expect("four tag bytes");
        let offset = u32::from_be_bytes([
            bytes[start + 8],
            bytes[start + 9],
            bytes[start + 10],
            bytes[start + 11],
        ]) as usize;
        let length = u32::from_be_bytes([
            bytes[start + 12],
            bytes[start + 13],
            bytes[start + 14],
            bytes[start + 15],
        ]) as usize;
        if tag == *b"kern" {
            continue;
        }
        records.push((tag, offset, length));
    }
    let mut out = Vec::with_capacity(bytes.len());
    out.extend_from_slice(&bytes[..4]);
    out.extend_from_slice(&(records.len() as u16).to_be_bytes());
    out.extend_from_slice(&bytes[6..12]);
    let mut data_offset = 12 + records.len() * 16;
    let mut data = Vec::new();
    for (tag, offset, length) in &records {
        out.extend_from_slice(tag);
        out.extend_from_slice(&0u32.to_be_bytes());
        out.extend_from_slice(&(data_offset as u32).to_be_bytes());
        out.extend_from_slice(&(*length as u32).to_be_bytes());
        data.extend_from_slice(&bytes[*offset..*offset + *length]);
        data_offset += length;
    }
    out.extend_from_slice(&data);
    out
}

#[test]
fn layout_data_measures_exactly_like_the_atlas() {
    let _gpu = common::lock();
    let Some(bytes) = roboto_bytes() else { return };
    let Some(h) = harness(16, 16) else { return };
    let chars = charset::latin_western();
    let styles = [
        TextStyle::at(16.0),
        TextStyle {
            size_px: 48.0,
            align: TextAlign::Center,
            tracking: 0.04,
            line_height: 1.5,
        },
        TextStyle {
            size_px: 7.0,
            align: TextAlign::Right,
            tracking: -0.01,
            line_height: 0.7,
        },
    ];
    let texts = [
        "", "A", "AV", "AVATAR", "Ao\nAV", "AV\n\nA", "A \n A", "A\r\nA",
    ];

    for (name, font_bytes) in [
        ("Roboto", bytes.clone()),
        (
            "Roboto without kern (GPOS only)",
            without_kern_table(&bytes),
        ),
    ] {
        let parser = ferrous_font::FontParser::new(font_bytes.clone()).expect("valid font");
        let atlas = ferrous_font::FontAtlas::new(h.device(), h.queue(), &parser, chars.clone())
            .expect("atlas");
        let data =
            ferrous_font::LayoutData::from_bytes(&font_bytes, chars.clone()).expect("layout data");
        for text in texts {
            for style in styles {
                let from_atlas = atlas.measure_block(text, style);
                let from_data = data.measure_block(text, style);
                assert_eq!(
                    from_atlas, from_data,
                    "{name}: {text:?} {style:?} must measure bit for bit"
                );
            }
        }
        // The GPOS-only face still kerns: AV is narrower than A + V.
        let style = TextStyle::at(100.0);
        let av = data.measure_block("AV", style).width;
        let sum = data.measure_block("A", style).width + data.measure_block("V", style).width;
        assert!(
            av < sum,
            "{name}: GPOS kerning must narrow AV ({av} vs {sum})"
        );
    }
}
