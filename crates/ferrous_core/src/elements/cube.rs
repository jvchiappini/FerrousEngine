//! Cube element type — re-exported for backwards compatibility.
//!
//! The canonical way to create a cube in the scene is via
//! [`ferrous_core::scene::World::spawn_cube`], which stores the entity as an
//! [`Element`] with [`ElementKind::Cube`].  Position, name and ID all live
//! inside that `Element` (and its `Transform`), so there is no need for a
//! separate `Cube` struct that duplicates those fields.
//!
//! This module is kept so that existing `use ferrous_core::elements::cube`
//! paths continue to compile, but no new code should construct `Cube`
//! directly.

/// Re-export the canonical element types so callers that import from
/// `ferrous_core::elements::cube` still compile without changes.
pub use crate::scene::{ElementKind, Handle};
pub use crate::scene::World;
