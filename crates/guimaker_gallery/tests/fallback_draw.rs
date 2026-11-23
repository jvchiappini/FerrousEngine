//! The fallback runs are drawn, not only measured: CJK, Arabic and emoji cover
//! pixels at 1.0, 1.5 and 2.0.

#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    clippy::cast_precision_loss,
    clippy::cast_sign_loss
)] // reason: pixel coordinates and sample counts in a render test

use guimaker::components::text as display_text;
use guimaker::geom::Size;
use guimaker::prelude::*;
use guimaker::theme::{ColorRole, Theme};
use guimaker_ferrous::FerrousTextSystem;
use guimaker_gallery::shot::{render_display_list, RenderedImage};

const MIXED: &str =
    "Intro \u{1F3AC} \u{52D5}\u{753B} \u{0629}\u{0628}\u{0631}\u{062C}\u{062A} \u{2713}";
/// Characters whose glyphs must cover pixels, one per script.
const TARGETS: [char; 4] = ['\u{52D5}', '\u{0629}', '\u{1F3AC}', '\u{2713}'];

fn view() -> Element<()> {
    Element::column()
        .width(SizeSpec::Fill)
        .height(SizeSpec::Fill)
        .pad(Spacing::S6)
        .bg(ColorRole::Bg)
        .child(display_text::<()>(MIXED, TypeRole::Heading, Tone::Default).key("line"))
}

fn px(image: &RenderedImage, x: i32, y: i32) -> [u8; 4] {
    if x < 0 || y < 0 || x >= image.width as i32 || y >= image.height as i32 {
        return [0; 4];
    }
    let index = ((y as u32 * image.width + x as u32) * 4) as usize;
    [
        image.rgba[index],
        image.rgba[index + 1],
        image.rgba[index + 2],
        image.rgba[index + 3],
    ]
}

fn ink_pixels(
    image: &RenderedImage,
    background: [f32; 3],
    x0: f32,
    x1: f32,
    y0: f32,
    y1: f32,
) -> usize {
    let mut ink = 0;
    for y in y0.max(0.0) as i32..y1.min(image.height as f32) as i32 {
        for x in x0.max(0.0) as i32..x1.min(image.width as f32) as i32 {
            let pixel = px(image, x, y);
            let delta = (0..3)
                .map(|channel| {
                    i16::from(pixel[channel]) - (background[channel] * 255.0).round() as i16
                })
                .map(i16::abs)
                .max()
                .unwrap_or(0);
            if delta > 12 {
                ink += 1;
            }
        }
    }
    ink
}

#[test]
fn mixed_script_glyphs_cover_pixels_at_every_scale() {
    let theme = Theme::dark();
    let background = theme.role(ColorRole::Bg).to_srgb();
    let background = [background[0], background[1], background[2]];
    let viewport = Size::new(760.0, 120.0);
    let mut ui: Ui<()> = Ui::new(theme.clone());
    ui.set_viewport(viewport, 1.0);
    let mut text = FerrousTextSystem::new();
    let mut frame = Frame::new();
    ui.paint(&view(), &mut frame, &mut text);
    assert!(
        text.lazy_loads() > 0,
        "the mixed string needs fallback faces"
    );

    let line = ui.tree().find_key("line").expect("line");
    let rect = ui.layout().rect(line);
    let shape = ui.layout().shape(line).expect("shaped").clone();
    let baseline = shape.lines.first().expect("line").baseline;
    let size = ui
        .layout()
        .resolved(line)
        .map_or(16.0, |resolved| resolved.text.style.size);

    for scale in [1.0_f32, 1.5, 2.0] {
        ui.set_viewport(viewport, scale);
        ui.paint(&view(), &mut frame, &mut text);
        let image = match render_display_list(&frame.display_list, &theme, viewport, scale) {
            Ok(image) => image,
            Err(error) => {
                eprintln!("skipping fallback draw test: no headless GPU ({error})");
                return;
            }
        };
        for target in TARGETS {
            let glyph = shape
                .glyphs
                .iter()
                .find(|glyph| glyph.ch == target)
                .unwrap_or_else(|| panic!("no glyph for {target:?} at scale {scale}"));
            let x0 = (rect.x + glyph.x) * scale;
            let x1 = x0 + glyph.advance.max(size * 0.5) * scale;
            let y0 = (rect.y + baseline - size) * scale;
            let y1 = (rect.y + baseline + size * 0.25) * scale;
            let ink = ink_pixels(&image, background, x0, x1, y0, y1);
            assert!(
                ink >= 3,
                "{target:?} at scale {scale} covers {ink} pixels (box {x0:.1}..{x1:.1})"
            );
        }
    }
}
