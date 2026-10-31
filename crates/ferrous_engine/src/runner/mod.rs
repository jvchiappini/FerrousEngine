//! Application runner — winit event loop + frame scheduling.
//!
//! The `Runner` struct wires together the winit event loop, the renderer,
//! the ECS world, asset loading and the user-supplied [`FerrousApp`] callbacks.
//! Application code should not instantiate `Runner` directly; use
//! [`AppBuilder::run`] or [`run_internal`] (called by the app builder).

mod events;
mod frame;
mod types;

pub(crate) use types::Runner;

use winit::event_loop::{ControlFlow, EventLoop};

use crate::builder::AppConfig;
use crate::traits::FerrousApp;

// ── Direct entry point ────────────────────────────────────────────────────────

/// Desktop entry point: blocks the calling thread running the event loop.
#[cfg(not(target_arch = "wasm32"))]
pub(crate) fn run_internal<A: FerrousApp + 'static>(config: AppConfig, app: A) {
    let mut runner = Runner::new(app, config);
    let event_loop = EventLoop::new().unwrap();
    event_loop.set_control_flow(ControlFlow::Wait);
    event_loop.run_app(&mut runner).unwrap();
}

/// wasm32 entry point: runs Runner directly in the browser event loop.
#[cfg(target_arch = "wasm32")]
pub(crate) fn run_internal<A: FerrousApp + 'static>(config: AppConfig, app: A) {
    console_error_panic_hook::set_once();
    use winit::platform::web::EventLoopExtWebSys;
    let runner = Runner::new(app, config);
    let event_loop = EventLoop::new().unwrap();
    event_loop.set_control_flow(ControlFlow::Wait);
    event_loop.spawn_app(runner);
}

// ── Plugin-based entry point ──────────────────────────────────────────────────

/// Lightweight [`FerrousApp`] shim used by the plugin path.
struct PluginApp;
impl FerrousApp for PluginApp {}

/// Entry point for [`crate::plugin::AppBuilder::run`].
pub(crate) fn run_plugin_app(mut builder: crate::plugin::AppBuilder) {
    let mut runner = Runner::new(PluginApp, builder.config.clone());

    for (stage, system) in builder.staged_systems.drain(..) {
        runner.systems.add_boxed(stage, system);
    }

    #[cfg(not(target_arch = "wasm32"))]
    {
        let event_loop = EventLoop::new().unwrap();
        event_loop.set_control_flow(ControlFlow::Wait);
        event_loop.run_app(&mut runner).unwrap();
    }
    #[cfg(target_arch = "wasm32")]
    {
        console_error_panic_hook::set_once();
        use winit::platform::web::EventLoopExtWebSys;
        let event_loop = EventLoop::new().unwrap();
        event_loop.set_control_flow(ControlFlow::Wait);
        event_loop.spawn_app(runner);
    }
}

// ── Exact web canvas sizing ───────────────────────────────────────────────────
//
// H7: the render buffer must be the **canvas element's** device-pixel size, not
// `window.innerWidth × dpr`. Measuring the element with `getBoundingClientRect`
// (the documented Safari fallback for `devicePixelContentBoxSize`) keeps the
// buffer exactly equal to the CSS box × DPR when `resolution_scale == 1`, so
// the browser never rescales the canvas (which would blur 1 px lines).

/// Physical (device) pixels of the winit canvas: `CSS box × DPR × scale`.
#[cfg(target_arch = "wasm32")]
pub(crate) fn canvas_physical_size(window: &winit::window::Window, scale: f64) -> (u32, u32) {
    use winit::platform::web::WindowExtWebSys;
    let dpr = web_sys::window()
        .map(|w| w.device_pixel_ratio())
        .unwrap_or(1.0)
        .max(1e-6);
    let scale = if scale.is_finite() && scale > 0.0 { scale } else { 1.0 };

    let css = window.canvas().and_then(|canvas| {
        let r = canvas.get_bounding_client_rect();
        if r.width() > 0.0 && r.height() > 0.0 {
            Some((r.width(), r.height()))
        } else {
            None
        }
    });

    let (css_w, css_h) = css.unwrap_or_else(|| {
        // Before the canvas is styled/attached: fall back to the window box.
        web_sys::window()
            .and_then(|w| {
                let ww = w.inner_width().ok()?.as_f64()?;
                let wh = w.inner_height().ok()?.as_f64()?;
                Some((ww, wh))
            })
            .unwrap_or((1.0, 1.0))
    });

    let w = (css_w * dpr * scale).round() as u32;
    let h = (css_h * dpr * scale).round() as u32;
    (w.max(1), h.max(1))
}

/// Resizes the winit canvas backing store to `(w, h)` device pixels.
#[cfg(target_arch = "wasm32")]
pub(crate) fn set_canvas_backing_size(window: &winit::window::Window, w: u32, h: u32) {
    use winit::platform::web::WindowExtWebSys;
    if let Some(canvas) = window.canvas() {
        if canvas.width() != w {
            canvas.set_width(w);
        }
        if canvas.height() != h {
            canvas.set_height(h);
        }
    }
}
