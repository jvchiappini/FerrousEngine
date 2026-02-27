//! Collection of convenience primitives used by the renderer.

pub mod cube;

// re-export common helpers so callers can simply write `renderer::meshes::cube`
pub use cube::cube;
