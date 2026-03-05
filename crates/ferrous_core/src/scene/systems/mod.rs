//! Built-in ECS systems for FerrousEngine.
//!
//! This module re-exports all system types from focused sub-modules:
//!
//! | Sub-module   | Contents                                              |
//! |-------------|-------------------------------------------------------|
//! | `time`      | `Velocity`, `TimeSystem`, `VelocitySystem`            |
//! | `animation` | `Keyframe`, `AnimationClip`, `AnimationPlayer`, `AnimationSystem` |
//! | `behavior`  | `Behavior`, `BehaviorComponent`, `BehaviorSystem`     |
//! | `hierarchy` | `Parent`, `Children`, `GlobalTransform`, `TransformSystem` |
//! | `lighting`  | `DirectionalLight`                                    |
//! | `camera`    | `Camera3D`, `Camera3DBuilder`, `OrbitCamera`, `OrbitCameraSystem` |
//!
//! # Recommended registration order
//! ```text
//! PreUpdate  →  TimeSystem
//! Update     →  VelocitySystem, AnimationSystem, BehaviorSystem, OrbitCameraSystem
//! PostUpdate →  TransformSystem
//! ```

pub mod animation;
pub mod behavior;
pub mod camera;
pub mod hierarchy;
pub mod lighting;
pub mod time;

// ── Re-export stage ──────────────────────────────────────────────────────────
pub use ferrous_ecs::system::Stage;

// ── animation ────────────────────────────────────────────────────────────────
pub use animation::{AnimationClip, AnimationPlayer, AnimationSystem, Keyframe};

// ── behavior ─────────────────────────────────────────────────────────────────
pub use behavior::{Behavior, BehaviorComponent, BehaviorSystem};

// ── camera ───────────────────────────────────────────────────────────────────
pub use camera::{Camera3D, Camera3DBuilder, OrbitCamera, OrbitCameraSystem};

// ── hierarchy ────────────────────────────────────────────────────────────────
pub use hierarchy::{Children, GlobalTransform, Parent, TransformSystem};

// ── lighting ─────────────────────────────────────────────────────────────────
pub use lighting::DirectionalLight;

// ── time ─────────────────────────────────────────────────────────────────────
pub use time::{TimeSystem, Velocity, VelocitySystem};

// ────────────────────────────────────────────────────────────────────────────
// Tests (integration — exercises all sub-modules together)
// ────────────────────────────────────────────────────────────────────────────
#[cfg(test)]
mod tests {
    use super::*;
    use crate::transform::Transform;
    use ferrous_ecs::prelude::*;
    use glam::Vec3;

    fn make_world_with_clock() -> (ferrous_ecs::world::World, ResourceMap) {
        let world = ferrous_ecs::world::World::new();
        let mut res = ResourceMap::new();
        let mut clock = crate::time::TimeClock::new();
        clock.tick();
        res.insert(clock);
        (world, res)
    }

    #[test]
    fn time_system_ticks_clock() {
        let mut world = ferrous_ecs::world::World::new();
        let mut res = ResourceMap::new();
        res.insert(crate::time::TimeClock::new());

        let mut sys = TimeSystem;
        sys.run(&mut world, &mut res);

        let t = res.get::<crate::time::TimeClock>().unwrap().at_tick();
        assert_eq!(t.frame_count, 0);

        sys.run(&mut world, &mut res);
        let t2 = res.get::<crate::time::TimeClock>().unwrap().at_tick();
        assert_eq!(t2.frame_count, 1);
    }

    #[test]
    fn velocity_system_moves_entity() {
        let (mut world, mut res) = make_world_with_clock();

        let t = Transform::from_position(Vec3::ZERO);
        let v = Velocity(Vec3::new(10.0, 0.0, 0.0));
        let entity = world.spawn((t, v));

        let dt = res.get::<crate::time::TimeClock>().unwrap().at_tick().delta;

        let mut sys = VelocitySystem;
        sys.run(&mut world, &mut res);

        let pos = world.get::<Transform>(entity).unwrap().position;
        assert!((pos.x - 10.0 * dt).abs() < 1e-4, "x = {}", pos.x);
    }

    #[test]
    fn animation_system_advances_and_applies_position() {
        let (mut world, mut res) = make_world_with_clock();

        let clip = AnimationClip {
            position_keys: vec![
                Keyframe { time: 0.0, value: Vec3::ZERO },
                Keyframe { time: 1.0, value: Vec3::new(1.0, 0.0, 0.0) },
            ],
            duration: 1.0,
            looping: false,
        };
        let t = Transform::from_position(Vec3::ZERO);
        let player = AnimationPlayer::new(clip);
        let entity = world.spawn((t, player));

        let mut sys = AnimationSystem;
        sys.run(&mut world, &mut res);

        let pos = world.get::<Transform>(entity).unwrap().position;
        assert!(pos.x >= 0.0);
    }

    #[test]
    fn transform_system_propagates_parent() {
        let mut world = ferrous_ecs::world::World::new();
        let mut res = ResourceMap::new();

        let parent_transform = Transform::from_position(Vec3::new(10.0, 0.0, 0.0));
        let child_transform  = Transform::from_position(Vec3::new(1.0, 0.0, 0.0));

        let parent_entity = world.spawn((parent_transform,));
        world.insert(parent_entity, GlobalTransform::default());

        let child_entity = world.spawn((child_transform,));
        world.insert(child_entity, GlobalTransform::default());
        world.insert(child_entity, Parent(parent_entity));

        let mut sys = TransformSystem;
        sys.run(&mut world, &mut res);

        let child_global = world.get::<GlobalTransform>(child_entity).unwrap().0;
        let child_pos = child_global.w_axis.truncate();
        assert!((child_pos.x - 11.0).abs() < 1e-4, "child global x = {}", child_pos.x);
    }

    #[test]
    fn behavior_system_calls_update() {
        use std::sync::{Arc, Mutex};

        let counter = Arc::new(Mutex::new(0u32));
        let counter_clone = Arc::clone(&counter);

        struct CountBehavior { counter: Arc<Mutex<u32>> }
        impl Behavior for CountBehavior {
            fn update(
                &mut self,
                _e: ferrous_ecs::entity::Entity,
                _w: &mut ferrous_ecs::world::World,
                _r: &mut ResourceMap,
                _dt: f32,
            ) {
                *self.counter.lock().unwrap() += 1;
            }
        }

        let (mut world, mut res) = make_world_with_clock();
        world.spawn_owned(BehaviorComponent::new(CountBehavior { counter: counter_clone }));

        let mut sys = BehaviorSystem;
        sys.run(&mut world, &mut res);
        assert_eq!(*counter.lock().unwrap(), 1);
        sys.run(&mut world, &mut res);
        assert_eq!(*counter.lock().unwrap(), 2);
    }

    // ── Phase 4.5 tests ──────────────────────────────────────────────────

    #[test]
    fn directional_light_implements_component() {
        fn assert_component<T: Component>() {}
        assert_component::<DirectionalLight>();
    }

    #[test]
    fn directional_light_default_values() {
        let light = DirectionalLight {
            direction: Vec3::new(-0.6, -0.8, -0.4).normalize(),
            color: crate::color::Color::WARM_WHITE,
            intensity: 3.5,
        };
        assert!((light.intensity - 3.5).abs() < 1e-6);
        assert!((light.direction.length() - 1.0).abs() < 1e-5);
    }

    #[test]
    fn camera3d_builder_sets_eye_at_distance() {
        let cam = Camera3D::looking_at(Vec3::ZERO).distance(5.0).build();
        let dist = cam.eye.length();
        assert!((dist - 5.0).abs() < 1e-4, "distance={}", dist);
        assert_eq!(cam.target, Vec3::ZERO);
    }

    #[test]
    fn camera3d_builder_custom_fov() {
        let cam = Camera3D::looking_at(Vec3::ZERO).fov(90.0).build();
        assert!((cam.fov_deg - 90.0).abs() < 1e-6);
    }

    #[test]
    fn orbit_camera_implements_component() {
        fn assert_component<T: Component>() {}
        assert_component::<OrbitCamera>();
    }

    #[test]
    fn orbit_camera_system_updates_eye() {
        let (mut world, mut res) = make_world_with_clock();

        world.spawn((
            OrbitCamera { yaw: 0.0, pitch: 0.0, distance: 5.0, target: Vec3::ZERO },
            Camera3D { eye: Vec3::new(0.0, 0.0, 5.0), target: Vec3::ZERO,
                       fov_deg: 60.0, near: 0.1, far: 1000.0 },
        ));

        let mut sys = OrbitCameraSystem;
        sys.run(&mut world, &mut res);

        let cams: Vec<Camera3D> = world.query::<Camera3D>().map(|(_, c)| *c).collect();
        assert_eq!(cams.len(), 1);
        let dist = (cams[0].eye - cams[0].target).length();
        assert!((dist - 5.0).abs() < 1e-3, "dist={}", dist);
    }
}
