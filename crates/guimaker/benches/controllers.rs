//! Controller benches: the scroll, virtual-list and tree controllers that
//! moved out of `ferrous_ui_core` (prompt 15 G-0001, phase 4).
//!
//! Run:
//! ```text
//! cargo bench -p guimaker --bench controllers
//! ```
//!
//! The virtual-list window is `O(1)` and the numbers must be flat from 100 to
//! 10 000 000 rows. The tree flatten and navigation run on a 10 000-row tree,
//! which is also the case that used to allocate a `Vec` per row.

#![allow(
    clippy::doc_markdown,
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss
)] // reason: bench fixtures use row counts and pixel sizes

use divan::AllocProfiler;
use guimaker::input::scroll::{ScrollController, ScrollKind};
use guimaker::input::tree::{Tree, TreeKey, TreeNode};
use guimaker::input::virtual_list::VirtualList;

#[global_allocator]
static ALLOC: AllocProfiler = AllocProfiler::system();

fn main() {
    divan::main();
}

#[divan::bench(args = [100, 1_000, 10_000, 10_000_000])]
fn virtual_list_window(bencher: divan::Bencher, count: usize) {
    let list = VirtualList::new(count, 24.0);
    let mut out = Vec::with_capacity(2);
    bencher.bench_local(|| {
        let rows = list.visible_range(offset(count), 900.0, &mut out);
        divan::black_box((rows, out[0].start))
    });
}

fn offset(count: usize) -> f32 {
    // A middle offset that is inside the list at every size.
    (count as f32 * 24.0 * 0.25).max(0.0)
}

#[divan::bench]
fn scroll_wheel_line(bencher: divan::Bencher) {
    let mut scroll = ScrollController::new();
    scroll.set_bounds(
        glam::Vec2::new(1280.0, 800.0),
        glam::Vec2::new(1280.0, 2_000_000.0),
    );
    bencher.bench_local(|| {
        scroll.wheel(glam::Vec2::new(0.0, -1.0), ScrollKind::Line, false);
        divan::black_box(scroll.offset())
    });
}

#[divan::bench]
fn scroll_momentum_step(bencher: divan::Bencher) {
    let mut scroll = ScrollController::new();
    scroll.set_bounds(
        glam::Vec2::new(1280.0, 800.0),
        glam::Vec2::new(1280.0, 2_000_000.0),
    );
    scroll.wheel(glam::Vec2::new(0.0, -30.0), ScrollKind::Pixel, false);
    bencher.bench_local(|| {
        scroll.advance(1.0 / 60.0);
        // Re-seed so the benchmark always measures a moving controller.
        if !scroll.is_moving() {
            scroll.wheel(glam::Vec2::new(0.0, -30.0), ScrollKind::Pixel, false);
        }
        divan::black_box(scroll.offset())
    });
}

#[divan::bench]
fn scroll_reveal_row(bencher: divan::Bencher) {
    let mut scroll = ScrollController::new();
    scroll.set_bounds(
        glam::Vec2::new(1280.0, 800.0),
        glam::Vec2::new(1280.0, 2_000_000.0),
    );
    let mut row = 0_usize;
    bencher.bench_local(|| {
        row = (row + 1) % 80_000;
        scroll.scroll_to_index(row, 24.0, 800.0);
        divan::black_box(scroll.offset())
    });
}

fn tree(rows: usize) -> Tree<usize> {
    let mut roots: Vec<TreeNode<usize>> = Vec::with_capacity(rows);
    for index in 0..rows {
        roots.push(TreeNode::new(format!("node {index:05}"), index));
    }
    let mut root = TreeNode::new("root", usize::MAX).expanded(true);
    for child in roots {
        root = root.add_child(child);
    }
    Tree::with_root(root)
}

#[divan::bench]
fn tree_flatten_10000(bencher: divan::Bencher) {
    let tree = tree(10_000);
    let mut out = Vec::with_capacity(10_001);
    bencher.bench_local(|| {
        tree.flatten_into(&mut out);
        divan::black_box(out.len())
    });
}

#[divan::bench]
fn tree_move_down_10000(bencher: divan::Bencher) {
    let mut tree = tree(10_000);
    tree.select_first();
    bencher.bench_local(|| {
        tree.move_selection(TreeKey::Down);
        divan::black_box(tree.selected())
    });
}

#[divan::bench]
fn tree_type_ahead_10000(bencher: divan::Bencher) {
    let mut tree = tree(10_000);
    tree.select_first();
    let mut time = 0.0_f64;
    bencher.bench_local(|| {
        time += 1.0;
        tree.type_ahead('n', time);
        divan::black_box(tree.selected())
    });
}
