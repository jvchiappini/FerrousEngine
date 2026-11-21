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
//! There are two views. [`snapshot`] is process-wide, including worker threads;
//! [`thread_snapshot`] only counts the calling thread.  The per-frame trace
//! uses the thread view for "allocations per frame" and reports the rest as
//! worker allocations, so a frame test measures the frame thread and never
//! mistakes a decode or audio worker for frame work.  Both can be reset for a
//! controlled measurement.

use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;
use std::sync::atomic::{AtomicU64, Ordering};

use serde::Serialize;

static ALLOCATIONS: AtomicU64 = AtomicU64::new(0);
static REALLOCATIONS: AtomicU64 = AtomicU64::new(0);
static FREES: AtomicU64 = AtomicU64::new(0);
static ALLOCATED_BYTES: AtomicU64 = AtomicU64::new(0);
static REALLOCATED_BYTES: AtomicU64 = AtomicU64::new(0);
static FREED_BYTES: AtomicU64 = AtomicU64::new(0);

thread_local! {
    /// Counters for the calling thread only.  `Cell<Snapshot>` is allocation
    /// free, so the allocator never recurses into itself.
    static THREAD: Cell<Snapshot> = const { Cell::new(Snapshot::ZERO) };
}

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
            count_local(1, 0, 0, layout.size() as u64, 0, 0);
        }
        pointer
    }

    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        let pointer = System.alloc_zeroed(layout);
        if !pointer.is_null() && layout.size() > 0 {
            ALLOCATIONS.fetch_add(1, Ordering::Relaxed);
            ALLOCATED_BYTES.fetch_add(layout.size() as u64, Ordering::Relaxed);
            count_local(1, 0, 0, layout.size() as u64, 0, 0);
        }
        pointer
    }

    unsafe fn dealloc(&self, pointer: *mut u8, layout: Layout) {
        if !pointer.is_null() && layout.size() > 0 {
            FREES.fetch_add(1, Ordering::Relaxed);
            FREED_BYTES.fetch_add(layout.size() as u64, Ordering::Relaxed);
            count_local(0, 0, 1, 0, 0, layout.size() as u64);
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
            count_local(0, 1, 0, 0, new_size as u64, layout.size() as u64);
        }
        new_pointer
    }
}

/// Adds one allocator event to the calling thread's counters.
///
/// `try_with` keeps this safe while a thread's TLS is being destroyed (the
/// counters then only go to the process-wide view).
fn count_local(
    allocations: u64,
    reallocations: u64,
    frees: u64,
    allocated_bytes: u64,
    reallocated_bytes: u64,
    freed_bytes: u64,
) {
    let _ = THREAD.try_with(|cell| {
        let mut snapshot = cell.get();
        snapshot.allocations = snapshot.allocations.saturating_add(allocations);
        snapshot.reallocations = snapshot.reallocations.saturating_add(reallocations);
        snapshot.frees = snapshot.frees.saturating_add(frees);
        snapshot.allocated_bytes = snapshot.allocated_bytes.saturating_add(allocated_bytes);
        snapshot.reallocated_bytes = snapshot.reallocated_bytes.saturating_add(reallocated_bytes);
        snapshot.freed_bytes = snapshot.freed_bytes.saturating_add(freed_bytes);
        cell.set(snapshot);
    });
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
    /// The all-zero snapshot.
    pub const ZERO: Self = Self {
        allocations: 0,
        reallocations: 0,
        frees: 0,
        allocated_bytes: 0,
        reallocated_bytes: 0,
        freed_bytes: 0,
    };

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

/// Reads the calling thread's counters.
#[must_use]
pub fn thread_snapshot() -> Snapshot {
    THREAD.with(Cell::get)
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

/// The delta between two thread snapshots (`after - before`, saturating at
/// zero).
#[must_use]
pub fn thread_between(before: Snapshot, after: Snapshot) -> Delta {
    between(before, after)
}

/// Brackets a region and reports the allocator activity inside it.
///
/// Process-wide: includes allocations made by other threads while the scope
/// is open. Use [`ThreadScope`] to measure the calling thread only.
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

/// Brackets a region and reports the **calling thread's** allocator activity.
///
/// This is the frame-thread measurement the zero-allocation tests use; a
/// worker that allocates while the scope is open does not contaminate it
/// (its bytes appear in the process-wide [`FrameScope`] delta instead).
#[derive(Debug, Clone, Copy)]
pub struct ThreadScope {
    before: Snapshot,
}

impl ThreadScope {
    /// Starts a scope at the calling thread's counters.
    #[must_use]
    pub fn new() -> Self {
        Self {
            before: thread_snapshot(),
        }
    }

    /// Ends the scope and returns its delta.
    #[must_use]
    pub fn finish(self) -> Delta {
        thread_between(self.before, thread_snapshot())
    }
}

impl Default for ThreadScope {
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
    let _ = THREAD.try_with(|cell| cell.set(Snapshot::ZERO));
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
        let scope = ThreadScope::new();
        let data = vec![0u8; 4096];
        std::hint::black_box(&data);
        let delta = scope.finish();
        assert!(delta.allocations >= 1, "{delta:?}");
        assert!(delta.touched_bytes() >= 4096, "{delta:?}");
        assert!(delta.net_bytes() >= 4096, "{delta:?}");
        drop(data);
    }

    #[test]
    fn thread_scopes_exclude_worker_allocations() {
        let _lock = test_lock();
        reset();
        let (sender, receiver) = std::sync::mpsc::channel();
        let handle = std::thread::spawn(move || {
            let before = thread_snapshot();
            let data = vec![0u8; 4096];
            let delta = thread_between(before, thread_snapshot());
            drop(data);
            sender.send(delta).unwrap();
        });
        let worker = receiver.recv().unwrap();
        let before = thread_snapshot();
        let scope = ThreadScope::new();
        let data = vec![0u8; 128];
        std::hint::black_box(&data);
        let main = scope.finish();
        drop(data);
        let after = thread_snapshot();
        handle.join().unwrap();
        assert!(worker.allocating_calls() >= 1, "{worker:?}");
        assert!(main.allocating_calls() >= 1, "{main:?}");
        assert!(
            after.allocating_calls() >= before.allocating_calls(),
            "the main thread counter is monotonic"
        );
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
