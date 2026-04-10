//! World module — VoxelWorld and chunk management.

pub mod chunk_manager;
pub mod voxel_edit;

pub use chunk_manager::{ChunkAABB, ChunkManager};
pub use voxel_edit::VoxelWorld;
