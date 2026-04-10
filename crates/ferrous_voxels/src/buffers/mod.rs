//! GPU buffer management for the voxel renderer.
//!
//! This module owns the persistent GPU-side storage buffers that the voxel
//! compute passes read every frame.  The module is gated behind the `gpu`
//! feature flag so the rest of the crate remains wgpu-free for pure CPU use.

pub mod persistent;

pub use persistent::PersistentBuffers;
