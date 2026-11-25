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
    /// A layered readback is still in flight; ask the runner to flush it on a
    /// short timer. The swapchain path never sets it.
    pub needs_flush: bool,
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
    render: bool,
) -> PresentOutcome {
    let device = target.device.clone();
    let queue = target.queue.clone();
    let handle = target.handle;
    let width = target.config.width.max(1);
    let height = target.config.height.max(1);
    let format = target.config.format;
    let transparent = target.transparent();
    let clear = if transparent {
        wgpu::Color::TRANSPARENT
    } else {
        wgpu::Color::BLACK
    };

    let outcome = match (target.present, render) {
        (PopupPresent::Swapchain, true) => {
            let Some(mut gui) = target.gui.take() else {
                return PresentOutcome::default();
            };
            gui.prepare(&queue, batch);
            // A batch larger than the initial capacity grows the buffers; keep
            // the `popups` memory entries truthful after the growth.
            register_popup_gui(handle, &gui);
            let outcome = present_swapchain(
                target, &device, &queue, &gui, batch, clear, position, capture, width, height,
                format,
            );
            target.gui = Some(gui);
            outcome
        }
        (PopupPresent::Swapchain, false) => PresentOutcome::default(),
        (PopupPresent::Layered, true) => {
            let Some(mut gui) = target.gui.take() else {
                return PresentOutcome::default();
            };
            gui.prepare(&queue, batch);
            register_popup_gui(handle, &gui);
            let outcome = present_layered(
                target, &device, &queue, &gui, batch, clear, position, capture, width, height,
            );
            target.gui = Some(gui);
            outcome
        }
        (PopupPresent::Layered, false) => flush_layered(target, position),
    };

    if outcome.presented {
        target.frames += 1;
        if target.first_frame_at.is_none() {
            target.first_frame_at = Some(std::time::Instant::now());
        }
    }
    outcome
}

/// Presents a finished layered readback without rendering a new frame.
///
/// The runner calls this on a short timer while [`PresentOutcome::needs_flush`]
/// is set, so the newest mapped frame reaches `UpdateLayeredWindow` on the next
/// callback and the popup keeps its previous image until then.
#[cfg(target_os = "windows")]
fn flush_layered(target: &mut PopupWindow, position: (i32, i32)) -> PresentOutcome {
    let Some(offscreen) = target.offscreen.as_ref() else {
        return PresentOutcome::default();
    };
    let Some(layered) = target.layered.as_mut() else {
        return PresentOutcome::default();
    };
    let frame = offscreen.readback.try_take();
    let presented = frame.is_some_and(|frame| {
        layered.upload(&frame.rgba);
        layered.update(position)
    });
    PresentOutcome {
        presented,
        needs_flush: offscreen.readback.has_pending(),
        ..PresentOutcome::default()
    }
}

#[cfg(not(target_os = "windows"))]
fn flush_layered(_target: &mut PopupWindow, _position: (i32, i32)) -> PresentOutcome {
    PresentOutcome::default()
}

/// Presents a finished layered readback without rendering a new frame.
///
/// The readback worker wakes the popup after a copy finishes; the runner calls
/// this when the main-frame generation has not advanced, so the same overlay
/// batch is never encoded twice.
pub fn flush_frame(target: &mut PopupWindow, position: (i32, i32)) -> PresentOutcome {
    let outcome = flush_layered(target, position);
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
        needs_flush: false,
    }
}

/// Renders `batch` into `offscreen`, then either reads it back synchronously
/// (`sync`) or hands the staging buffer to the readback worker.
///
/// The previous mapped frame is presented first, so a repaint never blocks on
/// the device queue: the popup keeps its last image until the new one is ready.
/// `sync` is used for the first frame and for evidence captures, where the
/// caller needs the current pixels in this callback.
#[cfg(target_os = "windows")]
#[allow(clippy::too_many_arguments)] // reason: one linear presentation call
pub fn present_offscreen(
    device: &std::sync::Arc<wgpu::Device>,
    queue: &wgpu::Queue,
    gui: &ferrous_ui_render::GuiRenderer,
    batch: &GuiBatch,
    offscreen: &OffscreenTarget,
    layered: &mut LayeredSurface,
    position: (i32, i32),
    clear: wgpu::Color,
    sync: bool,
) -> Option<Screenshot> {
    let width = offscreen.width.max(1);
    let height = offscreen.height.max(1);

    // Present a frame that finished since the last callback. The DIB keeps the
    // previous pixels when there is none, which is the whole point of the async
    // path.
    let mut presented = offscreen.readback.try_take();
    if let Some(frame) = &presented {
        layered.upload(&frame.rgba);
        if !layered.update(position) {
            presented = None;
        }
    }

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
    let buffer = screenshot::capture_buffer(device, width, height);
    encoder.copy_texture_to_buffer(
        wgpu::ImageCopyTexture {
            texture: &offscreen.texture,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        wgpu::ImageCopyBuffer {
            buffer: &buffer,
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

    let bgra = screenshot::is_bgra(offscreen.format);
    if sync {
        // First frame and evidence: the caller needs this frame now.
        let frame = screenshot::read_capture(device, &buffer, width, height, bgra)?;
        layered.upload(&frame.rgba);
        return layered.update(position).then_some(frame);
    }

    // Async: the worker blocks on the device queue, not the event loop.
    offscreen.readback.submit(crate::windows::ReadbackJob {
        device: std::sync::Arc::clone(device),
        buffer,
        width,
        height,
        bgra,
    });
    presented
}

/// Whether `FERROUS_POPUP_SYNC_READBACK=1` forces the pre-Phase-9B blocking
/// readback. It exists for the A/B measurement only; the default is off.
#[cfg(target_os = "windows")]
fn force_sync_readback() -> bool {
    static FORCE: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *FORCE.get_or_init(|| std::env::var_os("FERROUS_POPUP_SYNC_READBACK").is_some())
}

#[cfg(target_os = "windows")]
#[allow(clippy::too_many_arguments)] // reason: one linear presentation path
fn present_layered(
    target: &mut PopupWindow,
    device: &std::sync::Arc<wgpu::Device>,
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
    let first = target.first_frame_at.is_none();
    // The first frame is synchronous so the window never appears empty; every
    // later repaint maps off-thread. An evidence capture is synchronous too.
    // `FERROUS_POPUP_SYNC_READBACK=1` forces the old blocking path for an A/B
    // measurement of the stall behind a heavy main-window queue.
    let sync = capture || first || force_sync_readback();
    let PopupWindow {
        offscreen, layered, ..
    } = target;
    let (Some(offscreen), Some(layered)) = (offscreen.as_ref(), layered.as_mut()) else {
        return PresentOutcome::default();
    };
    let frame = present_offscreen(
        device, queue, gui, batch, offscreen, layered, position, clear, sync,
    );
    PresentOutcome {
        presented: frame.is_some(),
        screenshot: capture.then_some(frame).flatten(),
        first_frame: first && sync,
        needs_flush: offscreen.readback.has_pending(),
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
