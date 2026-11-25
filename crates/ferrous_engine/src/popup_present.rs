//! Presenting one popup frame through the selected [`PopupPresent`] backend.
//!
//! This is the shared worker behind [`crate::runner::Runner::render_popup`] and
//! the Phase 8F cost example. The swapchain path acquires the popup's surface;
//! the layered path never touches a surface: it renders into an offscreen
//! texture, reads the premultiplied RGBA back, and hands it to Win32 through
//! `UpdateLayeredWindow`.
//!
//! The GUI pass blends straight-alpha over a transparent clear, so the
//! framebuffer bytes are already premultiplied. The layered path therefore only
//! swaps R/B for the DIB; it never multiplies by alpha a second time.

use ferrous_ui_render::GuiBatch;

use crate::screenshot::{self, Screenshot};
use crate::windows::{register_popup_gui, PopupPresent, PopupWindow};
#[cfg(target_os = "windows")]
use crate::windows::{LayeredSurface, OffscreenTarget};

/// What one present call did.
#[derive(Debug, Default)]
pub struct PresentOutcome {
    /// A frame reached the screen (or the layered window).
    pub presented: bool,
    /// The frame the app asked to capture, when the backend could read it.
    pub screenshot: Option<Screenshot>,
    /// This frame was the first after the popup opened or was reopened.
    pub first_frame: bool,
}

/// Prepares, renders, submits and presents one popup frame.
///
/// `position` is the physical screen position a layered window should use;
/// the swapchain path ignores it (the OS window already sits there).
pub fn present_frame(
    target: &mut PopupWindow,
    batch: &GuiBatch,
    position: (i32, i32),
    capture: bool,
) -> PresentOutcome {
    let Some(mut gui) = target.gui.take() else {
        return PresentOutcome::default();
    };
    let device = target.device.clone();
    let queue = target.queue.clone();
    let handle = target.handle;
    let present = target.present;
    let width = target.config.width.max(1);
    let height = target.config.height.max(1);
    let format = target.config.format;
    let transparent = target.transparent();

    gui.prepare(&queue, batch);
    // A batch larger than the initial capacity grows the buffers; keep the
    // `popups` memory entries truthful after the growth.
    register_popup_gui(handle, &gui);

    let clear = if transparent {
        wgpu::Color::TRANSPARENT
    } else {
        wgpu::Color::BLACK
    };

    let outcome = match present {
        PopupPresent::Swapchain => present_swapchain(
            target, &device, &queue, &gui, batch, clear, position, capture, width, height, format,
        ),
        PopupPresent::Layered => present_layered(
            target, &device, &queue, &gui, batch, clear, position, capture, width, height,
        ),
    };

    target.gui = Some(gui);
    if outcome.presented {
        target.frames += 1;
        if target.first_frame_at.is_none() {
            target.first_frame_at = Some(std::time::Instant::now());
        }
    }
    outcome
}

#[allow(clippy::too_many_arguments)] // reason: one linear presentation path
fn present_swapchain(
    target: &mut PopupWindow,
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    gui: &ferrous_ui_render::GuiRenderer,
    batch: &GuiBatch,
    clear: wgpu::Color,
    _position: (i32, i32),
    capture: bool,
    width: u32,
    height: u32,
    format: wgpu::TextureFormat,
) -> PresentOutcome {
    let Some(surface) = target.surface.as_ref() else {
        return PresentOutcome::default();
    };
    let frame = match surface.get_current_texture() {
        Ok(frame) => frame,
        Err(wgpu::SurfaceError::Outdated | wgpu::SurfaceError::Lost) => {
            // Same recovery as the main window: rebuild the swapchain and ask
            // for the frame again.
            surface.configure(device, &target.config);
            target.window.request_redraw();
            return PresentOutcome::default();
        }
        Err(error) => {
            log::warn!("popup {}: {error}", target.handle);
            return PresentOutcome::default();
        }
    };
    let view = frame
        .texture
        .create_view(&wgpu::TextureViewDescriptor::default());
    let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
        label: Some("Popup frame"),
    });
    gui.render(&mut encoder, &view, None, batch, wgpu::LoadOp::Clear(clear));
    let can_copy = target.config.usage.contains(wgpu::TextureUsages::COPY_SRC);
    let capture_buffer = if capture && can_copy {
        let buffer = screenshot::capture_buffer(device, width, height);
        let padded = screenshot::padded_bytes_per_row(width);
        encoder.copy_texture_to_buffer(
            wgpu::ImageCopyTexture {
                texture: &frame.texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            wgpu::ImageCopyBuffer {
                buffer: &buffer,
                layout: wgpu::ImageDataLayout {
                    offset: 0,
                    bytes_per_row: Some(padded),
                    rows_per_image: Some(height.max(1)),
                },
            },
            wgpu::Extent3d {
                width: width.max(1),
                height: height.max(1),
                depth_or_array_layers: 1,
            },
        );
        Some(buffer)
    } else {
        None
    };
    queue.submit(Some(encoder.finish()));
    frame.present();
    let screenshot = capture_buffer.and_then(|buffer| {
        screenshot::read_capture(device, &buffer, width, height, screenshot::is_bgra(format))
    });
    let first_frame = target.first_frame_at.is_none();
    PresentOutcome {
        presented: true,
        screenshot,
        first_frame,
    }
}

/// Renders `batch` into `offscreen`, reads it back and presents it through
/// `layered`.
///
/// This is the whole layered frame: it always copies the texture back because
/// `UpdateLayeredWindow` needs CPU pixels. On success it returns the same
/// tightly packed premultiplied RGBA frame, so an evidence capture costs no
/// second readback.
///
/// Windows only; the caller never receives a layered popup elsewhere.
#[cfg(target_os = "windows")]
#[allow(clippy::too_many_arguments)] // reason: one linear presentation call
pub fn present_offscreen(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    gui: &ferrous_ui_render::GuiRenderer,
    batch: &GuiBatch,
    offscreen: &OffscreenTarget,
    layered: &mut LayeredSurface,
    position: (i32, i32),
    clear: wgpu::Color,
) -> Option<Screenshot> {
    let width = offscreen.width.max(1);
    let height = offscreen.height.max(1);
    let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
        label: Some("Popup offscreen frame"),
    });
    gui.render(
        &mut encoder,
        &offscreen.view,
        None,
        batch,
        wgpu::LoadOp::Clear(clear),
    );
    let padded = screenshot::padded_bytes_per_row(width);
    encoder.copy_texture_to_buffer(
        wgpu::ImageCopyTexture {
            texture: &offscreen.texture,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        wgpu::ImageCopyBuffer {
            buffer: &offscreen.staging,
            layout: wgpu::ImageDataLayout {
                offset: 0,
                bytes_per_row: Some(padded),
                rows_per_image: Some(height),
            },
        },
        wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
    );
    queue.submit(Some(encoder.finish()));

    // The readback polls the device, so the DIB upload only sees finished
    // pixels. The offscreen target is BGRA, so `read_capture` swaps to RGBA for
    // the screenshot; `upload` swaps back for the DIB.
    let bgra = screenshot::is_bgra(offscreen.format);
    let frame = screenshot::read_capture(device, &offscreen.staging, width, height, bgra)?;
    layered.upload(&frame.rgba);
    layered.update(position).then_some(frame)
}

#[cfg(target_os = "windows")]
#[allow(clippy::too_many_arguments)] // reason: one linear presentation path
fn present_layered(
    target: &mut PopupWindow,
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    gui: &ferrous_ui_render::GuiRenderer,
    batch: &GuiBatch,
    clear: wgpu::Color,
    position: (i32, i32),
    capture: bool,
    _width: u32,
    _height: u32,
) -> PresentOutcome {
    if !target.ensure_offscreen() || !target.ensure_layered(position) {
        return PresentOutcome::default();
    }
    let PopupWindow {
        offscreen, layered, ..
    } = target;
    let (Some(offscreen), Some(layered)) = (offscreen.as_ref(), layered.as_mut()) else {
        return PresentOutcome::default();
    };
    let Some(frame) = present_offscreen(
        device, queue, gui, batch, offscreen, layered, position, clear,
    ) else {
        log::warn!("popup {}: UpdateLayeredWindow failed", target.handle);
        return PresentOutcome::default();
    };
    let first_frame = target.first_frame_at.is_none();
    PresentOutcome {
        presented: true,
        screenshot: capture.then_some(frame),
        first_frame,
    }
}

/// Layered present is Windows-only; the resolved backend never reaches it on
/// another host, but the call must still compile.
#[cfg(not(target_os = "windows"))]
#[allow(clippy::too_many_arguments)] // reason: mirrors the Windows signature
fn present_layered(
    _target: &mut PopupWindow,
    _device: &wgpu::Device,
    _queue: &wgpu::Queue,
    _gui: &ferrous_ui_render::GuiRenderer,
    _batch: &GuiBatch,
    _clear: wgpu::Color,
    _position: (i32, i32),
    _capture: bool,
    _width: u32,
    _height: u32,
) -> PresentOutcome {
    PresentOutcome::default()
}
