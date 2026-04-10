//! DAG module — HashDAG, node types, and bit grids.

pub mod bit_grid;
pub mod hash_dag;
pub mod node;

#[cfg(feature = "gpu")]
pub mod gpu_types;
#[cfg(feature = "gpu")]
pub mod gpu_upload;

pub use bit_grid::{BitGrid3D, LevelGrids};
pub use hash_dag::{HashDAG, DAG_LEVELS, LEVEL_SIZES};
pub use node::{DAGNode, MaterialId, Voxel, VoxelFlags, CHILDREN_PER_NODE};
