pub mod mesh;
pub mod primitives;
pub mod vertex;

pub use vertex::Vertex;
// expose helper used by primitives
pub use mesh::Mesh;
pub use vertex::compute_tangents;
