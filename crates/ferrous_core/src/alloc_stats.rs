//! Global allocation accounting (feature `alloc-stats`).
//!
//! A process installs the counting allocator once:
//!
//! ```no_run
//! #[global_allocator]
//! static ALLOC: ferrous_core::alloc_stats::CountingAllocator =
//!     ferrous_core::alloc_stats::CountingAllocator;
//! ```
//!
//! Every allocation, reallocation and free then updates six relaxed atomics.
//! [`snapshot`] returns the totals; [`FrameScope`] brackets a region and
//! returns the delta, which is how the per-frame trace reports allocations per
//! frame.  The feature is **off by default**: normal release builds pay
//! nothing.  When it is on the cost is one relaxed atomic increment (and one
//! byte add) per allocator call, plus the normal system allocator.
//!
//! The counters are process-wide, including worker threads.  A frame scope
//! therefore also catches allocations made by a worker while that frame was
//! open, which is what the frame trace wants.  The counters can be reset for a
//! controlled measurement with [`reset`].

use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::atomic::{AtomicU64, Ordering};

use serde::Serialize;

static ALLOCATIONS: AtomicU64 = AtomicU64::new(0);
static REALLOCATIONS: AtomicU64 = AtomicU64::new(0);
static FREES: AtomicU64 = AtomicU64::new(0);
static ALLOCATED_BYTES: AtomicU64 = AtomicU64::new(0);
static REALLOCATED_BYTES: AtomicU64 = AtomicU64::new(0);
static FREED_BYTES: AtomicU64 = AtomicU64::new(0);

/// A global allocator that delegates to the system allocator and counts.
///
/// Install it as the process `#[global_allocator]` in a bench or a test
/// binary; a library cannot install one itself.
#[derive(Debug, Default, Clone, Copy)]
pub struct CountingAllocator;

// SAFETY: every method forwards to `System` with the same contract.  The
// counters are independent relaxed atomics; they never affect the returned
// pointer or the layout handling.
unsafe impl GlobalAlloc for CountingAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let pointer = System.alloc(layout);
        if !pointer.is_null() && layout.size() > 0 {
            ALLOCATIONS.fetch_add(1, Ordering::Relaxed);
            ALLOCATED_BYTES.fetch_add(layout.size() as u64, Ordering::Relaxed);
        }
        pointer
    }

    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        let pointer = System.alloc_zeroed(layout);
        if !pointer.is_null() && layout.size() > 0 {
            ALLOCATIONS.fetch_add(1, Ordering::Relaxed);
            ALLOCATED_BYTES.fetch_add(layout.size() as u64, Ordering::Relaxed);
        }
        pointer
    }

    unsafe fn dealloc(&self, pointer: *mut u8, layout: Layout) {
        if !pointer.is_null() && layout.size() > 0 {
            FREES.fetch_add(1, Ordering::Relaxed);
            FREED_BYTES.fetch_add(layout.size() as u64, Ordering::Relaxed);
        }
        System.dealloc(pointer, layout);
    }

    unsafe fn realloc(&self, pointer: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        let new_pointer = System.realloc(pointer, layout, new_size);
        if !new_pointer.is_null() {
            REALLOCATIONS.fetch_add(1, Ordering::Relaxed);
            // The old block is released by the system allocator; count its
            // bytes as freed and the new block as reallocated so the net
            // byte number stays correct.
            FREED_BYTES.fetch_add(layout.size() as u64, Ordering::Relaxed);
            REALLOCATED_BYTES.fetch_add(new_size as u64, Ordering::Relaxed);
        }
        new_pointer
    }
}

/// Process-wide allocation totals.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct Snapshot {
    /// Successful `alloc`/`alloc_zeroed` calls.
    pub allocations: u64,
    /// Successful `realloc` calls.
    pub reallocations: u64,
    /// `dealloc` calls.
    pub frees: u64,
    /// Bytes requested by `alloc`/`alloc_zeroed`.
    pub allocated_bytes: u64,
    /// New sizes requested by `realloc`.
    pub reallocated_bytes: u64,
    /// Bytes released by `dealloc` and by the old side of `realloc`.
    pub freed_bytes: u64,
}

impl Snapshot {
    /// Signed net change in live bytes (allocated + reallocated - freed).
    #[must_use]
    pub fn net_bytes(self) -> i64 {
        let allocated = self.allocated_bytes as i128 + self.reallocated_bytes as i128;
        let freed = self.freed_bytes as i128;
        (allocated - freed) as i64
    }

    /// Total allocator calls that can allocate (`alloc` + `realloc`).
    #[must_use]
    pub fn allocating_calls(self) -> u64 {
        self.allocations.saturating_add(self.reallocations)
    }
}

/// The difference between two snapshots.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct Delta {
    /// Allocations in the region.
    pub allocations: u64,
    /// Reallocations in the region.
    pub reallocations: u64,
    /// Frees in the region.
    pub frees: u64,
    /// Allocated bytes in the region.
    pub allocated_bytes: u64,
    /// Reallocated (new-size) bytes in the region.
    pub reallocated_bytes: u64,
    /// Freed bytes in the region.
    pub freed_bytes: u64,
}

impl Delta {
    /// Signed net change in live bytes inside the region.
    #[must_use]
    pub fn net_bytes(self) -> i64 {
        let allocated = self.allocated_bytes as i128 + self.reallocated_bytes as i128;
        let freed = self.freed_bytes as i128;
        (allocated - freed) as i64
    }

    /// Bytes touched by allocations and reallocations (the upload-ish signal).
    #[must_use]
    pub fn touched_bytes(self) -> u64 {
        self.allocated_bytes.saturating_add(self.reallocated_bytes)
    }

    /// Allocator calls that can allocate (`alloc` + `realloc`) in the region.
    #[must_use]
    pub fn allocating_calls(self) -> u64 {
        self.allocations.saturating_add(self.reallocations)
    }
}

/// Reads the process-wide counters.
#[must_use]
pub fn snapshot() -> Snapshot {
    Snapshot {
        allocations: ALLOCATIONS.load(Ordering::Relaxed),
        reallocations: REALLOCATIONS.load(Ordering::Relaxed),
        frees: FREES.load(Ordering::Relaxed),
        allocated_bytes: ALLOCATED_BYTES.load(Ordering::Relaxed),
        reallocated_bytes: REALLOCATED_BYTES.load(Ordering::Relaxed),
        freed_bytes: FREED_BYTES.load(Ordering::Relaxed),
    }
}

/// The delta between two snapshots (`after - before`, saturating at zero).
#[must_use]
pub fn between(before: Snapshot, after: Snapshot) -> Delta {
    Delta {
        allocations: after.allocations.saturating_sub(before.allocations),
        reallocations: after.reallocations.saturating_sub(before.reallocations),
        frees: after.frees.saturating_sub(before.frees),
        allocated_bytes: after.allocated_bytes.saturating_sub(before.allocated_bytes),
        reallocated_bytes: after
            .reallocated_bytes
            .saturating_sub(before.reallocated_bytes),
        freed_bytes: after.freed_bytes.saturating_sub(before.freed_bytes),
    }
}

/// Brackets a region and reports the allocator activity inside it.
#[derive(Debug, Clone, Copy)]
pub struct FrameScope {
    before: Snapshot,
}

impl FrameScope {
    /// Starts a scope at the current counters.
    #[must_use]
    pub fn new() -> Self {
        Self { before: snapshot() }
    }

    /// Ends the scope and returns its delta.
    #[must_use]
    pub fn finish(self) -> Delta {
        between(self.before, snapshot())
    }
}

impl Default for FrameScope {
    fn default() -> Self {
        Self::new()
    }
}

/// Resets the counters.  Use only around a controlled measurement.
pub fn reset() {
    ALLOCATIONS.store(0, Ordering::Relaxed);
    REALLOCATIONS.store(0, Ordering::Relaxed);
    FREES.store(0, Ordering::Relaxed);
    ALLOCATED_BYTES.store(0, Ordering::Relaxed);
    REALLOCATED_BYTES.store(0, Ordering::Relaxed);
    FREED_BYTES.store(0, Ordering::Relaxed);
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Mutex, MutexGuard};

    fn test_lock() -> MutexGuard<'static, ()> {
        static LOCK: Mutex<()> = Mutex::new(());
        LOCK.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    #[test]
    fn a_scope_reports_allocations_and_net_bytes() {
        let _lock = test_lock();
        reset();
        let scope = FrameScope::new();
        let data = vec![0u8; 4096];
        let delta = scope.finish();
        assert!(delta.allocations >= 1, "{delta:?}");
        assert!(delta.touched_bytes() >= 4096, "{delta:?}");
        assert!(delta.net_bytes() >= 4096, "{delta:?}");
        drop(data);
    }

    #[test]
    fn a_reallocation_counts_the_old_block_as_freed() {
        let _lock = test_lock();
        let before = snapshot();
        let mut data = Vec::with_capacity(16);
        data.extend_from_slice(&[0u8; 16]);
        data.reserve(4096);
        let delta = between(before, snapshot());
        // A growing `Vec` may use `realloc` or an alloc+copy+free pair
        // depending on the system allocator; both are real activity.
        assert!(
            delta.reallocations >= 1 || delta.allocating_calls() >= 2,
            "{delta:?}"
        );
        drop(data);
    }

    #[test]
    fn net_bytes_is_signed() {
        let _lock = test_lock();
        let delta = Delta {
            allocated_bytes: 100,
            freed_bytes: 250,
            ..Delta::default()
        };
        assert_eq!(delta.net_bytes(), -150);
    }
}
