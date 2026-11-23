//! Mechanics acceptance: negative gap, anchored overlays, shared table
//! columns, the shortcut column, tone inheritance, icon sizing and the arc.

#![allow(clippy::float_cmp)] // reason: the assertions compare snapped test rects with a tolerance

use guimaker::prelude::*;
use guimaker::view::Key;

fn paint<Msg: Clone>(view: &Element<Msg>) -> Ui<Msg> {
    let mut ui: Ui<Msg> = Ui::new(Theme::dark());
    ui.set_viewport(Size::new(800.0, 600.0), 1.0);
    let mut text = MetricsTextSystem::new();
    let mut frame = Frame::new();
    ui.paint(view, &mut frame, &mut text);
    ui
}

fn block<Msg: Clone>(key: &'static str) -> Element<Msg> {
    Element::row()
        .key(key)
        .width(SizeSpec::Token(Spacing::S8))
        .height(SizeSpec::Token(Spacing::S8))
}

#[test]
fn a_negative_gap_makes_children_overlap() {
    let view: Element<()> = Element::row()
        .gap_px(-8.0)
        .child(block("first"))
        .child(block("second"));
    let ui = paint(&view);
    let first = ui
        .layout()
        .rect(ui.tree().find_key("first").expect("first"));
    let second = ui
        .layout()
        .rect(ui.tree().find_key("second").expect("second"));
    assert!(
        (second.x - (first.right() - 8.0)).abs() < 0.01,
        "the second block moves 8 px into the first"
    );
}

#[test]
fn an_open_submenu_does_not_change_its_parent_layout() {
    let items = [
        MenuItem::item("New", ()),
        MenuItem::item("Open", ()).with_shortcut("Ctrl+O"),
        MenuItem::item("Export", ())
            .with_shortcut("Ctrl+E")
            .with_submenu(vec![MenuItem::item("PNG", ()), MenuItem::item("MP4", ())]),
    ];
    let closed: Element<()> = menu(&items, None);
    let open: Element<()> = menu(&items, Some(2));
    let closed_ui = paint(&closed);
    let open_ui = paint(&open);
    let closed_panel = closed_ui.tree().find_key("menu").expect("closed menu");
    let open_panel = open_ui.tree().find_key("menu").expect("open menu");
    let closed_children = &closed_ui.tree().nodes[closed_panel.index()].children;
    let open_children = &open_ui.tree().nodes[open_panel.index()].children;
    assert_eq!(closed_children.len(), open_children.len());
    for (closed_child, open_child) in closed_children.iter().zip(open_children) {
        assert_eq!(
            closed_ui.layout().rect(*closed_child).height,
            open_ui.layout().rect(*open_child).height,
            "a submenu must not add height to the row: closed {} open {}",
            closed_ui.layout().rect(*closed_child).height,
            open_ui.layout().rect(*open_child).height
        );
        assert!(
            (closed_ui.layout().rect(*closed_child).y - open_ui.layout().rect(*open_child).y).abs()
                < 0.01,
            "a submenu must not push the next row down"
        );
    }
    // The overlay itself must be a real panel, not a zero-size text spill.
    let overlay = open_children
        .iter()
        .find(|child| open_ui.tree().nodes[child.index()].kind == NodeKind::Overlay)
        .copied()
        .expect("the open row is an overlay");
    let overlay_children = &open_ui.tree().nodes[overlay.index()].children;
    let anchor_rect = open_ui.layout().rect(overlay_children[0]);
    let panel_rect = open_ui.layout().rect(overlay_children[1]);
    assert!(
        panel_rect.width > 0.0 && panel_rect.height > 0.0,
        "submenu panel {panel_rect:?}"
    );
    assert!(
        (panel_rect.x - (anchor_rect.right() + Spacing::S1.px())).abs() < 0.01,
        "submenu x {} starts at the row right {} + gap",
        panel_rect.x,
        anchor_rect.right()
    );
}

#[test]
fn every_menu_shortcut_shares_one_column() {
    let items = [
        MenuItem::item("Save", ()).with_shortcut("Ctrl+S"),
        MenuItem::item("Open", ()).with_shortcut("Ctrl+O"),
        MenuItem::item("Export", ()).with_shortcut("Ctrl+Shift+E"),
    ];
    let view: Element<()> = menu(&items, None);
    let ui = paint(&view);
    let columns: Vec<f32> = ui
        .tree()
        .nodes
        .iter()
        .filter(|node| matches!(&node.key, Some(Key::Static("menu_shortcut"))))
        .map(|node| ui.layout().rect(node.id).x)
        .collect();
    assert_eq!(columns.len(), 3, "one shortcut cell per row");
    assert!(
        columns
            .windows(2)
            .all(|pair| (pair[0] - pair[1]).abs() < 0.01),
        "shortcuts must align in one column: {columns:?}"
    );
}

#[test]
fn a_table_column_shares_one_x_and_width_in_every_row() {
    let columns = [
        TableColumn {
            title: "Name",
            width: ColumnWidth::Fixed(Spacing::S16),
            sortable: false,
        },
        TableColumn {
            title: "Kind",
            width: ColumnWidth::Fraction(1.0),
            sortable: false,
        },
    ];
    let mut model = TableModel::new(columns, 0);
    model.with_data(
        (0..4)
            .map(|index| vec![format!("Layer {index}"), "Group".to_string()])
            .collect(),
    );
    let view: Element<()> = table(&mut model, 0..4, |_| (), |_| ());
    let ui = paint(&view);
    let column_count = 2u64;
    let mut cells = vec![Vec::new(); usize::try_from(column_count).expect("column count")];
    for node in &ui.tree().nodes {
        if let Some(Key::Id(id)) = &node.key {
            // The row-background elements use ids near u64::MAX; only the
            // cell formula ids belong in this scan.
            if *id >= column_count && *id < u64::MAX - 1_000 {
                let column =
                    usize::try_from((id - column_count) % column_count).expect("column index");
                cells[column].push(ui.layout().rect(node.id));
            }
        }
    }
    for rects in cells {
        assert_eq!(rects.len(), 4, "every visible row contributes a cell");
        let first = rects[0];
        for rect in &rects {
            assert!((rect.x - first.x).abs() < 0.01, "one x per column");
            assert!(
                (rect.width - first.width).abs() < 0.01,
                "one width per column"
            );
        }
    }
}

#[test]
fn an_icon_inherits_the_context_tone() {
    let view: Element<()> = Element::column()
        .tone(Tone::Muted)
        .child(Element::<()>::icon(Icon::Search.path()).key("icon"));
    let ui = paint(&view);
    let icon = ui.tree().find_key("icon").expect("icon");
    assert_eq!(
        ui.layout().resolved(icon).expect("resolved").tone,
        Tone::Muted,
        "an icon with no explicit tone follows its context"
    );
}

#[test]
fn icon_sizes_come_from_the_type_role() {
    assert_eq!(TypeRole::Caption.icon_size(), Spacing::S3);
    assert_eq!(TypeRole::Label.icon_size(), Spacing::S4);
    assert_eq!(TypeRole::Body.icon_size(), Spacing::S4);
    assert_eq!(TypeRole::Heading.icon_size(), Spacing::S5);
    assert!(ControlSize::Lg.icon().px() > ControlSize::Sm.icon().px());
}

#[test]
fn progress_ring_sweeps_a_quarter_circle_at_a_quarter_value() {
    let view: Element<()> = progress_ring(0.25, ControlSize::Lg, Status::Success);
    let tree = Tree::build(&view);
    let path = tree
        .nodes
        .iter()
        .find(|node| node.kind == NodeKind::Path)
        .and_then(|node| node.path_points.as_ref())
        .expect("a path node");
    assert!(path.len() >= 2);
    let first = path.first().expect("first point");
    let last = path.last().expect("last point");
    assert!(
        (first.x - 0.5).abs() < 0.01 && first.y.abs() < 0.01,
        "starts at twelve o'clock"
    );
    assert!(
        (last.x - 1.0).abs() < 0.01 && (last.y - 0.5).abs() < 0.01,
        "ends at three o'clock"
    );
}

/// A row whose anchored dropdown is `menu_width` wide, 20 px from the right of
/// a 1000 px window.
fn edge_menu(menu_width: f32, open: bool) -> Element<()> {
    let anchor = Element::row()
        .key("menu_anchor")
        .width(SizeSpec::Px(80.0))
        .height(SizeSpec::Px(32.0));
    let mut overlay = Element::overlay();
    overlay = overlay.child(anchor);
    if open {
        overlay = overlay.child(
            Element::column()
                .key("popup_menu")
                .width(SizeSpec::Px(menu_width))
                .height(SizeSpec::Px(160.0))
                .bg(ColorRole::SurfaceRaised)
                .overlay_on(0, gomaker_side_bottom(), 4.0),
        );
    }
    Element::row()
        .width(SizeSpec::Px(1000.0))
        .height(SizeSpec::Px(300.0))
        .justify(gomaker_justify_end())
        .pad_x(Spacing::S5)
        .child(overlay)
}

fn gomaker_side_bottom() -> guimaker::overlay::Side {
    guimaker::overlay::Side::Bottom
}

fn gomaker_justify_end() -> guimaker::style::Justify {
    guimaker::style::Justify::End
}

fn paint_sized<Msg: Clone>(view: &Element<Msg>, width: f32, height: f32) -> (Ui<Msg>, Frame<Msg>) {
    let mut ui: Ui<Msg> = Ui::new(Theme::dark());
    ui.set_host_capabilities(guimaker::overlay::HostCapabilities::desktop());
    ui.set_viewport(Size::new(width, height), 1.0);
    let mut text = MetricsTextSystem::new();
    let mut frame = Frame::new();
    ui.paint(view, &mut frame, &mut text);
    (ui, frame)
}

#[test]
fn the_default_host_keeps_an_overflowing_menu_visible_in_the_window() {
    // Until the engine draws popup windows, the default must never hide an
    // overlay: it clamps to the canvas and is painted here.
    let mut ui: Ui<()> = Ui::new(Theme::dark());
    ui.set_viewport(Size::new(1000.0, 300.0), 1.0);
    let mut text = MetricsTextSystem::new();
    let mut frame = Frame::new();
    ui.paint(&edge_menu(990.0, true), &mut frame, &mut text);
    let menu = ui.tree().find_key("popup_menu").expect("popup menu");
    assert_eq!(
        ui.overlay_host(menu),
        Some(guimaker::overlay::OverlayHost::InWindow)
    );
    assert!(
        !frame.display_list.commands_for(menu).is_empty(),
        "the clamped overlay is painted in the main window"
    );
}

#[test]
fn a_menu_near_the_right_edge_is_hosted_in_a_popup_and_leaves_the_parent_alone() {
    // The anchor ends 20 px from the right edge. A 990 px dropdown fits on
    // neither side (right overflows, left starts at -10), so the solver keeps
    // the preferred side and the host chooses a popup window.
    let (open_ui, open_frame) = paint_sized(&edge_menu(990.0, true), 1000.0, 300.0);
    let (closed_ui, _) = paint_sized(&edge_menu(990.0, false), 1000.0, 300.0);

    let anchor_open = open_ui.tree().find_key("menu_anchor").expect("anchor");
    let anchor_closed = closed_ui.tree().find_key("menu_anchor").expect("anchor");
    assert_eq!(
        open_ui.layout().rect(anchor_open),
        closed_ui.layout().rect(anchor_closed),
        "the popup decision must not move the parent"
    );
    let anchor = open_ui.layout().rect(anchor_open);
    assert!(
        (1000.0 - anchor.right() - 20.0).abs() < 0.01,
        "the anchor must end 20 px from the right edge, got {}",
        anchor.right()
    );

    let menu = open_ui.tree().find_key("popup_menu").expect("popup menu");
    assert_eq!(
        open_ui.overlay_host(menu),
        Some(guimaker::overlay::OverlayHost::Popup),
        "a dropdown that fits on neither side is hosted in a popup"
    );
    let popup = open_ui.overlay_popup_rect(menu).expect("popup rect");
    assert!((popup.width - 990.0).abs() < 0.01);
    assert!(
        open_frame.display_list.commands_for(menu).is_empty(),
        "a popup-hosted overlay is never painted in the main window"
    );

    // A dropdown that stays inside the window is painted here.
    let (small_ui, small_frame) = paint_sized(&edge_menu(80.0, true), 1000.0, 300.0);
    let small = small_ui.tree().find_key("popup_menu").expect("small menu");
    assert_eq!(
        small_ui.overlay_host(small),
        Some(guimaker::overlay::OverlayHost::InWindow)
    );
    assert!(!small_frame.display_list.commands_for(small).is_empty());
}
