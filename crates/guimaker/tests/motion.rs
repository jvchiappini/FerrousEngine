//! Phase 7 motion acceptance tests: overlay scrollbars, focus-ring fade,
//! caret blink and the idle zero-frame budget.

#![allow(clippy::float_cmp)] // reason: exact token arithmetic in tests

use glam::Vec2;
use guimaker::prelude::*;
use guimaker::theme::Duration;

fn paint<Msg: Clone>(
    ui: &mut Ui<Msg>,
    view: &Element<Msg>,
    frame: &mut Frame<Msg>,
    text: &mut dyn TextSystem,
) {
    ui.paint(view, frame, text);
}

fn scroll_view() -> Element<()> {
    Element::column()
        .key("root")
        .width(SizeSpec::Fill)
        .height(SizeSpec::Fill)
        .child(
            Element::column()
                .key("scroll")
                .width(SizeSpec::Fill)
                .height(SizeSpec::Px(100.0))
                .scroll_axes(ScrollAxes::Vertical)
                .interactive()
                .child(
                    Element::column()
                        .width(SizeSpec::Fill)
                        .height(SizeSpec::Px(1_000.0))
                        .shrink(0.0),
                ),
        )
}

fn thumbs(frame: &Frame<()>, node: NodeId) -> Vec<Rect> {
    frame
        .display_list
        .commands
        .iter()
        .filter_map(|command| match command {
            DrawCommand::Rect {
                node: painted,
                rect,
                ..
            } if *painted == node => Some(*rect),
            _ => None,
        })
        .collect()
}

#[test]
fn an_active_scroll_container_paints_one_overlay_thumb() {
    let view = scroll_view();
    let mut ui: Ui<()> = Ui::new(Theme::dark());
    ui.set_viewport(Size::new(200.0, 100.0), 1.0);
    let mut text = MetricsTextSystem::new();
    let mut frame = Frame::new();
    paint(&mut ui, &view, &mut frame, &mut text);

    let scroll = ui.tree().find_key("scroll").expect("scroll");
    let rect = ui.layout().rect(scroll);
    let margin = Spacing::S1.px();
    let width = Spacing::S1.px();

    ui.push_event(InputEvent::PointerMoved {
        position: Vec2::new(rect.x + 10.0, rect.y + 10.0),
    });
    ui.set_time_ms(16.0);
    paint(&mut ui, &view, &mut frame, &mut text);

    let painted = thumbs(&frame, scroll);
    assert_eq!(painted.len(), 1, "one vertical thumb");
    assert_eq!(painted[0].width, width);
    assert_eq!(painted[0].x, rect.right() - margin - width);
    assert!(painted[0].height >= Spacing::S4.px());
}

#[test]
fn an_idle_scroll_container_paints_no_thumb() {
    let view = scroll_view();
    let mut ui: Ui<()> = Ui::new(Theme::dark());
    ui.set_viewport(Size::new(200.0, 100.0), 1.0);
    let mut text = MetricsTextSystem::new();
    let mut frame = Frame::new();
    paint(&mut ui, &view, &mut frame, &mut text);

    let scroll = ui.tree().find_key("scroll").expect("scroll");
    assert!(thumbs(&frame, scroll).is_empty());
}

fn focus_view() -> Element<()> {
    Element::column()
        .key("root")
        .width(SizeSpec::Fill)
        .height(SizeSpec::Fill)
        .child(
            Element::row()
                .key("button")
                .width(SizeSpec::Token(Spacing::S24))
                .height(SizeSpec::Token(Spacing::S8))
                .bg(ColorRole::Surface)
                .hover_bg(ColorRole::SurfaceRaised)
                .interactive()
                .transition(Duration::Fast),
        )
}

fn ring_alpha(frame: &Frame<()>, node: NodeId) -> Option<f32> {
    frame
        .display_list
        .commands
        .iter()
        .find_map(|command| match command {
            DrawCommand::Border {
                node: painted,
                color,
                ..
            } if *painted == node => Some(color.alpha()),
            _ => None,
        })
}

#[test]
fn the_focus_ring_fades_in_on_keyboard_focus_and_never_on_pointer_focus() {
    let view = focus_view();
    let mut ui: Ui<()> = Ui::new(Theme::dark());
    ui.set_viewport(Size::new(200.0, 100.0), 1.0);
    let mut text = MetricsTextSystem::new();
    let mut frame = Frame::new();
    paint(&mut ui, &view, &mut frame, &mut text);
    let button = ui.tree().find_key("button").expect("button");

    ui.push_event(InputEvent::KeyDown {
        key: ShortcutKey::Tab,
        repeat: false,
    });
    ui.set_time_ms(16.0);
    paint(&mut ui, &view, &mut frame, &mut text);
    let early = ring_alpha(&frame, button).unwrap_or(0.0);
    assert!(early < 0.1, "the ring starts near zero, got {early}");

    ui.set_time_ms(50.0);
    paint(&mut ui, &view, &mut frame, &mut text);
    let middle = ring_alpha(&frame, button).expect("a ring mid fade");
    assert!(middle > 0.0 && middle < 1.0, "mid alpha {middle}");

    ui.set_time_ms(200.0);
    paint(&mut ui, &view, &mut frame, &mut text);
    let settled = ring_alpha(&frame, button).expect("the ring settled");
    assert!((settled - 1.0).abs() < 1e-3, "settled alpha {settled}");

    let rect = ui.layout().rect(button);
    ui.push_event(InputEvent::PointerDown {
        position: Vec2::new(rect.x + rect.width * 0.5, rect.y + rect.height * 0.5),
        button: PointerButton::Primary,
    });
    ui.set_time_ms(220.0);
    paint(&mut ui, &view, &mut frame, &mut text);
    assert!(
        ring_alpha(&frame, button).is_none(),
        "pointer focus paints no keyboard ring"
    );
}

#[test]
fn idle_without_focus_renders_zero_frames() {
    let view: Element<()> = Element::column()
        .width(SizeSpec::Fill)
        .height(SizeSpec::Fill)
        .child(text("still"));
    let mut ui: Ui<()> = Ui::new(Theme::dark());
    ui.set_viewport(Size::new(200.0, 100.0), 1.0);
    let mut text_system = MetricsTextSystem::new();
    let mut frame = Frame::new();

    let mut deadline = ui.next_wake_ms(0.0);
    let mut now = 0.0;
    for _ in 0..=312 {
        ui.set_time_ms(now);
        if let Some(due) = deadline {
            if now >= due {
                paint(&mut ui, &view, &mut frame, &mut text_system);
                deadline = ui.next_wake_ms(now);
            }
        }
        now += 16.0;
    }
    assert!(deadline.is_none());
    assert_eq!(ui.frames_painted(), 0, "an idle view never paints");
    assert!(!ui.animating());
    assert!(ui.next_wake_ms(now).is_none());
}

#[test]
fn a_focused_caret_wakes_once_per_blink() {
    let view: Element<()> = text("hello").caret(1);
    let mut ui: Ui<()> = Ui::new(Theme::dark());
    ui.set_viewport(Size::new(200.0, 100.0), 1.0);
    ui.input_mut().keyboard_focus = true;
    let mut text_system = MetricsTextSystem::new();
    let mut frame = Frame::new();

    // The first paint establishes the layout and the initial visible phase.
    ui.set_time_ms(0.0);
    paint(&mut ui, &view, &mut frame, &mut text_system);
    assert_eq!(ui.frames_painted(), 1);
    assert!(ui.has_caret());

    let mut deadline = ui.next_wake_ms(0.0).expect("the caret schedules a wake");
    let mut boundary_paints = 0_u32;
    for step in 1_i32..=313 {
        let now = f64::from(step) * 16.0;
        ui.set_time_ms(now);
        if now >= deadline {
            paint(&mut ui, &view, &mut frame, &mut text_system);
            boundary_paints += 1;
            deadline = ui.next_wake_ms(now).expect("the caret keeps blinking");
        }
    }
    // 5.008 s crosses ten half cycles of a 500 ms blink, plus the initial
    // paint at t = 0.
    let expected_boundaries = 10_u32;
    assert_eq!(boundary_paints, expected_boundaries);
    assert_eq!(ui.frames_painted(), 1 + u64::from(boundary_paints));
}

#[test]
fn an_animation_wakes_only_while_it_runs() {
    let view = focus_view();
    let mut ui: Ui<()> = Ui::new(Theme::dark());
    ui.set_viewport(Size::new(200.0, 100.0), 1.0);
    let mut text = MetricsTextSystem::new();
    let mut frame = Frame::new();
    paint(&mut ui, &view, &mut frame, &mut text);

    let button = ui.tree().find_key("button").expect("button");
    let rect = ui.layout().rect(button);
    ui.push_event(InputEvent::PointerMoved {
        position: Vec2::new(rect.x + 8.0, rect.y + rect.height * 0.5),
    });
    ui.set_time_ms(16.0);
    paint(&mut ui, &view, &mut frame, &mut text);
    assert!(ui.animating(), "the hover fade is running");

    let mut now = 16.0;
    let mut frames_during = 0;
    while ui.animating() {
        now += 16.0;
        ui.set_time_ms(now);
        paint(&mut ui, &view, &mut frame, &mut text);
        frames_during += 1;
        assert!(now < 5_000.0, "the hover fade never settled");
    }
    assert!(frames_during > 1, "the fade spans several frames");
    assert!(!ui.animating());

    let painted = ui.frames_painted();
    let mut deadline = ui.next_wake_ms(now);
    for step in 1_i32..=100 {
        let later = now + f64::from(step) * 16.0;
        ui.set_time_ms(later);
        if let Some(due) = deadline {
            if later >= due {
                paint(&mut ui, &view, &mut frame, &mut text);
                deadline = ui.next_wake_ms(later);
            }
        }
    }
    assert_eq!(ui.frames_painted(), painted, "no frames after the fade");
}

#[test]
fn an_open_popup_renders_zero_idle_frames() {
    // An open menu is a static layer: opening it paints once, then the host has
    // nothing to do until something changes. This is the popup's half of the
    // "idle is zero frames" contract; the popup window uses the same frame loop.
    let view: Element<()> = Element::stack()
        .width(SizeSpec::Fill)
        .height(SizeSpec::Fill)
        .child(
            Element::row()
                .width(SizeSpec::Fill)
                .height(SizeSpec::Fill)
                .bg(ColorRole::Surface),
        )
        .child(
            Element::overlay().child(
                Element::column()
                    .key("open_popup")
                    .width(SizeSpec::Token(Spacing::S12))
                    .height(SizeSpec::Token(Spacing::S12))
                    .bg(ColorRole::SurfaceRaised),
            ),
        );
    let mut ui: Ui<()> = Ui::new(Theme::dark());
    ui.set_viewport(Size::new(320.0, 200.0), 1.0);
    let mut text_system = MetricsTextSystem::new();
    let mut frame = Frame::new();

    paint(&mut ui, &view, &mut frame, &mut text_system);
    let painted_once = ui.frames_painted();

    let mut deadline = ui.next_wake_ms(0.0);
    let mut now = 0.0;
    for _ in 0..=312 {
        ui.set_time_ms(now);
        if let Some(due) = deadline {
            if now >= due {
                paint(&mut ui, &view, &mut frame, &mut text_system);
                deadline = ui.next_wake_ms(now);
            }
        }
        now += 16.0;
    }
    assert!(deadline.is_none(), "an open popup schedules no wake");
    assert_eq!(
        ui.frames_painted(),
        painted_once,
        "an idle open popup paints no extra frames"
    );
}
