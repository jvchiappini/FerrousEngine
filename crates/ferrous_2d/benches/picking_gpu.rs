//! GPU id-buffer picking bench (prompt 15 phase 5).
//!
//! Run:
//! ```text
//! cargo bench -p ferrous_2d --bench picking_gpu
//! ```
//!
//! It prints the GPU submit + map + readback cost next to the CPU budgets from
//! the Phase 4B report (point pick 61 ns/query, marquee 5.4 us, contain 93 us,
//! lasso 14.9 us, nearest 174 ns at 10 000 shapes). The GPU target is tiny for
//! a point pick and region-sized for a marquee; every query is one render pass
//! plus a blocking readback, so the numbers answer "when does one pass beat
//! many indexed queries", not "is the GPU faster per pointer move".

#![allow(
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss
)] // reason: bench fixtures use grid coordinates

use std::time::{Duration, Instant};

use ferrous_2d::picking::{Aabb, IdBuffer, MarqueeMode, PickItem, PickScene, PickShape};
use glam::{Affine2, Vec2};

fn device() -> Option<(wgpu::Device, wgpu::Queue)> {
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
        backends: wgpu::Backends::all(),
        ..Default::default()
    });
    let options = wgpu::RequestAdapterOptions {
        power_preference: wgpu::PowerPreference::HighPerformance,
        compatible_surface: None,
        force_fallback_adapter: false,
    };
    let adapter = pollster::block_on(instance.request_adapter(&options)).or_else(|| {
        let fallback = wgpu::RequestAdapterOptions {
            force_fallback_adapter: true,
            ..options
        };
        pollster::block_on(instance.request_adapter(&fallback))
    })?;
    let descriptor = wgpu::DeviceDescriptor {
        label: Some("2D GPU picking bench"),
        required_features: wgpu::Features::empty(),
        required_limits: adapter.limits(),
        memory_hints: wgpu::MemoryHints::MemoryUsage,
    };
    pollster::block_on(adapter.request_device(&descriptor, None))
        .ok()
        .map(|(device, queue)| {
            println!("adapter: {}", adapter.get_info().name);
            (device, queue)
        })
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
        scene.add(PickItem::new(index as u32 + 1, shape).transform(transform));
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

fn median(samples: &mut [Duration]) -> Duration {
    samples.sort_unstable();
    samples[samples.len() / 2]
}

fn micros(duration: Duration) -> f64 {
    duration.as_secs_f64() * 1.0e6
}

fn main() {
    let Some((device, queue)) = device() else {
        println!("[skip] no GPU adapter; the CPU index is the documented fallback");
        return;
    };
    let mut picker = ferrous_2d::picking::GpuPicker::new(&device);
    let mut scene = scene(10_000);
    let queries = points(10_000, 1.0);
    let mut hits = Vec::with_capacity(256);

    for point in queries.iter().take(32) {
        let _ = picker.pick_gpu(&device, &queue, &scene, *point, 0.0);
    }

    let mut samples = Vec::with_capacity(10);
    for _ in 0..10 {
        let start = Instant::now();
        let mut found = 0usize;
        for point in &queries {
            if scene.pick(*point, 0.0).is_some() {
                found += 1;
            }
        }
        std::hint::black_box(found);
        samples.push(start.elapsed() / queries.len() as u32);
    }
    println!(
        "cpu point pick:            {:>8.1} ns/query",
        micros(median(&mut samples)) * 1000.0
    );

    let mut samples = Vec::with_capacity(40);
    for _ in 0..40 {
        let start = Instant::now();
        for point in queries.iter().take(8) {
            std::hint::black_box(
                picker
                    .pick_gpu(&device, &queue, &scene, *point, 0.0)
                    .expect("gpu pick"),
            );
        }
        samples.push(start.elapsed() / 8);
    }
    println!(
        "gpu point pick:            {:>8.1} us/query",
        micros(median(&mut samples))
    );

    let area = Aabb {
        min: Vec2::new(100.0, 100.0),
        max: Vec2::new(300.0, 300.0),
    };
    let mut samples = Vec::with_capacity(40);
    for _ in 0..40 {
        let start = Instant::now();
        scene.marquee(area, MarqueeMode::Intersect, 0.0, &mut hits);
        std::hint::black_box(hits.len());
        samples.push(start.elapsed());
    }
    println!(
        "cpu marquee 10k:           {:>8.1} us",
        micros(median(&mut samples))
    );

    let contain_area = Aabb {
        min: Vec2::ZERO,
        max: Vec2::new(800.0, 800.0),
    };
    let mut samples = Vec::with_capacity(20);
    for _ in 0..20 {
        let start = Instant::now();
        scene.marquee(contain_area, MarqueeMode::Contain, 0.0, &mut hits);
        std::hint::black_box(hits.len());
        samples.push(start.elapsed());
    }
    println!(
        "cpu contain 10k:           {:>8.1} us",
        micros(median(&mut samples))
    );

    let lasso = [
        Vec2::new(100.0, 100.0),
        Vec2::new(400.0, 120.0),
        Vec2::new(380.0, 400.0),
        Vec2::new(120.0, 360.0),
    ];
    let mut samples = Vec::with_capacity(20);
    for _ in 0..20 {
        let start = Instant::now();
        scene.lasso(&lasso, MarqueeMode::Intersect, 0.0, &mut hits);
        std::hint::black_box(hits.len());
        samples.push(start.elapsed());
    }
    println!(
        "cpu lasso 10k:             {:>8.1} us",
        micros(median(&mut samples))
    );

    let mut samples = Vec::with_capacity(10);
    for _ in 0..10 {
        let start = Instant::now();
        let mut found = 0usize;
        for point in queries.iter().take(1000) {
            if scene.nearest_point(*point, 8.0).is_some() {
                found += 1;
            }
        }
        std::hint::black_box(found);
        samples.push(start.elapsed() / 1000);
    }
    println!(
        "cpu nearest 10k:           {:>8.1} ns/query",
        micros(median(&mut samples)) * 1000.0
    );

    let mut samples = Vec::with_capacity(20);
    for _ in 0..20 {
        let start = Instant::now();
        picker
            .region_gpu(&device, &queue, &scene, area, 0.0, &mut hits)
            .expect("gpu region");
        std::hint::black_box(hits.len());
        samples.push(start.elapsed());
    }
    println!(
        "gpu region 200x200:        {:>8.1} us",
        micros(median(&mut samples))
    );

    for size in [64u32, 128u32, 256u32, 512u32, 1024u32] {
        let small = Aabb {
            min: Vec2::new(100.0, 100.0),
            max: Vec2::new(100.0 + size as f32, 100.0 + size as f32),
        };
        let mut samples = Vec::with_capacity(20);
        for _ in 0..20 {
            let start = Instant::now();
            let buffer = IdBuffer::rasterize(&mut scene, size, size);
            std::hint::black_box(buffer.iter().filter(|id| **id != u32::MAX).count());
            samples.push(start.elapsed());
        }
        println!(
            "cpu id reference {size}x{size}:  {:>8.1} us",
            micros(median(&mut samples))
        );

        let mut samples = Vec::with_capacity(20);
        for _ in 0..20 {
            let start = Instant::now();
            picker
                .region_gpu(&device, &queue, &scene, small, 0.0, &mut hits)
                .expect("gpu region");
            std::hint::black_box(hits.len());
            samples.push(start.elapsed());
        }
        println!(
            "gpu id region {size}x{size}:    {:>8.1} us",
            micros(median(&mut samples))
        );
    }
}
