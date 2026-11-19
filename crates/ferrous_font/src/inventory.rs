//! Optional bridge between the font caches and the engine memory inventory.
//!
//! `ferrous_font` deliberately does not depend on `ferrous_core`, so it cannot
//! call `ferrous_core::memory` directly.  The engine installs a sink once
//! (`ferrous_ui_render::GuiRenderer::new`), and every registration made by this
//! crate (and by `ferrous_assets`, which re-exports this module) is forwarded to
//! `ferrous_core::memory::{register_cpu, register_gpu}`.
//!
//! With no sink installed every call is a no-op, so a tool or a headless test
//! that uses `ferrous_font` on its own keeps working.  The labels are stable and
//! include the atlas key, so a resize or a cache update replaces the previous
//! byte count instead of adding a second entry.

use std::sync::OnceLock;

/// The subsystem that owns a registered resource.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Subsystem {
    /// Font atlases and layout caches.
    Fonts,
    /// Decoded images and texture caches owned by `ferrous_assets`.
    Assets,
}

/// Whether the registered bytes are GPU memory or a CPU-side copy.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MemorySide {
    /// CPU-side bytes (pixels, glyph tables, layout caches).
    Cpu,
    /// GPU bytes (textures).
    Gpu,
}

/// Signature of the registration callback installed by the host.
pub type RegisterFn = fn(&str, u64, MemorySide, Subsystem);

/// Signature of the removal callback installed by the host.
pub type UnregisterFn = fn(&str, MemorySide, Subsystem);

static REGISTER: OnceLock<RegisterFn> = OnceLock::new();
static UNREGISTER: OnceLock<UnregisterFn> = OnceLock::new();

/// Installs the host inventory bridge.  The first call wins; later calls are
/// ignored, so creating several renderers is harmless.
pub fn install(register: RegisterFn, unregister: UnregisterFn) {
    let _ = REGISTER.set(register);
    let _ = UNREGISTER.set(unregister);
}

/// Whether a host has installed the bridge.
#[must_use]
pub fn is_installed() -> bool {
    REGISTER.get().is_some()
}

/// Registers or updates a CPU-side resource.  A no-op without a sink.
pub fn register_cpu(label: &str, bytes: u64, subsystem: Subsystem) {
    if let Some(register) = REGISTER.get() {
        register(label, bytes, MemorySide::Cpu, subsystem);
    }
}

/// Registers or updates a GPU resource.  A no-op without a sink.
pub fn register_gpu(label: &str, bytes: u64, subsystem: Subsystem) {
    if let Some(register) = REGISTER.get() {
        register(label, bytes, MemorySide::Gpu, subsystem);
    }
}

/// Removes a CPU-side entry.  A no-op without a sink.
pub fn unregister_cpu(label: &str, subsystem: Subsystem) {
    if let Some(unregister) = UNREGISTER.get() {
        unregister(label, MemorySide::Cpu, subsystem);
    }
}

/// Removes a GPU entry.  A no-op without a sink.
pub fn unregister_gpu(label: &str, subsystem: Subsystem) {
    if let Some(unregister) = UNREGISTER.get() {
        unregister(label, MemorySide::Gpu, subsystem);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn installing_twice_keeps_the_first_sink() {
        fn first(_: &str, _: u64, _: MemorySide, _: Subsystem) {}
        fn second(_: &str, _: u64, _: MemorySide, _: Subsystem) {}
        fn remove(_: &str, _: MemorySide, _: Subsystem) {}
        // The process may already have a sink from another test; installing is
        // still idempotent and never panics.
        install(first, remove);
        install(second, remove);
        assert!(is_installed());
        register_cpu("test:font", 1, Subsystem::Fonts);
        unregister_cpu("test:font", Subsystem::Fonts);
    }
}
