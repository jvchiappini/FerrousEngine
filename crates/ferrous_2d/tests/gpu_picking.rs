//! GPU vs CPU picking parity for the 2D scene.
//!
//! Every test creates a windowless device (hardware, falling back to the
//! software adapter) and compares [`GpuPicker`] against [`PickScene`] on the
//! same scene. When no adapter is available the helper prints the reason and
//! the test returns early, so the suite stays green on CPU-only runners.

use std::sync::{Mutex, PoisonError};

use ferrous_2d::picking::{
    Aabb, FillRule, GpuPicker, IdBuffer, MarqueeMode, PickHit, PickItem, PickScene, PickShape,
    PickStroke, StrokeCap,
};
use glam::{Affine2, Vec2};

static GPU_LOCK: Mutex<()> = Mutex::new(());

fn device() -> Option<(wgpu::Device, wgpu::Queue)> {
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
        backends: wgpu::Backends::all(),
        ..Default::default()
    });
    let options = wgpu::RequestAdapterOptions {
        power_preference: wgpu::PowerPreference::LowPower,
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
        label: Some("2D GPU picking test"),
        required_features: wgpu::Features::empty(),
        required_limits: adapter.limits(),
        memory_hints: wgpu::MemoryHints::MemoryUsage,
    };
    pollster::block_on(adapter.request_device(&descriptor, None))
        .ok()
        .map(|(device, queue)| {
            println!("[gpu-picking] adapter: {}", adapter.get_info().name);
            (device, queue)
        })
}

fn translation(x: f32, y: f32) -> Affine2 {
    Affine2::from_translation(Vec2::new(x, y))
}

fn grid_scene() -> PickScene {
    let mut scene = PickScene::new();
    scene.add(
        PickItem::new(1, PickShape::rect(Vec2::new(40.0, 30.0))).transform(translation(30.5, 25.5)),
    );
    scene.add(
        PickItem::new(2, PickShape::rect(Vec2::new(24.0, 24.0))).transform(translation(70.5, 30.5)),
    );
    scene.add(
        PickItem::new(
            3,
            PickShape::Text {
                bounds: Aabb {
                    min: Vec2::splat(-10.0),
                    max: Vec2::splat(10.0),
                },
            },
        )
        .transform(translation(110.5, 30.5)),
    );
    scene.add(
        PickItem::new(
            4,
            PickShape::Image {
                bounds: Aabb {
                    min: Vec2::new(-12.0, -8.0),
                    max: Vec2::new(12.0, 8.0),
                },
                alpha: None,
            },
        )
        .transform(translation(150.5, 30.5)),
    );
    scene.add(
        PickItem::new(5, PickShape::rect(Vec2::new(30.0, 20.0))).transform(translation(30.5, 80.5)),
    );
    scene.rebuild();
    scene
}

fn pick_cpu(scene: &mut PickScene, point: Vec2) -> Option<PickHit> {
    scene.pick(point, 0.0)
}

fn compare_grid(
    scene: &mut PickScene,
    picker: &mut GpuPicker,
    device: &wgpu::Device,
    queue: &wgpu::Queue,
) {
    let mut mismatches = Vec::new();
    for y in 0..110 {
        for x in 0..180 {
            let point = Vec2::new(x as f32 + 0.25, y as f32 + 0.25);
            let cpu = pick_cpu(scene, point).map(|hit| hit.id);
            let gpu = picker
                .pick_gpu(device, queue, scene, point, 0.0)
                .expect("gpu pick")
                .map(|hit| hit.id);
            if cpu != gpu {
                mismatches.push((point, cpu, gpu));
                if mismatches.len() > 8 {
                    break;
                }
            }
        }
        if mismatches.len() > 8 {
            break;
        }
    }
    assert!(
        mismatches.is_empty(),
        "GPU and CPU disagree at {:?}",
        mismatches
    );
}

#[test]
fn gpu_point_pick_matches_cpu_on_a_mixed_grid() {
    let _guard = GPU_LOCK.lock().unwrap_or_else(PoisonError::into_inner);
    let Some((device, queue)) = device() else {
        println!("[skip] no GPU adapter for the 2D picking test");
        return;
    };
    let mut picker = GpuPicker::new(&device);
    let mut scene = grid_scene();
    compare_grid(&mut scene, &mut picker, &device, &queue);
    assert_eq!(picker.target_size(), (1, 1));
    assert_eq!(picker.id_texture_bytes(), 4);
}

#[test]
fn gpu_circles_rounded_rects_and_paths_match_cpu() {
    let _guard = GPU_LOCK.lock().unwrap_or_else(PoisonError::into_inner);
    let Some((device, queue)) = device() else {
        return;
    };
    let mut picker = GpuPicker::new(&device);
    let mut scene = PickScene::new();
    scene.add(PickItem::new(40, PickShape::circle(12.0)).transform(translation(80.5, 80.5)));
    scene.add(
        PickItem::new(41, PickShape::rounded_rect(Vec2::splat(40.0), 10.0))
            .transform(translation(120.5, 120.5)),
    );
    scene.add(
        PickItem::new(
            42,
            PickShape::polygon(vec![
                Vec2::new(-10.0, -10.0),
                Vec2::new(10.0, -10.0),
                Vec2::new(10.0, 10.0),
                Vec2::new(-10.0, 10.0),
            ]),
        )
        .transform(translation(50.5, 150.5)),
    );
    scene.add(
        PickItem::new(
            43,
            PickShape::Path {
                points: vec![Vec2::new(-20.0, 0.0), Vec2::new(20.0, 0.0)],
                closed: false,
                filled: false,
                fill_rule: FillRule::NonZero,
                stroke: Some(PickStroke {
                    width: 4.0,
                    cap: StrokeCap::Round,
                }),
            },
        )
        .transform(translation(110.5, 150.5)),
    );
    let mut points = vec![
        (80.5, 80.5),
        (91.75, 80.5),
        (93.25, 80.5),
        (88.5, 88.5),
        (88.5, 88.75),
        (120.5, 120.5),
        (139.75, 120.5),
        (139.75, 139.75),
        (134.5, 134.5),
        (50.5, 150.5),
        (58.25, 148.25),
        (62.5, 150.5),
        (110.5, 150.5),
        (110.5, 153.25),
        (110.5, 154.5),
    ];
    for (x, y) in points.drain(..) {
        let point = Vec2::new(x, y);
        let cpu = pick_cpu(&mut scene, point).map(|hit| hit.id);
        let gpu = picker
            .pick_gpu(&device, &queue, &scene, point, 0.0)
            .expect("gpu pick")
            .map(|hit| hit.id);
        assert_eq!(cpu, gpu, "GPU and CPU disagree at {point:?}");
    }
}

#[test]
fn gpu_respects_even_odd_holes() {
    let _guard = GPU_LOCK.lock().unwrap_or_else(PoisonError::into_inner);
    let Some((device, queue)) = device() else {
        return;
    };
    let mut picker = GpuPicker::new(&device);
    let mut scene = PickScene::new();
    let mut points = vec![
        Vec2::new(-15.0, -15.0),
        Vec2::new(15.0, -15.0),
        Vec2::new(15.0, 15.0),
        Vec2::new(-15.0, 15.0),
    ];
    points.extend_from_slice(&[
        Vec2::new(-5.0, -5.0),
        Vec2::new(5.0, -5.0),
        Vec2::new(5.0, 5.0),
        Vec2::new(-5.0, 5.0),
    ]);
    scene.add(
        PickItem::new(
            50,
            PickShape::Path {
                points,
                closed: true,
                filled: true,
                fill_rule: FillRule::EvenOdd,
                stroke: None,
            },
        )
        .transform(translation(160.5, 150.5)),
    );
    for offset in [
        Vec2::ZERO,
        Vec2::new(7.25, 0.0),
        Vec2::new(0.0, -7.25),
        Vec2::new(14.25, 14.25),
        Vec2::new(16.0, 0.0),
    ] {
        let point = Vec2::new(160.5, 150.5) + offset;
        let cpu = pick_cpu(&mut scene, point).map(|hit| hit.id);
        let gpu = picker
            .pick_gpu(&device, &queue, &scene, point, 0.0)
            .expect("gpu pick")
            .map(|hit| hit.id);
        assert_eq!(cpu, gpu, "even-odd hole disagreement at {offset:?}");
    }
}

#[test]
fn gpu_respects_z_clips_and_pickable_flags() {
    let _guard = GPU_LOCK.lock().unwrap_or_else(PoisonError::into_inner);
    let Some((device, queue)) = device() else {
        return;
    };
    let mut picker = GpuPicker::new(&device);
    let mut scene = PickScene::new();
    let outer = scene.add_clip(ferrous_2d::picking::ClipNode {
        rect: Aabb {
            min: Vec2::ZERO,
            max: Vec2::new(40.0, 40.0),
        },
        transform: translation(100.0, 50.0),
        parent: None,
        enabled: true,
    });
    let inner = scene.add_clip(ferrous_2d::picking::ClipNode {
        rect: Aabb {
            min: Vec2::ZERO,
            max: Vec2::new(12.0, 12.0),
        },
        transform: translation(108.0, 54.0),
        parent: Some(outer),
        enabled: true,
    });
    scene.add(
        PickItem::new(60, PickShape::rect(Vec2::splat(200.0)))
            .transform(translation(110.0, 60.0))
            .clip(Some(inner)),
    );
    scene.add(
        PickItem::new(61, PickShape::rect(Vec2::splat(20.0)))
            .transform(translation(110.0, 60.0))
            .z(5),
    );
    scene.add(
        PickItem::new(62, PickShape::rect(Vec2::splat(200.0)))
            .transform(translation(110.0, 60.0))
            .z(10)
            .pickable(false),
    );

    for (point, expected) in [
        (Vec2::new(112.25, 58.25), Some(61)),
        (Vec2::new(130.25, 60.25), None),
        (Vec2::new(200.25, 200.25), None),
    ] {
        let cpu = pick_cpu(&mut scene, point).map(|hit| hit.id);
        let gpu = picker
            .pick_gpu(&device, &queue, &scene, point, 0.0)
            .expect("gpu pick")
            .map(|hit| hit.id);
        assert_eq!(cpu, gpu, "clip/z disagreement at {point:?}");
        assert_eq!(cpu, expected, "unexpected CPU result at {point:?}");
    }
}

#[test]
fn gpu_pick_all_returns_the_stack_topmost_first() {
    let _guard = GPU_LOCK.lock().unwrap_or_else(PoisonError::into_inner);
    let Some((device, queue)) = device() else {
        return;
    };
    let mut picker = GpuPicker::new(&device);
    let mut scene = PickScene::new();
    for (id, z) in [(70, 0), (71, 1), (72, 2)] {
        scene.add(
            PickItem::new(id, PickShape::rect(Vec2::splat(30.0)))
                .transform(translation(50.5, 50.5))
                .z(z),
        );
    }
    let point = Vec2::new(50.25, 50.25);
    let mut cpu = Vec::new();
    scene.pick_all(point, 0.0, &mut cpu);
    let mut gpu = Vec::new();
    picker
        .pick_all_gpu(&device, &queue, &scene, point, 0.0, &mut gpu)
        .expect("gpu pick_all");
    let cpu_ids: Vec<u32> = cpu.iter().map(|hit| hit.id).collect();
    let gpu_ids: Vec<u32> = gpu.iter().map(|hit| hit.id).collect();
    assert_eq!(cpu_ids, vec![72, 71, 70]);
    assert_eq!(gpu_ids, cpu_ids);
}

#[test]
fn gpu_region_matches_the_cpu_intersect_marquee() {
    let _guard = GPU_LOCK.lock().unwrap_or_else(PoisonError::into_inner);
    let Some((device, queue)) = device() else {
        return;
    };
    let mut picker = GpuPicker::new(&device);
    let mut scene = grid_scene();
    let area = Aabb {
        min: Vec2::ZERO,
        max: Vec2::new(200.0, 120.0),
    };
    let mut cpu = Vec::new();
    scene.marquee(area, MarqueeMode::Intersect, 0.0, &mut cpu);
    let mut gpu = Vec::new();
    picker
        .region_gpu(&device, &queue, &scene, area, 0.0, &mut gpu)
        .expect("gpu region");

    let mut cpu_ids: Vec<u32> = cpu.iter().map(|hit| hit.id).collect();
    let mut gpu_ids: Vec<u32> = gpu.iter().map(|hit| hit.id).collect();
    cpu_ids.sort_unstable();
    gpu_ids.sort_unstable();
    assert_eq!(cpu_ids, vec![1, 2, 3, 4, 5]);
    assert_eq!(gpu_ids, cpu_ids);

    // The GPU result is topmost first, like the CPU marquee.
    let ranks: Vec<u32> = gpu.iter().map(|hit| hit.id).collect();
    assert_eq!(ranks.first().copied(), cpu.first().map(|hit| hit.id));
}

#[test]
fn gpu_region_handles_sparse_ids() {
    let _guard = GPU_LOCK.lock().unwrap_or_else(PoisonError::into_inner);
    let Some((device, queue)) = device() else {
        return;
    };
    let mut picker = GpuPicker::new(&device);
    let mut scene = PickScene::new();
    scene.add(
        PickItem::new(5_000_000, PickShape::rect(Vec2::splat(20.0)))
            .transform(translation(40.5, 40.5)),
    );
    let area = Aabb {
        min: Vec2::new(30.0, 30.0),
        max: Vec2::new(50.0, 50.0),
    };
    let mut out = Vec::new();
    picker
        .region_gpu(&device, &queue, &scene, area, 0.0, &mut out)
        .expect("gpu region");
    assert_eq!(out.len(), 1);
    assert_eq!(out[0].id, 5_000_000);
}

#[test]
fn id_buffer_integer_encoding_round_trips() {
    assert_eq!(IdBuffer::decode_u32(IdBuffer::encode_u32(0)), Some(0));
    assert_eq!(IdBuffer::decode_u32(IdBuffer::encode_u32(7)), Some(7));
    assert_eq!(IdBuffer::decode_u32(0), None);
}
