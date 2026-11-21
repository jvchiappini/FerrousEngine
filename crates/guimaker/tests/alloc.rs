//! Steady-state allocation test for the `GUIMaker` core.
//!
//! The budget (prompt 15) is **zero heap allocations per frame** after
//! warm-up. This test installs a counting allocator in its own test binary,
//! paints a still start screen enough times to fill every cache, then measures
//! ten frames with no input and no view change.

use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::atomic::{AtomicUsize, Ordering};

use guimaker::prelude::*;

struct Counting;

static ALLOCATIONS: AtomicUsize = AtomicUsize::new(0);
static BYTES: AtomicUsize = AtomicUsize::new(0);

unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        ALLOCATIONS.fetch_add(1, Ordering::Relaxed);
        BYTES.fetch_add(layout.size(), Ordering::Relaxed);
        System.alloc(layout)
    }

    unsafe fn dealloc(&self, pointer: *mut u8, layout: Layout) {
        System.dealloc(pointer, layout);
    }

    unsafe fn realloc(&self, pointer: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        ALLOCATIONS.fetch_add(1, Ordering::Relaxed);
        BYTES.fetch_add(new_size, Ordering::Relaxed);
        System.realloc(pointer, layout, new_size)
    }
}

#[global_allocator]
static GLOBAL: Counting = Counting;

fn reset() {
    ALLOCATIONS.store(0, Ordering::Relaxed);
    BYTES.store(0, Ordering::Relaxed);
}

fn allocations() -> usize {
    ALLOCATIONS.load(Ordering::Relaxed)
}

fn bytes() -> usize {
    BYTES.load(Ordering::Relaxed)
}

#[test]
fn steady_state_allocates_nothing() {
    let view = start_screen();
    let mut ui: Ui<StartMsg> = Ui::new(Theme::dark());
    ui.set_viewport(Size::new(1920.0, 1080.0), 1.5);
    let mut text = MetricsTextSystem::new();
    let mut frame = Frame::new();
    for _ in 0..3 {
        ui.paint(&view, &mut frame, &mut text);
    }

    reset();
    for _ in 0..10 {
        ui.paint(&view, &mut frame, &mut text);
        assert!(
            !frame.stats.repainted,
            "a still frame rebuilt the display list"
        );
        assert!(frame.display_list.damage.is_empty());
    }
    let measured_allocations = allocations();
    let measured_bytes = bytes();
    println!(
        "steady state: {measured_allocations} allocations, {measured_bytes} bytes over 10 frames"
    );
    assert_eq!(
        measured_allocations, 0,
        "steady state allocated {measured_allocations} times"
    );
    assert_eq!(
        measured_bytes, 0,
        "steady state allocated {measured_bytes} bytes"
    );
}
