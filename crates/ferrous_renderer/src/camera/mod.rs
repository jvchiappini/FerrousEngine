pub mod uniform;
pub mod controller;

pub use uniform::GpuCamera;
pub use controller::OrbitState;

// Re-export core camera types so callers only need to import from one place.
pub use ferrous_core::scene::{Camera, CameraUniform, Controller};
