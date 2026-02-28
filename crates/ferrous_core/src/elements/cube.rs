//! Simple marker/parameter set for a cube element.  This component lives in
//! `ferrous_core` so that editor, runtime and tests can refer to cube objects
//! without pulling in the renderer crate.

/// Cube component is currently just a unit‑sized marker.  future versions
/// could carry per‑entity colour, size or even procedural parameters.
#[derive(Clone, Debug, PartialEq)]
pub struct Cube {
    /// physical size of the cube in world units; 1.0 corresponds to the unit
    /// cube created by `ferrous_renderer::mesh::Mesh::cube()`.
    pub size: f32,
    /// user-visible name assigned when the cube is created.
    pub name: String,
    /// unique identifier for this cube instance.  generated automatically when
    /// the cube is constructed.
    pub id: u32,
    /// current position of the cube in world space.  this used to be stored in
    /// the scene `Transform`, but the requirement now is that each element
    /// remembers its own location.
    pub position: glam::Vec3,
}

impl Default for Cube {
    fn default() -> Self {
        let id = next_id();
        Cube {
            size: 1.0,
            name: format!("Cube {}", id),
            id,
            position: glam::Vec3::ZERO,
        }
    }
}

/// Create a new cube with the given name and position.  the id is assigned
/// automatically.
impl Cube {
    pub fn new(name: impl Into<String>, position: glam::Vec3) -> Self {
        let id = next_id();
        Cube {
            size: 1.0,
            name: name.into(),
            id,
            position,
        }
    }
}

// simple atomic counter for cube ids
use std::sync::atomic::{AtomicU32, Ordering};

fn next_id() -> u32 {
    static COUNTER: AtomicU32 = AtomicU32::new(1);
    COUNTER.fetch_add(1, Ordering::Relaxed)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_has_name_and_id() {
        let c = Cube::default();
        assert!(c.id != 0);
        assert!(c.name.contains(&c.id.to_string()));
        assert_eq!(c.position, glam::Vec3::ZERO);
    }

    #[test]
    fn new_sets_values() {
        let pos = glam::Vec3::new(1.0, 2.0, 3.0);
        let c = Cube::new("foo", pos);
        assert_eq!(c.name, "foo");
        assert_eq!(c.position, pos);
        assert!(c.id != 0);
    }
}
