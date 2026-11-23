//! Measures the backdrop-blur cost on the GPU.
//!
//! It renders the gallery's dialog story at 1600x900 @1.5 twice: once with its
//! backdrop blur and once with the `BackdropBlur` command removed (the card rect
//! stays transparent, so the content is otherwise identical). Each variant is
//! submitted `FRAMES` times inside one command encoder with a single GPU wait at
//! the end, so the wall time per submission is a GPU-bound upper bound; the
//! difference between the two variants is the blur cost, including the 2x
//! prefilter chain and the two Gaussian passes.
//!
//! Run with `cargo run --release -p guimaker_gallery --example blur_timing`.

#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::cast_precision_loss
)] // reason: timing and pixel counts in a measurement example

use std::time::Instant;

use ferrous_renderer::headless::{Pure2dHarness, DEFAULT_FORMAT};
use ferrous_ui_render::{Font, GuiBatch, GuiRenderer};
use guimaker::display_list::{DisplayList, DrawCommand};
use guimaker::geom::Size;
use guimaker::theme::Theme;
use guimaker_gallery::shot::backdrop_split;
use guimaker_gallery::{Component, Story};

const ROBOTO: &[u8] = include_bytes!("../../../assets/fonts/Roboto-Regular.ttf");
const FRAMES: usize = 200;

fn main() {
    if let Err(error) = run() {
        eprintln!("blur timing failed: {error}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), Box<dyn std::error::Error>> {
    let theme = Theme::dark();
    let logical = Size::new(1600.0, 900.0);
    let scale = 1.5_f32;
    let width = (logical.width * scale).round() as u32;
    let height = (logical.height * scale).round() as u32;

    let display_list = Story::Component(Component::Dialog).display_list(&theme, logical, scale);
    let split = backdrop_split(&display_list.commands).expect("the dialog story blurs a backdrop");
    let mut no_blur = DisplayList::new();
    no_blur.commands.extend_from_slice(&display_list.commands);
    no_blur
        .commands
        .retain(|command| !matches!(command, DrawCommand::BackdropBlur { .. }));

    let harness = Pure2dHarness::new(width, height, DEFAULT_FORMAT)?;
    let device = harness.renderer().context.device.clone();
    let queue = harness.renderer().context.queue.clone();
    let font = Font::load_bytes(
        ROBOTO,
        device.as_ref(),
        queue.as_ref(),
        ferrous_font::charset::latin_western(),
    );
    let mut renderer =
        GuiRenderer::new(device.clone(), DEFAULT_FORMAT, 4096, width, height, 1, None);
    renderer.set_font_atlas(&font.atlas.view, &font.atlas.sampler);
    let target = &harness.target().view;

    let prefix = encode_commands(&display_list.commands[..split], logical, scale, &font);
    let overlay = encode_commands(&display_list.commands[split..], logical, scale, &font);
    let flat = encode_commands(&no_blur.commands, logical, scale, &font);

    let mut measure = |label: &str, with_blur: bool| {
        let mut encoder =
            device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some(label) });
        // Warm the targets and uniforms before the timed submissions.
        renderer.prepare(queue.as_ref(), &prefix);
        renderer.prepare(queue.as_ref(), &overlay);
        if with_blur {
            renderer.set_backdrop_source(target, width, height);
        }
        let start = Instant::now();
        for _ in 0..FRAMES {
            if with_blur {
                renderer.prepare(queue.as_ref(), &prefix);
                renderer.render(
                    &mut encoder,
                    target,
                    None,
                    &prefix,
                    wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                );
                renderer.prepare(queue.as_ref(), &overlay);
                renderer.render(&mut encoder, target, None, &overlay, wgpu::LoadOp::Load);
            } else {
                renderer.prepare(queue.as_ref(), &flat);
                renderer.render(
                    &mut encoder,
                    target,
                    None,
                    &flat,
                    wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                );
            }
        }
        queue.submit(Some(encoder.finish()));
        device.poll(wgpu::Maintain::Wait);
        let elapsed = start.elapsed();
        let per_frame = elapsed.as_secs_f64() * 1000.0 / FRAMES as f64;
        eprintln!("{label}: {elapsed:?} for {FRAMES} submissions, {per_frame:.4} ms/frame");
        per_frame
    };

    let blurred = measure("with backdrop blur", true);
    let plain = measure("without backdrop blur", false);
    eprintln!(
        "blur cost: {:.4} ms/frame (upper bound: includes encode)",
        blurred - plain
    );
    Ok(())
}

fn encode_commands(commands: &[DrawCommand], logical: Size, scale: f32, font: &Font) -> GuiBatch {
    let mut list = DisplayList::new();
    list.commands.extend_from_slice(commands);
    let mut batch = GuiBatch::new();
    guimaker_ferrous::encode(&list, logical, scale, &mut batch, font);
    batch.damage_union = None;
    batch
}
