use core::hash::Hash;
use alloc::vec::Vec;

/// An agnostic reactivity system that just accumulates dirty subscriber IDs.
pub struct ReactivitySystem<S> {
    pending_dirty_nodes: Vec<S>,
}

impl<S: Eq + Hash + Clone> Default for ReactivitySystem<S> {
    fn default() -> Self {
        Self::new()
    }
}

impl<S: Eq + Hash + Clone> ReactivitySystem<S> {
    pub fn new() -> Self {
        Self {
            pending_dirty_nodes: Vec::new(),
        }
    }

    pub fn notify_change(&mut self, nodes: Vec<S>) {
        // Can add logic to avoid duplicates if required
        self.pending_dirty_nodes.extend(nodes);
    }

    /// Drains pending dirty node IDs and applies a provided marking function
    pub fn apply<F>(&mut self, mut mark_dirty: F)
    where
        F: FnMut(S),
    {
        for id in self.pending_dirty_nodes.drain(..) {
            mark_dirty(id);
        }
    }
}
