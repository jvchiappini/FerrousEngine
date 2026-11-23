//! Live DPI change on the `GUIMaker` layout core: `1.0 -> 1.5 -> 2.0`.

#![allow(clippy::float_cmp)] // reason: layout token arithmetic is exact here

use guimaker::components::{icon_box, text as display_text, ControlSize, StateSpec};
use guimaker::geom::Size;
use guimaker::icons::Icon;
use guimaker::prelude::*;
use guimaker::theme::{ColorRole, Theme};
use guimaker_ferrous::FerrousTextSystem;

fn view() -> Element<()> {
    Element::column()
        .key("root")
        .width(SizeSpec::Fill)
        .height(SizeSpec::Fill)
        .gap(Spacing::S4)
        .pad(Spacing::S4)
        .child(
            Element::row()
                .key("panel")
                .width(SizeSpec::Fill)
                .height(SizeSpec::Token(Spacing::S8))
                .border(ColorRole::Border),
        )
        .child(icon_box::<()>(Icon::Play, ControlSize::Md, StateSpec::default()).key("icon"))
        .child(
            Element::scroll(ScrollAxes::Vertical)
                .key("scroll")
                .width(SizeSpec::Fill)
                .height(SizeSpec::Token(Spacing::S12))
                .child(
                    Element::row()
                        .width(SizeSpec::Fill)
                        .height(SizeSpec::Token(Spacing::S24)),
                ),
        )
        .child(
            display_text::<()>("Hello selection", TypeRole::Body, Tone::Default)
                .key("label")
                .caret(3)
                .selection(0, 3),
        )
}

fn path_of(ui: &Ui<()>, key: &str) -> u64 {
    let node = ui.tree().find_key(key).expect("key");
    ui.tree().node(node).expect("node").path_hash
}

fn paint(ui: &mut Ui<()>, text: &mut FerrousTextSystem, frame: &mut Frame<()>) {
    ui.paint(&view(), frame, text);
}

#[test]
fn one_host_switches_1_0_1_5_2_0_and_keeps_its_state() {
    let theme = Theme::dark();
    let viewport = Size::new(420.0, 320.0);
    let mut ui: Ui<()> = Ui::new(theme);
    ui.set_viewport(viewport, 1.0);
    let mut text = FerrousTextSystem::new();
    let mut frame = Frame::new();
    paint(&mut ui, &mut text, &mut frame);

    let scroll_path = path_of(&ui, "scroll");
    let label_path = path_of(&ui, "label");
    let icon = ui.tree().find_key("icon").expect("icon");
    let token = ControlSize::Md.icon().px();

    ui.store_mut()
        .set_scroll(scroll_path, glam::Vec2::new(0.0, 30.0));
    ui.store_mut().set_focus(Some(label_path));
    let label_node = ui.tree().find_key("label").expect("label");
    ui.input_mut().focused = Some(label_node);
    ui.open_overlay(label_path);

    // The raster cell does not grow with the DPI scale: at 2.0 a Caption/Body
    // em is far below the 128 px threshold, and G-0006 measured no visible gain
    // from a 128 cell. The icon cache uses the same policy.
    let mut physical_icon = Vec::new();
    for scale in [1.0_f32, 1.5, 2.0] {
        ui.set_viewport(viewport, scale);
        paint(&mut ui, &mut text, &mut frame);

        // Hairlines: every laid-out edge sits on a device pixel.
        for node in &ui.tree().nodes {
            let rect = ui.layout().rect(node.id);
            for edge in [rect.x, rect.y, rect.right(), rect.bottom()] {
                let device = edge * scale;
                let fraction = device.fract().abs();
                assert!(
                    !(0.01..=0.99).contains(&fraction),
                    "scale {scale}: edge {edge} -> {device} is not a device pixel"
                );
            }
        }

        // The icon keeps its role token in logical units, so its physical size
        // is exactly token * scale.
        let icon_rect = ui.layout().rect(icon);
        assert!((icon_rect.width - token).abs() < 0.01, "icon token");
        physical_icon.push(icon_rect.width * scale);

        // State survives the scale change.
        assert_eq!(ui.store().scroll(scroll_path).y, 30.0, "scroll kept");
        assert_eq!(ui.store().focus(), Some(label_path), "focus kept");
        assert_eq!(ui.input().overlays, vec![label_path], "overlay kept");
        let label = ui.tree().find_key("label").expect("label");
        assert_eq!(
            ui.tree().node(label).expect("node").style.selection,
            Some((0, 3)),
            "selection kept"
        );

        // The cell policy does not grow with the scale.
        assert_eq!(ferrous_font::cell_for_em(12.0 * scale), 64, "caption");
        assert_eq!(ferrous_font::cell_for_em(14.0 * scale), 64, "body");
    }
    assert!((physical_icon[0] - token).abs() < 0.01);
    assert!((physical_icon[1] - token * 1.5).abs() < 0.01);
    assert!((physical_icon[2] - token * 2.0).abs() < 0.01);
}
