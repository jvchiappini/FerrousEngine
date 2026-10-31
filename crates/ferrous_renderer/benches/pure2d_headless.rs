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
use ferrous_2d::render::{FontId2d, ShapeInstance, SpriteInstance};
use ferrous_font::TextAlign;
use ferrous_renderer::headless::{Pure2dHarness, DEFAULT_FORMAT};
use ferrous_renderer::Render2dConfig;
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
            h.draw_sprite(
                tex,
                sprite(Vec2::new(x, y), Vec2::splat(6.0), 100.0 + i as f32),
            );
        }
    }

    /// Enable/disable Pure2D viewport culling for this scene.
    fn set_cull(&mut self, enabled: bool) {
        let cfg = Render2dConfig {
            cull_2d: enabled,
            ..self.harness.renderer().render_2d_config()
        };
        self.harness.renderer_mut().set_render_2d_config(cfg);
    }

    /// Culling scene: `visible` on-screen sprites (texture 0) interleaved in
    /// painter order with `offscreen` far-away sprites (texture 1).  With
    /// culling off the off-screen sprites are uploaded and each draws as its
    /// own run (they alternate with the visible texture); with culling on they
    /// vanish and the surviving visible sprites coalesce into a single draw.
    /// Assumes two registered textures.
    fn fill_culling(&mut self, visible: usize, offscreen: usize) {
        let h = &mut self.harness;
        h.begin_frame();
        let pairs = visible.max(offscreen);
        for i in 0..pairs {
            if i < offscreen {
                h.draw_sprite(
                    1,
                    sprite(
                        Vec2::new(10_000.0 + i as f32, 0.0),
                        Vec2::splat(6.0),
                        (i * 2) as f32,
                    ),
                );
            }
            if i < visible {
                let x = (i % 32) as f32 * 12.0 - 176.0;
                let y = (i / 32) as f32 * 12.0 - 176.0;
                h.draw_sprite(
                    0,
                    sprite(Vec2::new(x, y), Vec2::splat(6.0), (i * 2 + 1) as f32),
                );
            }
        }
    }

    fn render(&mut self) -> ferrous_renderer::headless::FrameReport {
        self.harness.render_frame()
    }
}

fn print_baseline(label: &str, report: &ferrous_renderer::headless::FrameReport) {
    eprintln!(
        "[baseline] {label}: draw_calls={} (shapes={} sprites={} text={}) instances={} culled={} static={}/{} bytes_uploaded={} buffer_reallocations={} bind_groups_total={} cpu_ms={:.3}",
        report.draw_calls,
        report.shape_draw_calls,
        report.sprite_draw_calls,
        report.text_draw_calls,
        report.instances_uploaded,
        report.culled_instances,
        report.static_batches_drawn,
        report.static_instances_drawn,
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
    // This bench measures the full extract/coalesce/upload path; the grid
    // intentionally overflows the 512px viewport for large `n`, so disable
    // culling to keep it a complete-workload baseline.  Culling is measured by
    // `pure2d_culling_on` / `pure2d_culling_off`.
    scene.set_cull(false);
    scene.fill();
    print_baseline(&format!("pure2d_shapes n={n}"), &scene.render());
    bencher
        .counter(BytesCount::from(
            (n * std::mem::size_of::<ferrous_2d::render::ShapeGpu>()) as u64,
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
    // Full-workload baseline: see `pure2d_shapes`.
    scene.set_cull(false);
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

/// Viewport culling ON: 512 visible sprites plus `n_off` far-off-screen sprites
/// are submitted; culling drops the off-screen ones before upload.  Compare the
/// `culled` / `instances` / `bytes_uploaded` / `draw_calls` fields against
/// [`pure2d_culling_off`] to see the hardware-independent delta.
#[divan::bench(args = [1024, 8192])]
fn pure2d_culling_on(bencher: divan::Bencher, n_off: usize) {
    let mut scene = Scene::new(0, 0, 2);
    scene.set_cull(true);
    scene.fill_culling(512, n_off);
    print_baseline(&format!("pure2d_culling_on off={n_off}"), &scene.render());
    bencher
        .counter(BytesCount::from(
            (n_off * std::mem::size_of::<SpriteInstance>()) as u64,
        ))
        .bench_local(|| {
            scene.fill_culling(512, n_off);
            divan::black_box(scene.render())
        });
}

/// Same scene as [`pure2d_culling_on`] but with culling disabled, to measure the
/// cost the off-screen geometry would have added.
#[divan::bench(args = [1024, 8192])]
fn pure2d_culling_off(bencher: divan::Bencher, n_off: usize) {
    let mut scene = Scene::new(0, 0, 2);
    scene.set_cull(false);
    scene.fill_culling(512, n_off);
    print_baseline(&format!("pure2d_culling_off off={n_off}"), &scene.render());
    bencher
        .counter(BytesCount::from(
            (n_off * std::mem::size_of::<SpriteInstance>()) as u64,
        ))
        .bench_local(|| {
            scene.fill_culling(512, n_off);
            divan::black_box(scene.render())
        });
}

/// Readback into a caller-owned, reused `Vec` (the steady-state video path).
///
/// The `allocs` column reported by `divan::AllocProfiler` counts *process*
/// allocations, including wgpu's command-encoder internals; the engine-owned
/// allocations are zero after the first call (one persistent staging buffer in
/// `OffscreenTarget::new`, one `Vec` owned by the caller).
#[divan::bench]
fn readback_into_reused_buffer(bencher: divan::Bencher) {
    let mut harness = Pure2dHarness::new(256, 256, DEFAULT_FORMAT).expect("headless gpu");
    harness.begin_frame();
    harness.draw_shape(ShapeInstance::rect(Vec2::ZERO, Vec2::splat(128.0), RED));
    harness.render_frame();
    let mut out = Vec::new();
    harness.read_pixels_into(&mut out).expect("first readback");
    eprintln!(
        "[baseline] readback 256x256: bytes={} staging_allocations={}",
        out.len(),
        harness.target().staging_allocations(),
    );
    bencher
        .counter(BytesCount::from(out.len() as u64))
        .bench_local(|| {
            harness
                .read_pixels_into(&mut out)
                .expect("steady-state readback");
            divan::black_box(out.len())
        });
}

/// Single-texel readback (1x1 copy into the small persistent buffer).
#[divan::bench]
fn readback_single_pixel(bencher: divan::Bencher) {
    let mut harness = Pure2dHarness::new(256, 256, DEFAULT_FORMAT).expect("headless gpu");
    harness.begin_frame();
    harness.draw_shape(ShapeInstance::rect(Vec2::ZERO, Vec2::splat(128.0), RED));
    harness.render_frame();

    bencher
        .counter(BytesCount::from(4u64))
        .bench_local(|| divan::black_box(harness.read_pixel(128, 128)));
}

/// Pipelined readback: submit the copy of the current frame, then map the
/// oldest pending slot.  In a real loop the submit of frame N overlaps the
/// rendering of frame N+1; this benchmark isolates the readback half.
#[divan::bench]
fn readback_pipelined(bencher: divan::Bencher) {
    let mut harness = Pure2dHarness::new(256, 256, DEFAULT_FORMAT).expect("headless gpu");
    harness.begin_frame();
    harness.draw_shape(ShapeInstance::rect(Vec2::ZERO, Vec2::splat(128.0), RED));
    harness.render_frame();
    let mut out = Vec::new();

    bencher
        .counter(BytesCount::from((256usize * 256 * 4) as u64))
        .bench_local(|| {
            harness.submit_readback().expect("submit");
            assert!(harness.take_readback(&mut out).expect("take"));
            divan::black_box(out.len())
        });
}

/// Pure2D MSDF text: `lines` lines of 32 characters, baked atlas reused.
///
/// The `allocs` column is the steady-state signal: layout reuses its scratch
/// buffer, the painter-order upload reuses its `Vec`, and the GPU text instance
/// buffer grows geometrically, so only the first frame allocates engine memory.
#[divan::bench(args = [1, 32])]
fn pure2d_text(bencher: divan::Bencher, lines: usize) {
    let manifest = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let font_path = manifest.join("../../assets/fonts/Roboto-Regular.ttf");
    let bytes = std::fs::read(&font_path).expect("Roboto-Regular.ttf");
    let mut harness = Pure2dHarness::new(512, 512, DEFAULT_FORMAT).expect("headless gpu");
    let font = harness
        .register_font(&bytes, ferrous_font::charset::ascii())
        .expect("font atlas");

    let fill = |h: &mut Pure2dHarness, font: FontId2d| {
        h.begin_frame();
        for i in 0..lines {
            let y = 236.0 - i as f32 * 14.0;
            h.draw_text(
                font,
                "FerrousEngine Pure2D text bench",
                Mat4::from_translation(Vec3::new(-250.0, y, i as f32)),
                12.0,
                TextAlign::Left,
                [1.0, 1.0, 1.0, 1.0],
            );
        }
    };

    fill(&mut harness, font);
    print_baseline(
        &format!("pure2d_text lines={lines}"),
        &harness.render_frame(),
    );
    bencher
        .counter(BytesCount::from(
            (lines * 32 * std::mem::size_of::<ferrous_2d::render::TextInstance>()) as u64,
        ))
        .bench_local(|| {
            fill(&mut harness, font);
            divan::black_box(harness.render_frame())
        });
}

/// Retained static batch: `n` shapes are uploaded **once** at creation and then
/// re-drawn every frame with `bytes_uploaded = 0` / `instances_uploaded = 0`
/// and one draw call.  Compare against [`pure2d_shapes`], which re-uploads the
/// same `n * size_of::<ShapeInstance>()` bytes on every frame.
#[divan::bench(args = [256, 4096])]
fn pure2d_static_batch(bencher: divan::Bencher, n: usize) {
    let mut harness = Pure2dHarness::new(512, 512, DEFAULT_FORMAT).expect("headless gpu");
    let colors = [RED, GREEN, BLUE];
    // Slice order is ascending z, so the batch draws in painter order.
    let shapes: Vec<ShapeInstance> = (0..n)
        .map(|i| {
            let x = (i % 32) as f32 * 12.0 - 176.0;
            let y = (i / 32) as f32 * 12.0 - 176.0;
            ShapeInstance::rect_z(
                Vec2::new(x, y),
                i as f32,
                Vec2::splat(6.0),
                colors[i % colors.len()],
            )
        })
        .collect();
    let _id = harness.renderer_mut().create_static_2d_batch(&shapes);

    harness.begin_frame();
    print_baseline(
        &format!("pure2d_static_batch n={n}"),
        &harness.render_frame(),
    );
    bencher
        // No per-iteration upload: the retained buffer is already on the GPU.
        .counter(BytesCount::from(0u64))
        .bench_local(|| {
            harness.begin_frame();
            divan::black_box(harness.render_frame())
        });
}
