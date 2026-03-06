//! Parent-child transform hierarchy components and TransformSystem.

#![cfg(feature = "ecs")]

use ferrous_ecs::prelude::*;
use ferrous_ecs::system::System;

// ────────────────────────────────────────────────────────────────────────────
// Hierarchy components

/// Parent link — set this on a child entity to form a scene hierarchy.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Parent(pub ferrous_ecs::entity::Entity);
impl Component for Parent {}

/// List of direct children.
#[derive(Debug, Clone)]
pub struct Children(pub Vec<ferrous_ecs::entity::Entity>);
impl Component for Children {}

impl Default for Children {
    fn default() -> Self { Children(Vec::new()) }
}

/// Computed world-space transform — read-only output of `TransformSystem`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GlobalTransform(pub glam::Mat4);
impl Component for GlobalTransform {}

impl Default for GlobalTransform {
    fn default() -> Self { GlobalTransform(glam::Mat4::IDENTITY) }
}

// ────────────────────────────────────────────────────────────────────────────
// TransformSystem

/// Propagates local `Transform` through the parent-child hierarchy to
/// produce `GlobalTransform` on every entity.
///
/// Register at `Stage::PostUpdate`.
pub struct TransformSystem;

impl System for TransformSystem {
    fn name(&self) -> &'static str { "TransformSystem" }

    fn run(
        &mut self,
        world: &mut ferrous_ecs::world::World,
        _resources: &mut ResourceMap,
    ) {
        // Pass 1: set GlobalTransform = local matrix.
        let locals: Vec<(ferrous_ecs::entity::Entity, glam::Mat4)> = world
            .query::<crate::transform::Transform>()
            .map(|(e, t)| (e, t.matrix()))
            .collect();

        for (entity, mat) in &locals {
            if world.get::<GlobalTransform>(*entity).is_none() {
                world.insert(*entity, GlobalTransform(glam::Mat4::IDENTITY));
            }
            if let Some(gt) = world.get_mut::<GlobalTransform>(*entity) {
                gt.0 = *mat;
            }
        }

        // Pass 2: apply parent transforms (single level).
        let parent_pairs: Vec<(ferrous_ecs::entity::Entity, ferrous_ecs::entity::Entity)> = world
            .query::<Parent>()
            .map(|(e, p)| (e, p.0))
            .collect();

        for (child, parent_entity) in parent_pairs {
            let parent_global = match world.get::<GlobalTransform>(parent_entity) {
                Some(gt) => gt.0,
                None => glam::Mat4::IDENTITY,
            };
            if let Some(child_gt) = world.get_mut::<GlobalTransform>(child) {
                child_gt.0 = parent_global * child_gt.0;
            }
        }
    }
}
