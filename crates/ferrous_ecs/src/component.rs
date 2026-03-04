//! Component trait and type-erased component metadata.
//!
//! Any type that implements `Component` can be stored in an archetype table.
//! The trait is intentionally minimal — it only requires the `Send + Sync +
//! 'static` bounds that the ECS needs for safe multi-threaded access.

use std::any::TypeId;

/// Marker trait for all component types.
///
/// Implement this for any struct or enum you want to attach to entities:
///
/// ```rust
/// use ferrous_ecs::component::Component;
///
/// #[derive(Debug, Clone)]
/// struct Health(f32);
/// impl Component for Health {}
/// ```
///
/// Components must be `Send + Sync + 'static` so they can live in shared
/// archetype storage and be accessed from worker threads.
pub trait Component: Send + Sync + 'static {}

// ---------------------------------------------------------------------------
// Type-erased vtable for archetype columns

/// Drop function pointer: drops the value at a given raw pointer.
pub(crate) type DropFn = unsafe fn(*mut u8);

/// Clone-into function pointer: clones src → dst (both must be valid + aligned).
pub(crate) type CloneFn = unsafe fn(src: *const u8, dst: *mut u8);

/// Metadata record for a single component type.
#[derive(Clone)]
pub struct ComponentInfo {
    pub(crate) type_id: TypeId,
    pub(crate) size: usize,
    pub(crate) align: usize,
    pub(crate) drop_fn: Option<DropFn>,
    pub(crate) clone_fn: CloneFn,
    /// Human-readable name for debugging.
    pub(crate) name: &'static str,
}

impl std::fmt::Debug for ComponentInfo {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ComponentInfo")
            .field("name", &self.name)
            .field("size", &self.size)
            .field("align", &self.align)
            .finish()
    }
}

impl PartialEq for ComponentInfo {
    fn eq(&self, other: &Self) -> bool {
        self.type_id == other.type_id
    }
}
impl Eq for ComponentInfo {}

impl ComponentInfo {
    /// Construct metadata from a concrete component type.
    pub fn of<C: Component + Clone>() -> Self {
        unsafe fn drop_impl<C>(ptr: *mut u8) {
            std::ptr::drop_in_place(ptr as *mut C);
        }
        unsafe fn clone_impl<C: Clone>(src: *const u8, dst: *mut u8) {
            let cloned = (*(src as *const C)).clone();
            std::ptr::write(dst as *mut C, cloned);
        }

        ComponentInfo {
            type_id: TypeId::of::<C>(),
            size: std::mem::size_of::<C>(),
            align: std::mem::align_of::<C>(),
            drop_fn: if std::mem::needs_drop::<C>() {
                Some(drop_impl::<C>)
            } else {
                None
            },
            clone_fn: clone_impl::<C>,
            name: std::any::type_name::<C>(),
        }
    }
}

// ---------------------------------------------------------------------------
// ComponentSet: an ordered, deduplicated set of TypeIds that identifies an
// archetype signature.

/// A sorted, deduplicated list of `TypeId`s that uniquely identifies an
/// archetype's component composition.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ComponentSet(pub(crate) Vec<TypeId>);

impl ComponentSet {
    pub fn new(mut ids: Vec<TypeId>) -> Self {
        ids.sort_unstable();
        ids.dedup();
        ComponentSet(ids)
    }

    pub fn empty() -> Self {
        ComponentSet(Vec::new())
    }

    pub fn contains(&self, id: TypeId) -> bool {
        self.0.binary_search(&id).is_ok()
    }

    pub fn add(&self, id: TypeId) -> Self {
        let mut v = self.0.clone();
        if let Err(pos) = v.binary_search(&id) {
            v.insert(pos, id);
        }
        ComponentSet(v)
    }

    pub fn remove(&self, id: TypeId) -> Self {
        let mut v = self.0.clone();
        if let Ok(pos) = v.binary_search(&id) {
            v.remove(pos);
        }
        ComponentSet(v)
    }

    pub fn len(&self) -> usize {
        self.0.len()
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    pub fn iter(&self) -> impl Iterator<Item = TypeId> + '_ {
        self.0.iter().copied()
    }
}

// ---------------------------------------------------------------------------
// Bundle trait — heterogeneous tuples of components that can be spawned together

/// A bundle is a collection of components that can be inserted/spawned as a unit.
///
/// Implemented for tuples up to arity 12.  You rarely need to implement this
/// yourself; just use tuple syntax with `World::spawn`.
pub trait Bundle: Send + 'static {
    /// Collect the `TypeId`s of every component in this bundle.
    fn type_ids() -> Vec<TypeId>;
    /// Collect `ComponentInfo` for every component type.
    fn component_infos() -> Vec<ComponentInfo>;
    /// Write each component into the provided raw byte slices (one per column).
    /// Each slice is already sized to `ComponentInfo::size`.
    ///
    /// # Safety
    /// `columns[i]` must point to properly aligned, writable memory of the
    /// correct size for component `i`.
    unsafe fn write_into(self, columns: &mut [*mut u8]);
}

// ---------------------------------------------------------------------------
// Macro-generated Bundle impls for tuples

macro_rules! impl_bundle {
    ($($idx:tt : $T:ident),+) => {
        impl<$($T: Component + Clone),+> Bundle for ($($T,)+) {
            fn type_ids() -> Vec<TypeId> {
                vec![$(TypeId::of::<$T>()),+]
            }

            fn component_infos() -> Vec<ComponentInfo> {
                vec![$(ComponentInfo::of::<$T>()),+]
            }

            unsafe fn write_into(self, columns: &mut [*mut u8]) {
                $(
                    std::ptr::write(columns[$idx] as *mut $T, self.$idx);
                )+
            }
        }
    };
}

impl_bundle!(0: A);
impl_bundle!(0: A, 1: B);
impl_bundle!(0: A, 1: B, 2: C);
impl_bundle!(0: A, 1: B, 2: C, 3: D);
impl_bundle!(0: A, 1: B, 2: C, 3: D, 4: E);
impl_bundle!(0: A, 1: B, 2: C, 3: D, 4: E, 5: F);
impl_bundle!(0: A, 1: B, 2: C, 3: D, 4: E, 5: F, 6: G);
impl_bundle!(0: A, 1: B, 2: C, 3: D, 4: E, 5: F, 6: G, 7: H);
impl_bundle!(0: A, 1: B, 2: C, 3: D, 4: E, 5: F, 6: G, 7: H, 8: I);
impl_bundle!(0: A, 1: B, 2: C, 3: D, 4: E, 5: F, 6: G, 7: H, 8: I, 9: J);
impl_bundle!(0: A, 1: B, 2: C, 3: D, 4: E, 5: F, 6: G, 7: H, 8: I, 9: J, 10: K);
impl_bundle!(0: A, 1: B, 2: C, 3: D, 4: E, 5: F, 6: G, 7: H, 8: I, 9: J, 10: K, 11: L);

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Clone)]
    struct Hp(f32);
    impl Component for Hp {}

    #[derive(Clone)]
    struct Name(String);
    impl Component for Name {}

    #[test]
    fn component_info_round_trip() {
        let info = ComponentInfo::of::<Hp>();
        assert_eq!(info.type_id, TypeId::of::<Hp>());
        assert_eq!(info.size, std::mem::size_of::<Hp>());
    }

    #[test]
    fn component_set_sorted() {
        let a = TypeId::of::<Hp>();
        let b = TypeId::of::<Name>();
        let s1 = ComponentSet::new(vec![b, a]);
        let s2 = ComponentSet::new(vec![a, b]);
        assert_eq!(s1, s2); // order-independent equality
    }

    #[test]
    fn bundle_type_ids() {
        let ids = <(Hp, Name) as Bundle>::type_ids();
        assert_eq!(ids.len(), 2);
        assert!(ids.contains(&TypeId::of::<Hp>()));
        assert!(ids.contains(&TypeId::of::<Name>()));
    }
}
