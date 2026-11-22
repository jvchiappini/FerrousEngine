//! Steady-state allocation test for scene picking.
//!
//! The budget is zero allocations per query with a reused result buffer. The
//! test installs a counting allocator, builds and warms a 10 000-shape scene,
//! then runs point, marquee and nearest queries and asserts that nothing was
//! allocated.

use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::atomic::{AtomicUsize, Ordering};

use ferrous_2d::picking::{Aabb, MarqueeMode, PickHit, PickItem, PickScene, PickShape};
use glam::{Affine2, Vec2};

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

fn scene() -> PickScene {
    let mut scene = PickScene::new();
    for index in 0..10_000u32 {
        let x = (index % 100) as f32 * 16.0;
        let y = (index / 100) as f32 * 16.0;
        let shape = match index % 3 {
            0 => PickShape::rect(Vec2::new(14.0, 14.0)),
            1 => PickShape::circle(7.0),
            _ => PickShape::rounded_rect(Vec2::new(14.0, 14.0), 3.0),
        };
        let transform = Affine2::from_translation(Vec2::new(x, y));
        scene.add(PickItem::new(index, shape).transform(transform));
    }
    scene.rebuild();
    scene
}

#[test]
fn warm_queries_allocate_nothing() {
    let mut scene = scene();
    let mut hits: Vec<PickHit> = Vec::with_capacity(256);
    let area = Aabb {
        min: Vec2::new(100.0, 100.0),
        max: Vec2::new(300.0, 300.0),
    };
    // Warm the index, the candidate buffer and the result buffer.
    for step in 0..4 {
        let point = Vec2::new(step as f32 * 40.0 + 1.0, 1.0);
        let _ = scene.pick(point, 0.0);
        let _ = scene.nearest_point(point, 8.0);
        scene.marquee(area, MarqueeMode::Intersect, 0.0, &mut hits);
    }
    hits.clear();
    ALLOCATIONS.store(0, Ordering::Relaxed);
    BYTES.store(0, Ordering::Relaxed);
    for step in 0..30 {
        let point = Vec2::new(
            (step % 80) as f32 * 16.0 + 1.0,
            (step % 40) as f32 * 16.0 + 1.0,
        );
        let _ = scene.pick(point, 0.0);
        let _ = scene.nearest_point(point, 8.0);
        scene.marquee(area, MarqueeMode::Intersect, 0.0, &mut hits);
    }
    let allocations = ALLOCATIONS.load(Ordering::Relaxed);
    let bytes = BYTES.load(Ordering::Relaxed);
    println!("picking steady state: {allocations} allocations, {bytes} bytes over 90 queries");
    assert_eq!(
        allocations, 0,
        "steady-state picking allocated {allocations} times"
    );
    assert_eq!(bytes, 0, "steady-state picking allocated {bytes} bytes");
}
