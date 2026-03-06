//! `SystemParam` — types that can be injected into plain-function systems.
//!
//! Any type implementing [`SystemParam`] can appear as a function argument
//! when that function is registered via [`IntoSystem`].  The scheduler calls
//! [`SystemParam::init`] once at registration time and [`SystemParam::fetch`]
//! every tick.
//!
//! # Built-in implementations
//!
//! | Marker type | Param in fn | Borrows |
//! |-------------|-------------|---------|
//! | `QueryParam<Q>` | `Query<'_, Q>` | `&World` |
//! | `ResParam<T>` | `Res<'_, T>` | `&ResourceMap` |
//! | `ResMutParam<T>` | `ResMut<'_, T>` | `&mut ResourceMap` |
//!
//! ## Why marker types?
//!
//! `SystemParam::fetch` uses a GAT (`Item<'w>`) as the return type so that
//! `fetch` can return `Res<'w, T>` (a type that *carries* a lifetime) without
//! encoding that lifetime in `Self`.  The marker types (`ResParam<T>`,
//! `ResMutParam<T>`, `QueryParam<Q>`) are the `Self` of the impl; the actual
//! values handed to the function are `Self::Item<'w>`.

use std::any::TypeId;
use std::marker::PhantomData;

use crate::query::{Query, WorldQuery};
use crate::resource::ResourceMap;
use crate::world::World;

// ---------------------------------------------------------------------------
// SystemParam trait

/// A type that acts as a "descriptor" for a function-system parameter.
///
/// The *value* handed to the function each tick is `Self::Item<'w>` (a GAT),
/// not `Self` itself.  `Self` is a zero-sized marker that carries type info.
///
/// # Safety
/// Implementors must ensure:
/// - `fetch` only produces references valid for `'w`.
/// - Mutable borrows must not alias any other live borrow for the same data.
pub unsafe trait SystemParam {
    /// Per-system cached state built once at registration.
    type State: Send + Sync + 'static;

    /// The concrete value type produced for lifetime `'w`.
    type Item<'w>;

    /// Build `State` from the world (called once at system registration).
    fn init(world: &World, resources: &ResourceMap) -> Self::State;

    /// Fetch the concrete param value for this tick.
    ///
    /// # Safety
    /// `state` was produced by [`Self::init`] for the same world layout.
    /// The caller guarantees no conflicting borrows exist simultaneously.
    unsafe fn fetch<'w>(
        state: &'w mut Self::State,
        world: &'w World,
        resources: &'w ResourceMap,
    ) -> Self::Item<'w>;

    /// Component types this param reads.
    fn reads() -> Vec<TypeId> { vec![] }
    /// Component types this param writes.
    fn writes() -> Vec<TypeId> { vec![] }
    /// Resource types this param reads.
    fn res_reads() -> Vec<TypeId> { vec![] }
    /// Resource types this param writes.
    fn res_writes() -> Vec<TypeId> { vec![] }
}

// ---------------------------------------------------------------------------
// () — zero-param systems

unsafe impl SystemParam for () {
    type State = ();
    type Item<'w> = ();

    fn init(_: &World, _: &ResourceMap) {}

    #[inline]
    unsafe fn fetch<'w>(_: &'w mut (), _: &'w World, _: &'w ResourceMap) {}
}

// ---------------------------------------------------------------------------
// QueryParam<Q> — wraps Query<'_, Q>

/// Zero-sized marker that identifies a `Query<'_, Q>` parameter.
pub struct QueryParam<Q>(PhantomData<Q>);

unsafe impl<Q> SystemParam for QueryParam<Q>
where
    Q: WorldQuery<State = Vec<usize>> + 'static,
{
    type State = Vec<usize>;
    type Item<'w> = Query<'w, Q>;

    fn init(world: &World, _resources: &ResourceMap) -> Vec<usize> {
        Q::init(world)
    }

    #[inline]
    unsafe fn fetch<'w>(
        state: &'w mut Vec<usize>,
        world: &'w World,
        _resources: &'w ResourceMap,
    ) -> Query<'w, Q> {
        Query::from_state(world, state.clone())
    }

    fn reads() -> Vec<TypeId> { Q::reads() }
    fn writes() -> Vec<TypeId> { Q::writes() }
}

// ---------------------------------------------------------------------------
// Res<'w, T> value type + ResParam<T> marker

/// Shared (immutable) borrow of resource `T`.
///
/// ```rust,ignore
/// fn print_time(time: Res<GameTime>) {
///     println!("t = {}", time.0);
/// }
/// ```
pub struct Res<'w, T: Send + Sync + 'static> {
    value: &'w T,
}

impl<'w, T: Send + Sync + 'static> std::ops::Deref for Res<'w, T> {
    type Target = T;
    #[inline]
    fn deref(&self) -> &T { self.value }
}

impl<'w, T: Send + Sync + 'static + std::fmt::Debug> std::fmt::Debug for Res<'w, T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.value.fmt(f)
    }
}

/// Zero-sized marker that identifies a `Res<'_, T>` parameter.
pub struct ResParam<T>(PhantomData<T>);

unsafe impl<T: Send + Sync + 'static> SystemParam for ResParam<T> {
    type State = ();
    type Item<'w> = Res<'w, T>;

    fn init(_: &World, _: &ResourceMap) {}

    #[inline]
    unsafe fn fetch<'w>(
        _state: &'w mut (),
        _world: &'w World,
        resources: &'w ResourceMap,
    ) -> Res<'w, T> {
        Res {
            value: resources
                .get::<T>()
                .expect("Res<T>: resource not found in ResourceMap"),
        }
    }

    fn res_reads() -> Vec<TypeId> { vec![TypeId::of::<T>()] }
}

// ---------------------------------------------------------------------------
// ResMut<'w, T> value type + ResMutParam<T> marker

/// Exclusive (mutable) borrow of resource `T`.
///
/// ```rust,ignore
/// fn tick_time(mut time: ResMut<GameTime>) {
///     time.0 += 1.0 / 60.0;
/// }
/// ```
pub struct ResMut<'w, T: Send + Sync + 'static> {
    value: &'w mut T,
}

impl<'w, T: Send + Sync + 'static> std::ops::Deref for ResMut<'w, T> {
    type Target = T;
    #[inline]
    fn deref(&self) -> &T { self.value }
}

impl<'w, T: Send + Sync + 'static> std::ops::DerefMut for ResMut<'w, T> {
    #[inline]
    fn deref_mut(&mut self) -> &mut T { self.value }
}

impl<'w, T: Send + Sync + 'static + std::fmt::Debug> std::fmt::Debug for ResMut<'w, T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.value.fmt(f)
    }
}

/// Zero-sized marker that identifies a `ResMut<'_, T>` parameter.
pub struct ResMutParam<T>(PhantomData<T>);

unsafe impl<T: Send + Sync + 'static> SystemParam for ResMutParam<T> {
    type State = ();
    type Item<'w> = ResMut<'w, T>;

    fn init(_: &World, _: &ResourceMap) {}

    #[inline]
    unsafe fn fetch<'w>(
        _state: &'w mut (),
        _world: &'w World,
        resources: &'w ResourceMap,
    ) -> ResMut<'w, T> {
        // SAFETY: caller guarantees exclusive access for the duration of `run`.
        let ptr = resources
            .get_mut_ptr::<T>()
            .expect("ResMut<T>: resource not found in ResourceMap");
        ResMut { value: &mut *ptr }
    }

    fn res_writes() -> Vec<TypeId> { vec![TypeId::of::<T>()] }
}

// ---------------------------------------------------------------------------
// Tuple impls for (P0,), (P0, P1), …  — combine multiple params

macro_rules! impl_system_param_tuple {
    ( $( $name:ident ),+ ) => {
        unsafe impl< $($name: SystemParam),+ > SystemParam for ( $($name,)+ ) {
            type State = ( $($name::State,)+ );
            type Item<'w> = ( $($name::Item<'w>,)+ );

            #[allow(unused_variables, clippy::unused_unit)]
            fn init(world: &World, resources: &ResourceMap) -> Self::State {
                ( $( $name::init(world, resources), )+ )
            }

            #[inline]
            #[allow(non_snake_case)]
            unsafe fn fetch<'w>(
                state: &'w mut Self::State,
                world: &'w World,
                resources: &'w ResourceMap,
            ) -> Self::Item<'w> {
                let ( $($name,)+ ) = state;
                ( $( $name::fetch($name, world, resources), )+ )
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

            fn res_reads() -> Vec<TypeId> {
                let mut v = Vec::new();
                $( v.extend($name::res_reads()); )+
                v
            }

            fn res_writes() -> Vec<TypeId> {
                let mut v = Vec::new();
                $( v.extend($name::res_writes()); )+
                v
            }
        }
    };
}

impl_system_param_tuple!(P0);
impl_system_param_tuple!(P0, P1);
impl_system_param_tuple!(P0, P1, P2);
impl_system_param_tuple!(P0, P1, P2, P3);
impl_system_param_tuple!(P0, P1, P2, P3, P4);
impl_system_param_tuple!(P0, P1, P2, P3, P4, P5);
impl_system_param_tuple!(P0, P1, P2, P3, P4, P5, P6);
impl_system_param_tuple!(P0, P1, P2, P3, P4, P5, P6, P7);

// ---------------------------------------------------------------------------
// Tests

#[cfg(test)]
mod tests {
    use super::*;
    use crate::world::World;

    #[derive(Clone, Debug)]
    struct GameTime(f64);

    #[test]
    fn res_fetch_deref() {
        let mut resources = ResourceMap::new();
        resources.insert(GameTime(1.5));
        let world = World::new();
        let mut state = ResParam::<GameTime>::init(&world, &resources);
        let r = unsafe { ResParam::<GameTime>::fetch(&mut state, &world, &resources) };
        assert_eq!(r.0, 1.5);
    }

    #[test]
    fn res_mut_fetch_mutates() {
        let mut resources = ResourceMap::new();
        resources.insert(GameTime(0.0));
        let world = World::new();
        let mut state = ResMutParam::<GameTime>::init(&world, &resources);
        {
            let mut r = unsafe { ResMutParam::<GameTime>::fetch(&mut state, &world, &resources) };
            r.0 = 42.0;
        }
        assert_eq!(resources.get::<GameTime>().unwrap().0, 42.0);
    }

    #[test]
    fn access_metadata() {
        assert!(ResParam::<GameTime>::res_reads().contains(&TypeId::of::<GameTime>()));
        assert!(ResMutParam::<GameTime>::res_writes().contains(&TypeId::of::<GameTime>()));
    }
}
