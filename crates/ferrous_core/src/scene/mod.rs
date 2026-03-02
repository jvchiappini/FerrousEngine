//! Scene subsystem — World, Element, Camera, Controller.

pub mod camera;
pub mod controller;
pub mod world;
pub mod gizmo;

// World types
pub use world::{Element, ElementKind, Handle, World};

// Camera
pub use camera::{Camera, CameraUniform};

// Controller (key mappings + motion parameters)
pub use controller::Controller;

// re-export commonly-used gizmo types so callers don't have to import the
// submodule manually.  The renderer will only depend on the drawing types
// (in `ferrous_renderer::scene::gizmo`), but the editor and any other
// application code that implements interaction should be able to refer to
// `ferrous_core::scene::GizmoState` directly.
pub use gizmo::{Axis, AxisColors, GizmoMode, GizmoState, GizmoStyle, Plane, PlaneColors};
// re-export helper too
pub use gizmo::axis_vector;
