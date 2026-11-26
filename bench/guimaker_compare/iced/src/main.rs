//! iced side of the Phase 9B comparison: the start-screen recipe on iced 0.13.
//! Run with `--seconds N` (default 8); `frames N` is printed to stdout on exit.
//!
//! Frames come from `iced::window::frames()` (the subscription tracks the
//! window refresh rate). iced's `Application::scale_factor` lets us force 1.5,
//! and the window size is 1280x800 logical. vsync/present mode is owned by
//! iced's wgpu backend (Fifo/AutoVsync), so there is no knob to set here.
//! The process is stopped with `std::process::exit` because iced 0.13 no longer
//! exposes a global exit and the bench only needs the counter on stdout.

use std::io::Write;
use std::sync::{
    atomic::{AtomicU64, Ordering},
    Arc,
};
use std::time::{Duration, Instant};

use iced::widget::{button, column, container, horizontal_space, row, stack, text, text_input, Space};
use iced::{Alignment, Background, Border, Color, Element, Length, Padding, Task, Theme};

const PAGE: Color = Color {
    r: 6.0 / 255.0,
    g: 8.0 / 255.0,
    b: 13.0 / 255.0,
    a: 1.0,
};
const CARD: Color = Color {
    r: 15.0 / 255.0,
    g: 18.0 / 255.0,
    b: 23.0 / 255.0,
    a: 1.0,
};
const WELL: Color = Color {
    r: 11.0 / 255.0,
    g: 13.0 / 255.0,
    b: 19.0 / 255.0,
    a: 1.0,
};
const RAISED: Color = Color {
    r: 24.0 / 255.0,
    g: 27.0 / 255.0,
    b: 32.0 / 255.0,
    a: 1.0,
};
const BORDER: Color = Color {
    r: 38.0 / 255.0,
    g: 41.0 / 255.0,
    b: 47.0 / 255.0,
    a: 1.0,
};
const TEXT: Color = Color {
    r: 241.0 / 255.0,
    g: 245.0 / 255.0,
    b: 254.0 / 255.0,
    a: 1.0,
};
const MUTED: Color = Color {
    r: 173.0 / 255.0,
    g: 177.0 / 255.0,
    b: 185.0 / 255.0,
    a: 1.0,
};
const FAINT: Color = Color {
    r: 113.0 / 255.0,
    g: 117.0 / 255.0,
    b: 124.0 / 255.0,
    a: 1.0,
};
const ACCENT: Color = Color {
    r: 39.0 / 255.0,
    g: 80.0 / 255.0,
    b: 199.0 / 255.0,
    a: 1.0,
};

struct App {
    started: Instant,
    frames: Arc<AtomicU64>,
    bench_input: String,
}

#[derive(Debug, Clone)]
enum Message {
    Settings,
    Docs,
    Create,
    Browse,
    BenchInput(String),
    BenchButton,
}

fn update(state: &mut App, message: Message) {
    match message {
        Message::BenchInput(value) => state.bench_input = value,
        Message::Settings
        | Message::Docs
        | Message::Create
        | Message::Browse
        | Message::BenchButton => {}
    }
}

fn tc(color: Color) -> impl Fn(&Theme) -> text::Style {
    move |_| text::Style { color: Some(color) }
}

fn page_style(_: &Theme) -> container::Style {
    container::Style {
        background: Some(Background::Color(PAGE)),
        ..Default::default()
    }
}

fn card_style(_: &Theme) -> container::Style {
    container::Style {
        background: Some(Background::Color(CARD)),
        border: Border {
            color: BORDER,
            width: 1.0,
            radius: 8.0.into(),
        },
        ..Default::default()
    }
}

fn well_style(_: &Theme) -> container::Style {
    container::Style {
        background: Some(Background::Color(WELL)),
        border: Border {
            color: BORDER,
            width: 1.0,
            radius: 6.0.into(),
        },
        ..Default::default()
    }
}

fn divider_style(_: &Theme) -> container::Style {
    container::Style {
        background: Some(Background::Color(BORDER)),
        ..Default::default()
    }
}

fn accent_button(_: &Theme, _status: button::Status) -> button::Style {
    button::Style {
        background: Some(Background::Color(ACCENT)),
        text_color: TEXT,
        border: Border {
            color: ACCENT,
            width: 0.0,
            radius: 8.0.into(),
        },
        ..Default::default()
    }
}

fn raised_button(_: &Theme, _status: button::Status) -> button::Style {
    button::Style {
        background: Some(Background::Color(RAISED)),
        text_color: TEXT,
        border: Border {
            color: BORDER,
            width: 1.0,
            radius: 8.0.into(),
        },
        ..Default::default()
    }
}

fn bench_button(_: &Theme, status: button::Status) -> button::Style {
    let background = match status {
        button::Status::Hovered | button::Status::Pressed => BORDER,
        _ => RAISED,
    };
    button::Style {
        background: Some(Background::Color(background)),
        text_color: TEXT,
        border: Border {
            color: BORDER,
            width: 1.0,
            radius: 8.0.into(),
        },
        ..Default::default()
    }
}

/// Bench-only control strip: the same logical coordinates in every app, so the
/// harness can park the pointer on a known point.
fn bench_label() -> Element<'static, Message> {
    container(text("Bench input").size(12).style(tc(MUTED)))
        .width(Length::Fill)
        .height(Length::Fill)
        .align_x(Alignment::Start)
        .align_y(Alignment::Start)
        .padding(Padding {
            top: 704.0,
            right: 0.0,
            bottom: 0.0,
            left: 40.0,
        })
        .into()
}

fn bench_strip(state: &App) -> Element<'_, Message> {
    container(
        row![
            container(
                text_input("", &state.bench_input)
                    .on_input(Message::BenchInput)
                    .width(Length::Fill)
                    .padding(10),
            )
            .width(Length::Fixed(320.0))
            .height(Length::Fixed(36.0))
            .style(well_style),
            button(text("Bench button").size(14))
                .width(Length::Fixed(140.0))
                .height(Length::Fixed(36.0))
                .style(bench_button)
                .on_press(Message::BenchButton),
        ]
        .spacing(20),
    )
    .width(Length::Fill)
    .height(Length::Fill)
    .align_x(Alignment::Start)
    .align_y(Alignment::End)
    .padding(Padding {
        top: 0.0,
        right: 0.0,
        bottom: 40.0,
        left: 40.0,
    })
    .into()
}

fn divider() -> Element<'static, Message> {
    container(Space::new(Length::Fill, Length::Fixed(1.0)))
        .width(Length::Fill)
        .height(Length::Fixed(1.0))
        .style(divider_style)
        .into()
}

fn header() -> Element<'static, Message> {
    container(
        row![
            column![
                text("Motioner").size(24).style(tc(TEXT)),
                text("Code-driven motion graphics")
                    .size(13)
                    .style(tc(MUTED)),
            ]
            .spacing(2),
            horizontal_space(),
            row![
                text("Settings").size(14).style(tc(MUTED)),
                text("Docs").size(14).style(tc(MUTED)),
            ]
            .spacing(18),
        ]
        .align_y(Alignment::Center),
    )
    .width(Length::Fill)
    .into()
}

fn card(n: usize) -> Element<'static, Message> {
    container(
        row![
            column![
                text(format!("Project {n}")).size(15).style(tc(TEXT)),
                text("Edited recently").size(12).style(tc(MUTED)),
            ]
            .spacing(2),
            horizontal_space(),
            text("11:42").size(12).style(tc(FAINT)),
        ]
        .align_y(Alignment::Center),
    )
    .width(Length::Fill)
    .padding(14)
    .style(card_style)
    .into()
}

fn field(label: &'static str, value: &'static str) -> Element<'static, Message> {
    column![
        text(label).size(12).style(tc(MUTED)),
        container(text(value).size(14).style(tc(TEXT)))
            .width(Length::Fill)
            .padding(10)
            .style(well_style),
    ]
    .spacing(5)
    .into()
}

fn actions() -> Element<'static, Message> {
    row![
        button(text("Create").size(14))
            .padding(10)
            .style(accent_button)
            .on_press(Message::Create),
        button(text("Browse").size(14))
            .padding(10)
            .style(raised_button)
            .on_press(Message::Browse),
    ]
    .spacing(12)
    .into()
}

fn recent_projects() -> Element<'static, Message> {
    column![
        text("Recent projects").size(18).style(tc(TEXT)),
        card(1),
        card(2),
        card(3),
        card(4),
        card(5),
    ]
    .spacing(10)
    .into()
}

fn new_project_form() -> Element<'static, Message> {
    column![
        text("New project").size(18).style(tc(TEXT)),
        text("Start from a blank canvas or a starter composition,")
            .size(13)
            .style(tc(MUTED)),
        text("then refine every keyframe.")
            .size(13)
            .style(tc(MUTED)),
        field("Name", "Untitled"),
        field("Size", "1920 x 1080"),
        field("Frame rate", "60 fps"),
        field("Engine", "FerrousEngine"),
        actions(),
    ]
    .spacing(8)
    .into()
}

fn footer() -> Element<'static, Message> {
    container(
        row![
            text("FerrousEngine 0.1").size(12).style(tc(FAINT)),
            horizontal_space(),
            text("Apache-2.0").size(12).style(tc(FAINT)),
        ],
    )
    .width(Length::Fill)
    .into()
}

fn view(state: &App) -> Element<'_, Message> {
    let frame = state.frames.fetch_add(1, Ordering::Relaxed) + 1;
    if frame == 1 {
        println!(
            "startup_ms {}",
            state.started.elapsed().as_secs_f64() * 1000.0
        );
        println!("bench_ready");
        let _ = std::io::stdout().flush();
    }
    let body = row![
        container(recent_projects())
            .width(Length::FillPortion(55))
            .height(Length::Fill)
            .align_x(Alignment::Start)
            .align_y(Alignment::Start),
        container(new_project_form())
            .width(Length::FillPortion(45))
            .height(Length::Fill)
            .align_x(Alignment::Start)
            .align_y(Alignment::Start),
    ]
    .spacing(20)
    .height(Length::Fill);

    let page: Element<'_, Message> = container(
        column![header(), divider(), body, footer()]
            .spacing(14)
            .height(Length::Fill),
    )
    .width(Length::Fill)
    .height(Length::Fill)
    .padding(24)
    .style(page_style)
    .into();

    stack([page, bench_label(), bench_strip(state)]).into()
}

fn parse_seconds() -> f64 {
    std::env::args()
        .skip_while(|arg| arg != "--seconds")
        .nth(1)
        .and_then(|value| value.parse().ok())
        .unwrap_or(8.0)
}

pub fn main() -> iced::Result {
    let seconds = parse_seconds();

    iced::application("guimaker-compare-iced", update, view)
        .theme(|_| Theme::Dark)
        .window_size(iced::Size::new(1280.0, 800.0))
        .resizable(false)
        .scale_factor(|_| 1.5)
        .antialiasing(true)
        .run_with(move || {
            let frames = Arc::new(AtomicU64::new(0));
            let counter = Arc::clone(&frames);
            // A watchdog: iced has no global exit, and an idle window must not
            // be kept awake just to observe the clock.
            std::thread::spawn(move || {
                std::thread::sleep(Duration::from_secs_f64(seconds));
                println!("frames {}", counter.load(Ordering::Relaxed));
                let _ = std::io::stdout().flush();
                std::process::exit(0);
            });
            (
                App {
                    started: Instant::now(),
                    frames,
                    bench_input: String::new(),
                },
                Task::none(),
            )
        })
}
