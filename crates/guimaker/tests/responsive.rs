//! Phase 3 acceptance: responsiveness, DPI and the sweep.
//!
//! The sweep (`guimaker::sweep`) paints a view at ten widths and four scales and
//! fails on overflow, sibling overlap, off-grid edges and an off-screen root.
//! The start-screen recipe and every adaptive pattern must pass all forty cases.

use std::time::Instant;

use guimaker::patterns::{
    collapsing_sidebar, dialog_to_sheet, nav_item, overflowing_toolbar, responsive_table,
    sidebar_drawer, split_panes, TableColumn,
};
use guimaker::prelude::*;
use guimaker::sweep::{sweep, Sweep};

#[derive(Clone, Debug, PartialEq)]
enum Msg {
    Action(u64),
}

fn paint(width: f32, height: f32, scale: f32, view: &Element<Msg>) -> Ui<Msg> {
    let mut ui = Ui::new(Theme::dark());
    ui.set_viewport(Size::new(width, height), scale);
    let mut text = MetricsTextSystem::new();
    let mut frame = Frame::new();
    ui.paint(view, &mut frame, &mut text);
    ui
}

fn assert_clean(name: &str, report: &Sweep) {
    let summary = report.summary();
    println!("{name}:\n{summary}");
    for case in &report.cases {
        for issue in &case.issues {
            eprintln!(
                "{name}: {} at {} @ {}x: {} ({})",
                issue.kind.name(),
                issue.path,
                issue.width,
                issue.scale,
                issue.detail
            );
        }
    }
    assert!(report.is_clean(), "{name} failed the sweep:\n{summary}");
}

#[test]
fn breakpoint_classes_follow_the_documented_scale() {
    let breakpoints = Breakpoints::default();
    for (width, expected) in [
        (0.0, Breakpoint::Xs),
        (479.0, Breakpoint::Xs),
        (480.0, Breakpoint::Sm),
        (767.0, Breakpoint::Sm),
        (768.0, Breakpoint::Md),
        (1023.0, Breakpoint::Md),
        (1024.0, Breakpoint::Lg),
        (1439.0, Breakpoint::Lg),
        (1440.0, Breakpoint::Xl),
        (3840.0, Breakpoint::Xl),
    ] {
        assert_eq!(breakpoints.class(width), expected, "class at {width}");
    }
    assert_eq!(breakpoints, Breakpoints::new(480.0, 768.0, 1024.0, 1440.0));
}

#[test]
#[allow(clippy::float_cmp)] // reason: token arithmetic is exact here
fn a_breakpoint_is_the_container_width_not_the_window() {
    let view: Element<Msg> = Element::column()
        .key("root")
        .width(SizeSpec::Fill)
        .height(SizeSpec::Fill)
        .child(
            Element::column()
                .key("narrow")
                .width(SizeSpec::Token(Spacing::S24))
                .child(
                    text::<Msg>("inside")
                        .key("inside")
                        .pad(resp![Xs: Spacing::S1, Md: Spacing::S8]),
                ),
        );
    let ui = paint(1920.0, 900.0, 1.0, &view);
    let inside = ui.tree().find_key("inside").expect("inside");
    let style = ui.layout().resolved(inside).expect("resolved");
    // The window is Xl, but the text sits in a 96 px column, so its container
    // is Xs and the responsive padding resolves to S1, not the window's S8.
    assert_eq!(style.padding.top, 4.0);

    let wide: Element<Msg> = Element::column()
        .key("root")
        .width(SizeSpec::Fill)
        .height(SizeSpec::Fill)
        .pad(resp![Xs: Spacing::S1, Md: Spacing::S8])
        .child(text("root"));
    let ui = paint(1920.0, 900.0, 1.0, &wide);
    let root = ui.tree().find_key("root").expect("root");
    assert_eq!(
        ui.layout().resolved(root).expect("resolved").padding.top,
        32.0
    );
}

#[test]
fn hidden_below_resolves_against_the_container() {
    let view: Element<Msg> = Element::column()
        .key("root")
        .width(SizeSpec::Fill)
        .height(SizeSpec::Fill)
        .child(
            Element::column()
                .key("sidebar")
                .width(SizeSpec::Token(Spacing::S24))
                .hidden_below(Breakpoint::Md)
                .child(text("nav")),
        );
    let narrow = paint(600.0, 800.0, 1.0, &view);
    let sidebar = narrow.tree().find_key("sidebar").expect("sidebar");
    assert!(narrow.layout().resolved(sidebar).expect("resolved").hidden);
    assert!(narrow.layout().rect(sidebar).is_empty());

    let wide = paint(1024.0, 800.0, 1.0, &view);
    assert!(!wide.layout().resolved(sidebar).expect("resolved").hidden);
    assert!(!wide.layout().rect(sidebar).is_empty());
}

#[test]
#[allow(clippy::float_cmp)] // reason: Fluid::resolve clamps to exact token values
fn fluid_widths_clamp_between_tokens() {
    let fluid = Fluid::new(Spacing::S12, 0.25, Spacing::S24);
    let view: Element<Msg> = Element::column()
        .key("root")
        .width(SizeSpec::Fill)
        .height(SizeSpec::Fill)
        .child(
            Element::row()
                .key("fluid")
                .fluid_width(fluid)
                .height(SizeSpec::Token(Spacing::S4)),
        );
    let narrow = paint(100.0, 600.0, 1.0, &view);
    let wide = paint(1000.0, 600.0, 1.0, &view);
    let fluid_id = narrow.tree().find_key("fluid").expect("fluid");
    assert_eq!(
        narrow.layout().rect(fluid_id).width,
        48.0,
        "100 * 0.25 -> min 48"
    );
    assert_eq!(
        wide.layout().rect(fluid_id).width,
        96.0,
        "1000 * 0.25 -> max 96"
    );
    let cause = wide.layout().cause(fluid_id);
    assert!(matches!(cause.width, Some(Cause::Fluid(_))));
}

#[test]
fn fluid_type_size_scales_between_roles() {
    let view: Element<Msg> = Element::column()
        .key("root")
        .width(SizeSpec::Fill)
        .height(SizeSpec::Fill)
        .child(
            text::<Msg>("Title")
                .key("title")
                .text_role(TypeRole::Display)
                .fluid_text(FluidText::new(TypeRole::Heading, 0.02, TypeRole::Display)),
        );
    let narrow = paint(360.0, 600.0, 1.0, &view);
    let wide = paint(1920.0, 600.0, 1.0, &view);
    let title = narrow.tree().find_key("title").expect("title");
    let narrow_size = narrow
        .layout()
        .resolved(title)
        .expect("resolved")
        .text
        .style
        .size;
    let wide_size = wide
        .layout()
        .resolved(title)
        .expect("resolved")
        .text
        .style
        .size;
    assert!(
        (narrow_size - 14.0).abs() < 0.01,
        "at 360 the size clamps to Heading (14), got {narrow_size}"
    );
    assert!(
        (wide_size - 22.0).abs() < 0.01,
        "at 1920 the size clamps to Display (22), got {wide_size}"
    );
    let middle = paint(1000.0, 600.0, 1.0, &view);
    let middle_size = middle
        .layout()
        .resolved(title)
        .expect("resolved")
        .text
        .style
        .size;
    assert!(
        middle_size > narrow_size && middle_size < wide_size,
        "the middle width must interpolate, got {middle_size}"
    );
}

#[test]
fn a_low_priority_label_gives_way_before_its_sibling() {
    let long = "A very long project name that cannot fit in the row";
    let view: Element<Msg> = Element::row()
        .key("row")
        .width(SizeSpec::Token(Spacing::S24))
        .align(Align::Center)
        .gap(Spacing::S2)
        .child(
            text::<Msg>(long)
                .key("label")
                .priority(1)
                .ellipsis(Ellipsis::End)
                .lines(1),
        )
        .child(text::<Msg>("Save").key("action").priority(9));
    let ui = paint(400.0, 200.0, 1.0, &view);
    let label = ui.tree().find_key("label").expect("label");
    let action = ui.tree().find_key("action").expect("action");
    let label_shape = ui.layout().shape(label).expect("label shape");
    assert!(
        label_shape.truncated,
        "the low-priority label must ellipsise"
    );
    let action_shape = ui.layout().shape(action).expect("action shape");
    assert!(
        !action_shape.truncated,
        "the high-priority label stays whole"
    );
    assert!(
        ui.layout().rect(action).width >= action_shape.width - 0.01,
        "the sibling keeps its natural width"
    );
}

#[test]
fn a_content_column_centres_on_a_wide_screen() {
    let view: Element<Msg> = Element::column()
        .key("root")
        .width(SizeSpec::Fill)
        .height(SizeSpec::Fill)
        .align(Align::Stretch)
        .child(
            Element::column()
                .key("prose")
                .width(SizeSpec::Fill)
                .max_width(ContentWidth::Prose)
                .align_self(Align::Center)
                .child(text("prose")),
        );
    let ui = paint(1920.0, 900.0, 1.0, &view);
    let prose = ui.tree().find_key("prose").expect("prose");
    let rect = ui.layout().rect(prose);
    assert!((rect.width - 672.0).abs() < 0.5, "width {}", rect.width);
    assert!(
        (rect.x - (1920.0 - 672.0) / 2.0).abs() < 0.5,
        "centred at {}",
        rect.x
    );
}

#[test]
fn every_rect_is_snapped_at_each_dpi_scale() {
    let view = start_screen(0.0);
    for scale in [1.0f32, 1.25, 1.5, 2.0] {
        let mut ui = Ui::new(Theme::dark());
        ui.set_viewport(Size::new(1600.0, 900.0), scale);
        let mut text = MetricsTextSystem::new();
        let mut frame = Frame::new();
        ui.paint(&view, &mut frame, &mut text);
        for rect in ui.layout().rects() {
            for value in [rect.x, rect.y, rect.right(), rect.bottom()] {
                let physical = value * scale;
                assert!(
                    (physical - physical.round()).abs() < 1e-3,
                    "edge {value} is not on the {scale}x grid"
                );
            }
        }
    }
}

#[test]
fn the_start_screen_passes_the_sweep() {
    let report = sweep(&start_screen(0.0), &Theme::dark());
    assert_eq!(report.cases.len(), 40, "10 widths x 4 scales");
    assert_clean("start_screen", &report);
}

fn shell(content: Element<Msg>) -> Element<Msg> {
    Element::column()
        .key("shell")
        .width(SizeSpec::Fill)
        .height(SizeSpec::Fill)
        .pad(Spacing::S3)
        .gap(Spacing::S3)
        .align(Align::Stretch)
        .bg(ColorRole::Bg)
        .child(
            Element::row()
                .key("shell_header")
                .align(Align::Center)
                .gap(Spacing::S2)
                .child(
                    text::<Msg>("Motioner")
                        .text_role(TypeRole::Title)
                        .priority(9),
                )
                .child(Element::spacer())
                .child(text::<Msg>("Settings").priority(1)),
        )
        .child(content)
}

fn sidebar_pattern() -> Element<Msg> {
    shell(
        Element::row()
            .key("app_body")
            .align(Align::Stretch)
            .gap(Spacing::S3)
            .grow()
            .direction(resp![Xs: Direction::Column, Md: Direction::Row])
            .child(collapsing_sidebar([
                nav_item("[]", "Files", Msg::Action(1)),
                nav_item("()", "Search", Msg::Action(2)),
                nav_item("{}", "Code", Msg::Action(3)),
            ]))
            .child(
                Element::column()
                    .key("content")
                    .grow()
                    .bg(ColorRole::Surface)
                    .pad(Spacing::S3)
                    .child(text::<Msg>("Canvas")),
            ),
    )
}

#[test]
fn the_collapsing_sidebar_pattern_passes_the_sweep() {
    assert_clean(
        "collapsing_sidebar",
        &sweep(&sidebar_pattern(), &Theme::dark()),
    );
}

#[test]
fn the_drawer_is_the_narrow_alternative() {
    // The drawer is shown by the app below Sm; on its own it must lay out
    // inside a 360 px container without overflow.
    let view = shell(
        Element::stack()
            .key("overlay")
            .grow()
            .child(sidebar_drawer([
                nav_item("[]", "Files", Msg::Action(1)),
                nav_item("()", "Search", Msg::Action(2)),
            ])),
    );
    assert_clean("sidebar_drawer", &sweep(&view, &Theme::dark()));
}

#[test]
fn the_overflowing_toolbar_pattern_passes_the_sweep() {
    let actions = (0..5u64)
        .map(|index| {
            text::<Msg>(format!("Action {}", index + 1))
                .pad_x(Spacing::S3)
                .pad_y(Spacing::S2)
                .bg(ColorRole::SurfaceRaised)
                .border(ColorRole::Border)
                .radius(Radius::Sm)
                .on_click(Msg::Action(index))
        })
        .collect::<Vec<_>>();
    let more = text::<Msg>("More")
        .pad_x(Spacing::S3)
        .pad_y(Spacing::S2)
        .bg(ColorRole::Surface)
        .border(ColorRole::Border)
        .radius(Radius::Sm)
        .on_click(Msg::Action(99));
    let view = shell(overflowing_toolbar(
        actions,
        more,
        [1, 2, 3, 4, 5],
        Breakpoint::Xl,
    ));
    assert_clean("overflowing_toolbar", &sweep(&view, &Theme::dark()));
}

#[test]
fn the_split_panes_pattern_passes_the_sweep() {
    let view = shell(split_panes(
        text::<Msg>("Left pane")
            .pad(Spacing::S3)
            .bg(ColorRole::Surface),
        text::<Msg>("Right pane")
            .pad(Spacing::S3)
            .bg(ColorRole::Surface),
        Spacing::S16,
        Spacing::S16,
    ));
    assert_clean("split_panes", &sweep(&view, &Theme::dark()));
}

#[test]
fn the_dialog_pattern_passes_the_sweep() {
    let view = shell(dialog_to_sheet(
        "New project",
        text::<Msg>("Start with a prompt, an example, or an empty composition"),
        Element::row()
            .gap(Spacing::S2)
            .child(text::<Msg>("Cancel").pad_x(Spacing::S3).pad_y(Spacing::S2))
            .child(text::<Msg>("Create").pad_x(Spacing::S3).pad_y(Spacing::S2)),
    ));
    assert_clean("dialog_to_sheet", &sweep(&view, &Theme::dark()));
}

#[test]
fn the_table_pattern_passes_the_sweep() {
    let columns = [
        TableColumn::flex("Name", Breakpoint::Xs),
        TableColumn::new("Status", Breakpoint::Md, Spacing::S16),
        TableColumn::new("Owner", Breakpoint::Md, Spacing::S16),
        TableColumn::new("Updated", Breakpoint::Lg, Spacing::S16),
    ];
    let rows = (0..6).map(|index| {
        vec![
            match index {
                0 => "Alpha",
                1 => "Beta",
                2 => "Gamma",
                _ => "Delta",
            },
            "Ready",
            "Motioner",
            "11:42",
        ]
    });
    let view = shell(responsive_table::<Msg>(&columns, rows));
    assert_clean("responsive_table", &sweep(&view, &Theme::dark()));
}

#[test]
fn a_responsive_view_is_deterministic() {
    let view = start_screen(0.0);
    let mut first: Ui<StartMsg> = Ui::new(Theme::light());
    let mut second: Ui<StartMsg> = Ui::new(Theme::light());
    first.set_viewport(Size::new(1024.0, 900.0), 1.25);
    second.set_viewport(Size::new(1024.0, 900.0), 1.25);
    let mut text_a = MetricsTextSystem::new();
    let mut text_b = MetricsTextSystem::new();
    let mut frame_a = Frame::new();
    let mut frame_b = Frame::new();
    first.paint(&view, &mut frame_a, &mut text_a);
    second.paint(&view, &mut frame_b, &mut text_b);
    assert_eq!(frame_a.display_list.commands, frame_b.display_list.commands);
    assert_eq!(first.inspect(), second.inspect());
}

#[test]
fn a_non_responsive_view_still_lays_out_in_one_pass() {
    let view: Element<Msg> = Element::column()
        .key("root")
        .pad(Spacing::S4)
        .gap(Spacing::S2)
        .child(text("Hello"));
    let mut ui = Ui::new(Theme::dark());
    ui.set_viewport(Size::new(800.0, 600.0), 1.0);
    let mut text = MetricsTextSystem::new();
    let mut frame = Frame::new();
    ui.paint(&view, &mut frame, &mut text);
    assert_eq!(frame.stats.layout.responsive_passes, 1);
}

#[test]
fn a_responsive_view_settles_in_at_most_two_passes() {
    let mut ui: Ui<StartMsg> = Ui::new(Theme::dark());
    ui.set_viewport(Size::new(360.0, 900.0), 1.0);
    let mut text = MetricsTextSystem::new();
    let mut frame = Frame::new();
    ui.paint(&start_screen(0.0), &mut frame, &mut text);
    let passes = frame.stats.layout.responsive_passes;
    println!("start screen at 360 px: {passes} layout pass(es)");
    assert!(
        (1..=2).contains(&passes),
        "a responsive view settles in at most two passes, got {passes}"
    );
}

/// Prints the layout cost at every sweep width (run in release for the report).
///
/// `full layout` is one `LayoutEngine::run` on a fresh engine (the cold pass,
/// including the second responsive pass when the values move); `cold frame` is
/// `Ui::paint` from scratch (build + layout + paint); `warm relayout` is a
/// `paint` after a one-pixel viewport change, which invalidates every cached
/// taffy entry without rebuilding the view.
#[test]
fn layout_cost_by_width() {
    use guimaker::layout::LayoutEngine;
    use guimaker::store::Store;
    use guimaker::view::Tree;

    let view = start_screen(0.0);
    let tree = Tree::build(&view);
    let theme = Theme::dark();
    let mut text = MetricsTextSystem::new();
    let mut frame = Frame::new();
    for width in guimaker::sweep::WIDTHS {
        let mut layout_best = f64::MAX;
        for _ in 0..5 {
            let mut engine = LayoutEngine::new();
            engine.set_viewport(Size::new(width, guimaker::sweep::HEIGHT), 1.0);
            let store = Store::new();
            let started = Instant::now();
            engine.run(&tree, &theme, &mut text, &store);
            let elapsed = started.elapsed().as_secs_f64() * 1_000.0;
            if elapsed < layout_best {
                layout_best = elapsed;
            }
        }
        let mut cold_best = f64::MAX;
        for _ in 0..5 {
            let mut ui: Ui<StartMsg> = Ui::new(Theme::dark());
            ui.set_viewport(Size::new(width, guimaker::sweep::HEIGHT), 1.0);
            let started = Instant::now();
            ui.paint(&view, &mut frame, &mut text);
            let elapsed = started.elapsed().as_secs_f64() * 1_000.0;
            if elapsed < cold_best {
                cold_best = elapsed;
            }
        }
        let mut ui: Ui<StartMsg> = Ui::new(Theme::dark());
        ui.set_viewport(Size::new(width, guimaker::sweep::HEIGHT), 1.0);
        ui.paint(&view, &mut frame, &mut text);
        let mut relayout_best = f64::MAX;
        let mut bump = 0.0f32;
        for _ in 0..5 {
            bump = if bump == 0.0 { 1.0 } else { 0.0 };
            ui.set_viewport(Size::new(width + bump, guimaker::sweep::HEIGHT), 1.0);
            let started = Instant::now();
            ui.paint(&view, &mut frame, &mut text);
            let elapsed = started.elapsed().as_secs_f64() * 1_000.0;
            if elapsed < relayout_best {
                relayout_best = elapsed;
            }
        }
        println!(
            "width {width:>5} px: full layout {layout_best:.4} ms | cold frame {cold_best:.4} ms | warm relayout {relayout_best:.4} ms"
        );
    }
}
