//! `ferrous_ecs` — Archetype-based Entity Component System for Ferrous Engine.
//!
//! # Architecture
//!
//! | Module        | Responsibility                                              |
//! |---------------|-------------------------------------------------------------|
//! | `entity`      | Entity ID (index + generation), EntityAllocator             |
//! | `component`   | Component trait, TypeId-keyed metadata                      |
//! | `archetype`   | Dense SoA storage; one archetype per unique component set   |
//! | `world`       | spawn / despawn / insert / remove / get                     |
//! | `query`       | `WorldQuery` trait + `Query<Q>` tuple iterators             |
//! | `resource`    | Non-entity global state (ResourceMap)                       |
//! | `system`      | `System` trait, `SystemScheduler`, `StagedScheduler`        |
//! | `system_param`| `SystemParam` trait, `Res<T>`, `ResMut<T>`                  |
//! | `fn_system`   | `IntoSystem` trait, `FnSystem` — plain-function systems     |
//!
//! ## Parallel scheduling (`feature = "parallel"`)
//!
//! Enable the `parallel` Cargo feature to get [`system::parallel::ParallelScheduler`].
//! Systems declare their component/resource access via [`system::parallel::SystemAccess`];
//! the scheduler groups them into conflict-free batches and dispatches each batch
//! with `rayon::scope`.
//!
//! ## Non-Clone components
//!
//! Components that contain `Box<dyn Trait>` or other non-Clone types can be
//! stored using [`world::World::spawn_owned`] / [`world::World::insert_owned`].
//! These use `ComponentInfo::of_owned()` whose clone stub panics — never
//! trigger an archetype move on owned components.
//!
//! # Example
//! ```rust
//! use ferrous_ecs::prelude::*;
//!
//! #[derive(Debug, Clone)]
//! struct Position { x: f32, y: f32, z: f32 }
//! impl Component for Position {}
//!
//! #[derive(Debug, Clone)]
//! struct Velocity { dx: f32, dy: f32, dz: f32 }
//! impl Component for Velocity {}
//!
//! let mut world = World::new();
//! let e = world.spawn((Position { x: 0.0, y: 0.0, z: 0.0 }, Velocity { dx: 1.0, dy: 0.0, dz: 0.0 }));
//! assert!(world.contains(e));
//! ```

pub mod archetype;
pub mod component;
pub mod entity;
pub mod fn_system;
pub mod query;
pub mod resource;
pub mod system;
pub mod system_param;
pub mod world;

pub mod prelude {
    pub use crate::component::Component;
    pub use crate::entity::Entity;
    // New: function-system ergonomics
    pub use crate::fn_system::IntoSystem;
    pub use crate::query::{Query, QueryMut, WorldQuery};
    pub use crate::resource::ResourceMap;
    // Note: `crate::system::fn_system` (the legacy closure constructor) is
    // intentionally NOT re-exported here to avoid name collision with the
    // `crate::fn_system` module.  Use `crate::system::fn_system(...)` directly.
    pub use crate::system::{Stage, StagedScheduler, System, SystemScheduler};
    // Param value types (what users receive in their fn args)
    pub use crate::system_param::{Res, ResMut};
    // Param marker types (used to spell out param sets, e.g. ResParam<T>)
    pub use crate::system_param::{QueryParam, ResMutParam, ResParam, SystemParam};
    pub use crate::world::World;

    // Parallel scheduling — only available with `feature = "parallel"`.
    #[cfg(feature = "parallel")]
    pub use crate::system::parallel::{ParallelScheduler, SystemAccess, SystemMeta};
}
