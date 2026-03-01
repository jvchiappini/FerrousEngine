/// Re-exports the shared GPU context supplied by `ferrous_core` and adds a
/// small helper so the rest of the renderer can reference device/queue without
/// repeating `context.device` / `context.queue` everywhere.
///
/// We deliberately keep this file thin: the real wgpu adapter/surface setup
/// lives in `ferrous_app` and `ferrous_core`.  The renderer only receives an
/// already-initialised `EngineContext`.
pub use ferrous_core::context::EngineContext;

use wgpu::{Device, Queue};

/// Convenience accessor — borrows the wgpu `Device` from an `EngineContext`.
#[inline]
pub fn device(ctx: &EngineContext) -> &Device {
    &ctx.device
}

/// Convenience accessor — borrows the wgpu `Queue` from an `EngineContext`.
#[inline]
pub fn queue(ctx: &EngineContext) -> &Queue {
    &ctx.queue
}
