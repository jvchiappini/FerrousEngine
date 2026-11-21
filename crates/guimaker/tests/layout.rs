//! Layout, hit-testing, damage and determinism acceptance tests.

use std::fs;
use std::path::PathBuf;

use guimaker::prelude::*;

fn paint<Msg: Clone>(
    ui: &mut Ui<Msg>,
    view: &Element<Msg>,
    frame: &mut Frame<Msg>,
    text: &mut dyn TextSystem,
) {
    ui.paint(view, frame, text);
}

fn snapshot(width: f32, height: f32, scale: f32) -> String {
    let view = start_screen();
    let mut ui: Ui<StartMsg> = Ui::new(Theme::dark());
    ui.set_viewport(Size::new(width, height), scale);
    let mut text = MetricsTextSystem::new();
    let mut frame = Frame::new();
    paint(&mut ui, &view, &mut frame, &mut text);
    ui.inspect()
}

fn snapshot_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/snapshots")
}

#[test]
fn layout_snapshots_at_three_sizes_and_two_scales() {
    let update = std::env::var_os("GUIMAKER_UPDATE_SNAPSHOTS").is_some();
    let dir = snapshot_dir();
    fs::create_dir_all(&dir).expect("snapshot directory");
    for (name, width, height, scale) in [
        ("w1280_s1", 1280.0, 800.0, 1.0),
        ("w1280_s15", 1280.0, 800.0, 1.5),
        ("w1920_s1", 1920.0, 1080.0, 1.0),
        ("w1920_s15", 1920.0, 1080.0, 1.5),
        ("w800_s1", 800.0, 600.0, 1.0),
        ("w800_s15", 800.0, 600.0, 1.5),
    ] {
        let actual = snapshot(width, height, scale);
        let path = dir.join(format!("{name}.txt"));
        if update {
            fs::write(&path, actual.as_bytes()).expect("write snapshot");
        } else {
            let expected = fs::read_to_string(&path).unwrap_or_else(|_| {
                panic!("missing snapshot {path:?}; run with GUIMAKER_UPDATE_SNAPSHOTS=1")
            });
            assert_eq!(actual, expected, "snapshot {name} changed");
        }
    }
}

#[test]
fn every_rect_is_snapped_to_the_physical_grid() {
    for scale in [1.0f32, 1.25, 1.5, 2.0] {
        let view = start_screen();
        let mut ui: Ui<StartMsg> = Ui::new(Theme::dark());
        ui.set_viewport(Size::new(1920.0, 1080.0), scale);
        let mut text = MetricsTextSystem::new();
        let mut frame = Frame::new();
        paint(&mut ui, &view, &mut frame, &mut text);
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
fn text_wraps_clamps_and_ellipsises() {
    let view: Element<()> = Element::column()
        .key("root")
        .width(SizeSpec::Token(Spacing::S24))
        .child(
            text("The quick brown fox jumps over the lazy dog")
                .key("wrapped")
                .text_role(TypeRole::Body),
        )
        .child(
            text("one two three four five six seven eight nine ten")
                .key("clamped")
                .text_role(TypeRole::Body)
                .lines(2)
                .ellipsis(Ellipsis::End),
        );
    let mut ui: Ui<()> = Ui::new(Theme::dark());
    ui.set_viewport(Size::new(400.0, 400.0), 1.0);
    let mut text = MetricsTextSystem::new();
    let mut frame = Frame::new();
    paint(&mut ui, &view, &mut frame, &mut text);
    let wrapped = ui.tree().find_key("wrapped").expect("wrapped");
    let clamped = ui.tree().find_key("clamped").expect("clamped");
    let wrapped_shape = ui.layout().shape(wrapped).expect("wrapped shape");
    let clamped_shape = ui.layout().shape(clamped).expect("clamped shape");
    assert!(wrapped_shape.lines.len() > 1);
    assert_eq!(clamped_shape.lines.len(), 2);
    assert!(clamped_shape.truncated);
    assert_eq!(
        clamped_shape.glyphs.last().map(|glyph| glyph.ch),
        Some('\u{2026}')
    );
}

#[test]
fn a_click_on_the_create_button_emits_new_project() {
    let view = start_screen();
    let mut ui: Ui<StartMsg> = Ui::new(Theme::dark());
    ui.set_viewport(Size::new(1920.0, 1080.0), 1.5);
    let mut text = MetricsTextSystem::new();
    let mut frame = Frame::new();
    paint(&mut ui, &view, &mut frame, &mut text);
    let create = ui.tree().find_key("create").expect("create");
    let rect = ui.layout().rect(create);
    let point = rect.origin() + glam::Vec2::new(rect.width / 2.0, rect.height / 2.0);
    ui.push_event(InputEvent::PointerMoved { position: point });
    ui.push_event(InputEvent::PointerDown {
        position: point,
        button: PointerButton::Primary,
    });
    ui.push_event(InputEvent::PointerUp {
        position: point,
        button: PointerButton::Primary,
    });
    paint(&mut ui, &view, &mut frame, &mut text);
    assert_eq!(frame.messages, vec![StartMsg::NewProject]);
}

#[test]
fn the_topmost_overlay_wins_and_clipped_children_do_not() {
    let view: Element<()> = Element::stack()
        .key("root")
        .width(SizeSpec::Token(Spacing::S24))
        .height(SizeSpec::Token(Spacing::S24))
        .child(Element::canvas().key("under"))
        .child(Element::canvas().key("over"));
    let mut ui: Ui<()> = Ui::new(Theme::dark());
    ui.set_viewport(Size::new(200.0, 200.0), 1.0);
    let mut text = MetricsTextSystem::new();
    let mut frame = Frame::new();
    paint(&mut ui, &view, &mut frame, &mut text);
    let over = ui.tree().find_key("over").expect("over");
    assert_eq!(
        ui.hit_index_mut().pick(glam::Vec2::new(10.0, 10.0), 0),
        Some(over)
    );

    let clipped: Element<()> = Element::column()
        .key("root")
        .width(SizeSpec::Token(Spacing::S12))
        .height(SizeSpec::Token(Spacing::S12))
        .clip()
        .child(
            Element::canvas()
                .key("inside")
                .width(SizeSpec::Token(Spacing::S8))
                .height(SizeSpec::Token(Spacing::S8)),
        );
    let mut ui: Ui<()> = Ui::new(Theme::dark());
    ui.set_viewport(Size::new(200.0, 200.0), 1.0);
    paint(&mut ui, &clipped, &mut frame, &mut text);
    let inside = ui.tree().find_key("inside").expect("inside");
    assert_eq!(
        ui.hit_index_mut().pick(glam::Vec2::new(4.0, 4.0), 0),
        Some(inside)
    );
    assert_eq!(
        ui.hit_index_mut().pick(glam::Vec2::new(150.0, 150.0), 0),
        None
    );
}

#[test]
fn describe_a_mistake_in_plain_english() {
    let view: Element<()> = Element::row()
        .key("shell")
        .width(SizeSpec::Token(Spacing::S12))
        .child(
            Element::column()
                .key("sidebar")
                .width(SizeSpec::Token(Spacing::S24))
                .shrink(0.0)
                .child(text("x")),
        )
        .child(
            Element::column()
                .key("content")
                .width(SizeSpec::Fill)
                .child(text("y")),
        );
    let mut ui: Ui<()> = Ui::new(Theme::dark());
    ui.set_viewport(Size::new(400.0, 300.0), 1.0);
    let mut text = MetricsTextSystem::new();
    let mut frame = Frame::new();
    paint(&mut ui, &view, &mut frame, &mut text);
    let explanation = ui.explain("sidebar").expect("explanation");
    assert!(explanation.contains("`sidebar` is"), "{explanation}");
    assert!(explanation.contains("Fix:"), "{explanation}");
}

#[test]
fn a_still_frame_reuses_the_display_list_and_reports_no_damage() {
    let view = start_screen();
    let mut ui: Ui<StartMsg> = Ui::new(Theme::dark());
    ui.set_viewport(Size::new(1920.0, 1080.0), 1.5);
    let mut text = MetricsTextSystem::new();
    let mut frame = Frame::new();
    paint(&mut ui, &view, &mut frame, &mut text);
    let first_commands = frame.display_list.commands.len();
    paint(&mut ui, &view, &mut frame, &mut text);
    assert!(!frame.stats.repainted);
    assert!(frame.display_list.damage.is_empty());
    assert_eq!(frame.display_list.commands.len(), first_commands);
    assert_eq!(frame.stats.layout.style_updates, 0);
    assert_eq!(frame.stats.layout.measure_calls, 0);
}

#[test]
fn changing_one_label_damages_its_subtree_only() {
    let first: Element<StartMsg> = Element::column()
        .key("root")
        .child(text::<StartMsg>("Motioner").key("title"))
        .child(text::<StartMsg>("Footer").key("footer"));
    let second: Element<StartMsg> = Element::column()
        .key("root")
        .child(text::<StartMsg>("Motioner 2").key("title"))
        .child(text::<StartMsg>("Footer").key("footer"));
    let mut ui: Ui<StartMsg> = Ui::new(Theme::dark());
    ui.set_viewport(Size::new(400.0, 300.0), 1.0);
    let mut text = MetricsTextSystem::new();
    let mut frame = Frame::new();
    paint(&mut ui, &first, &mut frame, &mut text);
    paint(&mut ui, &second, &mut frame, &mut text);
    assert!(frame.stats.repainted);
    assert!(!frame.display_list.damage.is_empty());
    assert!(frame.stats.layout.style_updates <= 4);
    assert!(frame.stats.layout.reused >= 1);
}

#[test]
fn two_instances_produce_identical_commands() {
    let view = start_screen();
    let mut text_a = MetricsTextSystem::new();
    let mut text_b = MetricsTextSystem::new();
    let mut first: Ui<StartMsg> = Ui::new(Theme::light());
    let mut second: Ui<StartMsg> = Ui::new(Theme::light());
    first.set_viewport(Size::new(1600.0, 900.0), 1.25);
    second.set_viewport(Size::new(1600.0, 900.0), 1.25);
    let mut frame_a = Frame::new();
    let mut frame_b = Frame::new();
    first.paint(&view, &mut frame_a, &mut text_a);
    second.paint(&view, &mut frame_b, &mut text_b);
    assert_eq!(frame_a.display_list.commands, frame_b.display_list.commands);
    assert_eq!(frame_a.display_list.damage, frame_b.display_list.damage);
    assert_eq!(first.inspect(), second.inspect());
}
