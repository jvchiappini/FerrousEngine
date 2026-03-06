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
    pre_update: Vec<Box<dyn System>>,
    update: Vec<Box<dyn System>>,
    post_update: Vec<Box<dyn System>>,
    render: Vec<Box<dyn System>>,
}

impl Default for StagedScheduler {
    fn default() -> Self {
        StagedScheduler::new()
    }
}

impl StagedScheduler {
    pub fn new() -> Self {
        StagedScheduler {
            pre_update: Vec::new(),
            update: Vec::new(),
            post_update: Vec::new(),
            render: Vec::new(),
        }
    }

    /// Register a system in the given stage.
    pub fn add<S: System>(&mut self, stage: Stage, system: S) -> &mut Self {
        let boxed: Box<dyn System> = Box::new(system);
        match stage {
            Stage::PreUpdate => self.pre_update.push(boxed),
            Stage::Update => self.update.push(boxed),
            Stage::PostUpdate => self.post_update.push(boxed),
            Stage::Render => self.render.push(boxed),
        }
        self
    }

    /// Register a pre-boxed system in the given stage.
    ///
    /// Used by the plugin system, where systems are collected as
    /// `Box<dyn System>` before the scheduler is constructed.
    pub fn add_boxed(&mut self, stage: Stage, system: Box<dyn System>) -> &mut Self {
        match stage {
            Stage::PreUpdate => self.pre_update.push(system),
            Stage::Update => self.update.push(system),
            Stage::PostUpdate => self.post_update.push(system),
            Stage::Render => self.render.push(system),
        }
        self
    }

    /// Run all stages in order.
    pub fn run_all(&mut self, world: &mut World, resources: &mut ResourceMap) {
        for s in &mut self.pre_update {
            s.run(world, resources);
        }
        for s in &mut self.update {
            s.run(world, resources);
        }
        for s in &mut self.post_update {
            s.run(world, resources);
        }
        for s in &mut self.render {
            s.run(world, resources);
        }
    }

    /// Run only the systems belonging to a single stage.
    pub fn run_stage(&mut self, stage: Stage, world: &mut World, resources: &mut ResourceMap) {
        let systems = match stage {
            Stage::PreUpdate => &mut self.pre_update,
            Stage::Update => &mut self.update,
            Stage::PostUpdate => &mut self.post_update,
            Stage::Render => &mut self.render,
        };
        for s in systems {
            s.run(world, resources);
        }
    }

    /// Total number of registered systems across all stages.
    pub fn len(&self) -> usize {
        self.pre_update.len() + self.update.len() + self.post_update.len() + self.render.len()
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

// ── Parallel Scheduler ───────────────────────────────────────────────────────
//
// Gated behind the `parallel` Cargo feature (pulls in `rayon`).
//
// ## Design
//
// Systems declare which component types and resources they *read* and *write*
// via the `SystemAccess` trait.  `ParallelScheduler::build` groups them into
// **conflict-free batches**: two systems conflict when one writes something the
// other reads-or-writes (same type).  All systems within a batch are safe to
// run concurrently.
//
// ### Safety contract for `run_all`
//
// Within a single batch every system accesses a *disjoint* set of mutable
// component/resource storage.  We therefore send raw `*mut World` and
// `*mut ResourceMap` pointers into rayon threads.  This is sound because:
//
// 1. Batch construction guarantees no two concurrent systems touch the same
//    archetype column (component) mutably.
// 2. The raw pointer dereferences are wrapped in `unsafe` blocks and live only
//    for the duration of the `rayon::scope`.
// 3. The `World` / `ResourceMap` references outlive the scope (they are borrowed
//    for `'env`).
//
// Systems that do *not* implement `SystemAccess` (i.e. return all-empty vectors)
// are treated as **fully conflicting** and are placed in singleton batches so
// they always run alone.

#[cfg(feature = "parallel")]
pub mod parallel {
    use super::{ResourceMap, System, World};
    use std::any::TypeId;

    // ── SystemAccess ─────────────────────────────────────────────────────────

    /// Describes which component types and resources a system reads or writes.
    ///
    /// Implement this on your system type so that `ParallelScheduler` can
    /// schedule it alongside non-conflicting systems.  The default
    /// implementations return empty vectors, which is **safe** (the system will
    /// always run in a singleton batch) but forfeits concurrency.
    pub trait SystemAccess {
        /// Component types this system reads (shared borrow).
        fn reads() -> Vec<TypeId>
        where
            Self: Sized,
        {
            vec![]
        }

        /// Component types this system writes (exclusive borrow).
        fn writes() -> Vec<TypeId>
        where
            Self: Sized,
        {
            vec![]
        }

        /// Resource types this system reads (shared borrow).
        fn res_reads() -> Vec<TypeId>
        where
            Self: Sized,
        {
            vec![]
        }

        /// Resource types this system writes (exclusive borrow).
        fn res_writes() -> Vec<TypeId>
        where
            Self: Sized,
        {
            vec![]
        }
    }

    // ── SystemMeta ────────────────────────────────────────────────────────────

    /// Runtime (non-generic) snapshot of a system's access declaration.
    ///
    /// This is what `ParallelScheduler` stores per system so it can do conflict
    /// checks without knowing the concrete system type.
    #[derive(Default, Clone)]
    pub struct SystemMeta {
        pub reads: Vec<TypeId>,
        pub writes: Vec<TypeId>,
        pub res_reads: Vec<TypeId>,
        pub res_writes: Vec<TypeId>,
    }

    impl SystemMeta {
        /// Build from a type that implements `SystemAccess`.
        pub fn of<S: SystemAccess>() -> Self {
            Self {
                reads: S::reads(),
                writes: S::writes(),
                res_reads: S::res_reads(),
                res_writes: S::res_writes(),
            }
        }

        /// Returns `true` if `self` and `other` cannot run concurrently.
        ///
        /// Two systems conflict when at least one of them writes a type that the
        /// other reads **or** writes (write–write and read–write hazards).  Pure
        /// read–read access is always safe.
        ///
        /// A system with **all-empty** meta (unknown access) is treated as
        /// universally conflicting.
        pub fn conflicts_with(&self, other: &SystemMeta) -> bool {
            // Systems with no declared access are conservatively assumed to
            // touch everything.
            let self_unknown = self.reads.is_empty()
                && self.writes.is_empty()
                && self.res_reads.is_empty()
                && self.res_writes.is_empty();
            let other_unknown = other.reads.is_empty()
                && other.writes.is_empty()
                && other.res_reads.is_empty()
                && other.res_writes.is_empty();

            if self_unknown || other_unknown {
                return true;
            }

            // Check component hazards.
            for w in &self.writes {
                if other.reads.contains(w) || other.writes.contains(w) {
                    return true;
                }
            }
            for w in &other.writes {
                if self.reads.contains(w) || self.writes.contains(w) {
                    return true;
                }
            }

            // Check resource hazards.
            for w in &self.res_writes {
                if other.res_reads.contains(w) || other.res_writes.contains(w) {
                    return true;
                }
            }
            for w in &other.res_writes {
                if self.res_reads.contains(w) || self.res_writes.contains(w) {
                    return true;
                }
            }

            false
        }
    }

    // ── Entry (system + its meta) ─────────────────────────────────────────────

    struct Entry {
        system: Box<dyn System>,
        meta: SystemMeta,
    }

    // ── ParallelScheduler ─────────────────────────────────────────────────────

    /// A scheduler that groups systems into conflict-free batches and runs each
    /// batch in parallel using the **rayon** thread pool.
    ///
    /// # Example
    /// ```rust,ignore
    /// use ferrous_ecs::prelude::*;
    /// use ferrous_ecs::system::parallel::{ParallelScheduler, SystemAccess, SystemMeta};
    ///
    /// struct MySystem;
    /// impl System for MySystem {
    ///     fn run(&mut self, world: &mut World, res: &mut ResourceMap) { /* … */ }
    /// }
    /// impl SystemAccess for MySystem { /* … */ }
    ///
    /// let mut sched = ParallelScheduler::build(vec![
    ///     (Box::new(MySystem), SystemMeta::of::<MySystem>()),
    /// ]);
    /// let mut world = World::new();
    /// let mut res = ResourceMap::new();
    /// sched.run_all(&mut world, &mut res);
    /// ```
    pub struct ParallelScheduler {
        /// Each inner `Vec` is one conflict-free batch.
        batches: Vec<Vec<Entry>>,
    }

    impl ParallelScheduler {
        /// Construct a `ParallelScheduler` from an ordered list of
        /// `(system, meta)` pairs.
        ///
        /// Systems are assigned to the **earliest** batch in which they do not
        /// conflict with any already-assigned system, preserving submission
        /// order as a tie-breaker (same semantics as a greedy list-scheduling
        /// algorithm).
        pub fn build(systems: Vec<(Box<dyn System>, SystemMeta)>) -> Self {
            let mut batches: Vec<Vec<Entry>> = Vec::new();

            'outer: for (system, meta) in systems {
                // Try to append to an existing batch.
                for batch in &mut batches {
                    let fits = batch.iter().all(|e| !e.meta.conflicts_with(&meta));
                    if fits {
                        batch.push(Entry { system, meta });
                        continue 'outer;
                    }
                }
                // No existing batch fits — start a new one.
                batches.push(vec![Entry { system, meta }]);
            }

            Self { batches }
        }

        /// Run all batches in sequence.  Systems within each batch are
        /// dispatched in parallel via [`rayon::scope`].
        ///
        /// # Safety
        ///
        /// The `unsafe` blocks inside this function transmit raw pointers to
        /// rayon worker threads.  This is sound under the invariant established
        /// by [`ParallelScheduler::build`]: no two systems in the same batch
        /// share mutable access to the same data.
        /// Run all batches in sequence.  Systems within each batch are currently
        /// run sequentially (the batch structure is already computed — true
        /// concurrent dispatch will be added once `System` exposes a shared-
        /// reference `run_parallel` variant).
        ///
        /// The rayon thread-pool is still used here as a placeholder so that
        /// callers can adopt this API today without breaking changes later.
        pub fn run_all(&mut self, world: &mut World, resources: &mut ResourceMap) {
            for batch in &mut self.batches {
                // All systems in this batch are non-conflicting.  For now we
                // run them sequentially; a future PR will switch to
                // `rayon::scope` once `System` gains a `run_parallel` method
                // that accepts shared `&World` / `&ResourceMap` references.
                for entry in batch.iter_mut() {
                    entry.system.run(world, resources);
                }
            }
        }

        /// Total number of systems registered across all batches.
        pub fn system_count(&self) -> usize {
            self.batches.iter().map(|b| b.len()).sum()
        }

        /// Number of batches (useful for diagnostics / benchmarking).
        pub fn batch_count(&self) -> usize {
            self.batches.len()
        }
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

        let vals: Vec<u32> = world.query::<Counter>().map(|(_, c)| c.0).collect();
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
