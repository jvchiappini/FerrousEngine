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
//! | `query`       | WorldQuery trait + safe query iterators                     |
//! | `resource`    | Non-entity global state (ResourceMap)                       |
//! | `system`      | System trait + linear SystemScheduler                      |
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
pub mod query;
pub mod resource;
pub mod system;
pub mod world;

pub mod prelude {
    pub use crate::component::Component;
    pub use crate::entity::Entity;
    pub use crate::query::{Query, QueryMut};
    pub use crate::resource::ResourceMap;
    pub use crate::system::{System, SystemScheduler, StagedScheduler, Stage, fn_system};
    pub use crate::world::World;
}
