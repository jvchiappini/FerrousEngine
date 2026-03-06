/// Re-exports the shared GPU context and adds a small helper so the rest of
/// the renderer can reference device/queue without repeating
/// `context.device` / `context.queue` everywhere.
///
/// `EngineContext` is now defined in `ferrous_gpu`; this module re-exports it
/// for internal use.  The backward-compat alias via `ferrous_core` still
/// works but new code should import from `ferrous_gpu` or this module.
pub use ferrous_gpu::EngineContext;

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
