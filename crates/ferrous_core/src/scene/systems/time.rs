//! Time and velocity ECS components and systems.

use ferrous_ecs::prelude::*;
use ferrous_ecs::system::System;

// ────────────────────────────────────────────────────────────────────────────
// Velocity component

/// Linear velocity in world space (metres / second).
///
/// Attach this component to any entity that should move automatically each
/// frame.  `VelocitySystem` integrates position by `velocity * delta_time`.
///
/// # Example
/// ```rust,ignore
/// world.ecs.spawn((Transform::from_position(Vec3::ZERO), Velocity(Vec3::X * 2.0)));
/// ```
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Velocity(pub glam::Vec3);
impl Component for Velocity {}

impl Default for Velocity {
    fn default() -> Self {
        Velocity(glam::Vec3::ZERO)
    }
}

// ────────────────────────────────────────────────────────────────────────────
// TimeSystem

/// Updates the `TimeClock` resource at the start of each frame.
///
/// Register at `Stage::PreUpdate` so all other systems see a consistent
/// `Time` value for the current frame.
pub struct TimeSystem;

impl System for TimeSystem {
    fn name(&self) -> &'static str {
        "TimeSystem"
    }

    fn run(&mut self, _world: &mut ferrous_ecs::world::World, resources: &mut ResourceMap) {
        if let Some(clock) = resources.get_mut::<crate::time::TimeClock>() {
            clock.tick();
        }
    }
}

// ────────────────────────────────────────────────────────────────────────────
// VelocitySystem

/// Integrates `Velocity` into `Transform::position` each frame.
///
/// Register at `Stage::Update`.  Reads `TimeClock` from resources for `dt`.
pub struct VelocitySystem;

impl System for VelocitySystem {
    fn name(&self) -> &'static str {
        "VelocitySystem"
    }

    fn run(
        &mut self,
        world: &mut ferrous_ecs::world::World,
        resources: &mut ResourceMap,
    ) {
        let dt = resources
            .get::<crate::time::TimeClock>()
            .map(|c| c.at_tick().delta)
            .unwrap_or(0.0);

        if dt <= 0.0 {
            return;
        }

        let pairs: Vec<(ferrous_ecs::entity::Entity, glam::Vec3)> = world
            .query::<Velocity>()
            .map(|(e, v)| (e, v.0))
            .collect();

        for (entity, vel) in pairs {
            if let Some(t) = world.get_mut::<crate::transform::Transform>(entity) {
                t.position += vel * dt;
            }
        }
    }
}
