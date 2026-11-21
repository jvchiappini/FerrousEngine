//! `GUIMaker` layout and hit-test benches (prompt 15 phase 2).
//!
//! Run:
//! ```text
//! cargo bench -p guimaker --bench layout
//! ```
//!
//! The start-screen threshold is 0.3 ms CPU per hover/typing frame (prompt 15
//! budget table); the full-layout and the 8,192-node numbers are the controller
//! move evidence against the prompt 18 `ui_layout_large_tree` bench.

use std::cell::Cell;

use divan::counter::ItemsCount;
use divan::AllocProfiler;
use guimaker::layout::LayoutEngine;
use guimaker::prelude::*;
use guimaker::spatial::HitIndex;
use guimaker::store::Store;

#[global_allocator]
static ALLOC: AllocProfiler = AllocProfiler::system();

fn main() {
    divan::main();
}

fn start_screen_ui() -> (
    Ui<StartMsg>,
    Element<StartMsg>,
    MetricsTextSystem,
    Frame<StartMsg>,
) {
    let view = start_screen();
    let mut ui: Ui<StartMsg> = Ui::new(Theme::dark());
    ui.set_viewport(Size::new(1920.0, 1080.0), 1.5);
    let text = MetricsTextSystem::new();
    let frame = Frame::new();
    (ui, view, text, frame)
}

/// A full cold frame: build, layout and paint the start screen from scratch.
#[divan::bench]
fn start_screen_cold_frame(bencher: divan::Bencher) {
    bencher.bench_local(|| {
        let (mut ui, view, mut text, mut frame) = start_screen_ui();
        ui.paint(&view, &mut frame, &mut text);
        divan::black_box((
            frame.stats,
            frame.stats.commands,
            frame.display_list.damage.len(),
        ))
    });
}

/// A hover frame on the start screen: one pointer move, one damaged node, no
/// layout work. This is the 0.3 ms budget's "hover / typing" number.
#[divan::bench]
fn start_screen_hover_frame(bencher: divan::Bencher) {
    let (mut ui, view, mut text, mut frame) = start_screen_ui();
    ui.paint(&view, &mut frame, &mut text);
    let create = ui.tree().find_key("create").expect("create");
    let docs = ui.tree().find_key("browse").expect("browse");
    let create_rect = ui.layout().rect(create);
    let docs_rect = ui.layout().rect(docs);
    let a =
        create_rect.origin() + glam::Vec2::new(create_rect.width / 2.0, create_rect.height / 2.0);
    let b = docs_rect.origin() + glam::Vec2::new(docs_rect.width / 2.0, docs_rect.height / 2.0);
    let toggle = Cell::new(false);
    bencher.bench_local(|| {
        let point = if toggle.get() { b } else { a };
        toggle.set(!toggle.get());
        ui.push_event(InputEvent::PointerMoved { position: point });
        ui.paint(&view, &mut frame, &mut text);
        divan::black_box(frame.stats.repainted)
    });
}

/// Rebuilds the view and relayouts it: a changed label, the "typing" worst case.
#[divan::bench]
fn start_screen_relayout_frame(bencher: divan::Bencher) {
    let (mut ui, mut view, mut text, mut frame) = start_screen_ui();
    ui.paint(&view, &mut frame, &mut text);
    let counter = Cell::new(0u32);
    bencher.bench_local(|| {
        let value = counter.get();
        counter.set(value.wrapping_add(1));
        view.children[0] = Element::row()
            .key("header")
            .align(Align::Center)
            .child(
                guimaker::view::text::<StartMsg>(format!("Motioner {value}"))
                    .key("changing")
                    .text_role(TypeRole::Title),
            )
            .clone();
        ui.paint(&view, &mut frame, &mut text);
        divan::black_box(frame.stats.layout.style_updates)
    });
}

/// The full layout pass on the start screen, with the viewport forced dirty.
#[divan::bench]
fn start_screen_full_layout(bencher: divan::Bencher) {
    let view = start_screen();
    let tree = Tree::build(&view);
    let theme = Theme::dark();
    let mut text = MetricsTextSystem::new();
    let store = Store::new();
    let mut engine = LayoutEngine::new();
    engine.set_viewport(Size::new(1920.0, 1080.0), 1.5);
    engine.run(&tree, &theme, &mut text, &store);
    let toggle = Cell::new(0u32);
    bencher.bench_local(|| {
        let value = toggle.get();
        toggle.set(value.wrapping_add(1));
        // A tiny viewport change invalidates every cached layout entry, so the
        // bench measures the full pass rather than taffy's clean-node cache.
        engine.set_viewport(
            Size::new(1920.0 + f32::from((value & 1) as u8), 1080.0),
            1.5,
        );
        engine.run(&tree, &theme, &mut text, &store);
        divan::black_box(engine.rects().len())
    });
}

/// The same shape as prompt 18's `ui_layout_large_tree` (a flat 8,192-node
/// tree), through `GUIMaker`'s controller.
#[divan::bench(args = [256, 2048, 8192])]
fn ui_layout_large_tree(bencher: divan::Bencher, n: usize) {
    let view: Element<()> = Element::column().children((0..n).map(|_| Element::spacer()));
    let tree = Tree::build(&view);
    let theme = Theme::dark();
    let mut text = MetricsTextSystem::new();
    let store = Store::new();
    let mut engine = LayoutEngine::new();
    engine.set_viewport(Size::new(1600.0, 900.0), 1.0);
    engine.run(&tree, &theme, &mut text, &store);
    let toggle = Cell::new(0u32);
    bencher.counter(ItemsCount::new(n)).bench_local(|| {
        let value = toggle.get();
        toggle.set(value.wrapping_add(1));
        engine.set_viewport(Size::new(1600.0 + f32::from((value & 1) as u8), 900.0), 1.0);
        engine.run(&tree, &theme, &mut text, &store);
        divan::black_box(engine.rects().len())
    });
}

/// 100 000 point queries on a 10 000-interactive-node grid.
#[divan::bench]
#[allow(clippy::cast_precision_loss)] // reason: the bench coordinates are small screen positions
fn spatial_query_10k(bencher: divan::Bencher) {
    // The same shape as the temporary pre-move engine bench: 10 000 15 px
    // interactive rects on a 16 px pitch, 100 per row, and 100 000 random
    // point queries. The rects are synthetic so the number measures the index,
    // not taffy.
    let view: Element<()> = Element::canvas().children((0..10_000).map(|_| Element::canvas()));
    let tree = Tree::build(&view);
    let mut rects = vec![Rect::ZERO; tree.len()];
    rects[0] = Rect::new(0.0, 0.0, 1600.0, 1600.0);
    for index in 0..10_000usize {
        let id = tree.nodes[index + 1].id;
        rects[id.index()] = Rect::new(
            (index % 100) as f32 * 16.0,
            (index / 100) as f32 * 16.0,
            15.0,
            15.0,
        );
    }
    let mut hit_index = HitIndex::new();
    hit_index.rebuild(&tree, &rects);
    let mut state = 0x1234_5678_9abc_def0_u64;
    let queries: Vec<glam::Vec2> = (0..100_000)
        .map(|_| {
            state = state
                .wrapping_mul(6_364_136_223_846_793_005)
                .wrapping_add(1);
            let x = (state % 1_600_000) as f32 / 1_000.0;
            state = state
                .wrapping_mul(6_364_136_223_846_793_005)
                .wrapping_add(1);
            let y = (state % 900_000) as f32 / 1_000.0;
            glam::Vec2::new(x, y)
        })
        .collect();
    bencher
        .counter(ItemsCount::new(100_000_usize))
        .bench_local(|| {
            let mut hits = 0usize;
            for point in &queries {
                if hit_index.query(*point).next().is_some() {
                    hits += 1;
                }
            }
            divan::black_box(hits)
        });
}
