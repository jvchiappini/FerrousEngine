//! WebView2 side of the Phase 9B comparison (best-effort, Windows only).
//! Run with `--seconds N` (default 8); `frames N` is printed on exit.
//!
//! Hosted on winit 0.30 (no tao). wry 0.57's default `x11` feature pulls
//! `tao/x11`; tao's GTK dependency collides with wry's on the `gdk-sys`
//! `links = "gdk-3"` key during Cargo resolution even on Windows (the resolver
//! walks the whole target-independent graph). Disabling default features drops
//! tao while `os-webview` keeps the target-gated webkit backend.
//!
//! DPI: winit creates a 1280x800 logical window; the webview fills it and
//! follows the monitor scale factor. "frames" counts the ~60 Hz RedrawRequested
//! ticks we schedule, not WebView2's internal compositor frames.

use std::io::Write;
use std::time::{Duration, Instant};

use dpi::{LogicalPosition, LogicalSize};
use winit::application::ApplicationHandler;
use winit::event::WindowEvent;
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::window::{Window, WindowId};
use wry::{Rect, WebViewBuilder};

const TICK: Duration = Duration::from_millis(16);

const HTML: &str = r#"<!doctype html>
<html>
<head>
<meta charset="utf-8">
<style>
  * { box-sizing: border-box; }
  html, body { margin: 0; height: 100%; }
  body {
    background: #06080D; color: #F1F5FE;
    font-family: "Segoe UI", system-ui, sans-serif;
    display: flex; flex-direction: column;
    padding: 18px 28px; gap: 12px;
  }
  header { display: flex; justify-content: space-between; align-items: flex-start; }
  .brand h1 { margin: 0; font-size: 24px; font-weight: 700; }
  .brand p { margin: 2px 0 0; font-size: 13px; color: #ADB1B9; }
  .links { display: flex; gap: 18px; font-size: 14px; color: #ADB1B9; }
  .divider { height: 1px; background: #26292F; }
  .body { display: flex; gap: 20px; flex: 1; min-height: 0; }
  .recent { flex: 55; display: flex; flex-direction: column; gap: 10px; min-width: 0; }
  .form { flex: 45; display: flex; flex-direction: column; gap: 8px; min-width: 0; }
  h2 { margin: 0 0 4px; font-size: 18px; font-weight: 600; }
  .card {
    display: flex; align-items: center; justify-content: space-between;
    background: #0F1217; border: 1px solid #26292F; border-radius: 8px;
    padding: 10px 14px;
  }
  .card .title { font-size: 15px; }
  .card .sub { font-size: 12px; color: #ADB1B9; }
  .card .time { font-size: 12px; color: #71757C; }
  .muted { margin: 0; font-size: 13px; color: #ADB1B9; line-height: 1.45; }
  .field label { display: block; font-size: 12px; color: #ADB1B9; margin-bottom: 4px; }
  .field .well {
    background: #0B0D13; border: 1px solid #26292F; border-radius: 6px;
    padding: 8px 12px; font-size: 14px;
  }
  .actions { display: flex; gap: 12px; margin-top: 6px; }
  .btn {
    width: 120px; height: 38px; border-radius: 8px;
    display: flex; align-items: center; justify-content: center;
    font-size: 14px;
  }
  .create { background: #2750C7; }
  .browse { background: #181B20; border: 1px solid #26292F; }
  footer { display: flex; justify-content: space-between; font-size: 12px; color: #71757C; }
  /* Bench-only strip: same logical coordinates as the native apps. */
  .bench-label {
    position: fixed; left: 40px; top: calc(100vh - 92px);
    font-size: 12px; color: #ADB1B9;
  }
  .bench-row {
    position: fixed; left: 40px; top: calc(100vh - 76px);
    display: flex; gap: 20px;
  }
  .bench-input {
    width: 320px; height: 36px; padding: 0 10px;
    background: #0B0D13; border: 1px solid #26292F; border-radius: 6px;
    color: #F1F5FE; font-size: 14px; outline: none;
  }
  .bench-button {
    width: 140px; height: 36px;
    background: #181B20; border: 1px solid #26292F; border-radius: 8px;
    color: #F1F5FE; font-size: 14px; cursor: pointer;
  }
  .bench-button:hover { background: #26292F; }
</style>
</head>
<body>
  <header>
    <div class="brand">
      <h1>Motioner</h1>
      <p>Code-driven motion graphics</p>
    </div>
    <nav class="links"><span>Settings</span><span>Docs</span></nav>
  </header>
  <div class="divider"></div>
  <main class="body">
    <section class="recent">
      <h2>Recent projects</h2>
      <div class="card"><div><div class="title">Project 1</div><div class="sub">Edited recently</div></div><div class="time">11:42</div></div>
      <div class="card"><div><div class="title">Project 2</div><div class="sub">Edited recently</div></div><div class="time">11:42</div></div>
      <div class="card"><div><div class="title">Project 3</div><div class="sub">Edited recently</div></div><div class="time">11:42</div></div>
      <div class="card"><div><div class="title">Project 4</div><div class="sub">Edited recently</div></div><div class="time">11:42</div></div>
      <div class="card"><div><div class="title">Project 5</div><div class="sub">Edited recently</div></div><div class="time">11:42</div></div>
    </section>
    <section class="form">
      <h2>New project</h2>
      <p class="muted">Start from a blank canvas or a starter composition, then refine every keyframe.</p>
      <div class="field"><label>Name</label><div class="well">Untitled</div></div>
      <div class="field"><label>Size</label><div class="well">1920 x 1080</div></div>
      <div class="field"><label>Frame rate</label><div class="well">60 fps</div></div>
      <div class="field"><label>Engine</label><div class="well">FerrousEngine</div></div>
      <div class="actions"><div class="btn create">Create</div><div class="btn browse">Browse</div></div>
    </section>
  </main>
  <footer><span>FerrousEngine 0.1</span><span>Apache-2.0</span></footer>
  <div class="bench-label">Bench input</div>
  <div class="bench-row">
    <input class="bench-input" id="bench-input" type="text">
    <button class="bench-button" id="bench-button">Bench button</button>
  </div>
</body>
</html>"#;

fn parse_seconds() -> f64 {
    std::env::args()
        .skip_while(|arg| arg != "--seconds")
        .nth(1)
        .and_then(|value| value.parse().ok())
        .unwrap_or(8.0)
}

struct State {
    // Declared before `window` so the webview controller is dropped first.
    webview: Option<wry::WebView>,
    window: Option<Window>,
    started: Instant,
    seconds: f64,
    frames: u64,
    first_frame: bool,
    next_tick: Instant,
}

impl State {
    fn new(seconds: f64) -> Self {
        let now = Instant::now();
        Self {
            webview: None,
            window: None,
            started: now,
            seconds,
            frames: 0,
            first_frame: false,
            next_tick: now,
        }
    }

    fn set_bounds(&self) {
        let (Some(window), Some(webview)) = (&self.window, &self.webview) else {
            return;
        };
        let size = window.inner_size().to_logical::<u32>(window.scale_factor());
        let _ = webview.set_bounds(Rect {
            position: LogicalPosition::new(0, 0).into(),
            size: LogicalSize::new(size.width, size.height).into(),
        });
    }
}

impl ApplicationHandler for State {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        let attributes = Window::default_attributes()
            .with_title("guimaker-compare-webview2")
            .with_inner_size(winit::dpi::LogicalSize::new(1280.0, 800.0))
            .with_resizable(false);
        let window = event_loop.create_window(attributes).unwrap();

        let webview = WebViewBuilder::new()
            .with_html(HTML)
            .build_as_child(&window)
            .unwrap();

        self.window = Some(window);
        self.webview = Some(webview);
        self.set_bounds();
    }

    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        _window_id: WindowId,
        event: WindowEvent,
    ) {
        match event {
            WindowEvent::CloseRequested => event_loop.exit(),
            WindowEvent::Resized(_) => self.set_bounds(),
            WindowEvent::RedrawRequested => {
                if !self.first_frame {
                    self.first_frame = true;
                    println!(
                        "startup_ms {}",
                        self.started.elapsed().as_secs_f64() * 1000.0
                    );
                    println!("bench_ready");
                    let _ = std::io::stdout().flush();
                }
                self.frames = self.frames.saturating_add(1);
            }
            _ => {}
        }
    }

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        if self.started.elapsed().as_secs_f64() >= self.seconds {
            event_loop.exit();
            return;
        }
        let now = Instant::now();
        if now >= self.next_tick {
            if let Some(window) = &self.window {
                window.request_redraw();
            }
            self.next_tick = now + TICK;
        }
        event_loop.set_control_flow(ControlFlow::WaitUntil(self.next_tick));
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let seconds = parse_seconds();

    let event_loop = EventLoop::new()?;
    event_loop.set_control_flow(ControlFlow::Wait);

    let mut state = State::new(seconds);
    event_loop.run_app(&mut state)?;

    println!("frames {}", state.frames);
    let _ = std::io::stdout().flush();
    Ok(())
}
