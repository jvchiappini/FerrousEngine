//! Scene picking benches (prompt 15 phase 4B).
//!
//! Run:
//! ```text
//! cargo bench -p ferrous_2d --bench picking
//! ```
//!
//! The budgets are a point pick under 20 µs and a marquee over 10 000 shapes
//! under 1 ms. The id-buffer reference runs the same scene one query per pixel
//! and shows why an on-demand GPU pass is not the per-move path.

#![allow(
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::doc_markdown
)] // reason: bench fixtures use grid coordinates and names

use divan::counter::ItemsCount;
use divan::AllocProfiler;
use ferrous_2d::picking::{
    Aabb, IdBuffer, MarqueeMode, PickCamera, PickItem, PickScene, PickShape,
};
use glam::{Affine2, Vec2};

#[global_allocator]
static ALLOC: AllocProfiler = AllocProfiler::system();

fn main() {
    divan::main();
}

fn scene(count: usize) -> PickScene {
    let mut scene = PickScene::new();
    for index in 0..count {
        let x = (index % 100) as f32 * 16.0;
        let y = (index / 100) as f32 * 16.0;
        let shape = match index % 3 {
            0 => PickShape::rect(Vec2::new(14.0, 14.0)),
            1 => PickShape::circle(7.0),
            _ => PickShape::rounded_rect(Vec2::new(14.0, 14.0), 3.0),
        };
        let transform = if index % 10 == 0 {
            Affine2::from_angle_translation(0.3, Vec2::new(x, y))
        } else {
            Affine2::from_translation(Vec2::new(x, y))
        };
        scene.add(PickItem::new(index as u32, shape).transform(transform));
    }
    scene.rebuild();
    scene
}

fn points(count: usize, scale: f32) -> Vec<Vec2> {
    let mut state = 0x1234_5678_9abc_def0_u64;
    let mut output = Vec::with_capacity(count);
    for _ in 0..count {
        state = state
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1);
        let x = (state % 1_600_000) as f32 / 1000.0 * scale;
        state = state
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1);
        let y = (state % 1_600_000) as f32 / 1000.0 * scale;
        output.push(Vec2::new(x, y));
    }
    output
}

#[divan::bench]
fn build_index_10k(bencher: divan::Bencher) {
    let mut scene = scene(10_000);
    bencher.bench_local(|| {
        scene.mark_dirty();
        scene.rebuild();
        divan::black_box(scene.rebuilds())
    });
}

#[divan::bench]
fn point_pick_10k(bencher: divan::Bencher) {
    let mut scene = scene(10_000);
    let queries = points(10_000, 1.0);
    bencher
        .counter(ItemsCount::new(queries.len()))
        .bench_local(|| {
            let mut hits = 0usize;
            for point in &queries {
                if scene.pick(*point, 0.0).is_some() {
                    hits += 1;
                }
            }
            divan::black_box(hits)
        });
}

#[divan::bench]
fn point_pick_100k(bencher: divan::Bencher) {
    let mut scene = scene(100_000);
    let queries = points(10_000, 3.0);
    bencher
        .counter(ItemsCount::new(queries.len()))
        .bench_local(|| {
            let mut hits = 0usize;
            for point in &queries {
                if scene.pick(*point, 0.0).is_some() {
                    hits += 1;
                }
            }
            divan::black_box(hits)
        });
}

#[divan::bench]
fn marquee_10k(bencher: divan::Bencher) {
    let mut scene = scene(10_000);
    let area = Aabb {
        min: Vec2::new(100.0, 100.0),
        max: Vec2::new(300.0, 300.0),
    };
    let mut out = Vec::with_capacity(256);
    bencher.bench_local(|| {
        scene.marquee(area, MarqueeMode::Intersect, 0.0, &mut out);
        divan::black_box(out.len())
    });
}

#[divan::bench]
fn marquee_contain_10k(bencher: divan::Bencher) {
    let mut scene = scene(10_000);
    let area = Aabb {
        min: Vec2::ZERO,
        max: Vec2::new(800.0, 800.0),
    };
    let mut out = Vec::with_capacity(1024);
    bencher.bench_local(|| {
        scene.marquee(area, MarqueeMode::Contain, 0.0, &mut out);
        divan::black_box(out.len())
    });
}

#[divan::bench]
fn lasso_10k(bencher: divan::Bencher) {
    let mut scene = scene(10_000);
    let lasso = vec![
        Vec2::new(100.0, 100.0),
        Vec2::new(400.0, 120.0),
        Vec2::new(380.0, 400.0),
        Vec2::new(120.0, 360.0),
    ];
    let mut out = Vec::with_capacity(256);
    bencher.bench_local(|| {
        scene.lasso(&lasso, MarqueeMode::Intersect, 0.0, &mut out);
        divan::black_box(out.len())
    });
}

#[divan::bench]
fn nearest_point_10k(bencher: divan::Bencher) {
    let mut scene = scene(10_000);
    let queries = points(1_000, 1.0);
    bencher.bench_local(|| {
        let mut found = 0usize;
        for point in &queries {
            if scene.nearest_point(*point, 8.0).is_some() {
                found += 1;
            }
        }
        divan::black_box(found)
    });
}

/// The CPU reference for an id-buffer pass: one pick per pixel.
#[divan::bench(args = [64, 128])]
fn id_buffer_reference(bencher: divan::Bencher, size: u32) {
    let mut scene = scene(10_000);
    bencher.bench_local(|| {
        let buffer = IdBuffer::rasterize(&mut scene, size, size);
        divan::black_box(buffer.iter().filter(|id| **id != u32::MAX).count())
    });
}

/// Screen-to-world mapping used by a host before a pick.
#[divan::bench]
fn camera_mapping(bencher: divan::Bencher) {
    let camera = PickCamera {
        center: Vec2::new(400.0, 400.0),
        zoom: 1.5,
        viewport: Vec2::new(1280.0, 800.0),
    };
    bencher.bench_local(|| {
        let mut checksum = 0.0_f32;
        for step in 0..1000 {
            let screen = Vec2::new(step as f32 * 0.5, step as f32 * 0.25);
            checksum += camera.screen_to_world(screen).x;
        }
        divan::black_box(checksum)
    });
}
