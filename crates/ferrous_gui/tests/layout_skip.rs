//! Acceptance tests proving the engine UI runs no layout work while idle.
//!
//! These use the CPU-only API: no GPU and no window are needed. The layout
//! engine counts every real pass, so a clean frame (no paint or hierarchy dirt
//! and an unchanged viewport) must leave the count untouched.

use ferrous_gui::{GuiBatch, LayoutContext, NodeId, Rect, Style, UiSystem, Units, Widget};
use ferrous_ui_core::Vec2;

struct Fixed(Vec2);

impl Widget<()> for Fixed {
    fn calculate_size(&self, _ctx: &mut LayoutContext) -> Vec2 {
        self.0
    }
}

fn styles(size: Vec2) -> Style {
    let mut style = Style::default();
    style.size = (Units::Px(size.x), Units::Px(size.y));
    style
}

fn add(system: &mut UiSystem<()>, size: Vec2, parent: Option<NodeId>) -> NodeId {
    let id = system.tree.add_node(Box::new(Fixed(size)), parent);
    system.tree.set_node_style(id, styles(size));
    id
}

fn frame(system: &mut UiSystem<()>, batch: &mut GuiBatch, width: f32, height: f32) {
    system.update(0.016, width, height);
    system.render_into(batch, Rect::new(0.0, 0.0, width, height), None);
}

fn system_with_child() -> (UiSystem<()>, NodeId) {
    let mut system: UiSystem<()> = UiSystem::new();
    let root = add(&mut system, Vec2::new(800.0, 600.0), None);
    add(&mut system, Vec2::new(220.0, 40.0), Some(root));
    (system, root)
}

#[test]
fn clean_frames_skip_layout() {
    let (mut system, root) = system_with_child();
    let mut batch = GuiBatch::new();

    frame(&mut system, &mut batch, 800.0, 600.0);
    assert_eq!(
        system.layout_passes(),
        1,
        "the first frame must run exactly one layout pass"
    );
    let root_rect = system.tree.get_node(root).map(|node| node.rect);
    assert_eq!(
        root_rect.map(|rect| (rect.width, rect.height)),
        Some((800.0, 600.0)),
        "the root must actually be laid out"
    );

    frame(&mut system, &mut batch, 800.0, 600.0);
    assert_eq!(
        system.layout_passes(),
        1,
        "a clean frame must not run layout"
    );

    system.tree.mark_paint_dirty(root);
    frame(&mut system, &mut batch, 800.0, 600.0);
    assert_eq!(
        system.layout_passes(),
        2,
        "a paint-dirty frame must run layout"
    );

    frame(&mut system, &mut batch, 1024.0, 700.0);
    assert_eq!(
        system.layout_passes(),
        3,
        "a viewport change must run layout"
    );

    frame(&mut system, &mut batch, 1024.0, 700.0);
    assert_eq!(
        system.layout_passes(),
        3,
        "a clean frame must not run layout"
    );
}

#[test]
fn adding_a_child_requests_layout() {
    let (mut system, root) = system_with_child();
    let mut batch = GuiBatch::new();

    frame(&mut system, &mut batch, 800.0, 600.0);
    let before = system.layout_passes();
    assert_eq!(before, 1, "the first frame must run exactly one layout pass");

    add(&mut system, Vec2::new(10.0, 10.0), Some(root));
    frame(&mut system, &mut batch, 800.0, 600.0);
    assert_eq!(
        system.layout_passes(),
        before + 1,
        "adding a child must run exactly one layout pass"
    );

    let after_add = system.layout_passes();
    frame(&mut system, &mut batch, 800.0, 600.0);
    assert_eq!(
        system.layout_passes(),
        after_add,
        "a clean frame must not run layout"
    );
}
