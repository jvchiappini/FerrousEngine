//! GUIMaker routing benches (prompt 15 phase 4).
//!
//! Run:
//! ```text
//! cargo bench -p guimaker --bench input
//! ```
//!
//! The point of the phase: a pointer event must not walk the tree. The benches
//! route one event into a 100-, 1 000- and 10 000-node screen and report the
//! per-event cost; the allocation profile shows zero heap traffic in steady
//! state.

#![allow(
    clippy::doc_markdown,
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss
)] // reason: bench fixtures use grid sizes and names, not design values

use divan::counter::ItemsCount;
use divan::AllocProfiler;
use guimaker::input::{InputEvent, PointerButton};
use guimaker::prelude::*;

#[global_allocator]
static ALLOC: AllocProfiler = AllocProfiler::system();

fn main() {
    divan::main();
}

#[derive(Clone, Debug, PartialEq)]
enum Msg {
    Hit(usize),
}

/// A grid of `count` non-overlapping interactive cells that fill the viewport,
/// the first two named so the bench can alternate between two targets. A grid
/// keeps every rect non-empty and distinct at 10 000 nodes (a tall column would
/// shrink or clip most of them and make the hit index skip them).
fn grid(count: usize) -> Element<Msg> {
    let columns = (count as f64).sqrt().ceil() as u16;
    let mut children: Vec<Element<Msg>> = Vec::with_capacity(count);
    for index in 0..count {
        let mut cell = Element::row()
            .key_id(index as u64 + 1)
            .width(SizeSpec::Fill)
            .height(SizeSpec::Fill)
            .bg(ColorRole::Surface)
            .on_click(Msg::Hit(index));
        if index == 0 {
            cell = cell.key("first");
        } else if index == 1 {
            cell = cell.key("second");
        }
        children.push(cell);
    }
    Element::grid(Columns::Fixed(columns))
        .key("root")
        .width(SizeSpec::Fill)
        .height(SizeSpec::Fill)
        .children(children)
}

struct Bed {
    ui: Ui<Msg>,
    view: Element<Msg>,
    text: MetricsTextSystem,
    frame: Frame<Msg>,
    messages: Vec<Msg>,
    first: glam::Vec2,
    second: glam::Vec2,
}

fn bed(count: usize) -> Bed {
    let view = grid(count);
    let mut ui = Ui::new(Theme::dark());
    ui.set_viewport(Size::new(1280.0, 800.0), 1.0);
    let mut text = MetricsTextSystem::new();
    let mut frame = Frame::new();
    ui.paint(&view, &mut frame, &mut text);
    let first = ui
        .layout()
        .rect(ui.tree().find_key("first").expect("first"));
    let second = ui
        .layout()
        .rect(ui.tree().find_key("second").expect("second"));
    let center = |rect: Rect| glam::vec2(rect.x + rect.width / 2.0, rect.y + rect.height / 2.0);
    Bed {
        ui,
        view,
        text,
        frame,
        messages: Vec::with_capacity(8),
        first: center(first),
        second: center(second),
    }
}

fn hover(bencher: divan::Bencher, count: usize) {
    let mut bed = bed(count);
    // Warm the hover state, the last-hit cache, the message buffer and the
    // store entries so the measured loop is steady state.
    for step in 0..8 {
        let point = if step % 2 == 0 { bed.first } else { bed.second };
        bed.ui
            .push_event(InputEvent::PointerMoved { position: point });
        bed.ui.dispatch_pending(&mut bed.messages);
    }
    bed.messages.clear();
    let mut toggle = false;
    bencher.counter(ItemsCount::new(1_usize)).bench_local(|| {
        let point = if toggle { bed.second } else { bed.first };
        toggle = !toggle;
        bed.ui
            .push_event(InputEvent::PointerMoved { position: point });
        bed.ui.dispatch_pending(&mut bed.messages);
        divan::black_box((bed.ui.input().hovered, bed.messages.len()))
    });
}

/// One pointer move between two targets in a 100-node screen.
#[divan::bench]
fn hover_route_100(bencher: divan::Bencher) {
    hover(bencher, 100);
}

/// One pointer move between two targets in a 1 000-node screen.
#[divan::bench]
fn hover_route_1000(bencher: divan::Bencher) {
    hover(bencher, 1_000);
}

/// One pointer move between two targets in a 10 000-node screen.
#[divan::bench]
fn hover_route_10000(bencher: divan::Bencher) {
    hover(bencher, 10_000);
}

/// A press and a release on one node: the click path.
#[divan::bench]
fn click_route_10000(bencher: divan::Bencher) {
    let mut bed = bed(10_000);
    bed.ui.push_event(InputEvent::PointerMoved {
        position: bed.first,
    });
    bed.ui.dispatch_pending(&mut bed.messages);
    bencher.bench_local(|| {
        bed.ui.push_event(InputEvent::PointerDown {
            position: bed.first,
            button: PointerButton::Primary,
        });
        bed.ui.push_event(InputEvent::PointerUp {
            position: bed.first,
            button: PointerButton::Primary,
        });
        bed.ui.dispatch_pending(&mut bed.messages);
        bed.messages.clear();
        divan::black_box(bed.ui.input().click_count)
    });
}

/// Tab moves focus through the precomputed order instead of the tree.
#[divan::bench]
fn focus_step_10000(bencher: divan::Bencher) {
    let mut bed = bed(10_000);
    bencher.bench_local(|| {
        bed.ui.push_event(InputEvent::KeyDown {
            key: guimaker::input::shortcut::ShortcutKey::Tab,
            repeat: false,
        });
        bed.ui.dispatch_pending(&mut bed.messages);
        divan::black_box(bed.ui.input().focused)
    });
}

/// A wheel notch over a scroll container: controller arithmetic only.
#[divan::bench]
fn wheel_route_10000(bencher: divan::Bencher) {
    let view: Element<Msg> = Element::column()
        .width(SizeSpec::Fill)
        .height(SizeSpec::Fill)
        .child(
            Element::scroll(ScrollAxes::Vertical)
                .key("scroll")
                .width(SizeSpec::Fill)
                .height(SizeSpec::Fill)
                .child(
                    Element::column()
                        .height(SizeSpec::Px(200_000.0))
                        .shrink(0.0),
                ),
        );
    let mut ui: Ui<Msg> = Ui::new(Theme::dark());
    ui.set_viewport(Size::new(1280.0, 800.0), 1.0);
    let mut text = MetricsTextSystem::new();
    let mut frame = Frame::new();
    ui.paint(&view, &mut frame, &mut text);
    let point = glam::vec2(64.0, 64.0);
    bencher.bench_local(|| {
        ui.push_event(InputEvent::Wheel {
            position: point,
            delta: glam::vec2(0.0, -1.0),
            kind: guimaker::input::ScrollKind::Line,
        });
        ui.dispatch_pending(&mut frame.messages);
        ui.set_time_ms(ui.input().now_ms + 16.0);
        divan::black_box(
            ui.store().scroll(
                ui.tree()
                    .find_key("scroll")
                    .and_then(|id| ui.tree().node(id))
                    .map_or(0, |node| node.path_hash),
            ),
        )
    });
}

/// Keeps the `view`/`text` fields alive for the compiler and the reader.
#[divan::bench(sample_count = 1)]
fn paint_after_input(bencher: divan::Bencher) {
    let mut bed = bed(1_000);
    bencher.bench_local(|| {
        bed.ui.push_event(InputEvent::PointerMoved {
            position: bed.second,
        });
        bed.ui.paint(&bed.view, &mut bed.frame, &mut bed.text);
        divan::black_box(bed.frame.stats.repainted)
    });
}
