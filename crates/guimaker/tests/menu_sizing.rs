//! Content sizing for menus: the width is the widest measured label plus the
//! reserved columns and padding, and the height is the real rows/separators.

#![allow(clippy::float_cmp)] // reason: token, metric and snapped layout values are exact literals in tests

use guimaker::components::overlays::{menu, MenuItem};
use guimaker::prelude::*;
use guimaker::text::{MetricsTextSystem, TextStyle, TextSystem};

fn label_width(label: &str) -> f32 {
    let theme = Theme::dark();
    let style = TextStyle::from_type(theme.typography.get(TypeRole::Label), false);
    MetricsTextSystem::new().shape(label, style, None).width
}

fn paint_menu(items: &[MenuItem<()>]) -> (Rect, Vec<Rect>, Rect) {
    let view: Element<()> = Element::column()
        .key("root")
        .width(SizeSpec::Fill)
        .height(SizeSpec::Fill)
        .align(Align::Start)
        .child(menu(items, None).key("menu"));
    let mut ui: Ui<()> = Ui::new(Theme::dark());
    ui.set_viewport(Size::new(800.0, 400.0), 1.0);
    let mut text = MetricsTextSystem::new();
    let mut frame = Frame::new();
    ui.paint(&view, &mut frame, &mut text);

    let menu_id = ui.tree().find_key("menu").expect("menu");
    let menu_rect = ui.layout().rect(menu_id);
    let children = ui.tree().nodes[menu_id.index()].children.clone();
    let mut rows = Vec::new();
    let mut shortcut = Rect::ZERO;
    for child in children {
        let node = &ui.tree().nodes[child.index()];
        if node.kind == NodeKind::Row {
            rows.push(ui.layout().rect(child));
            let shortcut_id = node.children[3];
            shortcut = ui.layout().rect(shortcut_id);
        }
    }
    (menu_rect, rows, shortcut)
}

#[test]
fn a_three_item_menu_sizes_to_its_widest_measured_label() {
    let items = [
        MenuItem::item("Rename", ()),
        MenuItem::item("Duplicate", ()),
        MenuItem::separator(),
        MenuItem::item("Delete", ()),
    ];
    let (menu, rows, shortcut) = paint_menu(&items);

    let widest = label_width("Duplicate");
    assert_eq!(
        widest, 48.979_004,
        "the real MetricsTextSystem width of Duplicate"
    );
    // widest label + 8 px panel pad + 16 px row pad + (16 check + 64 shortcut
    // + 16 chevron) + four 8 px row gaps = 48.979 + 152 = 200.979, snapped to
    // the 1x physical grid.
    assert_eq!(menu.width, 201.0, "widest label + reserved slots");
    assert_eq!(
        menu.height, 129.0,
        "three 40 px rows + 1 px divider + 8 px pad"
    );
    assert_eq!(rows.len(), 3);
    assert_eq!(
        shortcut.width, 64.0,
        "the shortcut column is always reserved"
    );
    for row in &rows {
        assert_eq!(
            row.width,
            menu.width - 8.0,
            "rows fill the panel content width"
        );
    }

    let last = rows.last().expect("last row");
    assert_eq!(
        menu.bottom() - last.bottom(),
        4.0,
        "only the panel's bottom padding follows the last row, never an empty band"
    );
}

#[test]
fn explicit_width_and_height_overrides_still_win() {
    let items = [
        MenuItem::item("Rename", ()),
        MenuItem::item("Duplicate", ()),
        MenuItem::separator(),
        MenuItem::item("Delete", ()),
    ];
    let view: Element<()> = Element::column()
        .key("root")
        .width(SizeSpec::Fill)
        .height(SizeSpec::Fill)
        .align(Align::Start)
        .child(
            menu(&items, None)
                .key("menu")
                .width(SizeSpec::Px(320.0))
                .height(SizeSpec::Px(140.0)),
        );
    let mut ui: Ui<()> = Ui::new(Theme::dark());
    ui.set_viewport(Size::new(800.0, 400.0), 1.0);
    let mut text = MetricsTextSystem::new();
    let mut frame = Frame::new();
    ui.paint(&view, &mut frame, &mut text);
    let id = ui.tree().find_key("menu").expect("menu");
    let rect = ui.layout().rect(id);
    assert_eq!(rect.width, 320.0);
    assert_eq!(rect.height, 140.0);
}

#[test]
fn a_shortcut_does_not_change_the_menu_width() {
    let plain = [
        MenuItem::item("Rename", ()),
        MenuItem::item("Duplicate", ()),
        MenuItem::separator(),
        MenuItem::item("Delete", ()),
    ];
    let with_shortcut = [
        MenuItem::item("Rename", ()).with_shortcut("Ctrl+R"),
        MenuItem::item("Duplicate", ()).with_shortcut("Ctrl+D"),
        MenuItem::separator(),
        MenuItem::item("Delete", ()).with_shortcut("Del"),
    ];
    let (plain_menu, _, plain_shortcut) = paint_menu(&plain);
    let (shortcut_menu, _, shortcut_cell) = paint_menu(&with_shortcut);

    assert_eq!(plain_shortcut.width, 64.0);
    assert_eq!(shortcut_cell.width, 64.0);
    assert_eq!(
        shortcut_menu.width, plain_menu.width,
        "the reserved shortcut column means adding a shortcut changes no label width"
    );
    assert_eq!(shortcut_menu.height, plain_menu.height);
}
