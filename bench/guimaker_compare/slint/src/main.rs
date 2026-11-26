//! Slint side of the Phase 9B comparison: the start-screen recipe on Slint 1.7.
//! Run with `--seconds N` (default 8); `frames N` is printed to stdout on exit.
//!
//! The whole UI lives in the `slint!` macro (no build.rs). Slint only repaints
//! on demand, so a 60 Hz Rust `Timer` requests a redraw and counts view ticks;
//! that tick count is the reported "frames". `SLINT_SCALE_FACTOR=1.5` is
//! Slint's documented-but-unstable scale override (see slint-ui/slint#4418),
//! and the window is 1280x800 logical.

use std::io::Write;
use std::sync::{
    atomic::{AtomicU64, Ordering},
    Arc,
};
use std::time::{Duration, Instant};

use slint::ComponentHandle;

slint::slint! {
    component Field inherits VerticalLayout {
        in property <string> label;
        in property <string> value;
        spacing: 4px;

        Text {
            text: root.label;
            color: #ADB1B9;
            font-size: 12px;
        }
        Rectangle {
            height: 36px;
            background: #0B0D13;
            border-radius: 6px;
            border-width: 1px;
            border-color: #26292F;

            Text {
                x: 12px;
                y: 0px;
                width: parent.width - 24px;
                height: parent.height;
                text: root.value;
                color: #F1F5FE;
                font-size: 14px;
                vertical-alignment: center;
            }
        }
    }

    component Card inherits Rectangle {
        in property <string> title;
        height: 56px;
        background: #0F1217;
        border-radius: 8px;
        border-width: 1px;
        border-color: #26292F;

        HorizontalLayout {
            x: 14px;
            y: 0px;
            width: parent.width - 28px;
            height: parent.height;
            spacing: 8px;
            alignment: center;

            VerticalLayout {
                spacing: 2px;
                Text {
                    text: root.title;
                    color: #F1F5FE;
                    font-size: 15px;
                }
                Text {
                    text: "Edited recently";
                    color: #ADB1B9;
                    font-size: 12px;
                }
            }
            Rectangle { horizontal-stretch: 1; }
            Text {
                text: "11:42";
                color: #71757C;
                font-size: 12px;
            }
        }
    }

    export component AppWindow inherits Window {
        title: "guimaker-compare-slint";
        width: 1280px;
        height: 800px;
        background: #06080D;
        in-out property <string> bench-text;

        VerticalLayout {
            x: 0;
            y: 0;
            width: parent.width;
            height: parent.height;
            padding-left: 28px;
            padding-right: 28px;
            padding-top: 18px;
            padding-bottom: 18px;
            spacing: 12px;

            // 1. Header.
            HorizontalLayout {
                spacing: 16px;
                alignment: start;
                VerticalLayout {
                    spacing: 2px;
                    Text { text: "Motioner"; color: #F1F5FE; font-size: 24px; font-weight: 700; }
                    Text { text: "Code-driven motion graphics"; color: #ADB1B9; font-size: 13px; }
                }
                Rectangle { horizontal-stretch: 1; }
                HorizontalLayout {
                    spacing: 18px;
                    alignment: center;
                    Text { text: "Settings"; color: #ADB1B9; font-size: 14px; }
                    Text { text: "Docs"; color: #ADB1B9; font-size: 14px; }
                }
            }

            // 2. Divider.
            Rectangle { height: 1px; background: #26292F; }

            // 3. Body: 55% recent projects / 45% new project form.
            HorizontalLayout {
                vertical-stretch: 1;
                spacing: 20px;

                VerticalLayout {
                    horizontal-stretch: 55;
                    spacing: 10px;
                    Text { text: "Recent projects"; color: #F1F5FE; font-size: 18px; font-weight: 600; }
                    for t in ["Project 1", "Project 2", "Project 3", "Project 4", "Project 5"]: Card { title: t; }
                }

                VerticalLayout {
                    horizontal-stretch: 45;
                    spacing: 10px;
                    Text { text: "New project"; color: #F1F5FE; font-size: 18px; font-weight: 600; }
                    Text {
                        text: "Start from a blank canvas or a starter composition, then refine every keyframe.";
                        color: #ADB1B9;
                        font-size: 13px;
                        wrap: word-wrap;
                    }
                    Field { label: "Name"; value: "Untitled"; }
                    Field { label: "Size"; value: "1920 x 1080"; }
                    Field { label: "Frame rate"; value: "60 fps"; }
                    Field { label: "Engine"; value: "FerrousEngine"; }

                    HorizontalLayout {
                        spacing: 12px;
                        Rectangle {
                            width: 120px;
                            height: 38px;
                            background: #2750C7;
                            border-radius: 8px;
                            Text {
                                text: "Create";
                                color: #F1F5FE;
                                font-size: 14px;
                                width: parent.width;
                                height: parent.height;
                                horizontal-alignment: center;
                                vertical-alignment: center;
                            }
                        }
                        Rectangle {
                            width: 120px;
                            height: 38px;
                            background: #181B20;
                            border-radius: 8px;
                            border-width: 1px;
                            border-color: #26292F;
                            Text {
                                text: "Browse";
                                color: #F1F5FE;
                                font-size: 14px;
                                width: parent.width;
                                height: parent.height;
                                horizontal-alignment: center;
                                vertical-alignment: center;
                            }
                        }
                    }
                }
            }

            // 4. Footer.
            HorizontalLayout {
                Text { text: "FerrousEngine 0.1"; color: #71757C; font-size: 12px; }
                Rectangle { horizontal-stretch: 1; }
                Text { text: "Apache-2.0"; color: #71757C; font-size: 12px; }
            }
        }

        // Bench-only control strip below the start screen: one editable text
        // field and one button whose background changes on hover. Same logical
        // coordinates in every app so the harness can park the pointer.
        Text {
            x: 40px;
            y: parent.height - 92px;
            text: "Bench input";
            color: #ADB1B9;
            font-size: 12px;
        }

        HorizontalLayout {
            x: 40px;
            y: parent.height - 76px;
            width: 480px;
            height: 36px;
            spacing: 20px;

            Rectangle {
                width: 320px;
                height: 36px;
                background: #0B0D13;
                border-radius: 6px;
                border-width: 1px;
                border-color: #26292F;

                TextInput {
                    x: 10px;
                    y: 0px;
                    width: parent.width - 20px;
                    height: parent.height;
                    text <=> root.bench-text;
                    single-line: true;
                    color: #F1F5FE;
                    font-size: 14px;
                    vertical-alignment: center;
                }
            }

            Rectangle {
                width: 140px;
                height: 36px;
                background: bench-btn.has-hover ? #26292F : #181B20;
                border-radius: 8px;
                border-width: 1px;
                border-color: #26292F;

                Text {
                    text: "Bench button";
                    color: #F1F5FE;
                    font-size: 14px;
                    width: parent.width;
                    height: parent.height;
                    horizontal-alignment: center;
                    vertical-alignment: center;
                }
                bench-btn := TouchArea { }
            }
        }
    }
}

fn parse_seconds() -> f64 {
    std::env::args()
        .skip_while(|arg| arg != "--seconds")
        .nth(1)
        .and_then(|value| value.parse().ok())
        .unwrap_or(8.0)
}

fn main() -> Result<(), slint::PlatformError> {
    // Undocumented (but supported) Slint override; must be set before the
    // window is created.
    std::env::set_var("SLINT_SCALE_FACTOR", "1.5");

    let seconds = parse_seconds();
    let started = Instant::now();
    let ui = AppWindow::new()?;

    // Slint paints on demand: no repeated timer, so an idle window is idle.
    // One single-shot reports the first frame; a watchdog prints the frame
    // count and exits, because Slint has no global exit from a thread.
    let frames = Arc::new(AtomicU64::new(1));
    let frames_for_print = Arc::clone(&frames);
    let first = started;
    slint::Timer::single_shot(Duration::from_millis(1), move || {
        println!("startup_ms {}", first.elapsed().as_secs_f64() * 1000.0);
        println!("bench_ready");
        let _ = std::io::stdout().flush();
    });
    std::thread::spawn(move || {
        std::thread::sleep(Duration::from_secs_f64(seconds));
        println!("frames {}", frames_for_print.load(Ordering::Relaxed));
        let _ = std::io::stdout().flush();
        std::process::exit(0);
    });

    ui.run()
}
