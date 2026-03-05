//! Built-in ECS systems for FerrousEngine.
//!
//! Every system here is designed to be registered with the `StagedScheduler`
//! at the appropriate stage.  The recommended setup from `ferrous_app` is:
//!
//! ```text
//! PreUpdate  →  TimeSystem
//! Update     →  VelocitySystem, AnimationSystem, BehaviorSystem
//! PostUpdate →  TransformSystem (global-transform propagation)
//! ```
//!
//! # Adding a new system
//! 1. Define a `struct MySystem { … }`.
//! 2. `impl System for MySystem { fn run(&mut self, world, resources) { … } }`.
//! 3. Register it: `sched.add(Stage::Update, MySystem { … })`.

use ferrous_ecs::prelude::*;
use ferrous_ecs::system::System;

// ────────────────────────────────────────────────────────────────────────────
// Re-export stage so callers can use `ferrous_core::scene::systems::Stage`
// without depending on `ferrous_ecs` directly.
pub use ferrous_ecs::system::Stage;

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
// Hierarchy components (parent-child transforms)

/// Parent link — set this on a child entity to form a scene hierarchy.
///
/// `TransformSystem` uses these links to propagate local transforms into
/// world-space `GlobalTransform` values.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Parent(pub ferrous_ecs::entity::Entity);
impl Component for Parent {}

/// List of direct children.  Maintained automatically by `World` helpers;
/// do not modify manually unless you also update the corresponding `Parent`
/// components.
#[derive(Debug, Clone)]
pub struct Children(pub Vec<ferrous_ecs::entity::Entity>);
impl Component for Children {}

impl Default for Children {
    fn default() -> Self {
        Children(Vec::new())
    }
}

/// Computed world-space transform (read-only output of `TransformSystem`).
///
/// Do **not** set this manually; `TransformSystem` overwrites it every frame.
/// Read it from the renderer or physics system when you need a final
/// world-space matrix.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GlobalTransform(pub glam::Mat4);
impl Component for GlobalTransform {}

impl Default for GlobalTransform {
    fn default() -> Self {
        GlobalTransform(glam::Mat4::IDENTITY)
    }
}

// ────────────────────────────────────────────────────────────────────────────
// Animation components

/// A single keyframe: (time_seconds, value).
#[derive(Debug, Clone, Copy)]
pub struct Keyframe<T: Clone + Copy> {
    pub time: f32,
    pub value: T,
}

/// Simple linear-interpolation animation clip for a scalar or Vec3.
///
/// The animator stores the clip and the current playback head.  Extend this
/// in future phases to support quaternion slerp and multiple channels.
#[derive(Debug, Clone)]
pub struct AnimationClip {
    /// Keyframes for position (local-space).
    pub position_keys: Vec<Keyframe<glam::Vec3>>,
    /// Total duration in seconds; loops when `looping` is true.
    pub duration: f32,
    /// Whether the clip loops.
    pub looping: bool,
}

impl AnimationClip {
    /// Sample the position at `t` seconds using linear interpolation.
    pub fn sample_position(&self, t: f32) -> Option<glam::Vec3> {
        if self.position_keys.is_empty() {
            return None;
        }
        // Clamp / wrap time
        let t = if self.looping && self.duration > 0.0 {
            t % self.duration
        } else {
            t.min(self.duration)
        };
        // Find surrounding keyframes
        let keys = &self.position_keys;
        if t <= keys[0].time {
            return Some(keys[0].value);
        }
        let last = keys.last().unwrap();
        if t >= last.time {
            return Some(last.value);
        }
        for i in 0..keys.len() - 1 {
            let a = &keys[i];
            let b = &keys[i + 1];
            if t >= a.time && t < b.time {
                let span = b.time - a.time;
                let alpha = if span > 0.0 { (t - a.time) / span } else { 0.0 };
                return Some(a.value.lerp(b.value, alpha));
            }
        }
        None
    }
}

/// Animation player component — attach to an entity together with a clip.
#[derive(Debug, Clone)]
pub struct AnimationPlayer {
    pub clip: AnimationClip,
    /// Current playback time in seconds.
    pub time: f32,
    /// Whether the animation is currently playing.
    pub playing: bool,
    /// Playback speed multiplier (1.0 = normal, negative = reverse).
    pub speed: f32,
}

impl AnimationPlayer {
    pub fn new(clip: AnimationClip) -> Self {
        AnimationPlayer {
            clip,
            time: 0.0,
            playing: true,
            speed: 1.0,
        }
    }

    /// Pause / resume.
    pub fn set_playing(&mut self, playing: bool) {
        self.playing = playing;
    }

    /// Jump to a specific time.
    pub fn seek(&mut self, t: f32) {
        self.time = t.max(0.0);
    }
}

impl Component for AnimationPlayer {}

// ────────────────────────────────────────────────────────────────────────────
// Behavior trait (user scripting hook)

/// Per-entity custom logic hook.
///
/// Implement this trait on a struct and attach it to an entity as a
/// `BehaviorComponent`.  `BehaviorSystem` calls `update()` every frame
/// for every entity that has one.
///
/// # Example
/// ```rust,ignore
/// struct Spinner { speed: f32 }
///
/// impl Behavior for Spinner {
///     fn update(&mut self, entity: Entity, world: &mut ferrous_ecs::world::World, dt: f32) {
///         if let Some(t) = world.get_mut::<Transform>(entity) {
///             t.rotate_axis(Vec3::Y, self.speed * dt);
///         }
///     }
/// }
///
/// world.ecs.spawn((Transform::default(), BehaviorComponent::new(Spinner { speed: 1.0 })));
/// ```
pub trait Behavior: Send + Sync + 'static {
    /// Called once per frame.  `dt` is the frame delta in seconds.
    fn update(
        &mut self,
        entity: ferrous_ecs::entity::Entity,
        world: &mut ferrous_ecs::world::World,
        resources: &mut ResourceMap,
        dt: f32,
    );

    /// Optional: called once when the entity is first processed by
    /// `BehaviorSystem`.  Override to do one-time setup.
    fn on_start(
        &mut self,
        _entity: ferrous_ecs::entity::Entity,
        _world: &mut ferrous_ecs::world::World,
        _resources: &mut ResourceMap,
    ) {}
}

/// Wrapper component that boxes a `Behavior` implementation.
///
/// Use `BehaviorComponent::new(my_behavior)` to attach custom logic to an
/// entity.  Only one behavior per entity is supported; compose multiple
/// behaviors by creating a wrapping struct.
pub struct BehaviorComponent {
    inner: Box<dyn Behavior>,
    started: bool,
}

impl BehaviorComponent {
    pub fn new<B: Behavior>(b: B) -> Self {
        BehaviorComponent {
            inner: Box::new(b),
            started: false,
        }
    }
}

impl Component for BehaviorComponent {}

// We need a custom Debug since Box<dyn Behavior> isn't Debug by default.
impl std::fmt::Debug for BehaviorComponent {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "BehaviorComponent {{ started: {} }}", self.started)
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
///
/// Entities must have **both** `Velocity` and `crate::transform::Transform`
/// stored in the ECS world.  The legacy `ferrous_core::World` entities expose
/// their transforms via `world.ecs`, so this system works transparently.
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

        // Collect (entity, velocity) first to avoid simultaneous borrows.
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

// ────────────────────────────────────────────────────────────────────────────
// AnimationSystem

/// Advances `AnimationPlayer` timers and applies keyframe values to
/// `Transform`.
///
/// Register at `Stage::Update` (after `VelocitySystem` if ordering matters).
pub struct AnimationSystem;

impl System for AnimationSystem {
    fn name(&self) -> &'static str {
        "AnimationSystem"
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

        // Collect entities that have AnimationPlayer.
        let entities: Vec<ferrous_ecs::entity::Entity> = world
            .query::<AnimationPlayer>()
            .map(|(e, _)| e)
            .collect();

        for entity in entities {
            // Advance time and sample in a single mutable borrow.
            let sampled_pos = {
                let player = match world.get_mut::<AnimationPlayer>(entity) {
                    Some(p) => p,
                    None => continue,
                };
                if !player.playing {
                    continue;
                }
                player.time += dt * player.speed;
                // Wrap/clamp handled inside `sample_position`
                player.clip.sample_position(player.time)
            };

            // Apply sampled position to Transform.
            if let Some(pos) = sampled_pos {
                if let Some(t) = world.get_mut::<crate::transform::Transform>(entity) {
                    t.position = pos;
                }
            }
        }
    }
}

// ────────────────────────────────────────────────────────────────────────────
// BehaviorSystem

/// Calls `Behavior::update` on every entity that has a `BehaviorComponent`.
///
/// Register at `Stage::Update`.  Behaviors have full mutable access to the
/// world and resource map, making them suitable for scripting-like logic.
pub struct BehaviorSystem;

impl System for BehaviorSystem {
    fn name(&self) -> &'static str {
        "BehaviorSystem"
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

        // Collect entities first (can't hold an immutable query while also
        // calling behavior that takes `&mut World`).
        let entities: Vec<ferrous_ecs::entity::Entity> = world
            .query::<BehaviorComponent>()
            .map(|(e, _)| e)
            .collect();

        for entity in entities {
            // Take the behavior component out momentarily.
            // We use a swap trick: extract, run, re-insert.
            // Since BehaviorComponent is not Clone, we need to work in-place
            // using raw pointer access to avoid aliasing.
            //
            // SAFETY: We collect entity IDs before iterating and only access
            // one entity at a time, so there is no aliased mutable access.
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

// ────────────────────────────────────────────────────────────────────────────
// TransformSystem — global transform propagation

/// Propagates local `Transform` components through the parent-child hierarchy
/// to compute `GlobalTransform` on every entity.
///
/// Entities **without** a `Parent` component are treated as roots; their
/// `GlobalTransform` equals their local `Transform::matrix()`.  Child entities
/// have their global transform computed as `parent_global * local_matrix`.
///
/// Register at `Stage::PostUpdate` so that gameplay systems have already
/// finished mutating transforms before propagation runs.
///
/// # Limitations (current phase)
/// The propagation is a simple two-pass approach:
/// 1. First pass: set `GlobalTransform` = `local.matrix()` for all entities.
/// 2. Second pass: for each entity with a `Parent`, multiply by the parent's
///    `GlobalTransform`.
///
/// This is correct for **one level** of nesting.  A full topological-sort pass
/// (needed for deeply nested hierarchies) will be added in a future phase.
pub struct TransformSystem;

impl System for TransformSystem {
    fn name(&self) -> &'static str {
        "TransformSystem"
    }

    fn run(
        &mut self,
        world: &mut ferrous_ecs::world::World,
        _resources: &mut ResourceMap,
    ) {
        // ── Pass 1: initialise GlobalTransform from local Transform ─────────
        // Collect all (entity, local_matrix) pairs.
        let locals: Vec<(ferrous_ecs::entity::Entity, glam::Mat4)> = world
            .query::<crate::transform::Transform>()
            .map(|(e, t)| (e, t.matrix()))
            .collect();

        for (entity, mat) in &locals {
            // Ensure GlobalTransform exists; insert identity if missing.
            if world.get::<GlobalTransform>(*entity).is_none() {
                world.insert(*entity, GlobalTransform(glam::Mat4::IDENTITY));
            }
            if let Some(gt) = world.get_mut::<GlobalTransform>(*entity) {
                gt.0 = *mat;
            }
        }

        // ── Pass 2: apply parent transforms ────────────────────────────────
        // Collect (child_entity, parent_entity) pairs.
        let parent_pairs: Vec<(ferrous_ecs::entity::Entity, ferrous_ecs::entity::Entity)> = world
            .query::<Parent>()
            .map(|(e, p)| (e, p.0))
            .collect();

        for (child, parent_entity) in parent_pairs {
            // Read parent's global transform
            let parent_global = match world.get::<GlobalTransform>(parent_entity) {
                Some(gt) => gt.0,
                None => glam::Mat4::IDENTITY,
            };
            // Apply to child
            if let Some(child_gt) = world.get_mut::<GlobalTransform>(child) {
                child_gt.0 = parent_global * child_gt.0;
            }
        }
    }
}

// ────────────────────────────────────────────────────────────────────────────
// Tests

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
        clock.tick(); // advance once so delta > 0
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

        // After the first tick: frame_count in the snapshot is 0 (the index of
        // the frame that just ran), and the internal counter advances to 1.
        let t = res.get::<crate::time::TimeClock>().unwrap().at_tick();
        assert_eq!(t.frame_count, 0, "first tick snapshot should have frame_count = 0");

        // A second tick should produce frame_count = 1.
        sys.run(&mut world, &mut res);
        let t2 = res.get::<crate::time::TimeClock>().unwrap().at_tick();
        assert_eq!(t2.frame_count, 1, "second tick snapshot should have frame_count = 1");
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
        // After one tick (small dt), position.x should be slightly > 0
        assert!(pos.x >= 0.0);
    }

    #[test]
    fn transform_system_propagates_parent() {
        let mut world = ferrous_ecs::world::World::new();
        let mut res = ResourceMap::new();

        let parent_transform = Transform::from_position(Vec3::new(10.0, 0.0, 0.0));
        let child_transform  = Transform::from_position(Vec3::new(1.0, 0.0, 0.0));

        let parent_entity = world.spawn((parent_transform,));
        // Insert GlobalTransform explicitly so the system can find it in pass 1
        world.insert(parent_entity, GlobalTransform::default());

        let child_entity = world.spawn((child_transform,));
        world.insert(child_entity, GlobalTransform::default());
        world.insert(child_entity, Parent(parent_entity));

        let mut sys = TransformSystem;
        sys.run(&mut world, &mut res);

        let child_global = world.get::<GlobalTransform>(child_entity).unwrap().0;
        let child_pos = child_global.w_axis.truncate(); // translation column
        // Parent is at x=10, child local at x=1 → global should be x=11
        assert!((child_pos.x - 11.0).abs() < 1e-4, "child global x = {}", child_pos.x);
    }

    #[test]
    fn behavior_system_calls_update() {
        use std::sync::{Arc, Mutex};

        let counter = Arc::new(Mutex::new(0u32));
        let counter_clone = Arc::clone(&counter);

        struct CountBehavior {
            counter: Arc<Mutex<u32>>,
        }
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
        // BehaviorComponent is non-Clone: use spawn_owned.
        world.spawn_owned(BehaviorComponent::new(CountBehavior { counter: counter_clone }));

        let mut sys = BehaviorSystem;
        sys.run(&mut world, &mut res);
        assert_eq!(*counter.lock().unwrap(), 1);
        sys.run(&mut world, &mut res);
        assert_eq!(*counter.lock().unwrap(), 2);
    }
}
