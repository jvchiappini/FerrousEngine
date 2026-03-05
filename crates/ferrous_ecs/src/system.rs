//! System trait and schedulers.
//!
//! A `System` is a unit of logic that operates on a `World` (and optionally
//! its `ResourceMap`).  The `SystemScheduler` runs systems in insertion order
//! within a single stage.  The `StagedScheduler` groups systems into named
//! stages (`PreUpdate`, `Update`, `PostUpdate`, `Render`) so that systems
//! execute in a well-defined order regardless of registration order.
//!
//! # Stage execution order
//! ```text
//! PreUpdate  → systems that must see the world before gameplay logic
//!   (e.g. TimeSystem reads real-wall time and stores it as a resource)
//! Update     → core gameplay logic
//!   (e.g. VelocitySystem, BehaviorSystem, AnimationSystem)
//! PostUpdate → fixup passes that consume Update output
//!   (e.g. TransformSystem propagates parent→child global transforms)
//! Render     → CPU-side render preparation (culling, packet building)
//! ```

use crate::resource::ResourceMap;
use crate::world::World;

// ---------------------------------------------------------------------------

/// A unit of game logic.
///
/// Implement this trait for structs that process components each frame:
///
/// ```rust
/// use ferrous_ecs::system::System;
/// use ferrous_ecs::world::World;
/// use ferrous_ecs::resource::ResourceMap;
/// use ferrous_ecs::component::Component;
///
/// #[derive(Clone)] struct Pos(f32);
/// impl Component for Pos {}
///
/// struct MoveSystem;
/// impl System for MoveSystem {
///     fn run(&mut self, world: &mut World, _res: &mut ResourceMap) {
///         let mut qm = ferrous_ecs::query::QueryMut::<Pos>::new(world);
///         qm.for_each_mut(|_, p| p.0 += 1.0);
///     }
/// }
/// ```
pub trait System: Send + 'static {
    /// Name used in profiling and debugging.
    fn name(&self) -> &'static str {
        std::any::type_name::<Self>()
    }

    /// Execute the system for one tick.
    fn run(&mut self, world: &mut World, resources: &mut ResourceMap);
}

// ---------------------------------------------------------------------------

/// Simple sequential system scheduler.
///
/// Systems run in the order they were added.  Each system receives exclusive
/// access to `World` and `ResourceMap`.
pub struct SystemScheduler {
    systems: Vec<Box<dyn System>>,
}

impl Default for SystemScheduler {
    fn default() -> Self {
        SystemScheduler::new()
    }
}

impl SystemScheduler {
    pub fn new() -> Self {
        SystemScheduler {
            systems: Vec::new(),
        }
    }

    /// Append a system. Systems run in insertion order.
    pub fn add<S: System>(&mut self, system: S) -> &mut Self {
        self.systems.push(Box::new(system));
        self
    }

    /// Run all systems once.
    pub fn run_all(&mut self, world: &mut World, resources: &mut ResourceMap) {
        for system in &mut self.systems {
            system.run(world, resources);
        }
    }

    /// Number of registered systems.
    pub fn len(&self) -> usize {
        self.systems.len()
    }

    pub fn is_empty(&self) -> bool {
        self.systems.is_empty()
    }

    /// Remove all systems.
    pub fn clear(&mut self) {
        self.systems.clear();
    }
}

// ---------------------------------------------------------------------------
// Convenience: impl System for closures

/// Wrapper so that `Fn(&mut World, &mut ResourceMap)` closures can be used
/// as systems without creating a named struct.
pub struct FnSystem<F> {
    name: &'static str,
    func: F,
}

impl<F: FnMut(&mut World, &mut ResourceMap) + Send + 'static> FnSystem<F> {
    pub fn new(name: &'static str, func: F) -> Self {
        FnSystem { name, func }
    }
}

impl<F: FnMut(&mut World, &mut ResourceMap) + Send + 'static> System for FnSystem<F> {
    fn name(&self) -> &'static str {
        self.name
    }
    fn run(&mut self, world: &mut World, resources: &mut ResourceMap) {
        (self.func)(world, resources);
    }
}

/// Convenience constructor: `fn_system("name", |world, res| { ... })`.
pub fn fn_system<F>(name: &'static str, f: F) -> FnSystem<F>
where
    F: FnMut(&mut World, &mut ResourceMap) + Send + 'static,
{
    FnSystem::new(name, f)
}

// ---------------------------------------------------------------------------
// Stage-based scheduler

/// Execution stage identifier.
///
/// Stages always run in the order defined by the enum discriminants, regardless
/// of the order in which systems are registered.  Use `StagedScheduler` to
/// register systems into specific stages.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Stage {
    /// Runs before gameplay logic.  Ideal for time/input systems that must
    /// have fresh data before other systems consume it.
    PreUpdate = 0,
    /// Main gameplay logic — movement, AI, physics integration.
    Update = 1,
    /// Fixup pass after Update — hierarchy propagation, constraint solving.
    PostUpdate = 2,
    /// CPU-side render preparation — visibility culling, packet building.
    Render = 3,
}

impl Stage {
    /// All stages in execution order.
    pub const ALL: [Stage; 4] = [
        Stage::PreUpdate,
        Stage::Update,
        Stage::PostUpdate,
        Stage::Render,
    ];
}

/// Stage-aware system scheduler.
///
/// Systems are grouped into [`Stage`]s and always execute in stage order
/// (`PreUpdate` → `Update` → `PostUpdate` → `Render`).  Within a stage,
/// systems run in insertion order.
///
/// # Example
/// ```rust
/// use ferrous_ecs::system::{StagedScheduler, Stage, fn_system};
/// use ferrous_ecs::world::World;
/// use ferrous_ecs::resource::ResourceMap;
///
/// let mut sched = StagedScheduler::new();
/// sched.add(Stage::Update, fn_system("tick", |_w, _r| {}));
/// sched.add(Stage::PreUpdate, fn_system("pre", |_w, _r| {}));
///
/// let mut world = World::new();
/// let mut res   = ResourceMap::new();
/// sched.run_all(&mut world, &mut res); // "pre" runs before "tick"
/// ```
pub struct StagedScheduler {
    pre_update:  Vec<Box<dyn System>>,
    update:      Vec<Box<dyn System>>,
    post_update: Vec<Box<dyn System>>,
    render:      Vec<Box<dyn System>>,
}

impl Default for StagedScheduler {
    fn default() -> Self {
        StagedScheduler::new()
    }
}

impl StagedScheduler {
    pub fn new() -> Self {
        StagedScheduler {
            pre_update:  Vec::new(),
            update:      Vec::new(),
            post_update: Vec::new(),
            render:      Vec::new(),
        }
    }

    /// Register a system in the given stage.
    pub fn add<S: System>(&mut self, stage: Stage, system: S) -> &mut Self {
        let boxed: Box<dyn System> = Box::new(system);
        match stage {
            Stage::PreUpdate  => self.pre_update.push(boxed),
            Stage::Update     => self.update.push(boxed),
            Stage::PostUpdate => self.post_update.push(boxed),
            Stage::Render     => self.render.push(boxed),
        }
        self
    }

    /// Run all stages in order.
    pub fn run_all(&mut self, world: &mut World, resources: &mut ResourceMap) {
        for s in &mut self.pre_update  { s.run(world, resources); }
        for s in &mut self.update      { s.run(world, resources); }
        for s in &mut self.post_update { s.run(world, resources); }
        for s in &mut self.render      { s.run(world, resources); }
    }

    /// Run only the systems belonging to a single stage.
    pub fn run_stage(&mut self, stage: Stage, world: &mut World, resources: &mut ResourceMap) {
        let systems = match stage {
            Stage::PreUpdate  => &mut self.pre_update,
            Stage::Update     => &mut self.update,
            Stage::PostUpdate => &mut self.post_update,
            Stage::Render     => &mut self.render,
        };
        for s in systems { s.run(world, resources); }
    }

    /// Total number of registered systems across all stages.
    pub fn len(&self) -> usize {
        self.pre_update.len()
            + self.update.len()
            + self.post_update.len()
            + self.render.len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Remove all systems from all stages.
    pub fn clear(&mut self) {
        self.pre_update.clear();
        self.update.clear();
        self.post_update.clear();
        self.render.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::component::Component;

    #[derive(Clone)]
    struct Counter(u32);
    impl Component for Counter {}

    struct IncrementSystem;
    impl System for IncrementSystem {
        fn run(&mut self, world: &mut World, _res: &mut ResourceMap) {
            let mut qm = crate::query::QueryMut::<Counter>::new(world);
            qm.for_each_mut(|_, c| c.0 += 1);
        }
    }

    #[test]
    fn scheduler_runs_in_order() {
        let mut world = World::new();
        let mut res = ResourceMap::new();

        world.spawn((Counter(0),));
        world.spawn((Counter(10),));

        let mut sched = SystemScheduler::new();
        sched.add(IncrementSystem);
        sched.run_all(&mut world, &mut res);

        let vals: Vec<u32> = world
            .query::<Counter>()
            .map(|(_, c)| c.0)
            .collect();
        assert!(vals.contains(&1));
        assert!(vals.contains(&11));
    }

    #[test]
    fn fn_system_works() {
        let mut world = World::new();
        let mut res = ResourceMap::new();

        world.spawn((Counter(5),));

        let mut sched = SystemScheduler::new();
        sched.add(fn_system("double", |w, _| {
            let mut qm = crate::query::QueryMut::<Counter>::new(w);
            qm.for_each_mut(|_, c| c.0 *= 2);
        }));
        sched.run_all(&mut world, &mut res);

        let val = world.query::<Counter>().next().unwrap().1 .0;
        assert_eq!(val, 10);
    }
}
