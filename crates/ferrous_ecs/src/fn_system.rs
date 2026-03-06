//! `IntoSystem` trait and `FnSystem` adapter.
//!
//! # Design
//!
//! The key challenge is that `SystemParam::fetch` returns `Self::Item<'w>` —
//! a type with a lifetime.  We cannot directly write `F: FnMut(P::Item<'_>)`
//! in a stable-Rust `where` clause inside a macro without triggering borrow
//! checker limitations with GATs.
//!
//! Solution: store the function as `F`, fetch params inside `run` using a
//! local lifetime, then call the function.  The `IntoSystem` bound on `F`
//! is expressed as a helper trait `SystemFn<Params>` which avoids the
//! problematic GAT-in-where-clause pattern.

use std::marker::PhantomData;

use crate::resource::ResourceMap;
use crate::system::System;
use crate::system_param::SystemParam;
use crate::world::World;

// ---------------------------------------------------------------------------
// IntoSystem

/// Converts a value (typically a function) into a [`System`].
pub trait IntoSystem<Params> {
    type Sys: System;
    fn into_system(self) -> Self::Sys;
}

// ---------------------------------------------------------------------------
// SystemFn — helper trait that lets us bound F correctly per arity
//
// `SystemFn<Params>` is implemented for `fn(P0, P1, ...)` closures where
// each Pi is a `SystemParam::Item<'_>`.  The macro generates one impl per
// arity, each with the correct concrete argument list.

/// A function (or closure) callable with the outputs of `Params::fetch`.
///
/// This trait is sealed; users interact with it only via `IntoSystem`.
pub trait SystemFn<Params: SystemParam>: Send + 'static {
    fn call<'w>(
        &mut self,
        state: &'w mut Params::State,
        world: &'w World,
        resources: &'w ResourceMap,
    );
}

// ---------------------------------------------------------------------------
// FnSystem

/// A [`System`] backed by a plain Rust function.
pub struct FnSystem<F, Params>
where
    Params: SystemParam,
    F: SystemFn<Params>,
{
    pub(crate) func: F,
    pub(crate) state: Option<Params::State>,
    name: &'static str,
    _marker: PhantomData<fn() -> Params>,
}

impl<F, Params> System for FnSystem<F, Params>
where
    Params: SystemParam + 'static,
    F: SystemFn<Params>,
{
    fn name(&self) -> &'static str { self.name }

    fn run(&mut self, world: &mut World, resources: &mut ResourceMap) {
        let state = self.state.get_or_insert_with(|| Params::init(world, resources));
        self.func.call(state, world, resources);
    }
}

// ---------------------------------------------------------------------------
// Zero-param specialisation

impl<F> SystemFn<()> for F
where
    F: FnMut() + Send + 'static,
{
    fn call<'w>(&mut self, _state: &'w mut (), _world: &'w World, _resources: &'w ResourceMap) {
        (self)();
    }
}

impl<F> IntoSystem<()> for F
where
    F: FnMut() + Send + 'static,
{
    type Sys = FnSystem<F, ()>;
    fn into_system(self) -> FnSystem<F, ()> {
        FnSystem {
            func: self,
            state: None,
            name: std::any::type_name::<F>(),
            _marker: PhantomData,
        }
    }
}

// ---------------------------------------------------------------------------
// Per-arity macro

macro_rules! impl_fn_system {
    ( $( $p:ident ),+ ) => {
        // SystemFn impl: F is a closure taking each P::Item<'w> separately.
        impl<F, $($p),+> SystemFn<($($p,)+)> for F
        where
            $( $p: SystemParam + 'static, )+
            F: for<'w> FnMut( $($p::Item<'w>),+ ) + Send + 'static,
        {
            #[allow(non_snake_case)]
            fn call<'w>(
                &mut self,
                state: &'w mut <($($p,)+) as SystemParam>::State,
                world: &'w World,
                resources: &'w ResourceMap,
            ) {
                // SAFETY: we hold exclusive &mut World / &mut ResourceMap.
                let ( $($p,)+ ) = unsafe {
                    <($($p,)+) as SystemParam>::fetch(state, world, resources)
                };
                (self)( $($p),+ );
            }
        }

        // IntoSystem impl
        impl<F, $($p),+> IntoSystem<($($p,)+)> for F
        where
            $( $p: SystemParam + 'static, )+
            F: for<'w> FnMut( $($p::Item<'w>),+ ) + Send + 'static,
        {
            type Sys = FnSystem<F, ($($p,)+)>;
            fn into_system(self) -> FnSystem<F, ($($p,)+)> {
                FnSystem {
                    func: self,
                    state: None,
                    name: std::any::type_name::<F>(),
                    _marker: PhantomData,
                }
            }
        }
    };
}

impl_fn_system!(P0);
impl_fn_system!(P0, P1);
impl_fn_system!(P0, P1, P2);
impl_fn_system!(P0, P1, P2, P3);
impl_fn_system!(P0, P1, P2, P3, P4);
impl_fn_system!(P0, P1, P2, P3, P4, P5);
impl_fn_system!(P0, P1, P2, P3, P4, P5, P6);
impl_fn_system!(P0, P1, P2, P3, P4, P5, P6, P7);
