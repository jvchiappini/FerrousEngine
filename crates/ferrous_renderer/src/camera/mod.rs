pub mod uniform;
pub mod controller;

pub use uniform::GpuCamera;
pub use controller::OrbitState;

// Re-export core camera types so callers only need to import from one place.
// Note: `CameraUniform` has moved to `crate::resources::camera::CameraUniform`
// and is no longer re-exported here; external users should import it directly
// from `ferrous_renderer::resources`.
pub use ferrous_core::scene::{Camera, Controller};
