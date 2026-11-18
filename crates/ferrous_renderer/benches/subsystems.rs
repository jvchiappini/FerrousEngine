//! Per-subsystem divan benches (prompt 18 P0).
//!
//! Every bench reports allocations per iteration through
//! [`divan::AllocProfiler`]: the `allocs`/`bytes` columns are the
//! hardware-independent signal for the zero-allocation goal (P2).
//!
//! Run:
//! ```text
//! cargo bench -p ferrous_renderer --bench subsystems
//! ```

use divan::AllocProfiler;
use ferrous_2d::render::{ShapeInstance, SpriteInstance};
use ferrous_font::{BakedAtlas, FontParser, LayoutData, TextAlign};
use ferrous_gpu::EngineContext;
use ferrous_layout::LayoutEngine;
use ferrous_renderer::headless::{Pure2dHarness, DEFAULT_FORMAT};
use ferrous_svg::SvgMesh;
use ferrous_ui_core::{LayoutContext, UiTree, Widget};
use ferrous_ui_render::{GuiBatch, GuiQuad, GuiRenderer, SvgCommand, TextQuad};
use glam::{Mat4, Quat, Vec2, Vec3};
use lyon_tessellation::path::Path;
use lyon_tessellation::{
    BuffersBuilder, FillOptions, FillTessellator, FillVertex, FillVertexConstructor, StrokeOptions,
    StrokeTessellator, StrokeVertex, StrokeVertexConstructor, VertexBuffers,
};

#[global_allocator]
static ALLOC: AllocProfiler = AllocProfiler::system();

fn main() {
    divan::main();
}

fn roboto() -> Vec<u8> {
    let manifest = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    std::fs::read(manifest.join("../../assets/fonts/Roboto-Regular.ttf"))
        .expect("Roboto-Regular.ttf")
}

// ── UI layout ───────────────────────────────────────────────────────────────

struct MockWidget;

impl Widget<()> for MockWidget {
    fn calculate_size(&self, _ctx: &mut LayoutContext) -> Vec2 {
        Vec2::new(24.0, 12.0)
    }
}

fn ui_tree(n: usize) -> UiTree<()> {
    let mut tree = UiTree::new();
    let root = tree.add_node(Box::new(MockWidget), None);
    for _ in 0..n {
        tree.add_node(Box::new(MockWidget), Some(root));
    }
    tree
}

/// Builds a large flat tree and lays it out once.  The allocation columns
/// include the tree construction; the layout-only number is the time column.
#[divan::bench(args = [256, 2048, 8192])]
fn ui_layout_large_tree(bencher: divan::Bencher, n: usize) {
    bencher.bench_local(move || {
        let mut tree = ui_tree(n);
        let mut engine = LayoutEngine::new();
        engine.compute_layout(&mut tree, 1600.0, 900.0);
        divan::black_box(tree.get_root())
    });
}

// ── GUI batch ───────────────────────────────────────────────────────────────

fn quad(index: usize) -> GuiQuad {
    GuiQuad::solid(
        [(index % 64) as f32 * 12.0, (index / 64) as f32 * 12.0],
        [10.0, 10.0],
        [1.0, 1.0, 1.0, 1.0],
        [2.0; 4],
        0,
        index as f32,
        index as u32,
    )
}

fn glyph(index: usize) -> TextQuad {
    TextQuad {
        pos: [(index % 64) as f32 * 12.0, (index / 64) as f32 * 12.0],
        size: [10.0, 12.0],
        uv0: [0.0, 0.0],
        uv1: [1.0, 1.0],
        color: [1.0, 1.0, 1.0, 1.0],
        z_order: index as f32,
        node_id: index as u32,
    }
}

#[divan::bench(args = [1024, 16384])]
fn gui_batch_quads(bencher: divan::Bencher, n: usize) {
    let mut batch = GuiBatch::new();
    bencher.bench_local(|| {
        batch.clear();
        for index in 0..n {
            batch.push_quad(quad(index));
        }
        divan::black_box(batch.segments.len())
    });
}

#[divan::bench(args = [1024, 16384])]
fn gui_batch_glyphs(bencher: divan::Bencher, n: usize) {
    let mut batch = GuiBatch::new();
    bencher.bench_local(|| {
        batch.clear();
        for index in 0..n {
            batch.push_text(glyph(index));
        }
        divan::black_box(batch.segments.len())
    });
}

// ── Text and MSDF ──────────────────────────────────────────────────────────

#[divan::bench(args = ["AV", "The quick brown fox jumps over the lazy dog AV 1234567890"])]
fn text_layout_line(bencher: divan::Bencher, text: &str) {
    let data = LayoutData::from_bytes(&roboto(), ferrous_font::charset::latin_western())
        .expect("layout data");
    let mut out = Vec::new();
    bencher.bench_local(|| {
        out.clear();
        let metrics = data.layout_line(text, 18.0, TextAlign::Left, &mut out);
        divan::black_box((metrics.advance, metrics.quad_count, out.len()))
    });
}

#[divan::bench]
fn msdf_glyph_rasterisation(bencher: divan::Bencher) {
    let bytes = roboto();
    let parser = FontParser::new(bytes).expect("font parser");
    let chars = ferrous_font::charset::ascii();
    bencher.bench_local(|| {
        let atlas = BakedAtlas::build(&parser, &chars).expect("atlas");
        divan::black_box((atlas.tex_size, atlas.pixels.len()))
    });
}

// ── Path tessellation (lyon) ───────────────────────────────────────────────

#[derive(Clone, Copy, Debug)]
#[allow(dead_code)] // the bench counts triangles; the position is what lyon writes
struct TessVertex([f32; 2]);

struct TessCtor;

impl FillVertexConstructor<TessVertex> for TessCtor {
    fn new_vertex(&mut self, vertex: FillVertex<'_>) -> TessVertex {
        TessVertex([vertex.position().x, vertex.position().y])
    }
}

impl StrokeVertexConstructor<TessVertex> for TessCtor {
    fn new_vertex(&mut self, vertex: StrokeVertex<'_, '_>) -> TessVertex {
        TessVertex([vertex.position().x, vertex.position().y])
    }
}

fn simple_path() -> Path {
    let mut builder = Path::builder();
    builder.begin(lyon_tessellation::math::point(0.0, 0.0));
    builder.line_to(lyon_tessellation::math::point(100.0, 0.0));
    builder.line_to(lyon_tessellation::math::point(100.0, 100.0));
    builder.line_to(lyon_tessellation::math::point(0.0, 100.0));
    builder.close();
    builder.build()
}

fn complex_path() -> Path {
    let mut builder = Path::builder();
    builder.begin(lyon_tessellation::math::point(0.0, 50.0));
    for index in 0..64 {
        let angle = index as f32 * 0.19;
        let radius = 40.0 + (index as f32 * 0.7).sin() * 18.0;
        let point = lyon_tessellation::math::point(
            50.0 + angle.cos() * radius,
            50.0 + angle.sin() * radius,
        );
        if index == 0 {
            builder.begin(point);
        } else {
            builder.line_to(point);
        }
    }
    for index in 0..32 {
        let x = index as f32 * 2.0;
        builder.cubic_bezier_to(
            lyon_tessellation::math::point(x + 1.0, 10.0),
            lyon_tessellation::math::point(x + 2.0, 20.0),
            lyon_tessellation::math::point(x + 3.0, 30.0),
        );
    }
    builder.close();
    builder.build()
}

#[divan::bench]
fn tessellation_simple_fill(bencher: divan::Bencher) {
    let path = simple_path();
    let mut tessellator = FillTessellator::new();
    let mut buffers: VertexBuffers<TessVertex, u32> = VertexBuffers::new();
    bencher.bench_local(|| {
        buffers.vertices.clear();
        buffers.indices.clear();
        tessellator
            .tessellate_path(
                &path,
                &FillOptions::default(),
                &mut BuffersBuilder::new(&mut buffers, TessCtor),
            )
            .expect("simple fill");
        divan::black_box(buffers.indices.len())
    });
}

#[divan::bench]
fn tessellation_complex_fill(bencher: divan::Bencher) {
    let path = complex_path();
    let mut tessellator = FillTessellator::new();
    let mut buffers: VertexBuffers<TessVertex, u32> = VertexBuffers::new();
    bencher.bench_local(|| {
        buffers.vertices.clear();
        buffers.indices.clear();
        tessellator
            .tessellate_path(
                &path,
                &FillOptions::default(),
                &mut BuffersBuilder::new(&mut buffers, TessCtor),
            )
            .expect("complex fill");
        divan::black_box(buffers.indices.len())
    });
}

#[divan::bench]
fn tessellation_complex_stroke(bencher: divan::Bencher) {
    let path = complex_path();
    let mut tessellator = StrokeTessellator::new();
    let mut buffers: VertexBuffers<TessVertex, u32> = VertexBuffers::new();
    bencher.bench_local(|| {
        buffers.vertices.clear();
        buffers.indices.clear();
        tessellator
            .tessellate_path(
                &path,
                &StrokeOptions::default().with_line_width(2.0),
                &mut BuffersBuilder::new(&mut buffers, TessCtor),
            )
            .expect("complex stroke");
        divan::black_box(buffers.indices.len())
    });
}

// ── Pure2D record + encode ────────────────────────────────────────────────

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

/// Mixed scene: `n` shapes + `n` sprites + `n` glyphs, all distinct z, one
/// frame record + upload + encode.
#[divan::bench(args = [256, 2048])]
fn pure2d_heavy_mixed(bencher: divan::Bencher, n: usize) {
    let mut harness = Pure2dHarness::new(1600, 900, DEFAULT_FORMAT).expect("headless gpu");
    let pixels = [255u8; 16];
    harness.register_texture_rgba8(2, 2, &pixels);
    let font_bytes = roboto();
    let font = harness
        .register_font(&font_bytes, ferrous_font::charset::ascii())
        .expect("font");
    let fill = |h: &mut Pure2dHarness| {
        h.begin_frame();
        for index in 0..n {
            let x = (index % 64) as f32 * 20.0 - 640.0;
            let y = (index / 64) as f32 * 20.0 - 400.0;
            h.draw_shape(ShapeInstance::rect_z(
                Vec2::new(x, y),
                index as f32,
                Vec2::splat(8.0),
                [0.2, 0.6, 0.9, 1.0],
            ));
            h.draw_sprite(
                0,
                sprite(
                    Vec2::new(x + 4.0, y + 4.0),
                    Vec2::splat(8.0),
                    (n + index) as f32,
                ),
            );
        }
        for line in 0..(n / 64).max(1) {
            h.draw_text(
                font,
                "FerrousEngine heavy mixed scene 0123456789",
                Mat4::from_translation(Vec3::new(-700.0, -420.0 + line as f32 * 14.0, 10_000.0)),
                12.0,
                TextAlign::Left,
                [1.0, 1.0, 1.0, 1.0],
            );
        }
    };
    fill(&mut harness);
    bencher.bench_local(|| {
        fill(&mut harness);
        divan::black_box(harness.render_frame())
    });
}

// ── SVG mesh upload ────────────────────────────────────────────────────────

#[divan::bench]
fn svg_mesh_upload(bencher: divan::Bencher) {
    let context = pollster::block_on(EngineContext::new()).expect("gpu context");
    let mut gui = GuiRenderer::new(
        context.device.clone(),
        DEFAULT_FORMAT,
        4096,
        800,
        600,
        1,
        None,
    );
    let mesh =
        SvgMesh::from_str("<svg><rect width=\"100\" height=\"100\"/></svg>").expect("svg mesh");
    let vertices = mesh.vertices.len();
    let mut batch = GuiBatch::new();
    batch.push_svg(SvgCommand {
        mesh,
        pos: [10.0, 10.0],
        color: [1.0, 1.0, 1.0, 1.0],
        z: 1.0,
    });
    bencher
        .counter(divan::counter::ItemsCount::new(vertices))
        .bench_local(|| {
            gui.prepare(&context.queue, &batch);
            divan::black_box(vertices)
        });
}
