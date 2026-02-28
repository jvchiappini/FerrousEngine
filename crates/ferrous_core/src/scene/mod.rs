//! Lightweight scene helpers built on top of `elements`.

pub mod camera;
pub mod controller;
pub mod world;

pub use world::{Element, World};
// also expose the handle type so callers don't need to reach into the
// submodule.
pub use world::Handle;

// camera data lives in core so it can be shared by renderer/editor/etc.
pub use camera::Camera;
// also re-export the uniform so callers (eg. renderer) can access it easily
pub use camera::CameraUniform;
// expose controller type so callers can configure key mappings
pub use controller::Controller;
