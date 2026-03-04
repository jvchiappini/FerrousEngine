//! Entity identifiers with generational indices.
//!
//! An `Entity` is a 64-bit value formed by packing a 32-bit index and a 32-bit
//! generation counter.  Reusing an index after a despawn increments the
//! generation, so dangling handles (from a previous incarnation of that slot)
//! are reliably detected.

/// A lightweight, copy-able entity handle.
///
/// Internally `{ index: u32, generation: u32 }` packed into a single `u64`
/// for cache-efficiency.  Null/sentinel value: `Entity::DANGLING`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Entity {
    /// Index into the entity table.
    pub index: u32,
    /// Generation counter — incremented every time the slot is recycled.
    pub generation: u32,
}

impl Entity {
    /// The sentinel "null" entity.  Never returned by `World::spawn`.
    pub const DANGLING: Entity = Entity {
        index: u32::MAX,
        generation: u32::MAX,
    };

    /// Pack into a single `u64` for use as a map key.
    #[inline]
    pub fn to_bits(self) -> u64 {
        ((self.generation as u64) << 32) | self.index as u64
    }

    /// Unpack from a `u64` produced by `to_bits`.
    #[inline]
    pub fn from_bits(bits: u64) -> Self {
        Entity {
            index: bits as u32,
            generation: (bits >> 32) as u32,
        }
    }
}

impl std::fmt::Display for Entity {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Entity({}:{})", self.index, self.generation)
    }
}

// ---------------------------------------------------------------------------

/// Internal slot in the entity table.
#[derive(Debug, Clone)]
pub(crate) struct EntityRecord {
    /// Current generation — must match the handle's generation to be live.
    pub generation: u32,
    /// Which archetype this entity's components live in (`None` = free slot).
    pub archetype_id: Option<usize>,
    /// Row within the archetype's table.
    pub row: usize,
}

impl EntityRecord {
    fn free() -> Self {
        EntityRecord {
            generation: 0,
            archetype_id: None,
            row: 0,
        }
    }
}

// ---------------------------------------------------------------------------

/// Manages entity ID allocation and recycling.
///
/// Free slots are stored in a LIFO stack so that recently freed indices get
/// reused quickly, keeping the working-set small.
#[derive(Debug, Default)]
pub struct EntityAllocator {
    records: Vec<EntityRecord>,
    free: Vec<u32>,
}

impl EntityAllocator {
    pub fn new() -> Self {
        Self::default()
    }

    /// Allocate a new live entity and return its handle + mutable record ref.
    pub(crate) fn alloc(&mut self) -> (Entity, usize) {
        if let Some(index) = self.free.pop() {
            let rec = &mut self.records[index as usize];
            debug_assert!(rec.archetype_id.is_none(), "free slot was still live");
            let entity = Entity {
                index,
                generation: rec.generation,
            };
            (entity, index as usize)
        } else {
            let index = self.records.len() as u32;
            self.records.push(EntityRecord::free());
            let entity = Entity {
                index,
                generation: 0,
            };
            (entity, index as usize)
        }
    }

    /// Mark an entity slot as free, incrementing its generation.
    /// Returns `false` if the entity was already dead (stale handle).
    pub(crate) fn free(&mut self, entity: Entity) -> bool {
        let rec = match self.records.get_mut(entity.index as usize) {
            Some(r) => r,
            None => return false,
        };
        if rec.generation != entity.generation {
            return false; // stale / already freed
        }
        rec.generation = rec.generation.wrapping_add(1);
        rec.archetype_id = None;
        self.free.push(entity.index);
        true
    }

    /// Check if an entity handle refers to a currently live slot.
    #[inline]
    pub fn is_alive(&self, entity: Entity) -> bool {
        self.records
            .get(entity.index as usize)
            .map(|r| r.generation == entity.generation && r.archetype_id.is_some())
            .unwrap_or(false)
    }

    /// Total number of slots allocated (live + free).
    #[inline]
    pub fn capacity(&self) -> usize {
        self.records.len()
    }

    /// Number of live entities.
    pub fn len(&self) -> usize {
        self.records.len() - self.free.len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Mutable access to a record — used by World when moving entities
    /// between archetypes.
    #[inline]
    pub(crate) fn get_mut(&mut self, index: usize) -> Option<&mut EntityRecord> {
        self.records.get_mut(index)
    }

    /// Immutable access to a record.
    #[inline]
    pub(crate) fn get(&self, entity: Entity) -> Option<&EntityRecord> {
        let rec = self.records.get(entity.index as usize)?;
        if rec.generation == entity.generation {
            Some(rec)
        } else {
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn alloc_and_free() {
        let mut alloc = EntityAllocator::new();
        let (e0, idx0) = alloc.alloc();
        let (e1, _idx1) = alloc.alloc();
        assert_ne!(e0, e1);

        // Simulate World setting archetype_id so is_alive returns true
        alloc.get_mut(idx0).unwrap().archetype_id = Some(0);

        assert!(alloc.is_alive(e0));
        alloc.free(e0);
        assert!(!alloc.is_alive(e0));
        let (e2, _) = alloc.alloc();
        // slot recycled, but generation bumped → different Entity
        assert_eq!(e2.index, e0.index);
        assert_ne!(e2.generation, e0.generation);
        // old handle is stale
        assert!(!alloc.is_alive(e0));
        // e1 has no archetype_id set so is_alive is false — that's expected
        // for raw allocator use without a World.
        let _ = e1;
    }

    #[test]
    fn bits_roundtrip() {
        let e = Entity {
            index: 42,
            generation: 7,
        };
        assert_eq!(Entity::from_bits(e.to_bits()), e);
    }
}
