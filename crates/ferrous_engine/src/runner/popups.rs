//! Popup windows: the queue drain that creates and pools them, and the
//! per-popup draw path.

use std::sync::Arc;

use winit::event_loop::ActiveEventLoop;
use winit::window::WindowId;

use super::types::Runner;
use crate::traits::{DrawContext, FerrousApp};
use crate::windows::{PopupBackend, PopupId, WindowMap, WindowTarget};

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
            main: gfx.renderer.ui_pass.gui_renderer(),
        };
        let redraw = crate::windows::drain(windows, &mut self.popups, context);
        for id in redraw {
            if let Some(handle) = windows.handle(id) {
                windows.backend().request_redraw(handle);
            }
        }
    }

    /// Draws and presents one popup frame.
    /// Draws and presents one popup frame.
    ///
    /// The popup owns its `GuiRenderer` buffers and bind groups, but (Phase 8F)
    /// reuses the main renderer's layouts, shaders, pipelines (or the cached
    /// sRGB/1 variant) and samplers through `new_sharing`, created when the
    /// window is; it also shares the main window's glyph and icon atlas bind
    /// groups, so both windows paint the same atlas.
    ///
    /// The swapchain and layered paths share [`crate::popup_present::present_frame`].
    /// A lost or outdated swapchain is reconfigured the way the main window is
    /// and the frame is retried on the next request.
    ///
    /// Returns whether a frame was presented.
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
        let (size, capture) = {
            let Some(windows) = self.windows.as_mut() else {
                return false;
            };
            let Some(target) = windows.backend_mut().window_mut(handle) else {
                return false;
            };
            let capture = std::mem::take(&mut target.capture_next);
            let size = target.size();
            // Share the main window's glyph atlas: the popup rebuilds its
            // group only when the atlas Arc changed (a runtime fallback bake
            // swaps it), never per frame. The engine wires no icon atlas, so
            // there is none to share; an icon draws as its path, which the
            // popup encodes like any other vector.
            if let Some(font) = &current_atlas {
                let same = target
                    .atlas
                    .as_ref()
                    .is_some_and(|old| Arc::ptr_eq(old, font));
                if !same {
                    if let Some(gui) = target.gui.as_mut() {
                        gui.set_font_atlas(&font.atlas.view, &font.atlas.sampler);
                    }
                    target.atlas = Some(font.clone());
                }
            }
            (size, capture)
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
        let outcome = crate::popup_present::present_frame(target, &batch, position, capture);
        target.batch = batch;
        if let Some(shot) = outcome.screenshot {
            self.app.on_popup_screenshot(crate::traits::PopupCapture {
                popup,
                position,
                size,
                screenshot: shot,
            });
        }
        outcome.presented
    }
}
