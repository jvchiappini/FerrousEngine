//! Full-frame presentation tests for the persistent UI target.
//!
//! The persistent-UI path renders into an off-screen texture and then puts the
//! whole image on the swapchain (a texel copy or a full-screen blit).  These
//! tests pin the pixel identity of both present methods against a known
//! pattern; the real window path is not reachable headlessly.

#![cfg(feature = "gui")]

use std::sync::Mutex;

use ferrous_renderer::headless::{OffscreenTarget, Pure2dHarness};
use ferrous_ui_render::UiPresenter;
use wgpu::TextureFormat;

static GPU_LOCK: Mutex<()> = Mutex::new(());

/// Deterministic RGBA8 pattern with a distinct value in every texel.
fn pattern(width: u32, height: u32) -> Vec<u8> {
    let mut data = Vec::with_capacity((width * height * 4) as usize);
    for y in 0..height {
        for x in 0..width {
            data.push((x * 7 + 3) as u8);
            data.push((y * 11 + 5) as u8);
            data.push((x * 3 + y * 5) as u8);
            data.push(255);
        }
    }
    data
}

#[test]
fn copy_full_and_blit_preserve_the_persistent_texture() {
    let _guard = GPU_LOCK.lock().unwrap_or_else(|poison| poison.into_inner());

    // Non-sRGB and an odd size: the copy is format-agnostic, the blit is exact
    // at 1:1 without an sRGB decode/encode round-trip.
    let format = TextureFormat::Rgba8Unorm;
    let (width, height) = (37, 23);
    let harness = match Pure2dHarness::new(width, height, format) {
        Ok(harness) => harness,
        Err(err) => {
            println!("[skip] headless GPU unavailable: {err}");
            return;
        }
    };
    let device = harness.device();
    let queue = harness.queue();

    let mut presenter = UiPresenter::new(device, format);
    presenter.ensure(device, width, height);

    let source = pattern(width, height);
    queue.write_texture(
        wgpu::ImageCopyTexture {
            texture: presenter.target().texture().expect("target allocated"),
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        &source,
        wgpu::ImageDataLayout {
            offset: 0,
            bytes_per_row: Some(width * 4),
            rows_per_image: Some(height),
        },
        wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
    );

    let copy_dst = OffscreenTarget::new(device, width, height, format);
    let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
        label: Some("persistent copy full"),
    });
    presenter.copy_full(&mut encoder, &copy_dst.texture);
    queue.submit(Some(encoder.finish()));
    assert_eq!(
        copy_dst.read_rgba8(device, queue),
        source,
        "copy_texture_to_texture must be bit-exact"
    );

    let blit_dst = OffscreenTarget::new(device, width, height, format);
    let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
        label: Some("persistent blit full"),
    });
    presenter.blit(&mut encoder, &blit_dst.view);
    queue.submit(Some(encoder.finish()));
    assert_eq!(
        blit_dst.read_rgba8(device, queue),
        source,
        "the nearest-sampler full-screen blit must be 1:1 on a linear format"
    );
}
