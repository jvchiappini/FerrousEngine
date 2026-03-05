//! Scene world — the primary container for all runtime objects.
//!
//! `World` is backed by a `Vec` indexed by a monotonically-increasing `u64` ID
//! (stable across insertions/removals) and mirrored into an ECS world for
//! component-based queries.
//!
//! # Quick start
//! ```rust,ignore
//! use ferrous_core::{World, ElementKind, Color};
//! use glam::Vec3;
//!
//! let mut world = World::new();
//! let h = world.spawn("Player")
//!     .with_position(Vec3::new(0.0, 0.5, 0.0))
//!     .build();
//! world.set_position(h, Vec3::new(1.0, 0.0, 0.0));
//! world.despawn(h);
//! ```

mod builder;
mod query;
mod scene;
mod types;

pub use builder::EntityBuilder;
pub use scene::World;
pub use types::{
    Element, ElementKind, Handle, MaterialComponent, PointLightComponent,
};

// ─── Tests ─────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use glam::Vec3;
    use crate::scene::{MaterialDescriptor, MaterialHandle, MATERIAL_DEFAULT};

    #[test]
    fn spawn_and_despawn() {
        let mut w = World::new();
        let h = w.spawn_cube("A", Vec3::ZERO);
        assert!(w.contains(h));
        assert_eq!(w.len(), 1);
        assert!(w.despawn(h));
        assert!(!w.contains(h));
        assert_eq!(w.len(), 0);
    }

    #[test]
    fn spawn_mesh_helper() {
        let mut w = World::new();
        let key = "foo.mesh";
        let h = w.spawn_mesh("MeshObj", key, Vec3::new(1.0, 2.0, 3.0));
        assert!(w.contains(h));
        let elem = w.get(h).unwrap();
        if let ElementKind::Mesh { asset_key } = &elem.kind {
            assert_eq!(asset_key, key);
        } else {
            panic!("expected Mesh kind");
        }
        assert_eq!(elem.transform.position, Vec3::new(1.0, 2.0, 3.0));
    }

    #[test]
    fn default_material_properties() {
        let mut w = World::new();
        let h = w.spawn_cube("M", Vec3::ZERO);
        let e = w.get(h).expect("entity missing");
        assert_eq!(e.material.handle, MATERIAL_DEFAULT);
        let desc = &e.material.descriptor;
        assert_eq!(desc.base_color, [1.0, 1.0, 1.0, 1.0]);
        assert_eq!(desc.roughness, 0.5);
    }

    #[test]
    fn spawn_quad_behavior() {
        let mut w = World::new();
        let h = w.spawn_quad("Q", Vec3::ZERO, 2.0, 4.0, false);
        assert!(w.contains(h));
        let e = w.get(h).expect("entity missing");
        if let ElementKind::Quad { width, height, double_sided } = e.kind.clone() {
            assert_eq!(width, 2.0);
            assert_eq!(height, 4.0);
            assert!(!double_sided);
        } else {
            panic!("wrong kind");
        }
        assert_eq!(w.len(), 1);
        assert!(w.despawn(h));
    }

    #[test]
    fn spawn_sphere_behavior() {
        let mut w = World::new();
        let h = w.spawn_sphere("S", Vec3::ZERO, 2.0, 16);
        assert!(w.contains(h));
        let e = w.get(h).expect("entity missing");
        if let ElementKind::Sphere { radius, latitudes, longitudes } = e.kind.clone() {
            assert_eq!(radius, 2.0);
            assert_eq!(latitudes, 16);
            assert_eq!(longitudes, 16);
        } else {
            panic!("wrong kind");
        }
        assert_eq!(w.get(h).unwrap().transform.scale, Vec3::splat(2.0));
        assert_eq!(w.len(), 1);
        assert!(w.despawn(h));
    }

    #[test]
    fn position_roundtrip() {
        let mut w = World::new();
        let h = w.spawn_cube("B", Vec3::ZERO);
        w.set_position(h, Vec3::new(1.0, 2.0, 3.0));
        assert_eq!(w.position(h), Some(Vec3::new(1.0, 2.0, 3.0)));
    }

    #[test]
    fn rotate_entity_about_world_origin() {
        let mut w = World::new();
        let h = w.spawn_cube("R", Vec3::new(1.0, 0.0, 0.0));
        w.rotate_around(h, Vec3::ZERO, Vec3::Z, std::f32::consts::FRAC_PI_2);
        let pos = w.position(h).expect("entity missing");
        assert!((pos - Vec3::new(0.0, 1.0, 0.0)).length() < 1e-5, "pos={:?}", pos);
        let e = w.get(h).expect("missing entity");
        let right = e.transform.right();
        assert!((right - Vec3::Y).length() < 1e-4, "right={:?}", right);
    }

    #[test]
    fn rotate_z_helper_works() {
        let mut w = World::new();
        let h = w.spawn_cube("Z", Vec3::new(2.0, 0.0, 0.0));
        w.rotate_around_z(h, Vec3::new(1.0, 0.0, 0.0), std::f32::consts::PI);
        let pos = w.position(h).expect("entity missing");
        assert!((pos - Vec3::ZERO).length() < 1e-4, "pos={:?}", pos);
    }

    #[test]
    fn rotate_axis_wrapper() {
        let mut w = World::new();
        let h = w.spawn_cube("A", Vec3::ZERO);
        w.rotate_axis(h, Vec3::Z, std::f32::consts::FRAC_PI_2);
        let pos = w.position(h).expect("entity missing");
        assert!(pos.length() < 1e-5, "pos={:?}", pos);
        let e = w.get(h).expect("missing entity");
        let right = e.transform.right();
        assert!((right - Vec3::Y).length() < 1e-3, "right={:?}", right);
    }

    #[test]
    fn rotate_y_wrapper() {
        let mut w = World::new();
        let h = w.spawn_cube("B", Vec3::ZERO);
        w.rotate_y(h, std::f32::consts::FRAC_PI_2);
        let e = w.get(h).expect("missing entity");
        let forward = e.transform.forward();
        assert!((forward - Vec3::NEG_X).length() < 1e-5);
    }

    #[test]
    fn tags() {
        let mut w = World::new();
        let h = w.spawn("C").with_tag("enemy").build();
        assert!(w.has_tag(h, "enemy"));
        assert!(!w.has_tag(h, "player"));
        let enemies: Vec<_> = w.iter_tagged("enemy").collect();
        assert_eq!(enemies.len(), 1);
    }

    #[test]
    fn handles_are_stable_after_other_despawn() {
        let mut w = World::new();
        let h1 = w.spawn_cube("X", Vec3::ZERO);
        let h2 = w.spawn_cube("Y", Vec3::ONE);
        w.despawn(h1);
        assert!(w.contains(h2));
        assert_eq!(w.position(h2), Some(Vec3::ONE));
    }

    #[test]
    fn material_descriptor_and_handle_manipulation() {
        let mut w = World::new();
        let h = w.spawn_cube("MatTest", Vec3::ZERO);
        assert_eq!(w.get(h).unwrap().material.handle, MATERIAL_DEFAULT);
        assert_eq!(
            w.get(h).unwrap().material.descriptor,
            MaterialDescriptor::default()
        );

        let mut desc = MaterialDescriptor::default();
        desc.roughness = 0.25;
        w.set_material_descriptor(h, desc.clone());
        assert_eq!(w.get(h).unwrap().material.descriptor, desc);

        let new_handle = MaterialHandle(5);
        w.set_material_handle(h, new_handle);
        assert_eq!(w.get(h).unwrap().material.handle, new_handle);
    }
}
