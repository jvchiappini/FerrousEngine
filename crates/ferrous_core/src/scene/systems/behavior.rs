//! User scripting hook — Behavior trait and BehaviorSystem.

#![cfg(feature = "ecs")]

use ferrous_ecs::prelude::*;
use ferrous_ecs::system::System;

// ────────────────────────────────────────────────────────────────────────────
// Behavior trait

/// Per-entity custom logic hook.
///
/// Implement this trait and attach it via `BehaviorComponent::new(my_behavior)`.
/// `BehaviorSystem` calls `update()` every frame for every entity that has one.
///
/// # Example
/// ```rust,ignore
/// struct Spinner { speed: f32 }
/// impl Behavior for Spinner {
///     fn update(&mut self, entity: Entity, world: &mut EcsWorld, _res: &mut ResourceMap, dt: f32) {
///         if let Some(t) = world.get_mut::<Transform>(entity) {
///             t.rotate_axis(Vec3::Y, self.speed * dt);
///         }
///     }
/// }
/// world.ecs.spawn((Transform::default(), BehaviorComponent::new(Spinner { speed: 1.0 })));
/// ```
pub trait Behavior: Send + Sync + 'static {
    fn update(
        &mut self,
        entity: ferrous_ecs::entity::Entity,
        world: &mut ferrous_ecs::world::World,
        resources: &mut ResourceMap,
        dt: f32,
    );

    fn on_start(
        &mut self,
        _entity: ferrous_ecs::entity::Entity,
        _world: &mut ferrous_ecs::world::World,
        _resources: &mut ResourceMap,
    ) {}
}

/// Wrapper component that boxes a `Behavior` implementation.
pub struct BehaviorComponent {
    pub(crate) inner: Box<dyn Behavior>,
    pub(crate) started: bool,
}

impl BehaviorComponent {
    pub fn new<B: Behavior>(b: B) -> Self {
        BehaviorComponent { inner: Box::new(b), started: false }
    }
}

impl Component for BehaviorComponent {}

impl std::fmt::Debug for BehaviorComponent {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "BehaviorComponent {{ started: {} }}", self.started)
    }
}

// ────────────────────────────────────────────────────────────────────────────
// BehaviorSystem

/// Calls `Behavior::update` on every entity with a `BehaviorComponent`.
///
/// Register at `Stage::Update`.
pub struct BehaviorSystem;

impl System for BehaviorSystem {
    fn name(&self) -> &'static str { "BehaviorSystem" }

    fn run(
        &mut self,
        world: &mut ferrous_ecs::world::World,
        resources: &mut ResourceMap,
    ) {
        let dt = resources
            .get::<crate::time::TimeClock>()
            .map(|c| c.at_tick().delta)
            .unwrap_or(0.0);

        let entities: Vec<ferrous_ecs::entity::Entity> = world
            .query::<BehaviorComponent>()
            .map(|(e, _)| e)
            .collect();

        for entity in entities {
            // SAFETY: We iterate one entity at a time, no aliased mutable access.
            let bc_ptr: *mut BehaviorComponent = match world.get_mut::<BehaviorComponent>(entity) {
                Some(bc) => bc as *mut _,
                None => continue,
            };
            let bc = unsafe { &mut *bc_ptr };
            if !bc.started {
                bc.inner.on_start(entity, world, resources);
                bc.started = true;
            }
            bc.inner.update(entity, world, resources, dt);
        }
    }
}
