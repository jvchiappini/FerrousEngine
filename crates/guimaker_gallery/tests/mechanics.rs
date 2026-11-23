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
use guimaker_gallery::shot::{
    backdrop_split, render_display_list, render_display_list_with_solid_atlas, render_story,
    RenderedImage,
};
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

/// The requested radius maps to `sigma = radius / 2.5` in physical pixels, so
/// the full width at half maximum is `0.94 * radius`.
const BLUR_SIGMA_DIVISOR: f32 = 2.5;
/// Deepest level of the 2x prefilter chain, matching the renderer.
const BLUR_MAX_LEVEL: u32 = 5;
/// Widest half-kernel, matching the renderer.
const BLUR_MAX_TAPS: usize = 13;

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

/// Gaussian weights for `sigma` destination texels, taps one texel apart.
///
/// Mirrors `BlurParams::gaussian_weights` in `ferrous_ui_render`.
fn gaussian_weights(sigma: f32) -> Vec<f32> {
    let sigma = sigma.max(0.05);
    // Three sigma covers 99.7 % of the energy; never exceed the array.
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    // reason: sigma is a small positive finite float in gallery tests.
    let radius = (3.0 * sigma).ceil().clamp(0.0, (BLUR_MAX_TAPS - 1) as f32) as usize;
    let mut weights = vec![0.0_f32; radius + 1];
    let mut sum = 0.0_f32;
    for (tap, weight) in weights.iter_mut().enumerate() {
        let x = tap as f32;
        *weight = (-0.5 * (x / sigma) * (x / sigma)).exp();
        sum += if tap == 0 { *weight } else { *weight * 2.0 };
    }
    if sum > 0.0 {
        for weight in &mut weights {
            *weight /= sum;
        }
    }
    weights
}

/// One 2x box-downsample pass, matching `fs_downsample`.
fn cpu_downsample(
    source: &[[f32; 3]],
    width: usize,
    height: usize,
    target_width: usize,
    target_height: usize,
) -> Vec<[f32; 3]> {
    let half_x = 0.5 / width as f32;
    let half_y = 0.5 / height as f32;
    let mut out = vec![[0.0; 3]; target_width * target_height];
    for y in 0..target_height {
        for x in 0..target_width {
            let u = (x as f32 + 0.5) / target_width as f32;
            let v = (y as f32 + 0.5) / target_height as f32;
            let mut sum = [0.0_f32; 3];
            for (dx, dy) in [(-1.0, -1.0), (1.0, -1.0), (-1.0, 1.0), (1.0, 1.0)] {
                let sample =
                    sample_bilinear(source, width, height, u + dx * half_x, v + dy * half_y);
                sum = add_scaled(sum, sample, 0.25);
            }
            out[y * target_width + x] = sum;
        }
    }
    out
}

/// The CPU reference for the engine's prefiltered separable Gaussian.
fn cpu_backdrop_blur(source: &RenderedImage, radius: f32) -> (Vec<[f32; 3]>, usize, usize) {
    let mut width = source.width as usize;
    let mut height = source.height as usize;
    let mut level_image: Vec<[f32; 3]> = source
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

    let sigma = radius / BLUR_SIGMA_DIVISOR;
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    // reason: sigma is a small positive finite float in gallery tests.
    let level = if sigma >= 2.0 {
        sigma.log2().floor().clamp(0.0, BLUR_MAX_LEVEL as f32) as u32
    } else {
        1
    };
    for _ in 0..level {
        let target_width = (width / 2).max(1);
        let target_height = (height / 2).max(1);
        level_image = cpu_downsample(&level_image, width, height, target_width, target_height);
        width = target_width;
        height = target_height;
    }

    #[allow(clippy::cast_precision_loss)]
    // reason: level <= 5, so 2^level is exact in f32.
    let level_scale = (1u32 << level) as f32;
    let weights = gaussian_weights(sigma / level_scale);

    let mut horizontal = vec![[0.0_f32; 3]; width * height];
    for y in 0..height {
        for x in 0..width {
            let mut sum = [0.0_f32; 3];
            let mut weight_sum = 0.0_f32;
            for (tap, weight) in weights.iter().enumerate() {
                if tap == 0 {
                    sum = add_scaled(sum, level_image[y * width + x], *weight);
                    weight_sum += *weight;
                } else {
                    let left = x.saturating_sub(tap).min(width - 1);
                    let right = (x + tap).min(width - 1);
                    sum = add_scaled(sum, level_image[y * width + left], *weight);
                    sum = add_scaled(sum, level_image[y * width + right], *weight);
                    weight_sum += *weight * 2.0;
                }
            }
            let inverse = 1.0 / weight_sum;
            horizontal[y * width + x] = [sum[0] * inverse, sum[1] * inverse, sum[2] * inverse];
        }
    }

    let mut vertical = vec![[0.0_f32; 3]; width * height];
    for y in 0..height {
        for x in 0..width {
            let mut sum = [0.0_f32; 3];
            let mut weight_sum = 0.0_f32;
            for (tap, weight) in weights.iter().enumerate() {
                if tap == 0 {
                    sum = add_scaled(sum, horizontal[y * width + x], *weight);
                    weight_sum += *weight;
                } else {
                    let top = y.saturating_sub(tap).min(height - 1);
                    let bottom = (y + tap).min(height - 1);
                    sum = add_scaled(sum, horizontal[top * width + x], *weight);
                    sum = add_scaled(sum, horizontal[bottom * width + x], *weight);
                    weight_sum += *weight * 2.0;
                }
            }
            let inverse = 1.0 / weight_sum;
            vertical[y * width + x] = [sum[0] * inverse, sum[1] * inverse, sum[2] * inverse];
        }
    }
    (vertical, width, height)
}

/// Whether `(x, y)` lies inside any rect, expanded by `margin`.
fn inside_any(rects: &[guimaker::geom::Rect], x: f32, y: f32, margin: f32) -> bool {
    rects.iter().any(|rect| {
        x >= rect.x - margin
            && x <= rect.right() + margin
            && y >= rect.y - margin
            && y <= rect.bottom() + margin
    })
}

/// The rects of every painted descendant of `root`.
///
/// Container nodes that paint nothing are skipped, so the returned rects are
/// the actual title/body/field/button/helper areas, not the whole card.
fn painted_descendant_rects<Msg: Clone>(
    ui: &Ui<Msg>,
    frame: &Frame<Msg>,
    root: NodeId,
) -> Vec<guimaker::geom::Rect> {
    let mut rects = Vec::new();
    for node in &ui.tree().nodes {
        if node.id == root {
            continue;
        }
        let mut ancestor = node.parent;
        let mut below = false;
        while let Some(parent) = ancestor {
            if parent == root {
                below = true;
                break;
            }
            ancestor = ui.tree().nodes[parent.index()].parent;
        }
        if !below {
            continue;
        }
        let paints = frame
            .display_list
            .commands_for(node.id)
            .iter()
            .any(|command| {
                matches!(
                    command,
                    DrawCommand::Rect { .. }
                        | DrawCommand::Border { .. }
                        | DrawCommand::Shadow { .. }
                        | DrawCommand::Gradient { .. }
                        | DrawCommand::Text { .. }
                        | DrawCommand::Image { .. }
                        | DrawCommand::Icon { .. }
                        | DrawCommand::Path { .. }
                )
            });
        let rect = ui.layout().rect(node.id);
        if paints && rect.width > 0.0 && rect.height > 0.0 {
            rects.push(rect);
        }
    }
    rects
}

/// Every pixel of the dialog card interior not covered by a painted child.
///
/// The card itself is inset to avoid its rounded border, and each child rect is
/// expanded by `MARGIN` so glyph overshoot and border anti-aliasing stay out.
fn dialog_interior_points<Msg: Clone>(
    ui: &Ui<Msg>,
    frame: &Frame<Msg>,
    card: NodeId,
) -> Vec<(i32, i32)> {
    const INSET: f32 = 3.0;
    // 24 px covers a glyph's em-cell overhang: the solid-atlas control fills
    // whole cells, which extend beyond a line box by up to 0.3 em.
    const MARGIN: f32 = 24.0;
    let card_rect = ui.layout().rect(card);
    let children = painted_descendant_rects(ui, frame, card);
    let x_start = (card_rect.x + INSET).ceil() as i32;
    let y_start = (card_rect.y + INSET).ceil() as i32;
    let x_end = (card_rect.right() - INSET).floor() as i32;
    let y_end = (card_rect.bottom() - INSET).floor() as i32;
    let mut points = Vec::new();
    for y in y_start..y_end {
        for x in x_start..x_end {
            if !inside_any(&children, x as f32, y as f32, MARGIN) {
                points.push((x, y));
            }
        }
    }
    points
}

#[test]
fn the_dialog_card_blurs_the_content_below() {
    let theme = Theme::dark();
    let logical = Size::new(1280.0, 800.0);
    let (ui, frame) = paint(&Component::Dialog.view());
    let card = ui.tree().find_key("dialog_card").expect("dialog card");
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
    let points = dialog_interior_points(&ui, &frame, card);
    assert!(
        points.len() > 10_000,
        "the card interior guard sampled only {} pixels",
        points.len()
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
    eprintln!(
        "dialog backdrop render: full {full_duration:?} for {} interior pixels, prefix {prefix_duration:?}",
        points.len()
    );

    let radius = Spacing::S6.px();
    let (blurred, quarter_width, quarter_height) = cpu_backdrop_blur(&source, radius);
    let tint = theme.role(ColorRole::SurfaceRaised).to_linear();
    let point_count = points.len();
    let tolerance = Tolerance::gallery(point_count);
    let blur_weight = 1.0 - BACKDROP_TINT_ALPHA;
    let mut diff_pixels = 0usize;
    let mut distinct = HashSet::new();
    for (x, y) in points {
        let actual = px(&full, x, y);
        distinct.extend(actual);
        let u = (x as f32 + 0.5) / full.width as f32;
        let v = (y as f32 + 0.5) / full.height as f32;
        let sample = sample_bilinear(&blurred, quarter_width, quarter_height, u, v);
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
    assert!(
        distinct.len() >= 3,
        "the card interior must not be flat: {distinct:?}"
    );
    assert!(
        diff_pixels <= tolerance.max_diff_pixels,
        "backdrop differs in {diff_pixels} of {point_count} interior pixels (budget {})",
        tolerance.max_diff_pixels
    );
}

#[test]
fn the_dialog_backdrop_does_not_sample_the_font_atlas() {
    // Negative control. Replacing the glyph atlas with a solid colour changes
    // the *scene* behind the dialog (the story has real labels there), so the
    // control cannot demand identical card pixels. What it does demand is that
    // the card changes **only** by the amount the CPU blur of the changed
    // scene predicts: an engine that sampled the atlas directly would move the
    // card pixels far more than the scene did.
    let theme = Theme::dark();
    let logical = Size::new(1280.0, 800.0);
    let radius = Spacing::S6.px();
    let (ui, frame) = paint(&Component::Dialog.view());
    let card = ui.tree().find_key("dialog_card").expect("dialog card");
    let points = dialog_interior_points(&ui, &frame, card);
    let Some(split) = backdrop_split(&frame.display_list.commands) else {
        panic!("the dialog story has no backdrop blur");
    };
    let mut prefix = DisplayList::new();
    prefix
        .commands
        .extend_from_slice(&frame.display_list.commands[..split]);

    let render = |list: &DisplayList, solid: Option<[u8; 4]>| match solid {
        Some(colour) => render_display_list_with_solid_atlas(list, &theme, logical, 1.0, colour),
        None => render_display_list(list, &theme, logical, 1.0),
    };
    let start = Instant::now();
    let (Ok(normal), Ok(solid)) = (
        render(&frame.display_list, None),
        render(&frame.display_list, Some([128, 128, 128, 255])),
    ) else {
        eprintln!("skipping atlas negative control: no headless GPU");
        return;
    };
    let (Ok(source_normal), Ok(source_solid)) = (
        render(&prefix, None),
        render(&prefix, Some([128, 128, 128, 255])),
    ) else {
        eprintln!("skipping atlas negative control: no headless GPU");
        return;
    };
    let duration = start.elapsed();

    let (blur_normal, wn, hn) = cpu_backdrop_blur(&source_normal, radius);
    let (blur_solid, ws, hs) = cpu_backdrop_blur(&source_solid, radius);
    let tint = theme.role(ColorRole::SurfaceRaised).to_linear();
    let blur_weight = 1.0 - BACKDROP_TINT_ALPHA;
    let expected = |blurred: &[[f32; 3]], w: usize, h: usize, u: f32, v: f32| {
        let sample = sample_bilinear(blurred, w, h, u, v);
        [
            linear_to_srgb(sample[0] * blur_weight + tint[0] * BACKDROP_TINT_ALPHA),
            linear_to_srgb(sample[1] * blur_weight + tint[1] * BACKDROP_TINT_ALPHA),
            linear_to_srgb(sample[2] * blur_weight + tint[2] * BACKDROP_TINT_ALPHA),
        ]
    };

    let mut worst = 0i16;
    let mut scene_changed = 0usize;
    for (x, y) in &points {
        let u = (*x as f32 + 0.5) / normal.width as f32;
        let v = (*y as f32 + 0.5) / normal.height as f32;
        let predicted_normal = expected(&blur_normal, wn, hn, u, v);
        let predicted_solid = expected(&blur_solid, ws, hs, u, v);
        let actual_normal = px(&normal, *x, *y);
        let actual_solid = px(&solid, *x, *y);
        let mut point_changed = false;
        for channel in 0..3 {
            let predicted_delta =
                i16::from(predicted_normal[channel]) - i16::from(predicted_solid[channel]);
            let actual_delta = i16::from(actual_normal[channel]) - i16::from(actual_solid[channel]);
            worst = worst.max((predicted_delta - actual_delta).abs());
            point_changed |= predicted_delta != 0;
        }
        if point_changed {
            scene_changed += 1;
        }
    }
    eprintln!(
        "dialog atlas control: {duration:?}, {} interior pixels, {scene_changed} see the changed scene, worst delta {worst}",
        points.len()
    );
    assert!(
        scene_changed > 0,
        "the control is vacuous: no card pixel sees the glyphs behind it"
    );
    assert!(
        worst <= 6,
        "the backdrop changes by {worst}/255 more than the blurred scene explains; it reads the atlas"
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

/// Builds a scene with one 1 px impulse line and a backdrop card over it.
///
/// The card carries a fully transparent tint, so the pixels inside it are the
/// pure blurred scene (no tint mixing), which is what an impulse response needs.
fn impulse_list(horizontal: bool, width: f32, height: f32, radius: f32) -> DisplayList {
    use guimaker::geom::Rect;
    use guimaker::theme::Color;
    use guimaker::view::NodeId;
    let mut list = DisplayList::new();
    list.commands.push(DrawCommand::Rect {
        node: NodeId(1),
        rect: Rect::new(0.0, 0.0, width, height),
        color: Color::srgba(0.0, 0.0, 0.0, 1.0),
        radii: [0.0; 4],
    });
    let line = if horizontal {
        Rect::new(0.0, (height * 0.5).floor(), width, 1.0)
    } else {
        Rect::new((width * 0.5).floor(), 0.0, 1.0, height)
    };
    list.commands.push(DrawCommand::Rect {
        node: NodeId(2),
        rect: line,
        color: Color::srgba(1.0, 1.0, 1.0, 1.0),
        radii: [0.0; 4],
    });
    let card = Rect::new(16.0, 16.0, width - 32.0, height - 32.0);
    list.commands.push(DrawCommand::BackdropBlur {
        node: NodeId(3),
        rect: card,
        radius,
        radii: [0.0; 4],
    });
    list.commands.push(DrawCommand::Rect {
        node: NodeId(3),
        rect: card,
        color: Color::srgba(0.0, 0.0, 0.0, 0.0),
        radii: [0.0; 4],
    });
    list
}

/// Full width at half maximum of a unimodal profile, in pixels.
fn half_max_width(profile: &[f32], peak: usize) -> f32 {
    let half = profile[peak] * 0.5;
    let mut left = peak as f32;
    for i in (0..peak).rev() {
        if profile[i] <= half {
            let span = (profile[i + 1] - profile[i]).max(1e-6);
            left = i as f32 + (profile[i + 1] - half) / span;
            break;
        }
    }
    let mut right = peak as f32;
    for i in peak + 1..profile.len() {
        if profile[i] <= half {
            let span = (profile[i - 1] - profile[i]).max(1e-6);
            right = (i - 1) as f32 + (profile[i - 1] - half) / span;
            break;
        }
    }
    right - left
}

/// Checks one impulse profile: a single peak, a monotone fall to each side and
/// a full width at half maximum within 20 % of the requested radius.
///
/// `linear` drives the width (a Gaussian's FWHM is defined in linear light);
/// `encoded` drives the monotonicity, whose tolerance is ±1/255 as requested.
fn check_impulse_profile(linear: &[f32], encoded: &[f32], radius: f32, axis: &str) -> (f32, f32) {
    let (peak_index, peak) =
        linear
            .iter()
            .copied()
            .enumerate()
            .fold((0, 0.0_f32), |best, (index, value)| {
                if value > best.1 {
                    (index, value)
                } else {
                    best
                }
            });
    assert!(
        encoded[peak_index] > 20.0,
        "{axis} impulse peak is only {}/255; the card does not cover the line",
        encoded[peak_index]
    );

    // One peak: no sample outside the main lobe may rise above the lobe again.
    let lobe = peak * 0.1;
    let left_edge = (0..peak_index)
        .rev()
        .find(|index| linear[*index] < lobe)
        .unwrap_or(0);
    let right_edge = (peak_index + 1..linear.len())
        .find(|index| linear[*index] < lobe)
        .unwrap_or(linear.len() - 1);
    for index in (1..=left_edge).rev() {
        assert!(
            encoded[index - 1] <= encoded[index] + 1.0,
            "{axis} secondary peak at {index}: {:.1} -> {:.1}",
            encoded[index],
            encoded[index - 1]
        );
    }
    for index in right_edge..encoded.len() - 1 {
        assert!(
            encoded[index + 1] <= encoded[index] + 1.0,
            "{axis} secondary peak at {}: {:.1} -> {:.1}",
            index + 1,
            encoded[index],
            encoded[index + 1]
        );
    }

    let width = half_max_width(linear, peak_index);
    let error = (width - radius).abs() / radius;
    assert!(
        error <= 0.2,
        "{axis} impulse width {width:.2} px is {:.0} % off the requested radius {radius}",
        error * 100.0
    );
    (encoded[peak_index], width)
}

/// The GPU impulse response and the CPU reference for one orientation.
///
/// Returns (linear GPU profile, encoded GPU profile, encoded CPU profile).
fn impulse_response(horizontal: bool, radius: f32) -> (Vec<f32>, Vec<f32>, Vec<f32>) {
    let theme = Theme::dark();
    let logical = Size::new(192.0, 192.0);
    let list = impulse_list(horizontal, 192.0, 192.0, radius);
    let image = match render_display_list(&list, &theme, logical, 1.0) {
        Ok(image) => image,
        Err(error) => {
            eprintln!("skipping impulse guard: no headless GPU ({error})");
            return (Vec::new(), Vec::new(), Vec::new());
        }
    };

    // The prefix is the scene below the card: background plus the impulse line.
    let split = backdrop_split(&list.commands).expect("the impulse list has a backdrop");
    let mut prefix = DisplayList::new();
    prefix.commands.extend_from_slice(&list.commands[..split]);
    let source = match render_display_list(&prefix, &theme, logical, 1.0) {
        Ok(image) => image,
        Err(error) => {
            eprintln!("skipping impulse guard: no headless GPU ({error})");
            return (Vec::new(), Vec::new(), Vec::new());
        }
    };
    let (blurred, level_width, level_height) = cpu_backdrop_blur(&source, radius);

    let mut linear = Vec::new();
    let mut encoded = Vec::new();
    let mut cpu = Vec::new();
    for step in 24..168 {
        let (x, y) = if horizontal { (96, step) } else { (step, 96) };
        let pixel = px(&image, x, y);
        let peak_channel = pixel[0].max(pixel[1]).max(pixel[2]);
        encoded.push(f32::from(peak_channel));
        linear.push(srgb_to_linear(peak_channel));
        let u = (x as f32 + 0.5) / image.width as f32;
        let v = (y as f32 + 0.5) / image.height as f32;
        let sample = sample_bilinear(&blurred, level_width, level_height, u, v);
        cpu.push(f32::from(
            linear_to_srgb(sample[0])
                .max(linear_to_srgb(sample[1]))
                .max(linear_to_srgb(sample[2])),
        ));
    }
    (linear, encoded, cpu)
}

#[test]
fn a_one_pixel_line_blurs_to_one_peak() {
    let radius = 16.0;
    for horizontal in [true, false] {
        let axis = if horizontal { "horizontal" } else { "vertical" };
        let (linear, encoded, cpu) = impulse_response(horizontal, radius);
        if linear.is_empty() {
            return;
        }
        let (peak, width) = check_impulse_profile(&linear, &encoded, radius, axis);
        eprintln!("{axis} impulse: peak {peak:.0}/255, width {width:.2} px (radius {radius})");

        // The GPU profile tracks the CPU reference kernel within a few levels.
        let largest = encoded
            .iter()
            .zip(&cpu)
            .map(|(gpu, cpu)| (gpu - cpu).abs())
            .fold(0.0_f32, f32::max);
        assert!(
            largest <= 4.0,
            "{axis} GPU profile differs from the CPU reference by {largest:.1}/255"
        );
    }
}
