//! Archetype storage — dense Structure-of-Arrays (SoA) per unique component set.
//!
//! Each archetype stores exactly the entities that possess a specific set of
//! components.  Within an archetype every component type gets its own
//! `ComponentColumn` — a type-erased, heap-allocated contiguous array.
//!
//! This layout gives O(1) random access per component and sequential access
//! that is maximally cache-friendly during system iteration.

use std::alloc::{self, Layout};
use std::any::TypeId;

use crate::component::{ComponentInfo, ComponentSet};
use crate::entity::Entity;

// ---------------------------------------------------------------------------
// ComponentColumn — type-erased Vec<T>

/// A type-erased, heap-allocated column of one component type.
///
/// Internally it is a raw byte array with a known stride (`info.size`).
/// Capacity grows like a `Vec` (doubling).
#[derive(Debug)]
pub struct ComponentColumn {
    pub(crate) info: ComponentInfo,
    /// Raw byte storage.  Length == `len * info.size`.
    data: *mut u8,
    pub(crate) len: usize,
    capacity: usize,
}

// SAFETY: ComponentInfo's component type is Send + Sync (required by Component).
unsafe impl Send for ComponentColumn {}
unsafe impl Sync for ComponentColumn {}

impl ComponentColumn {
    pub fn new(info: ComponentInfo) -> Self {
        ComponentColumn {
            info,
            data: std::ptr::null_mut(),
            len: 0,
            capacity: 0,
        }
    }

    /// Number of elements currently stored.
    #[inline]
    pub fn len(&self) -> usize {
        self.len
    }

    #[inline]
    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    /// Ensure at least `additional` more elements can be pushed without realloc.
    pub fn reserve(&mut self, additional: usize) {
        let needed = self.len + additional;
        if needed <= self.capacity {
            return;
        }
        let new_cap = needed.max(self.capacity * 2).max(4);
        self.realloc(new_cap);
    }

    fn realloc(&mut self, new_cap: usize) {
        let size = self.info.size;
        if size == 0 {
            self.capacity = new_cap;
            return;
        }
        let align = self.info.align;
        let new_layout = Layout::from_size_align(new_cap * size, align)
            .expect("invalid layout for ComponentColumn");

        let new_ptr = if self.data.is_null() {
            // SAFETY: layout is non-zero (size > 0 and cap > 0)
            unsafe { alloc::alloc(new_layout) }
        } else {
            let old_layout = Layout::from_size_align(self.capacity * size, align).unwrap();
            // SAFETY: old pointer was allocated with old_layout
            unsafe { alloc::realloc(self.data, old_layout, new_cap * size) }
        };

        if new_ptr.is_null() {
            alloc::handle_alloc_error(new_layout);
        }
        self.data = new_ptr;
        self.capacity = new_cap;
    }

    /// Push one element (ownership transferred from raw ptr).
    ///
    /// # Safety
    /// `src` must point to a valid, initialized value of type `T` (where `T`
    /// matches `self.info.type_id`).  The caller must NOT drop `src` after
    /// this call — ownership is moved.
    pub unsafe fn push_raw(&mut self, src: *const u8) {
        self.reserve(1);
        let dst = self.data.add(self.len * self.info.size);
        if self.info.size > 0 {
            std::ptr::copy_nonoverlapping(src, dst, self.info.size);
        }
        self.len += 1;
    }

    /// Remove the element at `row` using swap-remove (O(1)).
    /// Returns the entity that was swapped into `row` (the last element),
    /// or `None` if the removed element was the last one.
    ///
    /// The removed element's drop function is called if needed.
    pub unsafe fn swap_remove(&mut self, row: usize) -> bool {
        debug_assert!(row < self.len);
        let size = self.info.size;
        let last = self.len - 1;
        let was_last = row == last;

        if size > 0 {
            // Drop the element being removed
            let remove_ptr = self.data.add(row * size);
            if let Some(drop_fn) = self.info.drop_fn {
                drop_fn(remove_ptr);
            }

            // Swap-remove: copy last element over removed slot
            if !was_last {
                let last_ptr = self.data.add(last * size);
                std::ptr::copy_nonoverlapping(last_ptr, remove_ptr, size);
            }
        }

        self.len -= 1;
        !was_last // true if a swap actually happened (caller must fix entity table)
    }

    /// Typed immutable reference to element at `row`.
    ///
    /// # Safety
    /// `T` must match `self.info.type_id` and `row < self.len`.
    #[inline]
    pub unsafe fn get<T: 'static>(&self, row: usize) -> &T {
        debug_assert!(TypeId::of::<T>() == self.info.type_id);
        debug_assert!(row < self.len);
        &*(self.data.add(row * self.info.size) as *const T)
    }

    /// Typed mutable reference to element at `row`.
    ///
    /// # Safety
    /// Same as `get`.
    #[inline]
    pub unsafe fn get_mut<T: 'static>(&mut self, row: usize) -> &mut T {
        debug_assert!(TypeId::of::<T>() == self.info.type_id);
        debug_assert!(row < self.len);
        &mut *(self.data.add(row * self.info.size) as *mut T)
    }

    /// Raw pointer to element at `row`.
    #[inline]
    pub unsafe fn get_raw(&self, row: usize) -> *const u8 {
        self.data.add(row * self.info.size)
    }

    /// Mutable raw pointer to element at `row`.
    #[inline]
    pub unsafe fn get_raw_mut(&mut self, row: usize) -> *mut u8 {
        self.data.add(row * self.info.size)
    }

    /// Clone the element at `row` into `dst`.
    ///
    /// # Safety
    /// `dst` must be valid, aligned memory for the component type.
    pub unsafe fn clone_into(&self, row: usize, dst: *mut u8) {
        let src = self.data.add(row * self.info.size);
        (self.info.clone_fn)(src, dst);
    }
}

impl Drop for ComponentColumn {
    fn drop(&mut self) {
        if self.data.is_null() || self.info.size == 0 {
            return;
        }
        // Drop all live elements
        if let Some(drop_fn) = self.info.drop_fn {
            for i in 0..self.len {
                unsafe { drop_fn(self.data.add(i * self.info.size)) };
            }
        }
        let layout =
            Layout::from_size_align(self.capacity * self.info.size, self.info.align).unwrap();
        unsafe { alloc::dealloc(self.data, layout) };
    }
}

// ---------------------------------------------------------------------------
// Archetype

/// Stores all entities that share the same component set.
///
/// Row `i` across all columns corresponds to `entities[i]`.
#[derive(Debug)]
pub struct Archetype {
    /// The unique component-set signature for this archetype.
    pub(crate) signature: ComponentSet,
    /// Entity list — `entities[row]` gives the entity at that row.
    pub(crate) entities: Vec<Entity>,
    /// One column per component type, in the same order as `signature.0`.
    pub(crate) columns: Vec<ComponentColumn>,
}

impl Archetype {
    /// Create a new empty archetype for `signature` using `infos` for column metadata.
    /// `infos` must be sorted in the same order as `signature.0` (by TypeId).
    pub fn new(signature: ComponentSet, mut infos: Vec<ComponentInfo>) -> Self {
        // Sort infos to match signature order
        infos.sort_unstable_by_key(|i| i.type_id);
        infos.dedup_by_key(|i| i.type_id);
        let columns = infos.into_iter().map(ComponentColumn::new).collect();
        Archetype {
            signature,
            entities: Vec::new(),
            columns,
        }
    }

    /// Empty archetype (for entities with no components).
    pub fn empty() -> Self {
        Archetype {
            signature: ComponentSet::empty(),
            entities: Vec::new(),
            columns: Vec::new(),
        }
    }

    /// Number of entities in this archetype.
    #[inline]
    pub fn len(&self) -> usize {
        self.entities.len()
    }

    #[inline]
    pub fn is_empty(&self) -> bool {
        self.entities.is_empty()
    }

    /// Find the column index for `type_id`, or `None` if not present.
    pub fn column_index(&self, type_id: TypeId) -> Option<usize> {
        self.signature
            .0
            .binary_search(&type_id)
            .ok()
    }

    /// Typed immutable access to the column for `T`.
    ///
    /// Returns `None` if this archetype does not store `T`.
    pub fn column<T: 'static>(&self) -> Option<&ComponentColumn> {
        let idx = self.column_index(TypeId::of::<T>())?;
        self.columns.get(idx)
    }

    /// Typed mutable access to the column for `T`.
    pub fn column_mut<T: 'static>(&mut self) -> Option<&mut ComponentColumn> {
        let idx = self.column_index(TypeId::of::<T>())?;
        self.columns.get_mut(idx)
    }

    /// Push an entity and its raw component bytes into this archetype.
    ///
    /// # Safety
    /// `raw_components[i]` must point to a valid, initialized component of
    /// the type stored in `columns[i]`.  Ownership is transferred.
    pub unsafe fn push_entity(
        &mut self,
        entity: Entity,
        raw_components: &[*const u8],
    ) -> usize {
        debug_assert_eq!(raw_components.len(), self.columns.len());
        let row = self.entities.len();
        self.entities.push(entity);
        for (col, &src) in self.columns.iter_mut().zip(raw_components.iter()) {
            col.push_raw(src);
        }
        row
    }

    /// Swap-remove the entity at `row`, skipping drop on all components.
    ///
    /// Use this when the component data has already been moved out (e.g.
    /// cloned into another archetype).  Calling the drop function would be
    /// a double-free in that case.
    ///
    /// Returns the entity that was swapped in (`None` if it was the last).
    pub unsafe fn swap_remove_no_drop(&mut self, row: usize) -> Option<Entity> {
        debug_assert!(row < self.entities.len());
        let last = self.entities.len() - 1;
        let was_last = row == last;
        let last_entity = *self.entities.last().unwrap();

        self.entities.swap_remove(row);

        for col in &mut self.columns {
            let size = col.info.size;
            if size > 0 && !was_last {
                let remove_ptr = col.data.add(row * size);
                let last_ptr   = col.data.add(last * size);
                // Overwrite removed slot with last element's bytes (no drop on either).
                std::ptr::copy_nonoverlapping(last_ptr, remove_ptr, size);
            }
            col.len -= 1;
        }

        if was_last { None } else { Some(last_entity) }
    }

    /// Swap-remove the entity at `row`.
    ///
    /// Returns the entity that was swapped into `row` (the previously-last one),
    /// or `None` if the removed entity was the last.
    pub unsafe fn swap_remove(&mut self, row: usize) -> Option<Entity> {
        debug_assert!(row < self.entities.len());
        let last_entity = *self.entities.last().unwrap();
        let was_last = row == self.entities.len() - 1;

        // Swap-remove entity list
        self.entities.swap_remove(row);

        // Swap-remove each column
        for col in &mut self.columns {
            col.swap_remove(row);
        }

        if was_last { None } else { Some(last_entity) }
    }
}

// ---------------------------------------------------------------------------
// ArchetypeStore — the set of all archetypes

/// Manages all archetypes in the world.
#[derive(Debug, Default)]
pub struct ArchetypeStore {
    pub(crate) archetypes: Vec<Archetype>,
    /// Map from `ComponentSet` → archetype index.
    index: std::collections::HashMap<ComponentSet, usize>,
}

impl ArchetypeStore {
    pub fn new() -> Self {
        let empty = Archetype::empty();
        let empty_sig = ComponentSet::empty();
        let mut store = ArchetypeStore {
            archetypes: vec![empty],
            index: std::collections::HashMap::new(),
        };
        store.index.insert(empty_sig, 0);
        store
    }

    /// Get or create the archetype for `signature`.
    /// `infos` provides the column metadata.
    pub fn get_or_create(
        &mut self,
        signature: ComponentSet,
        infos: Vec<ComponentInfo>,
    ) -> usize {
        if let Some(&id) = self.index.get(&signature) {
            return id;
        }
        let id = self.archetypes.len();
        self.archetypes
            .push(Archetype::new(signature.clone(), infos));
        self.index.insert(signature, id);
        id
    }

    /// Archetype id for the empty archetype (index 0).
    pub fn empty_id() -> usize {
        0
    }

    pub fn get(&self, id: usize) -> Option<&Archetype> {
        self.archetypes.get(id)
    }

    pub fn get_mut(&mut self, id: usize) -> Option<&mut Archetype> {
        self.archetypes.get_mut(id)
    }

    pub fn len(&self) -> usize {
        self.archetypes.len()
    }

    pub fn iter(&self) -> impl Iterator<Item = &Archetype> {
        self.archetypes.iter()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::component::{Component, ComponentInfo};

    #[derive(Clone, Debug, PartialEq)]
    struct Pos(f32, f32, f32);
    impl Component for Pos {}

    #[derive(Clone, Debug, PartialEq)]
    struct Vel(f32);
    impl Component for Vel {}

    fn make_sig<C: Component + Clone>() -> (ComponentSet, Vec<ComponentInfo>) {
        let info = ComponentInfo::of::<C>();
        let sig = ComponentSet::new(vec![info.type_id]);
        (sig, vec![info])
    }

    #[test]
    fn push_and_get() {
        let (sig, infos) = make_sig::<Pos>();
        let mut arch = Archetype::new(sig, infos);

        let e = Entity { index: 0, generation: 0 };
        let pos = Pos(1.0, 2.0, 3.0);
        let src = &pos as *const Pos as *const u8;

        let row = unsafe { arch.push_entity(e, &[src]) };
        std::mem::forget(pos); // ownership transferred
        assert_eq!(row, 0);
        assert_eq!(arch.len(), 1);

        let got = unsafe { arch.column::<Pos>().unwrap().get::<Pos>(0) };
        assert_eq!(*got, Pos(1.0, 2.0, 3.0));
    }

    #[test]
    fn swap_remove_last() {
        let (sig, infos) = make_sig::<Pos>();
        let mut arch = Archetype::new(sig, infos);

        for i in 0..3u32 {
            let e = Entity { index: i, generation: 0 };
            let pos = Pos(i as f32, 0.0, 0.0);
            let src = &pos as *const Pos as *const u8;
            unsafe { arch.push_entity(e, &[src]); }
            std::mem::forget(pos);
        }
        assert_eq!(arch.len(), 3);
        let swapped = unsafe { arch.swap_remove(2) };
        assert!(swapped.is_none()); // was last
        assert_eq!(arch.len(), 2);
    }
}
