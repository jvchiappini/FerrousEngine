//! WebView2 side of the Phase 9B comparison (best-effort, Windows only).
//! Run with `--seconds N` (default 8); `frames N` is printed to stdout on exit.
//!
//! Versions: wry 0.57 + tao 0.38 (tao with the `rwh_06` feature so `Window`
//! implements `HasWindowHandle`, which wry 0.57 requires).
//!
//! Caveats: wry 0.57's dev-dependency is tao 0.37; tao 0.38 is source- and
//! raw-window-handle-compatible but not the exact pairing wry CI builds with.
//! tao/wry expose no per-window DPI override, so the window is 1280x800 logical
//! and the scale factor follows Windows. "frames" counts RedrawRequested ticks
//! driven by a ~60 Hz tao deadline, not WebView2's internal compositor frames.

use std::cell::Cell;
use std::io::Write;
use std::time::{Duration, Instant};

use tao::dpi::LogicalSize;
use tao::event::{Event, WindowEvent};
use tao::event_loop::{ControlFlow, EventLoop, EventLoopExtRunReturn};
use tao::window::WindowBuilder;
use wry::WebViewBuilder;

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
</body>
</html>"#;

fn parse_seconds() -> f64 {
    std::env::args()
        .skip_while(|arg| arg != "--seconds")
        .nth(1)
        .and_then(|value| value.parse().ok())
        .unwrap_or(8.0)
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let seconds = parse_seconds();

    let mut event_loop = EventLoop::new();
    let window = WindowBuilder::new()
        .with_title("guimaker-compare-webview2")
        .with_inner_size(LogicalSize::new(1280.0, 800.0))
        .with_resizable(false)
        .build(&event_loop)?;

    // Kept alive for the whole loop, and dropped before `window` at the end of
    // main (declaration order), so the WebView2 controller never outlives its
    // parent HWND.
    let _webview = WebViewBuilder::new()
        .with_background_color((6, 8, 13, 255))
        .with_html(HTML)
        .build(&window)?;

    let started = Instant::now();
    let frames = Cell::new(0u64);

    let _code = event_loop.run_return(|event, _target, control_flow| match event {
        Event::WindowEvent {
            event: WindowEvent::CloseRequested,
            ..
        } => {
            *control_flow = ControlFlow::Exit;
        }
        Event::MainEventsCleared => {
            if started.elapsed().as_secs_f64() >= seconds {
                *control_flow = ControlFlow::Exit;
            } else {
                window.request_redraw();
                *control_flow =
                    ControlFlow::WaitUntil(Instant::now() + Duration::from_millis(16));
            }
        }
        Event::RedrawRequested(_) => {
            frames.set(frames.get() + 1);
        }
        _ => {}
    });

    println!("frames {}", frames.get());
    let _ = std::io::stdout().flush();
    Ok(())
}
