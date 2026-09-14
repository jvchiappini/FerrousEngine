//! Headless Pure2D benchmarks.
//!
//! Run with:
//! ```text
//! export CARGO_TARGET_DIR=/opt/softwaremaker/workspaces/96a0b3c0-be13-4354-8674-72220ab22596/target
//! export VK_ICD_FILENAMES=/usr/share/vulkan/icd.d/lvp_icd.json
//! export VK_DRIVER_FILES=/usr/share/vulkan/icd.d/lvp_icd.json
//! export WGPU_BACKEND=vulkan
//! cargo bench -p ferrous_renderer --bench pure2d_headless
//! ```
//!
//! ## Why `divan` (and not `criterion`)
//!
//! * `divan::AllocProfiler` reports **allocations and bytes per iteration** on
//!   stable Rust.  Allocations/frame is one of the four hardware-independent
//!   metrics this audit must produce, and `criterion` has no equivalent.
//! * wgpu setup is synchronous from the benchmark's perspective (the device is
//!   created once, before `bencher.bench_local`), so neither framework's async
//!   story is needed; `divan` just needs less boilerplate.
//! * `divan` is a far smaller dependency tree than `criterion` (no plotters,
//!   no rayon, no serde requirement), which keeps the workspace build cheap on
//!   this modest VPS.
//!
//! ## Interpreting the numbers
//!
//! This VPS has **no GPU**: lavapipe rasterises on the CPU.  Absolute ns/ms
//! figures are therefore not representative of any real target.  Only compare
//! runs *on the same machine* (delta before/after a change).  The
//! hardware-independent counters printed as `[baseline]` are the portable
//! signal: draw calls, instances, bytes uploaded, buffer reallocations.

use divan::counter::BytesCount;
use divan::AllocProfiler;
use ferrous_2d::render::{ShapeInstance, SpriteInstance};
use ferrous_renderer::headless::{Pure2dHarness, DEFAULT_FORMAT};
use glam::{Mat4, Quat, Vec2, Vec3};

#[global_allocator]
static ALLOC: AllocProfiler = AllocProfiler::system();

fn main() {
    divan::main();
}

const RED: [f32; 4] = [1.0, 0.0, 0.0, 1.0];
const GREEN: [f32; 4] = [0.0, 1.0, 0.0, 1.0];
const BLUE: [f32; 4] = [0.0, 0.0, 1.0, 1.0];

fn sprite(center: Vec2, size: Vec2, z: f32) -> SpriteInstance {
    let model = Mat4::from_scale_rotation_translation(
        Vec3::new(size.x, size.y, 1.0),
        Quat::IDENTITY,
        Vec3::new(center.x, center.y, z),
    );
    SpriteInstance {
        transform_c0: model.x_axis.into(),
        transform_c1: model.y_axis.into(),
        transform_c2: model.z_axis.into(),
        transform_c3: model.w_axis.into(),
        color: [1.0, 1.0, 1.0, 1.0],
        uv_rect: [0.0, 0.0, 1.0, 1.0],
        properties: [0.0, 0.0, 0.0, 0.0],
    }
}

struct Scene {
    harness: Pure2dHarness,
    n_shapes: usize,
    n_sprites: usize,
    n_textures: usize,
}

impl Scene {
    fn new(n_shapes: usize, n_sprites: usize, n_textures: usize) -> Self {
        let mut harness = Pure2dHarness::new(512, 512, DEFAULT_FORMAT).expect("headless gpu");
        // One 2x2 white texture per requested texture id.  Registration is
        // sequential, so ids are `0..n_textures`.
        let pixels = [255u8; 16];
        for _ in 0..n_textures {
            harness.register_texture_rgba8(2, 2, &pixels);
        }
        Self {
            harness,
            n_shapes,
            n_sprites,
            n_textures,
        }
    }

    fn fill(&mut self) {
        let h = &mut self.harness;
        h.begin_frame();
        let colors = [RED, GREEN, BLUE];
        for i in 0..self.n_shapes {
            let x = (i % 32) as f32 * 12.0 - 176.0;
            let y = (i / 32) as f32 * 12.0 - 176.0;
            h.draw_shape(ShapeInstance::rect_z(
                Vec2::new(x, y),
                i as f32,
                Vec2::splat(6.0),
                colors[i % colors.len()],
            ));
        }
        for i in 0..self.n_sprites {
            let x = (i % 32) as f32 * 12.0 - 176.0;
            let y = (i / 32) as f32 * 12.0 - 176.0;
            let tex = (i % self.n_textures) as u32;
            h.draw_sprite(tex, sprite(Vec2::new(x, y), Vec2::splat(6.0), 100.0 + i as f32));
        }
    }

    fn render(&mut self) -> ferrous_renderer::headless::FrameReport {
        self.harness.render_frame()
    }
}

fn print_baseline(label: &str, report: &ferrous_renderer::headless::FrameReport) {
    eprintln!(
        "[baseline] {label}: draw_calls={} (shapes={} sprites={}) instances={} bytes_uploaded={} buffer_reallocations={} bind_groups_total={} cpu_ms={:.3}",
        report.draw_calls,
        report.shape_draw_calls,
        report.sprite_draw_calls,
        report.instances_uploaded,
        report.bytes_uploaded,
        report.buffer_reallocations,
        report.bind_groups_total,
        report.cpu_ms,
    );
}

/// Shape (SDF) throughput: `n` non-overlapping rects.
///
/// Each shape gets a **distinct** z, which is a total order and makes the CPU
/// painter-order sort deterministic.  Since the H3 coalescing pass uploads the
/// shapes in painter order and merges *consecutive same-pipeline* runs, this
/// scene now collapses to **1** shape draw call regardless of `n` (it used to
/// report `n`).  The `[baseline]` line below is the before/after signal.
#[divan::bench(args = [16, 256, 4096])]
fn pure2d_shapes(bencher: divan::Bencher, n: usize) {
    let mut scene = Scene::new(n, 0, 0);
    scene.fill();
    print_baseline(&format!("pure2d_shapes n={n}"), &scene.render());
    bencher
        .counter(BytesCount::from(
            (n * std::mem::size_of::<ShapeInstance>()) as u64,
        ))
        .bench_local(|| {
            scene.fill();
            divan::black_box(scene.render())
        });
}

/// Sprite path: `n` sprites spread across `t` textures, each with a distinct
/// ascending z (deterministic).  The H3 coalescing pass uploads each texture's
/// sprites in painter order and merges consecutive same-texture runs, so this
/// scene collapses to **`t`** draw calls (one per texture) instead of `n`.
/// `t` therefore now directly bounds the draw-call count.
#[divan::bench(args = [(64, 1), (512, 8), (2048, 64)])]
fn pure2d_sprites(bencher: divan::Bencher, (n, t): (usize, usize)) {
    let mut scene = Scene::new(0, n, t);
    scene.fill();
    print_baseline(&format!("pure2d_sprites n={n} t={t}"), &scene.render());
    bencher
        .counter(BytesCount::from(
            (n * std::mem::size_of::<SpriteInstance>()) as u64,
        ))
        .bench_local(|| {
            scene.fill();
            divan::black_box(scene.render())
        });
}
