//! `ferrous_voxels` — HashDAG voxel world, chunk management, and GPU passes.
//!
//! # Overview
//!
//! This crate is the foundation of the FerrousEngine voxel renderer roadmap.
//! It provides:
//!
//! - **`dag`** — `HashDAG`, `Voxel`, `DAGNode`, `BitGrid3D`: the CPU-side world representation.
//! - **`world`** — `VoxelWorld`, `ChunkManager`: high-level edit API and dirty tracking.
//! - **`buffers`** *(gpu feature)* — `PersistentBuffers`: GPU storage buffer management.
//! - **`passes`** *(gpu feature)* — `VoxelGpuUploadPass` and future compute passes.
//!
//! Future phases will add HDDA raymarching shaders, ReSTIR DI,
//! GI (DDGI + SSRC + WSRC), SVGF denoising, and TAA.
//!
//! See `VOXEL_RENDERER_ROADMAP.md` at the workspace root for the full plan.
//!
//! # Quick start (CPU only)
//!
//! ```rust
//! use ferrous_voxels::world::VoxelWorld;
//!
//! let mut world = VoxelWorld::new();
//!
//! // Build a 10×10×10 room.
//! world.fill_box((0,0,0), (9,9,9), 1 /*stone*/);
//!
//! // Blow a sphere out of the center.
//! world.destroy_sphere(5, 5, 5, 3);
//!
//! println!("Voxels: {}", world.voxel_count());
//! println!("Dirty chunks: {}", world.chunks.take_dirty().len());
//! ```

#![deny(missing_docs)]
#![warn(clippy::all)]

// Force `bitflags` as a direct dependency since `node.rs` uses it.
#[allow(unused_extern_crates)]
extern crate bitflags;

pub mod dag;
pub mod world;

// GPU-only modules (gated behind the `gpu` feature).
#[cfg(feature = "gpu")]
pub mod buffers;
#[cfg(feature = "gpu")]
pub mod passes;

// ── Top-level re-exports (convenience) ───────────────────────────────────────

pub use dag::{BitGrid3D, DAGNode, HashDAG, MaterialId, Voxel, VoxelFlags};
pub use world::{ChunkAABB, ChunkManager, VoxelWorld};

#[cfg(feature = "gpu")]
pub use buffers::PersistentBuffers;
#[cfg(feature = "gpu")]
pub use passes::VoxelGpuUploadPass;
#[cfg(feature = "gpu")]
pub use passes::HddaPrimaryPass;
