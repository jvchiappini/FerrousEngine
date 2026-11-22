//! The painted text run sits in the padded content box, not the border box.
//!
//! The display list is the contract the adapter consumes: a padded text element
//! must hand the shaper a rectangle whose left/top edges are the content box
//! and whose height is the shaped line box, vertically centred in the content
//! box. These tests check every main-axis placement.

use guimaker::display_list::{DisplayList, DrawCommand};
use guimaker::geom::{Rect, Size};
use guimaker::style::{Align, Justify, SizeSpec};
use guimaker::text::MetricsTextSystem;
use guimaker::theme::{ColorRole, Spacing, Theme, TypeRole};
use guimaker::view::NodeId;
use guimaker::view::{text, Element};
use guimaker::{Frame, Ui};

const EPSILON: f32 = 1e-4;

fn paint(view: &Element<()>) -> (DisplayList, Ui<()>) {
    let mut ui = Ui::new(Theme::dark());
    ui.set_viewport(Size::new(480.0, 240.0), 1.0);
    let _ = ui.set_time_ms(0.0);
    let mut text_system = MetricsTextSystem::new();
    let mut frame = Frame::new();
    ui.paint(view, &mut frame, &mut text_system);
    (frame.display_list, ui)
}

fn text_rect(list: &DisplayList, node: NodeId) -> Rect {
    list.commands
        .iter()
        .find_map(|command| match command {
            DrawCommand::Text {
                node: owner, rect, ..
            } if *owner == node => Some(*rect),
            _ => None,
        })
        .expect("the text node painted a run")
}

fn assert_close(actual: f32, expected: f32, label: &str) {
    assert!(
        (actual - expected).abs() <= EPSILON,
        "{label}: {actual} is not {expected}"
    );
}

#[test]
fn a_padded_run_paints_in_the_content_box_for_each_placement() {
    for align in [Align::Start, Align::Center, Align::End] {
        for justify in [Justify::Start, Justify::Center, Justify::End] {
            let view = Element::row()
                .key("row")
                .width(SizeSpec::Px(300.0))
                .height(SizeSpec::Px(120.0))
                .justify(justify)
                .align(align)
                .child(
                    text::<()>("Padded run")
                        .key("label")
                        .text_role(TypeRole::Label)
                        .pad(Spacing::S4)
                        .width(SizeSpec::Px(160.0))
                        .bg(ColorRole::Surface),
                );
            let (list, ui) = paint(&view);
            let node = ui.tree().find_key("label").expect("label node");
            let border = ui.layout().rect(node);
            let resolved = ui.layout().resolved(node).expect("resolved style");
            let shape = ui.layout().shape(node).expect("shaped text");
            let padding = resolved.padding;

            let content_width = border.width - padding.left - padding.right;
            let content_height = border.height - padding.top - padding.bottom;
            let expected = Rect::new(
                border.x + padding.left,
                border.y + padding.top + (content_height - shape.height) * 0.5,
                content_width,
                shape.height,
            );
            let actual = text_rect(&list, node);

            assert_close(actual.x, expected.x, "content left");
            assert_close(actual.y, expected.y, "centred top");
            assert_close(actual.width, expected.width, "content width");
            assert_close(actual.height, expected.height, "line-box height");

            assert!(
                actual.x > border.x + EPSILON,
                "{align:?}/{justify:?}: the run starts at the border box left"
            );
            assert!(
                (actual.y - border.y).abs() > EPSILON,
                "{align:?}/{justify:?}: the run starts at the border box top"
            );
            assert!(
                (actual.y - expected.y).abs() <= EPSILON,
                "{align:?}/{justify:?}: the run is not vertically centred on its line box"
            );
        }
    }
}
