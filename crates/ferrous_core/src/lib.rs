// ferrous_core: tipos básicos y utilidades

#[derive(Copy, Clone, Debug)]
pub struct Transform {
    pub position: glam::Vec3,
    pub rotation: glam::Vec3,
    pub scale: glam::Vec3,
}

impl Default for Transform {
    fn default() -> Self {
        Self {
            position: glam::Vec3::ZERO,
            rotation: glam::Vec3::ZERO,
            scale: glam::Vec3::ONE,
        }
    }
}

// expose context module
pub mod context;

// input helper for keyboard / mouse state
pub mod input;

// re-export common input types so callers don't need to depend on winit
pub use input::{InputState, KeyCode, MouseButton};

// process / system metrics helpers; see `metrics.rs` for details.
pub mod metrics;

// re-export the most common helpers so consumers don't need to type
// `metrics::` if they just want the convenience functions.
pub use metrics::{get_cpu_usage, get_ram_usage_bytes, get_virtual_memory_bytes};
// also expose the convenience megabyte variants
pub use metrics::{get_ram_usage_mb, get_virtual_memory_mb};

// simple component definitions for game/editor objects. the `elements`
// namespace lives inside the core crate because it represents abstract data
// that is independent of any particular renderer or platform. earlier
// versions of the engine contained a full `World` type here, but that
// functionality has since been moved to `scene`. at the moment the module
// only contains a `cube` component; more types can be added later.

pub mod elements;

// simple scene wrapper that holds high‑level elements and exposes a very
// small API for adding them.  the existing `elements::World` is a full
// entity/component container; this module lives alongside it and provides an
// even easier entry point for demos or tests where the caller just wants to
// treat a scene as "a bunch of cubes".
pub mod scene;
