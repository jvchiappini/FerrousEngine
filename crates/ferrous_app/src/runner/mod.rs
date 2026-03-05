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
    let mut runner = Runner::new(app, config);
    let event_loop = EventLoop::new().unwrap();
    event_loop.set_control_flow(ControlFlow::Wait);
    event_loop.run_app(&mut runner).unwrap();
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
        let event_loop = EventLoop::new().unwrap();
        event_loop.set_control_flow(ControlFlow::Wait);
        event_loop.run_app(&mut runner).unwrap();
    }
}
