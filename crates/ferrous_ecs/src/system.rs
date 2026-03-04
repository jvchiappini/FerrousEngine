//! System trait and linear scheduler.
//!
//! A `System` is a unit of logic that operates on a `World` (and optionally
//! its `ResourceMap`).  The `SystemScheduler` runs systems in insertion order,
//! which is sufficient for a single-threaded update loop.  A parallel
//! scheduler (using `rayon`) will be added in a later phase.

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
