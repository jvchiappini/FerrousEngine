//! System for updating skeleton matrices.

use ferrous_ecs::prelude::*;
use ferrous_ecs::system::System;
use crate::scene::Skeleton;

pub struct SkinningSystem;

impl System for SkinningSystem {
    fn name(&self) -> &'static str { "SkinningSystem" }

    fn run(&mut self, world: &mut ferrous_ecs::world::World, _resources: &mut ResourceMap) {
        // Find entities with a Skeleton and update their bone matrices.
        // We do this by collecting IDs first because we need mutable access.
        let entities: Vec<ferrous_ecs::entity::Entity> = world
            .query::<Skeleton>()
            .map(|(e, _)| e)
            .collect();

        for entity in entities {
            if let Some(skeleton) = world.get_mut::<Skeleton>(entity) {
                skeleton.update_matrices();
            }
        }
    }
}
