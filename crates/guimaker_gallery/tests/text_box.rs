//! Every component story keeps its text runs inside their node's content box.
//!
//! The display list is what the adapter consumes, so a component that pads a
//! label or a value must hand the shaper a rectangle inside the padded content
//! box at every node, not just in the start recipe.

use guimaker::display_list::{DisplayList, DrawCommand};
use guimaker::geom::{Rect, Size};
use guimaker::text::MetricsTextSystem;
use guimaker::theme::Theme;
use guimaker::view::NodeId;
use guimaker::{Frame, Ui};
use guimaker_gallery::Component;

const EPSILON: f32 = 0.5;

fn paint(view: &guimaker::view::Element<()>, theme: &Theme) -> (DisplayList, Ui<()>) {
    let mut ui = Ui::new(theme.clone());
    ui.set_viewport(Size::new(1280.0, 800.0), 1.5);
    let _ = ui.set_time_ms(0.0);
    let mut text_system = MetricsTextSystem::new();
    let mut frame = Frame::new();
    ui.paint(view, &mut frame, &mut text_system);
    (frame.display_list, ui)
}

fn content_box(ui: &Ui<()>, node: NodeId) -> Option<Rect> {
    let rect = ui.layout().rect(node);
    let resolved = ui.layout().resolved(node)?;
    let padding = resolved.padding;
    Some(Rect::new(
        rect.x + padding.left,
        rect.y + padding.top,
        (rect.width - padding.left - padding.right).max(0.0),
        (rect.height - padding.top - padding.bottom).max(0.0),
    ))
}

fn inside(inner: Rect, outer: Rect) -> bool {
    inner.x >= outer.x - EPSILON
        && inner.y >= outer.y - EPSILON
        && inner.x + inner.width <= outer.x + outer.width + EPSILON
        && inner.y + inner.height <= outer.y + outer.height + EPSILON
}

#[test]
fn every_component_text_run_sits_in_its_content_box() {
    for component in Component::ALL {
        for theme in [Theme::light(), Theme::dark()] {
            let view = component.view();
            let (list, ui) = paint(&view, &theme);
            for command in &list.commands {
                let DrawCommand::Text { node, rect, .. } = command else {
                    continue;
                };
                let Some(content) = content_box(&ui, *node) else {
                    continue;
                };
                assert!(
                    inside(*rect, content),
                    "{}: a text run {rect:?} escapes its content box {content:?}",
                    component.name()
                );
            }
        }
    }
}
