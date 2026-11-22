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

#[test]
fn steady_state_input_allocates_nothing() {
    let mut children: Vec<Element<()>> = Vec::with_capacity(1000);
    for index in 0..1000u64 {
        let mut row = Element::row()
            .key_id(index + 1)
            .width(SizeSpec::Fill)
            .height(SizeSpec::Px(20.0))
            .on_click(());
        if index == 0 {
            row = row.key("first");
        } else if index == 1 {
            row = row.key("second");
        }
        children.push(row);
    }
    let view: Element<()> = Element::column()
        .width(SizeSpec::Fill)
        .height(SizeSpec::Fill)
        .children(children);
    let mut ui: Ui<()> = Ui::new(Theme::dark());
    ui.set_viewport(Size::new(1280.0, 800.0), 1.0);
    let mut text = MetricsTextSystem::new();
    let mut frame = Frame::new();
    let mut messages: Vec<()> = Vec::with_capacity(8);
    ui.paint(&view, &mut frame, &mut text);
    let first = ui
        .layout()
        .rect(ui.tree().find_key("first").expect("first"));
    let second = ui
        .layout()
        .rect(ui.tree().find_key("second").expect("second"));
    let center = |rect: Rect| glam::vec2(rect.x + rect.width / 2.0, rect.y + rect.height / 2.0);
    let (a, b) = (center(first), center(second));

    // Warm every cache: two hover targets, the store entries, the index cache
    // and the message buffer.
    for step in 0..8 {
        let point = if step % 2 == 0 { a } else { b };
        ui.push_event(InputEvent::PointerMoved { position: point });
        ui.dispatch_pending(&mut messages);
    }
    messages.clear();

    reset();
    for step in 0..30 {
        let point = if step % 2 == 0 { a } else { b };
        ui.push_event(InputEvent::PointerMoved { position: point });
        ui.dispatch_pending(&mut messages);
        messages.clear();
    }
    let measured_allocations = allocations();
    let measured_bytes = bytes();
    println!(
        "input steady state: {measured_allocations} allocations, {measured_bytes} bytes over 30 events"
    );
    assert_eq!(
        measured_allocations, 0,
        "steady-state input allocated {measured_allocations} times"
    );
    assert_eq!(
        measured_bytes, 0,
        "steady-state input allocated {measured_bytes} bytes"
    );
}
