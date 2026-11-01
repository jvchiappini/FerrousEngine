//! Generation-tracked handles and a slot registry.
//!
//! A `Handle` is a small `Copy` token made of a slot index and a generation
//! counter. `SlotRegistry` stores values in reusable slots and bumps a slot's
//! generation whenever its value is removed, so a handle that outlives its
//! value is rejected instead of silently aliasing a new one.
//!
//! # Generation wrap policy
//!
//! Generations are 32-bit and advance with [`u32::wrapping_add`]. A handle is
//! valid only for the slot's *current* generation. After `2^32` remove/insert
//! cycles on the same slot the counter wraps back to `0`, at which point a
//! handle from the very first incarnation would be accepted again. This ABA is
//! documented and practically unreachable; no attempt is made to detect it.

/// A generation-tracked reference to a value stored in a `SlotRegistry`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Handle {
    index: u32,
    generation: u32,
}

impl Handle {
    /// Builds a handle from raw parts.
    ///
    /// This is the low-level constructor: a handle is only meaningful when
    /// paired with the registry that produced the same `index`/`generation`
    /// pair. Prefer the `SlotRegistry` methods in normal code.
    pub const fn from_parts(index: u32, generation: u32) -> Self {
        Self { index, generation }
    }

    /// Returns the slot index.
    pub const fn index(&self) -> u32 {
        self.index
    }

    /// Returns the generation of the slot when the handle was issued.
    pub const fn generation(&self) -> u32 {
        self.generation
    }
}

/// A generational slot registry with index reuse.
///
/// Values live in slots; removing a value frees its slot for reuse while
/// bumping the slot's generation. Inserting into a free slot keeps the bumped
/// generation, so handles issued before the removal become stale. A slot that
/// has never been used starts at generation `0`.
///
/// Removing a handle that was already removed, was never inserted, or whose
/// index is out of range returns `None` and changes nothing.
pub struct SlotRegistry<T> {
    slots: Vec<Option<(u32, T)>>,
    /// Generation of every slot, including freed ones (whose `slots` entry is
    /// `None`). Keeping it outside the `Option` is what preserves the bumped
    /// generation across the free-list round trip.
    generations: Vec<u32>,
    /// Indices of freed slots, reused before the table grows.
    free: Vec<u32>,
    /// Number of live (non-`None`) slots.
    live: usize,
}

impl<T> SlotRegistry<T> {
    /// Creates an empty registry.
    pub fn new() -> Self {
        Self {
            slots: Vec::new(),
            generations: Vec::new(),
            free: Vec::new(),
            live: 0,
        }
    }

    /// Inserts a value and returns its handle.
    ///
    /// A freed slot is reused before the table grows, keeping the generation
    /// that [`Self::remove`] bumped into it.
    pub fn insert(&mut self, value: T) -> Handle {
        if let Some(index) = self.free.pop() {
            let generation = self.generations[index as usize];
            self.slots[index as usize] = Some((generation, value));
            self.live += 1;
            Handle::from_parts(index, generation)
        } else {
            let index = self.slots.len() as u32;
            self.slots.push(Some((0, value)));
            self.generations.push(0);
            self.live += 1;
            Handle::from_parts(index, 0)
        }
    }

    /// Returns the value for `handle`, or `None` if the handle is stale.
    pub fn get(&self, handle: Handle) -> Option<&T> {
        self.slots
            .get(handle.index as usize)
            .and_then(|slot| slot.as_ref())
            .filter(|(generation, _)| *generation == handle.generation)
            .map(|(_, value)| value)
    }

    /// Returns a mutable value for `handle`, or `None` if the handle is stale.
    pub fn get_mut(&mut self, handle: Handle) -> Option<&mut T> {
        self.slots
            .get_mut(handle.index as usize)
            .and_then(|slot| slot.as_mut())
            .filter(|(generation, _)| *generation == handle.generation)
            .map(|(_, value)| value)
    }

    /// Removes and returns the value for `handle`.
    ///
    /// Removing an already-removed, never-inserted, or out-of-range handle is a
    /// no-op that returns `None`. A successful removal bumps the slot's
    /// generation with [`u32::wrapping_add`] and frees the index for reuse.
    pub fn remove(&mut self, handle: Handle) -> Option<T> {
        let index = handle.index as usize;
        let slot = self.slots.get_mut(index)?;
        let (generation, _) = slot.as_ref()?;
        if *generation != handle.generation {
            return None;
        }
        let (_, value) = slot.take()?;
        self.generations[index] = self.generations[index].wrapping_add(1);
        self.free.push(handle.index);
        self.live -= 1;
        Some(value)
    }

    /// Returns the number of live values.
    pub fn len(&self) -> usize {
        self.live
    }

    /// Returns `true` when the registry holds no live values.
    pub fn is_empty(&self) -> bool {
        self.live == 0
    }

    /// Iterates over every live `(handle, value)` pair in slot order.
    pub fn iter(&self) -> impl Iterator<Item = (Handle, &T)> {
        self.slots.iter().enumerate().filter_map(|(index, slot)| {
            slot.as_ref()
                .map(|(generation, value)| (Handle::from_parts(index as u32, *generation), value))
        })
    }

    /// Returns `true` when `handle` refers to a live value.
    pub fn contains(&self, handle: Handle) -> bool {
        self.get(handle).is_some()
    }

    /// Overwrites a slot's generation. Test-only hook for exercising the
    /// wrap-around policy without performing `2^32` removals.
    #[cfg(test)]
    fn seed_generation(&mut self, index: u32, generation: u32) {
        self.generations[index as usize] = generation;
        if let Some((slot_generation, _)) = self.slots[index as usize].as_mut() {
            *slot_generation = generation;
        }
    }
}

impl<T> Default for SlotRegistry<T> {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stale_handle_is_rejected_after_remove_and_reinsert() {
        let mut registry = SlotRegistry::new();
        let old = registry.insert("old");
        assert_eq!(registry.get(old), Some(&"old"));

        assert_eq!(registry.remove(old), Some("old"));
        assert!(registry.get(old).is_none());
        assert!(!registry.contains(old));

        let new = registry.insert("new");
        assert_eq!(new.index(), old.index(), "the freed slot is reused");
        assert_ne!(new.generation(), old.generation());
        assert!(
            registry.get(old).is_none(),
            "the stale handle must not alias the new value"
        );
        assert_eq!(registry.get(new), Some(&"new"));
    }

    #[test]
    fn removing_twice_is_a_no_op() {
        let mut registry = SlotRegistry::new();
        let handle = registry.insert(1);
        assert_eq!(registry.remove(handle), Some(1));
        assert_eq!(registry.remove(handle), None);
        assert_eq!(registry.len(), 0);
        assert!(registry.is_empty());
    }

    #[test]
    fn never_inserted_and_out_of_range_handles_are_ignored() {
        let mut registry: SlotRegistry<u32> = SlotRegistry::new();
        assert!(registry.get(Handle::from_parts(0, 0)).is_none());
        assert!(registry.get_mut(Handle::from_parts(0, 0)).is_none());
        assert_eq!(registry.remove(Handle::from_parts(0, 0)), None);

        let handle = registry.insert(7);
        assert!(registry
            .get(Handle::from_parts(handle.index() + 10, 0))
            .is_none());
        assert_eq!(
            registry.remove(Handle::from_parts(handle.index() + 10, 0)),
            None
        );
        assert_eq!(registry.len(), 1);
    }

    #[test]
    fn len_and_iter_only_report_live_values() {
        let mut registry = SlotRegistry::new();
        let a = registry.insert("a");
        let b = registry.insert("b");
        let c = registry.insert("c");
        assert_eq!(registry.len(), 3);

        registry.remove(b);
        assert_eq!(registry.len(), 2);
        assert!(!registry.is_empty());

        let live: Vec<(Handle, &str)> = registry
            .iter()
            .map(|(handle, value)| (handle, *value))
            .collect();
        assert_eq!(live, vec![(a, "a"), (c, "c")]);
        assert!(registry.contains(a));
        assert!(!registry.contains(b));
        assert!(registry.contains(c));
    }

    #[test]
    fn insert_after_remove_reuses_the_index_with_a_new_generation() {
        let mut registry = SlotRegistry::new();
        let first = registry.insert(10);
        let index = first.index();
        assert_eq!(registry.remove(first), Some(10));

        let second = registry.insert(20);
        assert_eq!(second.index(), index);
        assert_eq!(second.generation(), first.generation().wrapping_add(1));
        assert_eq!(registry.get(second), Some(&20));
    }

    #[test]
    fn generations_wrap_without_panicking() {
        let mut registry = SlotRegistry::new();
        let original = registry.insert("first");
        let index = original.index();

        // Pretend this slot has been recycled all the way to the last
        // generation, then make one more remove/insert round trip.
        registry.seed_generation(index, u32::MAX);
        let last = Handle::from_parts(index, u32::MAX);
        assert_eq!(registry.get(last), Some(&"first"));
        assert_eq!(registry.remove(last), Some("first"));

        let reused = registry.insert("second");
        assert_eq!(reused.index(), index);
        assert_eq!(reused.generation(), 0, "wrapping_add must wrap to 0");
        assert!(
            registry.get(last).is_none(),
            "the pre-wrap handle must be stale"
        );
        // Documented ABA: after 2^32 reuses the original generation-0 handle
        // aliases the reused slot again. This is accepted as practically
        // unreachable.
        assert_eq!(registry.get(original), Some(&"second"));
    }
}
