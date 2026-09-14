//! `WorldQuery` trait and typed query iterators.
//!
//! # Design
//!
//! [`WorldQuery`] is the fundamental abstraction: a type that knows how to
//! match archetypes and fetch component references from them.  Primitive impls
//! are provided for `&T`, `&mut T`, and `Option<&T>`.  Tuple impls (up to 8
//! elements) let you query multiple components at once:
//!
//! ```rust
//! use ferrous_ecs::prelude::*;
//!
//! #[derive(Clone, Debug)] struct Pos(f32);
//! impl Component for Pos {}
//! #[derive(Clone, Debug)] struct Vel(f32);
//! impl Component for Vel {}
//!
//! let mut world = World::new();
//! world.spawn((Pos(0.0), Vel(1.0)));
//!
//! // Immutable multi-component query
//! for (_entity, (pos, vel)) in Query::<(&Pos, &Vel)>::new(&world).iter() {
//!     let _ = (pos, vel);
//! }
//! ```
//!
//! # Safety
//!
//! `WorldQuery` implementations use raw pointer casts to produce component
//! references with the correct lifetimes.  The safety invariants are
//! maintained by:
//! - Only ever producing references valid for the lifetime `'w` of the world
//!   borrow held by [`Query`].
//! - The `&mut T` impl requires that no two tuple elements fetch the same
//!   `TypeId` mutably — enforced by convention (checked in debug builds).
//! - Zero-size types are handled: `size == 0` paths skip pointer arithmetic.

use std::any::TypeId;

use crate::archetype::{Archetype, ComponentColumn};
use crate::component::Component;
use crate::entity::Entity;
use crate::world::World;

// ---------------------------------------------------------------------------
// WorldQuery trait

/// A type that can be used as a multi-component query parameter.
///
/// Implementing this trait lets you use the type as the generic parameter
/// of [`Query`]:
///
/// ```rust,ignore
/// Query::<(&Transform, &mut Velocity)>::new(&world)
/// ```
///
/// # Safety
/// Implementors must uphold:
/// - `fetch` only produces references with lifetime bounded by `'w`.
/// - Mutable fetches (`&mut T`) must not alias any other fetch in the same
///   query tuple for the same component type.
pub unsafe trait WorldQuery: Sized {
    /// The item produced per archetype row.
    type Item<'w>;

    /// Cached per-query state: archetype indices that match this query.
    type State;

    /// Per-archetype fetch state.
    ///
    /// Holds the column locations resolved for one specific archetype so that
    /// [`WorldQuery::fetch`] does not have to re-discover them for every row.
    /// Built once by [`WorldQuery::init_fetch`] before iterating an archetype.
    type Fetch<'w>;

    /// Build state by scanning `world`'s archetypes.
    fn init(world: &World) -> Self::State;

    /// Returns `true` if `arch` contains all required components.
    fn matches(arch: &Archetype) -> bool;

    /// Resolve the column locations for `arch` once.
    ///
    /// Called once per archetype when an iterator is built, *not* per row, so
    /// the `binary_search` over the archetype signature happens O(archetypes)
    /// times instead of O(rows).
    ///
    /// # Safety
    /// `matches(arch)` must have returned `true` for this archetype (optional
    /// fetches may still resolve to an absent column).
    unsafe fn init_fetch<'w>(arch: &'w Archetype) -> Self::Fetch<'w>;

    /// Fetch the item at `row` using pre-resolved [`WorldQuery::Fetch`] state.
    ///
    /// # Safety
    /// `row < arch.len()`, and `fetch` was produced by
    /// [`WorldQuery::init_fetch`] from the same archetype this row belongs to.
    unsafe fn fetch<'w>(fetch: &Self::Fetch<'w>, row: usize) -> Self::Item<'w>;

    /// Component types this query reads (for scheduler conflict detection).
    fn reads() -> Vec<TypeId> {
        vec![]
    }

    /// Component types this query writes (for scheduler conflict detection).
    fn writes() -> Vec<TypeId> {
        vec![]
    }
}

// ---------------------------------------------------------------------------
// Primitive impl: &T  (immutable reference)

unsafe impl<T: Component> WorldQuery for &T {
    type Item<'w> = &'w T;
    type State = Vec<usize>;
    type Fetch<'w> = &'w ComponentColumn;

    fn init(world: &World) -> Self::State {
        world
            .archetypes
            .archetypes
            .iter()
            .enumerate()
            .filter(|(_, a)| Self::matches(a))
            .map(|(i, _)| i)
            .collect()
    }

    #[inline]
    fn matches(arch: &Archetype) -> bool {
        arch.column::<T>().is_some()
    }

    #[inline]
    unsafe fn init_fetch<'w>(arch: &'w Archetype) -> Self::Fetch<'w> {
        // SAFETY: the caller guarantees `matches(arch)` was true.
        arch.column::<T>().unwrap_unchecked()
    }

    #[inline]
    unsafe fn fetch<'w>(fetch: &Self::Fetch<'w>, row: usize) -> Self::Item<'w> {
        // SAFETY: `fetch` points at a column that stores `T`; row is in bounds.
        fetch.get::<T>(row)
    }

    fn reads() -> Vec<TypeId> {
        vec![TypeId::of::<T>()]
    }
}

// ---------------------------------------------------------------------------
// Primitive impl: &mut T  (mutable reference)

unsafe impl<T: Component> WorldQuery for &mut T {
    type Item<'w> = &'w mut T;
    type State = Vec<usize>;
    type Fetch<'w> = *mut ComponentColumn;

    fn init(world: &World) -> Self::State {
        world
            .archetypes
            .archetypes
            .iter()
            .enumerate()
            .filter(|(_, a)| Self::matches(a))
            .map(|(i, _)| i)
            .collect()
    }

    #[inline]
    fn matches(arch: &Archetype) -> bool {
        arch.column::<T>().is_some()
    }

    #[inline]
    unsafe fn init_fetch<'w>(arch: &'w Archetype) -> Self::Fetch<'w> {
        // SAFETY: the caller guarantees `matches(arch)` was true.  The pointer
        // is only ever dereferenced mutably through `fetch`, and the query
        // contract forbids two aliasing mutable fetches of the same `T`.
        arch.column::<T>().unwrap_unchecked() as *const ComponentColumn as *mut ComponentColumn
    }

    #[inline]
    unsafe fn fetch<'w>(fetch: &Self::Fetch<'w>, row: usize) -> Self::Item<'w> {
        // SAFETY: `fetch` points at a column that stores `T`; row is in bounds,
        // and the caller upholds the no-aliasing rule for `&mut T`.
        let col = *fetch;
        (*col).get_mut::<T>(row)
    }

    fn writes() -> Vec<TypeId> {
        vec![TypeId::of::<T>()]
    }
}

// ---------------------------------------------------------------------------
// Primitive impl: Option<&T>  (optional immutable reference)

unsafe impl<T: Component> WorldQuery for Option<&T> {
    type Item<'w> = Option<&'w T>;
    type State = Vec<usize>;
    type Fetch<'w> = Option<&'w ComponentColumn>;

    fn init(world: &World) -> Self::State {
        // Optional components match *all* archetypes (they return None when absent).
        (0..world.archetypes.archetypes.len()).collect()
    }

    #[inline]
    fn matches(_arch: &Archetype) -> bool {
        true
    }

    #[inline]
    unsafe fn init_fetch<'w>(arch: &'w Archetype) -> Self::Fetch<'w> {
        arch.column::<T>()
    }

    #[inline]
    unsafe fn fetch<'w>(fetch: &Self::Fetch<'w>, row: usize) -> Self::Item<'w> {
        // SAFETY: when present, `fetch` points at a column that stores `T`.
        fetch.map(|col| col.get::<T>(row))
    }

    fn reads() -> Vec<TypeId> {
        vec![TypeId::of::<T>()]
    }
}

// ---------------------------------------------------------------------------
// Tuple impls — macro-generated for 2..=8 elements

macro_rules! impl_world_query_tuple {
    ( $( $name:ident ),+ ) => {
        unsafe impl< $($name: WorldQuery),+ > WorldQuery for ( $($name,)+ ) {
            type Item<'w> = ( $($name::Item<'w>,)+ );
            type State = Vec<usize>;
            type Fetch<'w> = ( $($name::Fetch<'w>,)+ );

            fn init(world: &World) -> Self::State {
                world
                    .archetypes
                    .archetypes
                    .iter()
                    .enumerate()
                    .filter(|(_, a)| Self::matches(a))
                    .map(|(i, _)| i)
                    .collect()
            }

            #[inline]
            fn matches(arch: &Archetype) -> bool {
                $( $name::matches(arch) )&&+
            }

            #[inline]
            unsafe fn init_fetch<'w>(arch: &'w Archetype) -> Self::Fetch<'w> {
                ( $( $name::init_fetch(arch), )+ )
            }

            #[inline]
            #[allow(non_snake_case)]
            unsafe fn fetch<'w>(fetch: &Self::Fetch<'w>, row: usize) -> Self::Item<'w> {
                // Bind each per-archetype fetch state to a local named after the
                // element type; types and values live in separate namespaces.
                let ( $( ref $name, )+ ) = *fetch;
                ( $( $name::fetch($name, row), )+ )
            }

            fn reads() -> Vec<TypeId> {
                let mut v = Vec::new();
                $( v.extend($name::reads()); )+
                v
            }

            fn writes() -> Vec<TypeId> {
                let mut v = Vec::new();
                $( v.extend($name::writes()); )+
                v
            }
        }
    };
}

impl_world_query_tuple!(Q0, Q1);
impl_world_query_tuple!(Q0, Q1, Q2);
impl_world_query_tuple!(Q0, Q1, Q2, Q3);
impl_world_query_tuple!(Q0, Q1, Q2, Q3, Q4);
impl_world_query_tuple!(Q0, Q1, Q2, Q3, Q4, Q5);
impl_world_query_tuple!(Q0, Q1, Q2, Q3, Q4, Q5, Q6);
impl_world_query_tuple!(Q0, Q1, Q2, Q3, Q4, Q5, Q6, Q7);

// ---------------------------------------------------------------------------
// Query<'w, Q>

/// A cached, typed, multi-component query over a [`World`].
///
/// Build once with [`Query::new`], then iterate repeatedly with
/// [`Query::iter`].  The cached archetype list becomes stale if the world
/// structure changes (entity spawned/despawned); rebuild in that case.
///
/// # Example
/// ```rust
/// use ferrous_ecs::prelude::*;
///
/// #[derive(Clone, Debug)] struct Pos(f32, f32);
/// impl Component for Pos {}
/// #[derive(Clone, Debug)] struct Vel(f32, f32);
/// impl Component for Vel {}
///
/// let mut world = World::new();
/// world.spawn((Pos(0.0, 0.0), Vel(1.0, 0.0)));
///
/// let q = Query::<(&Pos, &Vel)>::new(&world);
/// for (_entity, (pos, vel)) in q.iter() {
///     let _ = (pos, vel);
/// }
/// ```
pub struct Query<'w, Q: WorldQuery<State = Vec<usize>>> {
    world: &'w World,
    /// Cached archetype indices matching `Q`.
    state: Vec<usize>,
    _marker: std::marker::PhantomData<Q>,
}

impl<'w, Q: WorldQuery<State = Vec<usize>>> Query<'w, Q> {
    /// Build the query — scans archetypes once and caches matching indices.
    pub fn new(world: &'w World) -> Self {
        let state = Q::init(world);
        Query {
            world,
            state,
            _marker: std::marker::PhantomData,
        }
    }

    /// Reconstruct a `Query` from a pre-built state (used by `SystemParam`).
    ///
    /// The caller is responsible for ensuring `state` was built from a world
    /// with the same archetype layout as `world`.
    #[inline]
    pub(crate) fn from_state(world: &'w World, state: Vec<usize>) -> Self {
        Query {
            world,
            state,
            _marker: std::marker::PhantomData,
        }
    }

    /// Iterate over `(Entity, Q::Item<'_>)` pairs.
    ///
    /// Allocation-free: walks the cached archetype list and yields references
    /// directly from the SoA columns.
    pub fn iter(&self) -> impl Iterator<Item = (Entity, Q::Item<'_>)> + '_ {
        self.state.iter().flat_map(move |&arch_id| {
            let arch = &self.world.archetypes.archetypes[arch_id];
            let count = arch.entities.len();
            // Resolve the column locations once for this archetype instead of
            // re-running `binary_search` for every row (H6).
            // SAFETY: arch matched Q::matches at init time.
            let fetch = unsafe { Q::init_fetch(arch) };
            (0..count).map(move |row| {
                let entity = arch.entities[row];
                // SAFETY: row < count; `fetch` was built from this archetype.
                let item = unsafe { Q::fetch(&fetch, row) };
                (entity, item)
            })
        })
    }

    /// Total number of entities matching this query.
    pub fn len(&self) -> usize {
        self.state
            .iter()
            .map(|&id| self.world.archetypes.archetypes[id].len())
            .sum()
    }

    /// Returns `true` if no entities match.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Component types read by this query (for `ParallelScheduler`).
    pub fn reads() -> Vec<TypeId> {
        Q::reads()
    }

    /// Component types written by this query (for `ParallelScheduler`).
    pub fn writes() -> Vec<TypeId> {
        Q::writes()
    }
}

// ---------------------------------------------------------------------------
// Backward-compatibility shim for QueryMut
//
// Old code: `QueryMut::<C>::new(&mut world).for_each_mut(|_, c| ...)`
// New code: `Query::<&mut C>::new(&mut world).iter()` — but we keep QueryMut
// as a thin wrapper so existing call sites in system.rs compile without changes.

/// Single-component mutable query — kept for backward compatibility.
///
/// New code should use `Query::<&mut C>::new(world).iter()` instead.
pub struct QueryMut<'w, C: Component> {
    inner: Query<'w, &'w mut C>,
}

impl<'w, C: Component> QueryMut<'w, C> {
    /// Build a mutable query over `world`.
    pub fn new(world: &'w mut World) -> Self {
        QueryMut {
            inner: Query::<&'w mut C>::new(world),
        }
    }

    /// Call `f` for every `(Entity, &mut C)` pair.
    pub fn for_each_mut<F: FnMut(Entity, &mut C)>(&mut self, mut f: F) {
        for (entity, comp) in self.inner.iter() {
            f(entity, comp);
        }
    }
}

// ---------------------------------------------------------------------------
// Tests

#[cfg(test)]
mod tests {
    use super::*;
    use crate::component::Component;
    use crate::world::World;

    #[derive(Clone, Debug, PartialEq)]
    struct Pos(f32);
    impl Component for Pos {}

    #[derive(Clone, Debug, PartialEq)]
    struct Vel(f32);
    impl Component for Vel {}

    #[derive(Clone, Debug, PartialEq)]
    struct Health(f32);
    impl Component for Health {}

    // -----------------------------------------------------------------------
    // Single-component &T query

    #[test]
    fn single_ref_query() {
        let mut world = World::new();
        world.spawn((Pos(1.0),));
        world.spawn((Pos(2.0), Vel(1.0)));
        world.spawn((Vel(3.0),));

        let q = Query::<&Pos>::new(&world);
        assert_eq!(q.len(), 2);

        let mut vals: Vec<f32> = q.iter().map(|(_, p)| p.0).collect();
        vals.sort_by(|a, b| a.partial_cmp(b).unwrap());
        assert_eq!(vals, vec![1.0, 2.0]);
    }

    // -----------------------------------------------------------------------
    // Two-component tuple query

    #[test]
    fn tuple2_query() {
        let mut world = World::new();
        world.spawn((Pos(0.0), Vel(1.0)));
        world.spawn((Pos(5.0), Vel(2.0)));
        world.spawn((Pos(9.0),)); // no Vel — must be excluded

        let q = Query::<(&Pos, &Vel)>::new(&world);
        assert_eq!(q.len(), 2);

        let mut pairs: Vec<(f32, f32)> = q.iter().map(|(_, (p, v))| (p.0, v.0)).collect();
        pairs.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap());
        assert_eq!(pairs, vec![(0.0, 1.0), (5.0, 2.0)]);
    }

    // -----------------------------------------------------------------------
    // Three-component tuple query

    #[test]
    fn tuple3_query() {
        let mut world = World::new();
        world.spawn((Pos(1.0), Vel(2.0), Health(100.0)));
        world.spawn((Pos(3.0), Vel(4.0))); // no Health
        world.spawn((Pos(5.0), Health(50.0))); // no Vel

        let q = Query::<(&Pos, &Vel, &Health)>::new(&world);
        assert_eq!(q.len(), 1);

        let (_, (pos, vel, hp)) = q.iter().next().unwrap();
        assert_eq!(pos.0, 1.0);
        assert_eq!(vel.0, 2.0);
        assert_eq!(hp.0, 100.0);
    }

    // -----------------------------------------------------------------------
    // Mutable query via &mut T

    #[test]
    fn mut_query_updates() {
        let mut world = World::new();
        world.spawn((Pos(0.0),));
        world.spawn((Pos(5.0),));

        {
            let q = Query::<&mut Pos>::new(&mut world);
            for (_, pos) in q.iter() {
                pos.0 += 1.0;
            }
        }

        let q = Query::<&Pos>::new(&world);
        let mut vals: Vec<f32> = q.iter().map(|(_, p)| p.0).collect();
        vals.sort_by(|a, b| a.partial_cmp(b).unwrap());
        assert_eq!(vals, vec![1.0, 6.0]);
    }

    // -----------------------------------------------------------------------
    // Mixed mutable tuple: (&mut Pos, &Vel)

    #[test]
    fn mut_tuple_query() {
        let mut world = World::new();
        world.spawn((Pos(0.0), Vel(2.0)));
        world.spawn((Pos(10.0), Vel(3.0)));

        {
            let q = Query::<(&mut Pos, &Vel)>::new(&mut world);
            for (_, (pos, vel)) in q.iter() {
                pos.0 += vel.0;
            }
        }

        let q = Query::<&Pos>::new(&world);
        let mut vals: Vec<f32> = q.iter().map(|(_, p)| p.0).collect();
        vals.sort_by(|a, b| a.partial_cmp(b).unwrap());
        assert_eq!(vals, vec![2.0, 13.0]);
    }

    // -----------------------------------------------------------------------
    // Optional component

    #[test]
    fn optional_component() {
        let mut world = World::new();
        world.spawn((Pos(1.0), Vel(10.0)));
        world.spawn((Pos(2.0),)); // no Vel

        let q = Query::<(&Pos, Option<&Vel>)>::new(&world);
        assert_eq!(q.len(), 2);

        let mut results: Vec<(f32, Option<f32>)> =
            q.iter().map(|(_, (p, v))| (p.0, v.map(|v| v.0))).collect();
        results.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap());
        assert_eq!(results, vec![(1.0, Some(10.0)), (2.0, None)]);
    }

    // -----------------------------------------------------------------------
    // Backward-compat: QueryMut wrapper

    #[test]
    fn query_mut_compat() {
        let mut world = World::new();
        world.spawn((Pos(0.0),));
        world.spawn((Pos(5.0),));

        let mut qm = QueryMut::<Pos>::new(&mut world);
        qm.for_each_mut(|_, p| p.0 += 1.0);

        let q = Query::<&Pos>::new(&world);
        let mut vals: Vec<f32> = q.iter().map(|(_, p)| p.0).collect();
        vals.sort_by(|a, b| a.partial_cmp(b).unwrap());
        assert_eq!(vals, vec![1.0, 6.0]);
    }

    // -----------------------------------------------------------------------
    // H6: column resolution is per-archetype, not per-row

    #[test]
    fn iter_resolves_columns_once_per_archetype() {
        use crate::archetype::{column_index_lookups, reset_column_index_lookups};

        let mut world = World::new();
        for i in 0..64 {
            world.spawn((Pos(i as f32), Vel(1.0)));
        }

        let q = Query::<(&Pos, &Vel)>::new(&world);

        // Count only the lookups performed while iterating rows.
        reset_column_index_lookups();
        let mut seen: Vec<f32> = Vec::new();
        for (_, (p, v)) in q.iter() {
            seen.push(p.0 + v.0);
        }

        assert_eq!(seen.len(), 64);
        let lookups = column_index_lookups();
        // Two components × one archetype = 2 lookups.  The old per-row path
        // performed 2 × 64 = 128.  Allow a small constant margin.
        assert!(
            lookups <= 8,
            "expected per-archetype resolution (<= 8 lookups), got {lookups}"
        );
    }

    #[test]
    fn iter_matches_len_across_archetypes() {
        use crate::archetype::{column_index_lookups, reset_column_index_lookups};

        let mut world = World::new();
        world.spawn((Pos(0.0), Vel(1.0)));
        world.spawn((Pos(2.0),));
        world.spawn((Pos(3.0), Vel(4.0), Health(9.0)));

        let q = Query::<(&Pos, Option<&Vel>)>::new(&world);
        assert_eq!(q.len(), 3);

        reset_column_index_lookups();
        let mut rows: Vec<(f32, Option<f32>)> =
            q.iter().map(|(_, (p, v))| (p.0, v.map(|v| v.0))).collect();
        rows.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap());

        assert_eq!(rows, vec![(0.0, Some(1.0)), (2.0, None), (3.0, Some(4.0))]);
        // Three archetypes × two fetches = 6 lookups (plus margin), never one
        // per row.
        let lookups = column_index_lookups();
        assert!(
            lookups <= 16,
            "expected per-archetype resolution (<= 16 lookups), got {lookups}"
        );
    }

    // -----------------------------------------------------------------------
    // Access metadata

    #[test]
    fn access_metadata() {
        assert!(Query::<&Pos>::reads().contains(&TypeId::of::<Pos>()));
        assert!(Query::<&mut Pos>::writes().contains(&TypeId::of::<Pos>()));
        assert!(Query::<(&Pos, &mut Vel)>::reads().contains(&TypeId::of::<Pos>()));
        assert!(Query::<(&Pos, &mut Vel)>::writes().contains(&TypeId::of::<Vel>()));
    }
}
