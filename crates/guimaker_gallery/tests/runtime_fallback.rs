//! Runtime lazy fallback through the host's frame path (not the screenshot
//! harness): the first frame with "動画" has no glyph, the runner's re-bake
//! makes the same frame's re-encoded list draw ink, and a second frame does not
//! re-bake.

#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    clippy::cast_precision_loss,
    clippy::cast_sign_loss
)] // reason: pixel coordinates and sample counts in a render test

use ferrous_renderer::headless::{Pure2dHarness, DEFAULT_FORMAT};
use ferrous_ui_render::{GuiBatch, GuiRenderer};
use guimaker::components::text as display_text;
use guimaker::display_list::DrawCommand;
use guimaker::geom::Size;
use guimaker::prelude::*;
use guimaker::theme::{ColorRole, Theme};
use guimaker_ferrous::FerrousTextSystem;

const ROBOTO: &[u8] = include_bytes!("../../../assets/fonts/Roboto-Regular.ttf");
const TEXT: &str = "Intro \u{52D5}\u{753B}";

fn view() -> Element<()> {
    Element::column()
        .width(SizeSpec::Fill)
        .height(SizeSpec::Fill)
        .pad(Spacing::S6)
        .bg(ColorRole::Bg)
        .child(display_text::<()>(TEXT, TypeRole::Heading, Tone::Default).key("line"))
}

fn px(image: &(u32, u32, Vec<u8>), x: i32, y: i32) -> [u8; 4] {
    let (width, height, rgba) = image;
    if x < 0 || y < 0 || x >= *width as i32 || y >= *height as i32 {
        return [0; 4];
    }
    let i = ((y as u32 * *width + x as u32) * 4) as usize;
    [rgba[i], rgba[i + 1], rgba[i + 2], rgba[i + 3]]
}

fn ink(
    image: &(u32, u32, Vec<u8>),
    background: [f32; 3],
    x0: f32,
    x1: f32,
    y0: f32,
    y1: f32,
) -> usize {
    let mut count = 0;
    for y in y0.max(0.0) as i32..y1.min(image.1 as f32) as i32 {
        for x in x0.max(0.0) as i32..x1.min(image.0 as f32) as i32 {
            let pixel = px(image, x, y);
            let delta = (0..3)
                .map(|c| i16::from(pixel[c]) - (background[c] * 255.0).round() as i16)
                .map(i16::abs)
                .max()
                .unwrap_or(0);
            if delta > 12 {
                count += 1;
            }
        }
    }
    count
}

#[test]
#[allow(clippy::too_many_lines)] // reason: one end-to-end frame-path test
fn runtime_fallback_draws_cjk_and_rebakes_once() {
    let theme = Theme::dark();
    let background = theme.role(ColorRole::Bg).to_srgb();
    let background = [background[0], background[1], background[2]];
    let viewport = Size::new(420.0, 140.0);

    let mut ui: Ui<()> = Ui::new(theme.clone());
    ui.set_viewport(viewport, 1.0);
    let mut text_system = FerrousTextSystem::new();
    let mut frame = Frame::new();
    ui.paint(&view(), &mut frame, &mut text_system);

    let line = ui.tree().find_key("line").expect("line");
    let rect = ui.layout().rect(line);
    let shape = ui.layout().shape(line).expect("shaped").clone();
    let baseline = shape.lines.first().expect("line").baseline;
    let size = ui
        .layout()
        .resolved(line)
        .map_or(20.0, |resolved| resolved.text.style.size);

    let requested: Vec<char> = frame
        .display_list
        .commands
        .iter()
        .filter_map(|command| match command {
            DrawCommand::Text { run, .. } => Some(run.text.chars().collect::<Vec<_>>()),
            _ => None,
        })
        .flatten()
        .collect();
    assert!(requested.contains(&'\u{52D5}'));

    let harness = match Pure2dHarness::new(
        viewport.width as u32,
        viewport.height as u32,
        DEFAULT_FORMAT,
    ) {
        Ok(harness) => harness,
        Err(error) => {
            eprintln!("skipping runtime fallback test: no headless GPU ({error})");
            return;
        }
    };
    let device = harness.renderer().context.device.clone();
    let queue = harness.renderer().context.queue.clone();
    let target = &harness.target().view;

    let mut runtime =
        ferrous_engine::runtime_font::RuntimeFont::new(ROBOTO, device.as_ref(), queue.as_ref())
            .expect("startup atlas");
    let before_bytes = runtime.atlas_bytes();
    let missing = runtime.missing(requested.iter());
    assert!(missing.contains(&'\u{52D5}') && missing.contains(&'\u{753B}'));
    assert_eq!(runtime.bakes(), 0);
    assert!(
        !runtime.missing(['A', 'e', ' '].iter()).contains(&'A'),
        "ASCII is already in the atlas"
    );

    let start = std::time::Instant::now();
    assert!(runtime.ensure(&requested, device.as_ref(), queue.as_ref()));
    let rebake = start.elapsed();
    // The second frame with the same text must not re-bake.
    assert!(!runtime.ensure(&requested, device.as_ref(), queue.as_ref()));
    assert_eq!(runtime.bakes(), 1);
    let after_bytes = runtime.atlas_bytes();
    // A character no face covers (an unassigned code point) draws `.notdef`
    // and must not re-bake on every frame, or the app never goes idle.
    let uncovered = ['\u{0378}'];
    assert!(!runtime.ensure(&uncovered, device.as_ref(), queue.as_ref()));
    assert!(runtime.missing(uncovered.iter()).is_empty());
    assert!(!runtime.ensure(&uncovered, device.as_ref(), queue.as_ref()));
    assert_eq!(runtime.bakes(), 1, "an uncovered character never re-bakes");

    // Render the *host's* display list with the runtime atlas.
    let render = |font: &ferrous_font::Font| -> (u32, u32, Vec<u8>) {
        let mut batch = GuiBatch::new();
        guimaker_ferrous::encode(&frame.display_list, viewport, 1.0, &mut batch, font);
        batch.damage_union = None;
        let mut gui = GuiRenderer::new(
            device.clone(),
            DEFAULT_FORMAT,
            u32::try_from(batch.quads.len() + batch.text_quads.len() + 64).unwrap_or(4096),
            viewport.width as u32,
            viewport.height as u32,
            1,
            None,
        );
        gui.set_font_atlas(&font.atlas.view, &font.atlas.sampler);
        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
        gui.prepare(queue.as_ref(), &batch);
        gui.render(
            &mut encoder,
            target,
            None,
            &batch,
            wgpu::LoadOp::Clear(wgpu::Color::BLACK),
        );
        queue.submit(Some(encoder.finish()));
        let mut rgba = Vec::new();
        harness.read_pixels_into(&mut rgba).expect("readback");
        (viewport.width as u32, viewport.height as u32, rgba)
    };

    let cjk = shape
        .glyphs
        .iter()
        .find(|glyph| glyph.ch == '\u{52D5}')
        .expect("CJK glyph");
    let x0 = rect.x + cjk.x;
    let x1 = x0 + cjk.advance * 2.0;
    let y0 = rect.y + baseline - size;
    let y1 = rect.y + baseline + size * 0.25;

    let with_font = render(runtime.font());
    let covered = ink(&with_font, background, x0, x1, y0, y1);
    assert!(
        covered >= 3,
        "'動画' covers {covered} pixels after the runtime re-bake (box {x0:.1}..{x1:.1})"
    );
    eprintln!(
        "runtime fallback: {missing:?}, re-bake {rebake:?}, atlas {before_bytes} -> {after_bytes} bytes, {covered} ink pixels"
    );
}
