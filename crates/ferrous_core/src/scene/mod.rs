//! Scene subsystem — World, Element, Camera, Controller.

pub mod camera;
pub mod controller;
pub mod world;

// World types
pub use world::{Element, ElementKind, Handle, World};

// Camera
pub use camera::{Camera, CameraUniform};

// Controller (key mappings + motion parameters)
pub use controller::Controller;
