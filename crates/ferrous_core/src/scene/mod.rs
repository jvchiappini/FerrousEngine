//! Scene subsystem — World, Element, Camera, Controller.

pub mod camera;
pub mod controller;
pub mod gizmo;
pub mod material;
pub mod systems;
pub mod world;

// World types
pub use world::{Element, ElementKind, Handle, PointLightComponent, World};

// Systems and stage enum
pub use systems::{
    AnimationClip, AnimationPlayer, AnimationSystem, Behavior, BehaviorComponent, BehaviorSystem,
    Camera3D, Camera3DBuilder, Children, DirectionalLight, GlobalTransform, Keyframe, OrbitCamera,
    OrbitCameraSystem, Parent, Stage, TimeSystem, TransformSystem, Velocity, VelocitySystem,
};

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

// material types are also part of the scene API; they live here so that the
// renderer can depend on `ferrous_core` while client code can still build
// descriptors without pulling in the renderer crate.
pub use material::{
    AlphaMode, Material, MaterialBuilder, MaterialDescriptor, MaterialHandle, RenderQuality,
    RenderStyle, MATERIAL_DEFAULT,
};
