//! The ECS World — the central store for entities and components.
//!
//! `World` orchestrates the `EntityAllocator` and `ArchetypeStore`, providing
//! a safe high-level API for spawning, despawning, and querying entities.

use std::any::TypeId;

use crate::archetype::ArchetypeStore;
use crate::component::{Bundle, Component, ComponentInfo, ComponentSet};
use crate::entity::{Entity, EntityAllocator};

/// Central container for all ECS state.
///
/// # Panics
/// Most methods panic in debug builds on bad inputs (stale entity handles, etc.)
/// and are no-ops or return `None` in release.
#[derive(Debug)]
pub struct World {
    pub(crate) entities: EntityAllocator,
    pub(crate) archetypes: ArchetypeStore,
    /// Generation counter — incremented whenever the world structure changes.
    /// Systems can use this to detect invalidated queries.
    pub change_tick: u64,
}

impl Default for World {
    fn default() -> Self {
        World::new()
    }
}

impl World {
    /// Create an empty `World`.
    pub fn new() -> Self {
        World {
            entities: EntityAllocator::new(),
            archetypes: ArchetypeStore::new(),
            change_tick: 0,
        }
    }

    // -----------------------------------------------------------------------
    // Spawn / despawn

    /// Spawn a new entity with the given component bundle.
    ///
    /// ```rust
    /// use ferrous_ecs::prelude::*;
    ///
    /// #[derive(Clone)] struct Pos(f32);
    /// impl Component for Pos {}
    ///
    /// let mut world = World::new();
    /// let e = world.spawn((Pos(1.0),));
    /// assert!(world.contains(e));
    /// ```
    pub fn spawn<B: Bundle>(&mut self, bundle: B) -> Entity {
        // Collect metadata
        let mut type_ids = B::type_ids();
        type_ids.sort_unstable();
        type_ids.dedup();

        let mut infos = B::component_infos();
        infos.sort_unstable_by_key(|i| i.type_id);
        infos.dedup_by_key(|i| i.type_id);

        let sig = ComponentSet(type_ids.clone());
        let arch_id = self.archetypes.get_or_create(sig, infos.clone());

        // Allocate entity
        let (entity, idx) = self.entities.alloc();

        // Write component data into the archetype
        // We need to call write_into with column pointers in signature order.
        let arch = &mut self.archetypes.archetypes[arch_id];
        arch.entities.push(entity);

        // Reserve one slot per column
        for col in &mut arch.columns {
            col.reserve(1);
        }

        // Build sorted column pointer array (one *mut u8 per component)
        let row = arch.entities.len() - 1;
        
        // Match the order of B::type_ids() which write_into expects.
        // B::type_ids() returns them in tuple order (0, 1, 2...).
        let bundle_types = B::type_ids();
        let mut ptrs: Vec<*mut u8> = Vec::with_capacity(bundle_types.len());
        
        for tid in bundle_types {
            let col = arch.columns.iter_mut().find(|c| c.info.type_id == tid).unwrap();
            unsafe {
                ptrs.push(col.get_raw_mut(row));
                col.len += 1;
            }
        }

        // SAFETY: we just reserved space for `row`, ptrs are valid write targets
        unsafe {
            bundle.write_into(&mut ptrs);
        }

        // Update entity record
        let rec = self.entities.get_mut(idx).unwrap();
        rec.archetype_id = Some(arch_id);
        rec.row = row;

        self.change_tick += 1;
        entity
    }

    /// Spawn an entity with a **single** non-`Clone` component by move.
    ///
    /// This is the escape hatch for components that box a trait object (e.g.
    /// `BehaviorComponent`).  The component is written directly into the
    /// archetype column without requiring `Clone`.
    ///
    /// # Limitations
    /// - Only a single component per call.  Compose with `insert_owned` if you
    ///   need more components.
    /// - Archetype migration (e.g. calling `insert` to add another component
    ///   later) will **panic** if it tries to clone this column.  Add all
    ///   non-Clone components last, after all Clone ones are in place.
    pub fn spawn_owned<C: Component>(&mut self, component: C) -> Entity {
        use crate::component::{ComponentInfo, ComponentSet};
        let info = ComponentInfo::of_owned::<C>();
        let sig  = ComponentSet::new(vec![info.type_id]);
        let arch_id = self.archetypes.get_or_create(sig, vec![info.clone()]);

        let (entity, idx) = self.entities.alloc();
        let arch = &mut self.archetypes.archetypes[arch_id];
        arch.entities.push(entity);

        let row = arch.entities.len() - 1;
        for col in &mut arch.columns {
            col.reserve(1);
        }
        let col = arch.columns.iter_mut().find(|c| c.info.type_id == info.type_id).unwrap();
        unsafe {
            let ptr = col.get_raw_mut(row);
            std::ptr::write(ptr as *mut C, component);
            col.len += 1;
        }

        let rec = self.entities.get_mut(idx).unwrap();
        rec.archetype_id = Some(arch_id);
        rec.row = row;

        self.change_tick += 1;
        entity
    }

    /// Insert a **non-`Clone`** component into an existing entity by move.
    ///
    /// Same rules as `spawn_owned` — only call this after all `Clone`
    /// components have been inserted.
    pub fn insert_owned<C: Component>(&mut self, entity: Entity, component: C) {
        use std::any::TypeId;
        use crate::component::ComponentInfo;

        // If already has C, overwrite in-place (no archetype move needed).
        {
            let rec = match self.entities.get(entity) {
                Some(r) => r.clone(),
                None => return,
            };
            if let Some(arch_id) = rec.archetype_id {
                let arch = &mut self.archetypes.archetypes[arch_id];
                if let Some(col) = arch.column_mut::<C>() {
                    let slot = unsafe { col.get_mut::<C>(rec.row) };
                    *slot = component;
                    self.change_tick += 1;
                    return;
                }
            }
        }

        // New component — extend the archetype signature.
        let old_rec = self.entities.get(entity).unwrap().clone();
        let old_arch_id = match old_rec.archetype_id {
            Some(id) => id,
            None => return,
        };

        let new_type_id  = TypeId::of::<C>();
        let new_info     = ComponentInfo::of_owned::<C>();
        let old_sig      = self.archetypes.archetypes[old_arch_id].signature.clone();
        let new_sig      = old_sig.add(new_type_id);

        let mut new_infos: Vec<ComponentInfo> = self.archetypes.archetypes[old_arch_id]
            .columns.iter().map(|c| c.info.clone()).collect();
        new_infos.push(new_info);
        new_infos.sort_unstable_by_key(|i| i.type_id);

        let new_arch_id = self.archetypes.get_or_create(new_sig, new_infos);

        let old_row = old_rec.row;
        self.move_entity_between_archetypes(entity, old_arch_id, old_row, new_arch_id);

        let new_row = self.entities.get(entity).unwrap().row;
        let new_arch = &mut self.archetypes.archetypes[new_arch_id];
        if let Some(col) = new_arch.column_mut::<C>() {
            let slot = unsafe { col.get_mut::<C>(new_row) };
            *slot = component;
        }

        self.change_tick += 1;
    }

    /// Despawn an entity, removing all its components.
    ///
    /// Returns `false` if the entity was already dead (stale handle).
    pub fn despawn(&mut self, entity: Entity) -> bool {
        let rec = match self.entities.get(entity) {
            Some(r) => r.clone(),
            None => return false,
        };
        let arch_id = match rec.archetype_id {
            Some(id) => id,
            None => return false,
        };
        let row = rec.row;

        let swapped = unsafe {
            self.archetypes.archetypes[arch_id].swap_remove(row)
        };

        // If a swap happened, update the moved entity's record
        if let Some(moved_entity) = swapped {
            let moved_rec = self
                .entities
                .get_mut(moved_entity.index as usize)
                .unwrap();
            moved_rec.row = row;
        }

        self.entities.free(entity);
        self.change_tick += 1;
        true
    }

    /// Returns `true` if the entity exists and is alive.
    #[inline]
    pub fn contains(&self, entity: Entity) -> bool {
        self.entities.is_alive(entity)
    }

    /// Total number of live entities.
    pub fn len(&self) -> usize {
        self.entities.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entities.is_empty()
    }

    // -----------------------------------------------------------------------
    // Component access

    /// Get an immutable reference to component `C` of `entity`.
    ///
    /// Returns `None` if the entity is dead or does not have `C`.
    pub fn get<C: Component>(&self, entity: Entity) -> Option<&C> {
        let rec = self.entities.get(entity)?;
        let arch_id = rec.archetype_id?;
        let arch = &self.archetypes.archetypes[arch_id];
        let col = arch.column::<C>()?;
        Some(unsafe { col.get::<C>(rec.row) })
    }

    /// Get a mutable reference to component `C` of `entity`.
    pub fn get_mut<C: Component>(&mut self, entity: Entity) -> Option<&mut C> {
        let rec = self.entities.get(entity)?.clone();
        let arch_id = rec.archetype_id?;
        let arch = &mut self.archetypes.archetypes[arch_id];
        let col = arch.column_mut::<C>()?;
        self.change_tick += 1;
        Some(unsafe { col.get_mut::<C>(rec.row) })
    }

    /// Returns `true` if the entity has component `C`.
    pub fn has<C: Component>(&self, entity: Entity) -> bool {
        self.get::<C>(entity).is_some()
    }

    /// Insert a component into an existing entity.
    ///
    /// If the entity already has component `C`, it is replaced.
    /// Internally moves the entity to a new archetype.
    pub fn insert<C: Component + Clone>(&mut self, entity: Entity, component: C) {
        // If already has C, just overwrite in-place (no archetype change)
        {
            let rec = match self.entities.get(entity) {
                Some(r) => r.clone(),
                None => return,
            };
            if let Some(arch_id) = rec.archetype_id {
                let arch = &mut self.archetypes.archetypes[arch_id];
                if let Some(col) = arch.column_mut::<C>() {
                    let slot = unsafe { col.get_mut::<C>(rec.row) };
                    *slot = component;
                    self.change_tick += 1;
                    return;
                }
            }
        }

        // New component type — move entity to a bigger archetype
        let old_rec = self.entities.get(entity).unwrap().clone();
        let old_arch_id = match old_rec.archetype_id {
            Some(id) => id,
            None => return,
        };

        // Build new signature
        let new_type_id = TypeId::of::<C>();
        let new_info = ComponentInfo::of::<C>();
        let old_sig = self.archetypes.archetypes[old_arch_id].signature.clone();
        let new_sig = old_sig.add(new_type_id);

        // Collect infos for the new archetype
        let mut new_infos: Vec<ComponentInfo> = self.archetypes.archetypes[old_arch_id]
            .columns
            .iter()
            .map(|c| c.info.clone())
            .collect();
        new_infos.push(new_info);
        new_infos.sort_unstable_by_key(|i| i.type_id);

        let new_arch_id = self.archetypes.get_or_create(new_sig, new_infos);

        // Move all existing components from old to new archetype
        let old_row = old_rec.row;
        self.move_entity_between_archetypes(entity, old_arch_id, old_row, new_arch_id);

        // Now insert the new component into the new archetype
        let new_row = self.entities.get(entity).unwrap().row;
        let new_arch = &mut self.archetypes.archetypes[new_arch_id];
        if let Some(col) = new_arch.column_mut::<C>() {
            // The column was pushed with uninitialized data; write the actual value
            let slot = unsafe { col.get_mut::<C>(new_row) };
            *slot = component;
        }

        self.change_tick += 1;
    }

    /// Remove component `C` from an entity, moving it to a smaller archetype.
    ///
    /// Returns `true` if the component existed and was removed.
    pub fn remove<C: Component>(&mut self, entity: Entity) -> bool {
        let rec = match self.entities.get(entity) {
            Some(r) => r.clone(),
            None => return false,
        };
        let arch_id = match rec.archetype_id {
            Some(id) => id,
            None => return false,
        };

        // Check the entity actually has C
        {
            let arch = &self.archetypes.archetypes[arch_id];
            if arch.column::<C>().is_none() {
                return false;
            }
        }

        let remove_type = TypeId::of::<C>();
        let old_sig = self.archetypes.archetypes[arch_id].signature.clone();
        let new_sig = old_sig.remove(remove_type);

        // Build infos for the new archetype (minus C)
        let new_infos: Vec<ComponentInfo> = self.archetypes.archetypes[arch_id]
            .columns
            .iter()
            .filter(|c| c.info.type_id != remove_type)
            .map(|c| c.info.clone())
            .collect();

        let new_arch_id = self.archetypes.get_or_create(new_sig, new_infos);

        let old_row = rec.row;
        self.move_entity_between_archetypes_without(entity, arch_id, old_row, new_arch_id, remove_type);

        self.change_tick += 1;
        true
    }

    // -----------------------------------------------------------------------
    // Query helpers

    /// Iterate over all entities that have component `C`.
    ///
    /// For multi-component queries use `World::query2` / `query3` or the
    /// `Query` type.
    pub fn query<C: Component>(&self) -> impl Iterator<Item = (Entity, &C)> {
        self.archetypes
            .archetypes
            .iter()
            .filter(|a| a.column::<C>().is_some())
            .flat_map(|a| {
                let col = a.column::<C>().unwrap();
                a.entities.iter().enumerate().map(move |(row, &entity)| {
                    let comp = unsafe { col.get::<C>(row) };
                    (entity, comp)
                })
            })
    }

    /// Iterate over all entities that have both `A` and `B`.
    ///
    /// # Deprecated
    /// Use `Query::<(&A, &B)>::new(world).iter()` instead.
    #[deprecated(since = "0.2.0", note = "use Query::<(&A, &B)>::new(world).iter()")]
    pub fn query2<A: Component, B: Component>(
        &self,
    ) -> impl Iterator<Item = (Entity, &A, &B)> {
        self.archetypes
            .archetypes
            .iter()
            .filter(|a| a.column::<A>().is_some() && a.column::<B>().is_some())
            .flat_map(|a| {
                let col_a = a.column::<A>().unwrap();
                let col_b = a.column::<B>().unwrap();
                a.entities.iter().enumerate().map(move |(row, &entity)| {
                    let ca = unsafe { col_a.get::<A>(row) };
                    let cb = unsafe { col_b.get::<B>(row) };
                    (entity, ca, cb)
                })
            })
    }

    /// Iterate over all entities that have `A`, `B`, and `C`.
    ///
    /// # Deprecated
    /// Use `Query::<(&A, &B, &C)>::new(world).iter()` instead.
    #[deprecated(since = "0.2.0", note = "use Query::<(&A, &B, &C)>::new(world).iter()")]
    pub fn query3<A: Component, B: Component, C: Component>(
        &self,
    ) -> impl Iterator<Item = (Entity, &A, &B, &C)> {
        self.archetypes
            .archetypes
            .iter()
            .filter(|a| {
                a.column::<A>().is_some()
                    && a.column::<B>().is_some()
                    && a.column::<C>().is_some()
            })
            .flat_map(|a| {
                let col_a = a.column::<A>().unwrap();
                let col_b = a.column::<B>().unwrap();
                let col_c = a.column::<C>().unwrap();
                a.entities.iter().enumerate().map(move |(row, &entity)| {
                    let ca = unsafe { col_a.get::<A>(row) };
                    let cb = unsafe { col_b.get::<B>(row) };
                    let cc = unsafe { col_c.get::<C>(row) };
                    (entity, ca, cb, cc)
                })
            })
    }

    // -----------------------------------------------------------------------
    // Internal helpers

    /// Move an entity from one archetype to another, cloning all components
    /// that exist in both and inserting an uninitialized slot for new ones.
    fn move_entity_between_archetypes(
        &mut self,
        entity: Entity,
        old_id: usize,
        old_row: usize,
        new_id: usize,
    ) {
        // Phase 1: collect everything we need from the old archetype into owned buffers.
        // (old_id != new_id is guaranteed by callers since we only call this when adding
        //  a brand-new component type)
        debug_assert_ne!(old_id, new_id);

        let new_sig = self.archetypes.archetypes[new_id].signature.clone();

        // For each column in the NEW archetype that also exists in the OLD, clone the bytes.
        // We collect (new_col_idx, owned_bytes) pairs.
        let mut cloned_data: Vec<(usize, Vec<u8>)> = Vec::new();
        {
            let old_arch = &self.archetypes.archetypes[old_id];
            for (old_col_idx, old_col) in old_arch.columns.iter().enumerate() {
                let tid = old_col.info.type_id;
                if let Ok(new_col_idx) = new_sig.0.binary_search(&tid) {
                    let size = old_col.info.size;
                    let mut buf = vec![0u8; size];
                    if size > 0 {
                        unsafe { old_col.clone_into(old_row, buf.as_mut_ptr()) };
                    }
                    cloned_data.push((new_col_idx, buf));
                }
                let _ = old_col_idx;
            }
        }

        // Phase 2: push entity + data into the new archetype.
        let new_row = {
            let new_arch = &mut self.archetypes.archetypes[new_id];
            new_arch.entities.push(entity);
            let row = new_arch.entities.len() - 1;
            for col in &mut new_arch.columns {
                col.reserve(1);
                col.len += 1;
            }
            for (new_col_idx, buf) in &cloned_data {
                let col = &mut new_arch.columns[*new_col_idx];
                if col.info.size > 0 {
                    unsafe {
                        let dst = col.get_raw_mut(row);
                        std::ptr::copy_nonoverlapping(buf.as_ptr(), dst, col.info.size);
                    }
                }
            }
            row
        };

        // Phase 3: drop the component value from the old archetype WITHOUT
        // triggering the column's normal drop (it was moved/cloned above).
        // We do a raw swap-remove and skip the drop by overwriting before removing.
        let swapped = unsafe { self.archetypes.archetypes[old_id].swap_remove_no_drop(old_row) };
        if let Some(moved) = swapped {
            let moved_rec = self.entities.get_mut(moved.index as usize).unwrap();
            moved_rec.row = old_row;
        }

        // Phase 4: update entity record.
        let rec = self.entities.get_mut(entity.index as usize).unwrap();
        rec.archetype_id = Some(new_id);
        rec.row = new_row;
    }

    /// Like `move_entity_between_archetypes` but skips `skip_type` during clone
    /// (used when removing a component).
    fn move_entity_between_archetypes_without(
        &mut self,
        entity: Entity,
        old_id: usize,
        old_row: usize,
        new_id: usize,
        skip_type: TypeId,
    ) {
        debug_assert_ne!(old_id, new_id);

        let new_sig = self.archetypes.archetypes[new_id].signature.clone();

        let mut cloned_data: Vec<(usize, Vec<u8>)> = Vec::new();
        {
            let old_arch = &self.archetypes.archetypes[old_id];
            for old_col in old_arch.columns.iter() {
                let tid = old_col.info.type_id;
                if tid == skip_type {
                    continue;
                }
                if let Ok(new_col_idx) = new_sig.0.binary_search(&tid) {
                    let size = old_col.info.size;
                    let mut buf = vec![0u8; size];
                    if size > 0 {
                        unsafe { old_col.clone_into(old_row, buf.as_mut_ptr()) };
                    }
                    cloned_data.push((new_col_idx, buf));
                }
            }
        }

        let new_row = {
            let new_arch = &mut self.archetypes.archetypes[new_id];
            new_arch.entities.push(entity);
            let row = new_arch.entities.len() - 1;
            for col in &mut new_arch.columns {
                col.reserve(1);
                col.len += 1;
            }
            for (new_col_idx, buf) in &cloned_data {
                let col = &mut new_arch.columns[*new_col_idx];
                if col.info.size > 0 {
                    unsafe {
                        let dst = col.get_raw_mut(row);
                        std::ptr::copy_nonoverlapping(buf.as_ptr(), dst, col.info.size);
                    }
                }
            }
            row
        };

        // For the removed component: let the old archetype's swap_remove DROP it normally.
        let swapped = unsafe { self.archetypes.archetypes[old_id].swap_remove(old_row) };
        if let Some(moved) = swapped {
            let moved_rec = self.entities.get_mut(moved.index as usize).unwrap();
            moved_rec.row = old_row;
        }

        let rec = self.entities.get_mut(entity.index as usize).unwrap();
        rec.archetype_id = Some(new_id);
        rec.row = new_row;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::component::Component;

    #[derive(Clone, Debug, PartialEq)]
    struct Pos(f32, f32, f32);
    impl Component for Pos {}

    #[derive(Clone, Debug, PartialEq)]
    struct Vel(f32);
    impl Component for Vel {}

    #[derive(Clone, Debug, PartialEq)]
    struct Health(f32);
    impl Component for Health {}

    #[test]
    fn spawn_and_query() {
        let mut world = World::new();
        let e0 = world.spawn((Pos(1.0, 2.0, 3.0), Vel(5.0)));
        let e1 = world.spawn((Pos(4.0, 5.0, 6.0),));

        assert!(world.contains(e0));
        assert!(world.contains(e1));

        let pos0 = world.get::<Pos>(e0).unwrap();
        assert_eq!(*pos0, Pos(1.0, 2.0, 3.0));

        let vel0 = world.get::<Vel>(e0).unwrap();
        assert_eq!(*vel0, Vel(5.0));

        assert!(world.get::<Vel>(e1).is_none());
    }

    #[test]
    fn despawn_cleans_up() {
        let mut world = World::new();
        let e = world.spawn((Pos(0.0, 0.0, 0.0),));
        assert!(world.despawn(e));
        assert!(!world.contains(e));
        // Double despawn is a no-op
        assert!(!world.despawn(e));
    }

    #[test]
    fn insert_new_component() {
        let mut world = World::new();
        let e = world.spawn((Pos(1.0, 0.0, 0.0),));
        world.insert(e, Vel(3.0));
        assert!(world.has::<Vel>(e));
        assert_eq!(*world.get::<Vel>(e).unwrap(), Vel(3.0));
        // Pos should still be there
        assert_eq!(*world.get::<Pos>(e).unwrap(), Pos(1.0, 0.0, 0.0));
    }

    #[test]
    fn remove_component() {
        let mut world = World::new();
        let e = world.spawn((Pos(1.0, 0.0, 0.0), Vel(3.0)));
        assert!(world.remove::<Vel>(e));
        assert!(!world.has::<Vel>(e));
        assert!(world.has::<Pos>(e));
    }

    #[test]
    fn query_multi() {
        let mut world = World::new();
        world.spawn((Pos(1.0, 0.0, 0.0), Vel(1.0)));
        world.spawn((Pos(2.0, 0.0, 0.0), Vel(2.0)));
        world.spawn((Pos(3.0, 0.0, 0.0),)); // no Vel

        let count = world.query2::<Pos, Vel>().count();
        assert_eq!(count, 2);
    }

    #[test]
    fn despawn_swap_removes_correctly() {
        let mut world = World::new();
        let e0 = world.spawn((Pos(0.0, 0.0, 0.0),));
        let e1 = world.spawn((Pos(1.0, 0.0, 0.0),));
        let e2 = world.spawn((Pos(2.0, 0.0, 0.0),));

        world.despawn(e0); // swap removes e2 into row 0
        assert!(!world.contains(e0));
        assert!(world.contains(e1));
        assert!(world.contains(e2));

        // e2's position should still be correct even after the swap
        let pos2 = world.get::<Pos>(e2).unwrap();
        assert_eq!(*pos2, Pos(2.0, 0.0, 0.0));
    }
}
