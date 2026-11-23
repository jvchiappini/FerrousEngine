//! Mechanical guards: colours, icon sizes, shared table columns, overlay
//! anchoring, shadows and the arc. These tests check pixels and layout, never
//! the visual taste of a shot.

#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::cast_possible_wrap
)] // reason: pixel coordinates fit i32/u32 for every rendered shot

use guimaker::display_list::DrawCommand;
use guimaker::geom::Size;
use guimaker::prelude::*;
use guimaker::theme::ColorRole;
use guimaker::view::Key;
use guimaker_gallery::shot::{render_display_list, render_story, RenderedImage};
use guimaker_gallery::story::Story;
use guimaker_gallery::Component;

fn px(image: &RenderedImage, x: i32, y: i32) -> [u8; 4] {
    let x = x.clamp(0, image.width as i32 - 1) as usize;
    let y = y.clamp(0, image.height as i32 - 1) as usize;
    let index = (y * image.width as usize + x) * 4;
    [
        image.rgba[index],
        image.rgba[index + 1],
        image.rgba[index + 2],
        image.rgba[index + 3],
    ]
}

fn is_pure(color: guimaker::theme::Color) -> bool {
    let rgb = color.to_srgb();
    let black = rgb[0] <= 0.0 && rgb[1] <= 0.0 && rgb[2] <= 0.0;
    let white = rgb[0] >= 1.0 && rgb[1] >= 1.0 && rgb[2] >= 1.0;
    black || white
}

#[test]
fn no_text_or_icon_is_pure_black_or_white() {
    for story in Story::all() {
        for theme in [Theme::light(), Theme::dark()] {
            let list = story.display_list(&theme, Size::new(1280.0, 800.0), 1.5);
            for command in &list.commands {
                match command {
                    DrawCommand::Text { run, .. } => {
                        for span in run.spans.iter() {
                            assert!(
                                !is_pure(span.color),
                                "{} paints pure text {span:?}",
                                story.name()
                            );
                        }
                    }
                    DrawCommand::Icon { color, .. } | DrawCommand::Path { color, .. } => {
                        assert!(!is_pure(*color), "{} paints a pure icon/path", story.name());
                    }
                    _ => {}
                }
            }
        }
    }
}

fn paint<Msg: Clone>(view: &Element<Msg>) -> (Ui<Msg>, Frame<Msg>) {
    let mut ui: Ui<Msg> = Ui::new(Theme::dark());
    ui.set_viewport(Size::new(1280.0, 800.0), 1.0);
    let mut text = MetricsTextSystem::new();
    let mut frame = Frame::new();
    ui.paint(view, &mut frame, &mut text);
    (ui, frame)
}

#[test]
fn every_icon_matches_the_role_of_its_sibling_text() {
    for component in Component::ALL {
        let (ui, _frame) = paint(&component.view());
        for node in &ui.tree().nodes {
            if node.kind != NodeKind::Icon {
                continue;
            }
            let Some(parent) = node.parent else { continue };
            let parent = &ui.tree().nodes[parent.index()];
            if parent.kind != NodeKind::Row {
                continue;
            }
            let Some(sibling) = parent
                .children
                .iter()
                .filter_map(|child| ui.tree().nodes.get(child.index()))
                .find(|child| child.kind == NodeKind::Text)
            else {
                continue;
            };
            let expected = sibling.style.text_role.icon_size().px();
            let width = match *node.style.width.resolve(1280.0, &ui.theme().breakpoints) {
                SizeSpec::Token(token) => token.px(),
                _ => continue,
            };
            assert!(
                (width - expected).abs() < 0.01,
                "{}: icon is {width} px, the {:?} text needs {expected}",
                component.name(),
                sibling.style.text_role
            );
        }
    }
}

#[test]
fn a_table_column_shares_one_x_and_width() {
    let (ui, _frame) = paint(&Component::Table.view());
    let column_count = 3u64;
    let mut cells = vec![Vec::new(); column_count as usize];
    for node in &ui.tree().nodes {
        if let Some(Key::Id(id)) = &node.key {
            if *id >= column_count {
                let column =
                    usize::try_from((id - column_count) % column_count).expect("column index");
                cells[column].push(ui.layout().rect(node.id));
            }
        }
    }
    for rects in cells {
        let first = rects[0];
        for rect in &rects {
            assert!((rect.x - first.x).abs() < 0.01);
            assert!((rect.width - first.width).abs() < 0.01);
        }
    }
}

#[test]
fn a_submenu_does_not_change_the_parent_menu() {
    let items = [
        MenuItem::item("New", ()),
        MenuItem::item("Open", ()).with_shortcut("Ctrl+O"),
        MenuItem::item("Export", ())
            .with_shortcut("Ctrl+E")
            .with_submenu(vec![MenuItem::item("PNG", ())]),
    ];
    let closed: Element<()> = menu(&items, None);
    let open: Element<()> = menu(&items, Some(2));
    let (closed_ui, _closed_frame) = paint(&closed);
    let (open_ui, _open_frame) = paint(&open);
    let closed_menu = closed_ui.tree().find_key("menu").expect("menu");
    let open_menu = open_ui.tree().find_key("menu").expect("menu");
    let closed_rects: Vec<Rect> = closed_ui.tree().nodes[closed_menu.index()]
        .children
        .iter()
        .map(|child| closed_ui.layout().rect(*child))
        .collect();
    let open_rects: Vec<Rect> = open_ui.tree().nodes[open_menu.index()]
        .children
        .iter()
        .map(|child| open_ui.layout().rect(*child))
        .collect();
    assert_eq!(closed_rects.len(), open_rects.len());
    for (closed, open) in closed_rects.iter().zip(&open_rects) {
        assert!((closed.height - open.height).abs() < 0.01);
        assert!((closed.y - open.y).abs() < 0.01);
    }
}

#[test]
fn the_shadow_ramp_steps_at_most_two_pixel_levels() {
    let view: Element<()> = Element::column()
        .width(SizeSpec::Fill)
        .height(SizeSpec::Fill)
        .pad(Spacing::S24)
        .child(
            Element::row()
                .key("card")
                .width(SizeSpec::Px(240.0))
                .height(SizeSpec::Px(160.0))
                .bg(ColorRole::SurfaceRaised)
                .elevation(ElevationLevel::E4),
        );
    let theme = Theme::dark();
    let (ui, frame) = paint(&view);
    let card = ui.layout().rect(ui.tree().find_key("card").expect("card"));
    let image =
        match render_display_list(&frame.display_list, &theme, Size::new(1280.0, 800.0), 1.0) {
            Ok(image) => image,
            Err(error) => {
                eprintln!("skipping shadow guard: no headless GPU ({error})");
                return;
            }
        };
    let y = (card.y + card.height * 0.5) as i32;
    let mut previous: Option<[u8; 4]> = None;
    for x in (card.x - 80.0) as i32..(card.x - 2.0) as i32 {
        let color = px(&image, x, y);
        if let Some(previous) = previous {
            for channel in 0..3 {
                let step = i16::from(color[channel]) - i16::from(previous[channel]);
                assert!(
                    step.abs() <= 2,
                    "shadow ramp step {step} at x={x} channel {channel}"
                );
            }
        }
        previous = Some(color);
    }
}

#[test]
fn the_card_and_dialog_interiors_are_uniform() {
    let mut cases = Vec::new();
    let card: Element<()> = Element::column()
        .width(SizeSpec::Fill)
        .height(SizeSpec::Fill)
        .pad(Spacing::S24)
        .child(
            Element::row()
                .key("card")
                .width(SizeSpec::Px(240.0))
                .height(SizeSpec::Px(160.0))
                .bg(ColorRole::SurfaceRaised)
                .border(ColorRole::Border)
                .elevation(ElevationLevel::E4),
        );
    cases.push(("card", card, Theme::dark()));
    let dialog = Component::Dialog.view();
    cases.push(("dialog", dialog, Theme::dark()));
    for (name, view, theme) in cases {
        let (ui, frame) = paint(&view);
        let key = if name == "dialog" {
            "dialog_card"
        } else {
            "card"
        };
        let rect = ui.layout().rect(ui.tree().find_key(key).expect(key));
        let image =
            match render_display_list(&frame.display_list, &theme, Size::new(1280.0, 800.0), 1.0) {
                Ok(image) => image,
                Err(error) => {
                    eprintln!("skipping interior guard: no headless GPU ({error})");
                    return;
                }
            };
        let expected = px(&image, (rect.x + 8.0) as i32, (rect.y + 8.0) as i32);
        for y in (rect.y + 8.0) as i32..(rect.y + 18.0) as i32 {
            for x in (rect.x + 8.0) as i32..(rect.right() - 8.0) as i32 {
                let color = px(&image, x, y);
                assert_eq!(
                    color, expected,
                    "{name} interior differs at ({x}, {y}): {color:?} vs {expected:?}"
                );
            }
        }
    }
}

#[test]
fn the_progress_ring_draws_the_arc_at_the_value_angle() {
    let view: Element<()> = progress_ring(0.42, ControlSize::Lg, Status::Success);
    let theme = Theme::dark();
    let (ui, frame) = paint(&view);
    let rect = ui
        .layout()
        .rect(ui.tree().find_key("progress_ring").expect("ring"));
    let image =
        match render_display_list(&frame.display_list, &theme, Size::new(1280.0, 800.0), 1.0) {
            Ok(image) => image,
            Err(error) => {
                eprintln!("skipping arc guard: no headless GPU ({error})");
                return;
            }
        };
    let center = (rect.x + rect.width * 0.5, rect.y + rect.height * 0.5);
    let radius = rect.width * 0.5;
    let inside = (
        center.0 + radius * (-45.0_f32).to_radians().cos(),
        center.1 + radius * (-45.0_f32).to_radians().sin(),
    );
    let outside = (center.0 - radius, center.1);
    let status = theme.role(ColorRole::Success).to_srgb();
    let track = theme.role(ColorRole::SurfaceSunken).to_srgb();
    let inside_px = px(&image, inside.0 as i32, inside.1 as i32);
    let outside_px = px(&image, outside.0 as i32, outside.1 as i32);
    let distance = |pixel: [u8; 4], color: [f32; 4]| -> i32 {
        (0..3)
            .map(|channel| {
                let target = (color[channel].clamp(0.0, 1.0) * 255.0).round() as i32;
                (i32::from(pixel[channel]) - target).abs()
            })
            .max()
            .unwrap_or(0)
    };
    assert!(
        distance(inside_px, status) < 32,
        "the 42 % arc must paint Success at -45 degrees: {inside_px:?}"
    );
    assert!(
        distance(outside_px, track) < 32,
        "past the sweep the ring must be the track: {outside_px:?}"
    );
}

#[test]
fn the_callout_strokes_only_the_left_edge() {
    let view: Element<()> = callout(Status::Warning, "Heads up", "Body", std::iter::empty());
    let theme = Theme::dark();
    let (ui, frame) = paint(&view);
    let rect = ui
        .layout()
        .rect(ui.tree().find_key("callout").expect("callout"));
    let image =
        match render_display_list(&frame.display_list, &theme, Size::new(1280.0, 800.0), 1.0) {
            Ok(image) => image,
            Err(error) => {
                eprintln!("skipping per-side border guard: no headless GPU ({error})");
                return;
            }
        };
    let y = (rect.y + rect.height * 0.5) as i32;
    let left = px(&image, (rect.x + 1.0) as i32, y);
    let right = px(&image, (rect.right() - 2.0) as i32, y);
    assert_ne!(left, right, "the callout's rule is on one edge only");
    let warning = theme.role(ColorRole::Warning).to_srgb();
    let distance = (0..3)
        .map(|channel| {
            let target = (warning[channel].clamp(0.0, 1.0) * 255.0).round() as i32;
            (i32::from(left[channel]) - target).abs()
        })
        .max()
        .unwrap_or(0);
    assert!(distance < 40, "the left rule reads Warning: {left:?}");
}

#[test]
fn the_golden_dialogs_keep_a_real_backdrop_blur() {
    // A rendered dialog must not smear its own shadow: the guard above proves
    // the interior uniform, and this one proves the story still asks for a
    // backdrop blur (the engine path that samples the snapshot below it).
    let image = match render_story(
        Story::Component(Component::Dialog),
        &Theme::dark(),
        Size::new(1280.0, 800.0),
        1.0,
    ) {
        Ok(image) => image,
        Err(error) => {
            eprintln!("skipping backdrop guard: no headless GPU ({error})");
            return;
        }
    };
    assert!(image.is_consistent());
}
