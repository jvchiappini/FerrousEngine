//! Popup windows: the queue drain that creates and pools them, and the
//! per-popup draw path.

use std::sync::Arc;

use winit::event_loop::ActiveEventLoop;
use winit::window::WindowId;

use super::types::Runner;
use crate::traits::{DrawContext, FerrousApp};
use crate::windows::{PopupBackend, PopupId, PopupWindow, WindowMap, WindowTarget};

/// The popup an event for `window` belongs to.
///
/// This is the runner's routing decision, factored out so the headless unit
/// test proves the dispatch boundary without a GPU window: a popup
/// `WindowId` reaches [`FerrousApp::on_popup_event`] with this id, and every
/// other window keeps the main-window path.
pub(super) fn popup_for_event<B: PopupBackend>(
    map: Option<&WindowMap<B>>,
    window: WindowId,
) -> Option<PopupId> {
    match map.map_or(WindowTarget::Main, |map| map.route(window)) {
        WindowTarget::Popup(popup) => Some(popup),
        WindowTarget::Main => None,
    }
}

impl<A: FerrousApp + 'static> Runner<A> {
    /// Applies the app's queued popup commands and asks the OS for the frames
    /// they need.
    ///
    /// Called only from `about_to_wait`: creating a window needs the
    /// `ActiveEventLoop`, and this is the one callback that has it.
    pub(super) fn drain_popups(&mut self, event_loop: &ActiveEventLoop) {
        if self.popups.is_empty() {
            return;
        }
        let Some(gfx) = self.graphics.as_ref() else {
            return;
        };
        let Some(windows) = self.windows.as_mut() else {
            return;
        };
        let context = crate::windows::PopupContext {
            event_loop,
            instance: &gfx.renderer.context.instance,
            adapter: &gfx.renderer.context.adapter,
            device: &gfx.renderer.context.device,
            queue: &gfx.renderer.context.queue,
            pipeline_cache: gfx.renderer.context.pipeline_cache.clone(),
            format: gfx.config.format,
        };
        let redraw = crate::windows::drain(windows, &mut self.popups, context);
        for id in redraw {
            if let Some(handle) = windows.handle(id) {
                windows.backend().request_redraw(handle);
            }
        }
    }

    /// Draws and presents one popup frame.
    ///
    /// The popup owns its `GuiRenderer` buffers and bind groups, but (Phase 8E)
    /// reuses the main renderer's layouts, shader modules, pipelines and
    /// samplers through `new_sharing_global`; it also shares the main window's
    /// glyph and icon atlas bind groups, so both windows paint the same atlas.
    /// A lost or outdated swapchain is reconfigured the way the main window is
    /// and the frame is retried on the next request.
    ///
    /// Returns whether a frame was presented.
    #[allow(clippy::too_many_lines)] // reason: one linear acquire → draw → present path
    pub(super) fn render_popup(&mut self, popup: PopupId) -> bool {
        let Some(handle) = self
            .windows
            .as_ref()
            .and_then(|windows| windows.handle(popup))
        else {
            return false;
        };
        let time = self.clock.peek();
        // A popup can sit on another monitor: its own factor, not the main
        // window's, decides the buffer and the text size.
        let scale = self
            .windows
            .as_ref()
            .and_then(|windows| windows.backend().window_scale(handle))
            .unwrap_or(self.scale_factor);
        let current_atlas = self.font.clone();
        let (frame, view, size, capture) = {
            if self.graphics.is_none() {
                return false;
            }
            let Some(windows) = self.windows.as_mut() else {
                return false;
            };
            let Some(target) = windows.backend_mut().window_mut(handle) else {
                return false;
            };
            let capture = std::mem::take(&mut target.capture_next);
            let PopupWindow {
                surface,
                window: popup_window,
                config,
                device,
                pipeline_cache,
                gui,
                atlas,
                ..
            } = target;
            let (width, height) = (config.width.max(1), config.height.max(1));
            let format = config.format;
            let mut created = false;
            let gui = gui.get_or_insert_with(|| {
                created = true;
                // Reuse the main renderer's layouts, shader modules, pipelines
                // and samplers (Phase 8E): a popup then allocates only its own
                // buffers and bind groups. Without a published main renderer
                // this falls back to a private, depth-less pipeline set.
                ferrous_ui_render::GuiRenderer::new_sharing_global(
                    device.clone(),
                    format,
                    1024,
                    width,
                    height,
                    1,
                    pipeline_cache.clone(),
                )
                // A popup draws a handful of icon paths: its own small SVG
                // scratch set instead of the main window's 65k vertices.
                .with_svg_capacity(4096)
            });
            if created {
                // The popup owns these buffers; report them under `popups`, not
                // by overwriting the main renderer's `gui:*` entries.
                crate::windows::register_popup_gui(handle, gui);
            }
            // Share the main window's glyph atlas: the popup rebuilds its
            // group only when the atlas Arc changed (a runtime fallback bake
            // swaps it), never per frame. The engine wires no icon atlas, so
            // there is none to share; an icon draws as its path, which the
            // popup encodes like any other vector.
            if let Some(font) = &current_atlas {
                let same = atlas.as_ref().is_some_and(|old| Arc::ptr_eq(old, font));
                if !same {
                    gui.set_font_atlas(&font.atlas.view, &font.atlas.sampler);
                    *atlas = Some(font.clone());
                }
            }
            let frame = match surface.get_current_texture() {
                Ok(frame) => frame,
                Err(wgpu::SurfaceError::Outdated | wgpu::SurfaceError::Lost) => {
                    // Same recovery as the main window: rebuild the swapchain
                    // and ask for the frame again.
                    surface.configure(device, config);
                    popup_window.request_redraw();
                    return false;
                }
                Err(error) => {
                    log::warn!("popup {popup:?}: {error}");
                    return false;
                }
            };
            let view = frame
                .texture
                .create_view(&wgpu::TextureViewDescriptor::default());
            (frame, view, (width, height), capture)
        };

        // Paint through the same `AppContext` builder as every other callback.
        // `window_size` is the popup's, so the app's draw code can use it.
        let batch = {
            let Some(windows) = self.windows.as_mut() else {
                return false;
            };
            let Some(target) = windows.backend_mut().window_mut(handle) else {
                return false;
            };
            let mut batch = std::mem::take(&mut target.batch);
            batch.clear();
            batch
        };
        let popup_window = self
            .windows
            .as_ref()
            .and_then(|windows| windows.backend().window(handle))
            .map(|target| target.window.clone());
        let Some(popup_window) = popup_window else {
            return false;
        };
        let Some(batch) = self.with_ctx(
            &popup_window,
            time,
            ferrous_core::RenderStats::default(),
            ferrous_core::glam::Vec3::ZERO,
            ferrous_core::glam::Vec3::ZERO,
            size,
            scale,
            move |app, ctx| {
                let mut batch = batch;
                if let Some(font) = ctx.font {
                    let mut dc = DrawContext {
                        gui: &mut batch,
                        font,
                        ctx,
                    };
                    app.draw_popup(popup, &mut dc);
                }
                batch
            },
        ) else {
            return false;
        };

        let Some(windows) = self.windows.as_mut() else {
            return false;
        };
        let position = windows.spec(popup).map_or((0, 0), |spec| spec.position);
        let Some(target) = windows.backend_mut().window_mut(handle) else {
            return false;
        };
        let transparent = target.transparent();
        let can_copy = target.config.usage.contains(wgpu::TextureUsages::COPY_SRC);
        let mut capture_buffer = None;
        {
            let PopupWindow {
                gui, queue, device, ..
            } = &mut *target;
            let Some(gui) = gui.as_mut() else {
                return false;
            };
            let clear = if transparent {
                wgpu::Color::TRANSPARENT
            } else {
                wgpu::Color::BLACK
            };
            gui.prepare(queue, &batch);
            let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("Popup frame"),
            });
            gui.render(
                &mut encoder,
                &view,
                None,
                &batch,
                wgpu::LoadOp::Clear(clear),
            );
            if capture && can_copy {
                // Copy before presenting: the swapchain image is the frame the
                // host will compose. Nothing here reads the desktop.
                let (width, height) = size;
                let buffer = crate::screenshot::capture_buffer(device, width, height);
                let padded = crate::screenshot::padded_bytes_per_row(width);
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
                capture_buffer = Some(buffer);
            }
            queue.submit(Some(encoder.finish()));
        }
        frame.present();
        let (device, format) = {
            target.batch = batch;
            target.frames += 1;
            if target.first_frame_at.is_none() {
                target.first_frame_at = Some(std::time::Instant::now());
            }
            (target.device.clone(), target.config.format)
        };
        if let Some(buffer) = capture_buffer {
            let bgra = crate::screenshot::is_bgra(format);
            if let Some(shot) =
                crate::screenshot::read_capture(&device, &buffer, size.0, size.1, bgra)
            {
                self.app.on_popup_screenshot(crate::traits::PopupCapture {
                    popup,
                    position,
                    size,
                    screenshot: shot,
                });
            }
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use super::popup_for_event;
    use crate::windows::{drain, PopupQueue, PopupSpec, WindowMap};
    use winit::window::WindowId;

    #[derive(Default)]
    struct Recorder {
        delivered: Vec<crate::windows::PopupId>,
    }

    impl Recorder {
        /// The callback the runner reaches for a popup event.
        fn on_popup_event(&mut self, popup: crate::windows::PopupId) {
            self.delivered.push(popup);
        }
    }

    #[test]
    fn a_popup_window_id_is_delivered_to_on_popup_event_with_its_queue_id() {
        let mut map = WindowMap::new(WindowId::from(7), crate::windows::MockBackend::default());
        let mut queue = PopupQueue::new();
        let first = queue.open(PopupSpec::default());
        let second = queue.open(PopupSpec::default());
        drain(&mut map, &mut queue, ());
        let mut recorder = Recorder::default();
        // The mock gives handle `n` the routing key `n + 1000`, so popup
        // `second` is `WindowId::from(1)`.
        if let Some(popup) = popup_for_event(Some(&map), WindowId::from(1)) {
            recorder.on_popup_event(popup);
        }
        assert_eq!(recorder.delivered, vec![second]);
        assert_ne!(first, second);
        assert_eq!(popup_for_event(Some(&map), WindowId::from(7)), None);
        let empty: Option<&WindowMap<crate::windows::MockBackend>> = None;
        assert_eq!(popup_for_event(empty, WindowId::from(1)), None);
    }
}
