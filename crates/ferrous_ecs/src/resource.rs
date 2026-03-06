//! Global resources — non-entity state stored in the `World`.
//!
//! Resources are singleton values (like game time, input state, or render
//! configuration) that live outside the entity/component model.  They are
//! stored in a type-erased map and accessed by concrete type.

use std::any::{Any, TypeId};
use std::collections::HashMap;

// ---------------------------------------------------------------------------

/// Type-erased container for global resources.
///
/// ```rust
/// use ferrous_ecs::resource::ResourceMap;
///
/// struct GameTime(f64);
///
/// let mut res = ResourceMap::new();
/// res.insert(GameTime(0.0));
/// assert!(res.contains::<GameTime>());
/// let t = res.get::<GameTime>().unwrap();
/// assert_eq!(t.0, 0.0);
/// ```
#[derive(Default)]
pub struct ResourceMap {
    map: HashMap<TypeId, Box<dyn Any + Send + Sync>>,
}

impl ResourceMap {
    pub fn new() -> Self {
        Self::default()
    }

    /// Insert a resource, replacing any existing value of the same type.
    pub fn insert<R: Send + Sync + 'static>(&mut self, resource: R) {
        self.map.insert(TypeId::of::<R>(), Box::new(resource));
    }

    /// Remove a resource.  Returns `true` if it existed.
    pub fn remove<R: Send + Sync + 'static>(&mut self) -> bool {
        self.map.remove(&TypeId::of::<R>()).is_some()
    }

    /// Immutable reference to resource `R`.
    pub fn get<R: Send + Sync + 'static>(&self) -> Option<&R> {
        self.map
            .get(&TypeId::of::<R>())
            .and_then(|b| b.downcast_ref::<R>())
    }

    /// Mutable reference to resource `R`.
    pub fn get_mut<R: Send + Sync + 'static>(&mut self) -> Option<&mut R> {
        self.map
            .get_mut(&TypeId::of::<R>())
            .and_then(|b| b.downcast_mut::<R>())
    }

    /// Returns `true` if resource `R` is present.
    pub fn contains<R: Send + Sync + 'static>(&self) -> bool {
        self.map.contains_key(&TypeId::of::<R>())
    }

    /// Get or insert a default value.
    pub fn get_or_insert_default<R: Default + Send + Sync + 'static>(&mut self) -> &mut R {
        self.map
            .entry(TypeId::of::<R>())
            .or_insert_with(|| Box::new(R::default()))
            .downcast_mut::<R>()
            .unwrap()
    }

    /// Number of resources stored.
    pub fn len(&self) -> usize {
        self.map.len()
    }

    /// Returns a raw mutable pointer to resource `R`.
    ///
    /// # Safety
    /// The caller must ensure no other reference to `R` is alive for the
    /// duration of the pointer's use.  This is intended for `SystemParam`
    /// implementations that need `&mut R` while only holding `&ResourceMap`.
    pub unsafe fn get_mut_ptr<R: Send + Sync + 'static>(&self) -> Option<*mut R> {
        self.map
            .get(&TypeId::of::<R>())
            .and_then(|b| {
                let ptr = b.downcast_ref::<R>()? as *const R as *mut R;
                Some(ptr)
            })
    }

    pub fn is_empty(&self) -> bool {
        self.map.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Counter(u32);
    struct Label(&'static str);

    #[test]
    fn insert_and_get() {
        let mut r = ResourceMap::new();
        r.insert(Counter(0));
        r.insert(Label("hello"));

        assert_eq!(r.get::<Counter>().unwrap().0, 0);
        assert_eq!(r.get::<Label>().unwrap().0, "hello");
    }

    #[test]
    fn remove() {
        let mut r = ResourceMap::new();
        r.insert(Counter(42));
        assert!(r.remove::<Counter>());
        assert!(!r.contains::<Counter>());
        assert!(!r.remove::<Counter>()); // idempotent
    }

    #[test]
    fn get_or_insert_default() {
        #[derive(Default)]
        struct Score(u32);

        let mut r = ResourceMap::new();
        r.get_or_insert_default::<Score>().0 = 10;
        assert_eq!(r.get::<Score>().unwrap().0, 10);
    }

    #[test]
    fn replace_value() {
        let mut r = ResourceMap::new();
        r.insert(Counter(1));
        r.insert(Counter(99));
        assert_eq!(r.get::<Counter>().unwrap().0, 99);
    }
}
