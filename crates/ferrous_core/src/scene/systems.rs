//! Logic systems that operate on the ECS World and Resources.

use ferrous_ecs::system::System;
use ferrous_ecs::resource::ResourceMap;

/// Updates the frame delta in the global resource map.
///
/// This is a "core" system that should always run at the start of the frame.
pub struct TimeSystem;

impl System for TimeSystem {
    fn name(&self) -> &'static str { "TimeSystem" }

    fn run(&mut self, _world: &mut ferrous_ecs::world::World, resources: &mut ResourceMap) {
        if let Some(clock) = resources.get_mut::<crate::time::TimeClock>() {
            clock.tick();
        }
    }
}

/// A system that could handle simple transform updates or physics in the future.
/// For now, it's a placeholder for logic that operates on all entities with a Transform.
pub struct TransformSystem;

impl System for TransformSystem {
    fn name(&self) -> &'static str { "TransformSystem" }

    fn run(&mut self, _world: &mut ferrous_ecs::world::World, _resources: &mut ResourceMap) {
        // Placeholder: currently Transform is updated manually via World::set_position
        // In a full ECS model, we would iterate and update matrices here if they were separate.
    }
}
