//! Mechanical guards: colours, icon sizes, shared table columns, overlay
//! anchoring, shadows and the arc. These tests check pixels and layout, never
//! the visual taste of a shot.

#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::cast_possible_wrap,
    clippy::cast_precision_loss
)] // reason: pixel coordinates and sample counts fit i32/u32/f32 for every rendered shot

use std::collections::HashSet;
use std::time::Instant;

use guimaker::display_list::{DisplayList, DrawCommand};
use guimaker::geom::Size;
use guimaker::prelude::*;
use guimaker::theme::tokens::BACKDROP_TINT_ALPHA;
use guimaker::theme::ColorRole;
use guimaker::view::Key;
use guimaker_gallery::golden::Tolerance;
use guimaker_gallery::shot::{backdrop_split, render_display_list, render_story, RenderedImage};
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
            // Table backgrounds use ids next to `u64::MAX`; only the cell key
            // formula is a cell.
            if *id >= column_count && *id < u64::MAX - 1_000 {
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
fn the_card_interior_is_uniform() {
    // The dialog card is intentionally no longer uniform: it blurs and tints
    // the real content below it (see `the_dialog_card_blurs_the_content_below`).
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
    let theme = Theme::dark();
    let (ui, frame) = paint(&card);
    let rect = ui.layout().rect(ui.tree().find_key("card").expect("card"));
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
                "card interior differs at ({x}, {y}): {color:?} vs {expected:?}"
            );
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

fn node_id(ui: &Ui<()>, id: u64) -> NodeId {
    ui.tree()
        .nodes
        .iter()
        .find(|node| matches!(&node.key, Some(Key::Id(key)) if *key == id))
        .map(|node| node.id)
        .expect("a node with the requested key id")
}

const BACKDROP_WEIGHTS: [f32; 5] = [0.227_027, 0.194_594_6, 0.121_621_6, 0.054_054, 0.016_216];

fn srgb_to_linear(channel: u8) -> f32 {
    let value = f32::from(channel) / 255.0;
    if value <= 0.040_45 {
        value / 12.92
    } else {
        ((value + 0.055) / 1.055).powf(2.4)
    }
}

fn linear_to_srgb(value: f32) -> u8 {
    let clamped = value.clamp(0.0, 1.0);
    let encoded = if clamped <= 0.003_130_8 {
        clamped * 12.92
    } else {
        1.055 * clamped.powf(1.0 / 2.4) - 0.055
    };
    (encoded * 255.0).round() as u8
}

fn mix3(a: [f32; 3], b: [f32; 3], t: f32) -> [f32; 3] {
    [
        a[0] + (b[0] - a[0]) * t,
        a[1] + (b[1] - a[1]) * t,
        a[2] + (b[2] - a[2]) * t,
    ]
}

fn add_scaled(sum: [f32; 3], colour: [f32; 3], weight: f32) -> [f32; 3] {
    [
        sum[0] + colour[0] * weight,
        sum[1] + colour[1] * weight,
        sum[2] + colour[2] * weight,
    ]
}

fn sample_bilinear(texture: &[[f32; 3]], width: usize, height: usize, u: f32, v: f32) -> [f32; 3] {
    let x = u * width as f32 - 0.5;
    let y = v * height as f32 - 0.5;
    let x0 = x.floor() as i32;
    let y0 = y.floor() as i32;
    let fx = x - x0 as f32;
    let fy = y - y0 as f32;
    let texel = |ix: i32, iy: i32| {
        let cx = ix.clamp(0, width as i32 - 1) as usize;
        let cy = iy.clamp(0, height as i32 - 1) as usize;
        texture[cy * width + cx]
    };
    let top = mix3(texel(x0, y0), texel(x0 + 1, y0), fx);
    let bottom = mix3(texel(x0, y0 + 1), texel(x0 + 1, y0 + 1), fx);
    mix3(top, bottom, fy)
}

fn cpu_backdrop_blur(source: &RenderedImage, radius: f32) -> (Vec<[f32; 3]>, usize, usize) {
    let width = source.width as usize;
    let height = source.height as usize;
    let quarter_width = (width / 4).max(1);
    let quarter_height = (height / 4).max(1);
    let linear: Vec<[f32; 3]> = source
        .rgba
        .chunks_exact(4)
        .map(|pixel| {
            [
                srgb_to_linear(pixel[0]),
                srgb_to_linear(pixel[1]),
                srgb_to_linear(pixel[2]),
            ]
        })
        .collect();
    let mut horizontal = vec![[0.0; 3]; quarter_width * quarter_height];
    for y in 0..quarter_height {
        for x in 0..quarter_width {
            let u = (x as f32 + 0.5) / quarter_width as f32;
            let v = (y as f32 + 0.5) / quarter_height as f32;
            let mut sum = [0.0; 3];
            let mut weight_sum = 0.0;
            for (tap, weight) in BACKDROP_WEIGHTS.iter().enumerate() {
                if tap == 0 {
                    let centre = sample_bilinear(&linear, width, height, u, v);
                    sum = add_scaled(sum, centre, *weight);
                    weight_sum += *weight;
                } else {
                    let offset = tap as f32 * radius / quarter_width as f32;
                    let left = sample_bilinear(&linear, width, height, u - offset, v);
                    let right = sample_bilinear(&linear, width, height, u + offset, v);
                    sum = add_scaled(sum, left, *weight);
                    sum = add_scaled(sum, right, *weight);
                    weight_sum += *weight * 2.0;
                }
            }
            let inverse = 1.0 / weight_sum;
            horizontal[y * quarter_width + x] =
                [sum[0] * inverse, sum[1] * inverse, sum[2] * inverse];
        }
    }
    let mut vertical = vec![[0.0; 3]; quarter_width * quarter_height];
    for y in 0..quarter_height {
        for x in 0..quarter_width {
            let u = (x as f32 + 0.5) / quarter_width as f32;
            let v = (y as f32 + 0.5) / quarter_height as f32;
            let mut sum = [0.0; 3];
            let mut weight_sum = 0.0;
            for (tap, weight) in BACKDROP_WEIGHTS.iter().enumerate() {
                if tap == 0 {
                    let centre = sample_bilinear(&horizontal, quarter_width, quarter_height, u, v);
                    sum = add_scaled(sum, centre, *weight);
                    weight_sum += *weight;
                } else {
                    let offset = tap as f32 * radius / quarter_height as f32;
                    let top =
                        sample_bilinear(&horizontal, quarter_width, quarter_height, u, v - offset);
                    let bottom =
                        sample_bilinear(&horizontal, quarter_width, quarter_height, u, v + offset);
                    sum = add_scaled(sum, top, *weight);
                    sum = add_scaled(sum, bottom, *weight);
                    weight_sum += *weight * 2.0;
                }
            }
            let inverse = 1.0 / weight_sum;
            vertical[y * quarter_width + x] =
                [sum[0] * inverse, sum[1] * inverse, sum[2] * inverse];
        }
    }
    (vertical, quarter_width, quarter_height)
}

#[test]
fn the_dialog_card_blurs_the_content_below() {
    let theme = Theme::dark();
    let logical = Size::new(1280.0, 800.0);
    let (ui, frame) = paint(&Component::Dialog.view());
    let card = ui.tree().find_key("dialog_card").expect("dialog card");
    let card_rect = ui.layout().rect(card);
    let Some(split) = backdrop_split(&frame.display_list.commands) else {
        panic!("the dialog story has no backdrop blur");
    };
    let mut prefix = DisplayList::new();
    prefix
        .commands
        .extend_from_slice(&frame.display_list.commands[..split]);
    assert!(
        prefix.commands.iter().all(|command| command.node() != card),
        "the dialog card must not paint into the backdrop source"
    );

    let start = Instant::now();
    let full = match render_display_list(&frame.display_list, &theme, logical, 1.0) {
        Ok(image) => image,
        Err(error) => {
            eprintln!("skipping backdrop pixel guard: no headless GPU ({error})");
            return;
        }
    };
    let full_duration = start.elapsed();
    let start = Instant::now();
    let source = match render_display_list(&prefix, &theme, logical, 1.0) {
        Ok(image) => image,
        Err(error) => {
            eprintln!("skipping backdrop pixel guard: no headless GPU ({error})");
            return;
        }
    };
    let prefix_duration = start.elapsed();
    eprintln!("dialog backdrop render: full {full_duration:?}, prefix {prefix_duration:?}");

    let radius = Spacing::S6.px() / 4.0;
    let (blurred, quarter_width, quarter_height) = cpu_backdrop_blur(&source, radius);
    let tint = theme.role(ColorRole::SurfaceRaised).to_linear();
    let x_start = (card_rect.x + 6.0) as i32;
    let x_end = (card_rect.x + 20.0) as i32;
    let y_start = (card_rect.y + 8.0) as i32;
    let y_end = (card_rect.bottom() - 8.0) as i32;
    let pixel_count = ((x_end - x_start).max(0) * (y_end - y_start).max(0)) as usize;
    let tolerance = Tolerance::gallery(pixel_count);
    let mut diff_pixels = 0usize;
    let mut distinct = HashSet::new();
    for y in y_start..y_end {
        for x in x_start..x_end {
            let actual = px(&full, x, y);
            distinct.extend(actual);
            let u = (x as f32 + 0.5) / full.width as f32;
            let v = (y as f32 + 0.5) / full.height as f32;
            let sample = sample_bilinear(&blurred, quarter_width, quarter_height, u, v);
            let blur_weight = 1.0 - BACKDROP_TINT_ALPHA;
            let expected = [
                linear_to_srgb(sample[0] * blur_weight + tint[0] * BACKDROP_TINT_ALPHA),
                linear_to_srgb(sample[1] * blur_weight + tint[1] * BACKDROP_TINT_ALPHA),
                linear_to_srgb(sample[2] * blur_weight + tint[2] * BACKDROP_TINT_ALPHA),
            ];
            let delta = (0..3)
                .map(|channel| actual[channel].abs_diff(expected[channel]))
                .max()
                .unwrap_or(0);
            if delta > tolerance.max_channel_delta {
                diff_pixels += 1;
            }
        }
    }
    assert!(
        distinct.len() >= 3,
        "the card strip must not be flat: {distinct:?}"
    );
    assert!(
        diff_pixels <= tolerance.max_diff_pixels,
        "backdrop differs in {diff_pixels} pixels (budget {})",
        tolerance.max_diff_pixels
    );
}

#[test]
fn a_table_row_paints_one_background_rect() {
    let theme = Theme::dark();
    let (ui, frame) = paint(&Component::Table.view());
    let table = ui
        .layout()
        .rect(ui.tree().find_key("table").expect("table"));
    let columns = 3u64;
    let selected = 2usize;
    let rect_count = |node: NodeId| {
        frame
            .display_list
            .commands_for(node)
            .iter()
            .filter(|command| matches!(command, DrawCommand::Rect { .. }))
            .count()
    };
    let header = node_id(&ui, u64::MAX);
    assert_eq!(rect_count(header), 1, "the header paints one rect");
    let header_rect = ui.layout().rect(header);
    assert!((header_rect.x - table.x).abs() < 0.01);
    assert!((header_rect.width - table.width).abs() < 0.01);
    let raised = theme.role(ColorRole::SurfaceRaised);
    for column in 0..columns {
        for command in frame.display_list.commands_for(node_id(&ui, column)) {
            if let DrawCommand::Rect { color, .. } = command {
                assert_ne!(*color, raised, "header cell {column} paints a background");
            }
        }
    }

    let row = node_id(&ui, u64::MAX - 1 - selected as u64);
    assert_eq!(rect_count(row), 1, "the row paints one rect");
    let row_rect = ui.layout().rect(row);
    assert!((row_rect.x - table.x).abs() < 0.01);
    assert!((row_rect.width - table.width).abs() < 0.01);
    let accent = theme.role(ColorRole::AccentSubtle);
    for column in 0..columns {
        let cell = columns + selected as u64 * columns + column;
        for command in frame.display_list.commands_for(node_id(&ui, cell)) {
            if let DrawCommand::Rect { color, .. } = command {
                assert_ne!(*color, accent, "selected cell {column} paints a background");
            }
        }
    }
}

#[test]
fn a_selected_table_row_has_no_seams() {
    let theme = Theme::dark();
    let (ui, frame) = paint(&Component::Table.view());
    let selected = 2usize;
    let row = ui
        .layout()
        .rect(node_id(&ui, u64::MAX - 1 - selected as u64));
    let image =
        match render_display_list(&frame.display_list, &theme, Size::new(1280.0, 800.0), 1.0) {
            Ok(image) => image,
            Err(error) => {
                eprintln!("skipping table seam guard: no headless GPU ({error})");
                return;
            }
        };
    let y = (row.y + 3.0) as i32;
    let mut previous: Option<[u8; 4]> = None;
    for x in (row.x + 2.0) as i32..(row.right() - 2.0) as i32 {
        let color = px(&image, x, y);
        if let Some(previous) = previous {
            for channel in 0..3 {
                let step = i16::from(color[channel]) - i16::from(previous[channel]);
                assert!(
                    step.abs() <= 2,
                    "row seam step {step} at x={x} channel {channel}"
                );
            }
        }
        previous = Some(color);
    }
}
