//! Phase 8D evidence: a menu hosted in a real popup window.
//!
//! The host claims `HostCapabilities::desktop()`, so a menu that does not fit
//! the window is painted in a borderless, owned window of its own. The run:
//!
//! 1. opens a menu at the right edge (its popup sits outside the window),
//! 2. waits two frames after the popup's first frame,
//! 3. captures the desktop (window bounds + 300 px) to
//!    `out/shots/p8d/popup-outside.png`,
//! 4. hides and reopens the menu, timing the pooled popup,
//! 5. idles five seconds (the popup must present zero frames) and exits.
//!
//! Run with `cargo run --release -p guimaker_gallery --example p8d_popup`.

use std::process::Command;
use std::time::{Duration, Instant};

use ferrous_engine::winit::dpi::PhysicalPosition;
use ferrous_engine::{App, AppContext, AppMode, RedrawPolicy};
use guimaker::components::overlays::{menu, MenuItem};
use guimaker::components::text as label;
use guimaker::overlay::Side;
use guimaker::prelude::*;
use guimaker_gallery::host::{GuiApp, GuiHost};

const ROBOTO: &[u8] = include_bytes!("../../../assets/fonts/Roboto-Regular.ttf");
/// Logical size of the menu the example anchors (the popup's content).
const MENU_LOGICAL: (f32, f32) = (300.0, 200.0);
/// How long the idle measurement lasts.
const IDLE: Duration = Duration::from_secs(5);

#[derive(Clone, Debug, PartialEq)]
enum Msg {
    Toggle,
    Pick(&'static str),
}

#[allow(clippy::struct_excessive_bools)] // reason: independent one-shot timeline flags
struct Demo {
    open: bool,
    started: Instant,
    positioned: bool,
    frames: u64,
    first_frames: Vec<f64>,
    popup_frames: u64,
    frames_since_first: Option<u32>,
    capture_child: Option<std::process::Child>,
    captured: bool,
    reopen_at: Option<Instant>,
    reopened: bool,
    close_at: Option<Instant>,
    idle_started: Option<Instant>,
    idle_popup_frames: u64,
    idle_start_frame: u64,
    cpu_idle_start: Option<u64>,
    rss_before: Option<u64>,
    rss_after: Option<u64>,
    rss_before_pooled: Option<u64>,
    rss_after_pooled: Option<u64>,
    printed: bool,
}

impl Default for Demo {
    fn default() -> Self {
        Self {
            open: false,
            started: Instant::now(),
            positioned: false,
            frames: 0,
            first_frames: Vec::new(),
            popup_frames: 0,
            frames_since_first: None,
            capture_child: None,
            captured: false,
            reopen_at: None,
            reopened: false,
            close_at: None,
            idle_started: None,
            idle_popup_frames: 0,
            idle_start_frame: 0,
            cpu_idle_start: None,
            rss_before: None,
            rss_after: None,
            rss_before_pooled: None,
            rss_after_pooled: None,
            printed: false,
        }
    }
}

impl Demo {
    fn step(&mut self, ctx: &mut AppContext<'_>) {
        self.frames += 1;
        let elapsed = self.started.elapsed();

        if !self.positioned {
            // Keep the window away from the screen edges so the 300 px capture
            // margin exists on every side.
            ctx.window
                .set_outer_position(PhysicalPosition::new(320, 180));
            self.positioned = true;
        }

        // Open the menu once the window is warm: a new popup window. The RSS
        // baseline is taken here, after the main window's first-frame
        // allocations, so the delta is the popup's.
        if self.frames > 10 && !self.open && self.reopen_at.is_none() && !self.captured {
            self.rss_before = rss_bytes();
            self.open = true;
        }

        // Two frames after the popup's first frame the picture is settled.
        // The capture runs as a child process while the app keeps drawing, so
        // Windows never marks the window as unresponsive (a hung owner gets a
        // DWM ghost window with a title bar over the popup).
        if let Some(child) = self.capture_child.as_mut() {
            if matches!(child.try_wait(), Ok(Some(_))) {
                self.capture_child = None;
                self.captured = true;
                self.rss_after = rss_bytes();
                self.open = false;
                self.reopen_at = Some(Instant::now() + Duration::from_millis(400));
            }
        } else if !self.captured
            && self.frames_since_first.as_mut().is_some_and(|count| {
                *count += 1;
                *count >= 2
            })
        {
            self.capture_child = start_capture(ctx, "out/shots/p8d/popup-outside.png");
        }

        if !self.open && self.reopen_at.is_some_and(|at| Instant::now() >= at) && !self.reopened {
            self.reopened = true;
            self.rss_before_pooled = rss_bytes();
            self.open = true;
        }

        if self.reopened
            && self.first_frames.len() >= 2
            && self.close_at.is_none()
            && self.reopen_at.is_some_and(|at| Instant::now() >= at)
        {
            self.close_at = Some(Instant::now() + Duration::from_millis(300));
        }
        if self.close_at.is_some_and(|at| Instant::now() >= at) && self.idle_started.is_none() {
            // The popup stays open and idle: this is the window the evidence
            // measures. Nothing asks for a frame while it lasts.
            self.idle_started = Some(Instant::now());
            self.idle_popup_frames = self.popup_frames;
            self.idle_start_frame = self.frames;
            self.cpu_idle_start = cpu_millis();
            // Nothing will ask for a frame while the popup is idle; schedule
            // the single wake that reports and exits.
            ctx.redraw_at(Instant::now() + IDLE);
        }

        if self.idle_started.is_some_and(|at| at.elapsed() >= IDLE) && !self.printed {
            self.printed = true;
            self.report(ctx, elapsed);
            ctx.request_exit();
            return;
        }

        // One frame at a time: the run is on demand, so the timeline advances
        // only while it asks for the next frame.
        if self.idle_started.is_none() {
            ctx.request_redraw();
        }

        if elapsed > Duration::from_secs(20) && !self.printed {
            self.printed = true;
            eprintln!(
                "p8d TIMEOUT frames={} popup_frames={} first_frames={:?} captured={}",
                self.frames, self.popup_frames, self.first_frames, self.captured
            );
            ctx.request_exit();
        }
    }

    #[allow(clippy::cast_precision_loss)] // reason: process CPU milliseconds fit f64 exactly
    #[allow(clippy::cast_possible_truncation)] // reason: menu sizes stay far below 2^24
    #[allow(clippy::cast_sign_loss)] // reason: the menu size is positive
    fn report(&self, ctx: &AppContext<'_>, elapsed: Duration) {
        let idle_ms = self
            .idle_started
            .map_or(0.0, |at| at.elapsed().as_secs_f64() * 1000.0);
        let cpu = match (self.cpu_idle_start, cpu_millis()) {
            (Some(start), Some(end)) => {
                f64::from((end.saturating_sub(start)) as u32) / idle_ms.max(1.0) * 100.0
            }
            _ => f64::NAN,
        };
        let scale = ctx.scale_factor;
        let (width, height) = (
            (f64::from(MENU_LOGICAL.0) * scale).round() as u64,
            (f64::from(MENU_LOGICAL.1) * scale).round() as u64,
        );
        // The popup's own VRAM: its three instance buffers, the reduced SVG
        // scratch set and the Fifo swapchain (3 images).
        let quad = std::mem::size_of::<ferrous_ui_render::GuiQuad>() as u64;
        let text = std::mem::size_of::<ferrous_ui_render::TextQuad>() as u64;
        let instances = 1024 * (quad + text * 2);
        let svg = 4096 * 28 + 4096 * 4;
        let swapchain = 3 * width * height * 4;
        eprintln!(
            "p8d {{ \"first_frame_ms\": {:?}, \"popup_frames\": {}, \"idle_ms\": {:.0}, \
             \"idle_popup_frames\": {}, \"idle_main_frames\": {}, \"idle_cpu_percent\": {:.3}, \
             \"rss_delta_bytes\": {}, \"rss_pooled_delta_bytes\": {}, \"popup_vram_bytes\": {}, \
             \"vram_instances\": {}, \"vram_svg\": {}, \"vram_swapchain\": {}, \
             \"popup_physical\": [{}, {}] }}",
            self.first_frames,
            self.popup_frames,
            idle_ms,
            self.popup_frames.saturating_sub(self.idle_popup_frames),
            self.frames.saturating_sub(self.idle_start_frame),
            cpu,
            self.rss_after
                .unwrap_or(0)
                .saturating_sub(self.rss_before.unwrap_or(0)),
            self.rss_after_pooled
                .unwrap_or(0)
                .saturating_sub(self.rss_before_pooled.unwrap_or(0)),
            instances + svg + swapchain,
            instances,
            svg,
            swapchain,
            width,
            height
        );
        eprintln!(
            "p8d elapsed_seconds {:.2}, frames {}",
            elapsed.as_secs_f64(),
            self.frames
        );
    }
}

impl GuiApp for Demo {
    type Msg = Msg;

    fn view(&self) -> Element<Msg> {
        let items = [
            MenuItem::item("Rename", Msg::Pick("rename")),
            MenuItem::item("Duplicate", Msg::Pick("duplicate")),
            MenuItem::separator(),
            MenuItem::item("Delete", Msg::Pick("delete")),
        ];
        let mut overlay = Element::overlay().child(
            guimaker::components::button("File", ButtonVariant::Secondary, Msg::Toggle)
                .key("p8d_anchor"),
        );
        if self.open {
            // Bottom on a right-edge anchor: the preferred rect overflows the
            // window on the right and the opposite side overflows too, so the
            // solver keeps it and the host moves it into a popup window.
            let panel = menu(&items, None)
                .key("p8d_menu")
                .width(SizeSpec::Px(MENU_LOGICAL.0))
                .height(SizeSpec::Px(MENU_LOGICAL.1))
                .overlay_on(0, Side::Bottom, 6.0);
            overlay = overlay.child(panel);
        }
        Element::column()
            .width(SizeSpec::Fill)
            .height(SizeSpec::Fill)
            .bg(ColorRole::Bg)
            .pad(Spacing::S5)
            .gap(Spacing::S4)
            .child(
                Element::row()
                    .width(SizeSpec::Fill)
                    .justify(Justify::End)
                    .child(overlay),
            )
            .child(
                Element::column()
                    .width(SizeSpec::Fill)
                    .height(SizeSpec::Fill)
                    .pad(Spacing::S6)
                    .gap(Spacing::S2)
                    .child(label("Phase 8D", TypeRole::Title, Tone::Default))
                    .child(label(
                        "The File menu overflows the window and is drawn in a real popup window.",
                        TypeRole::Body,
                        Tone::Muted,
                    )),
            )
    }

    fn update(&mut self, message: Msg) {
        match message {
            Msg::Toggle => self.open = !self.open,
            Msg::Pick(item) => {
                eprintln!("p8d: picked {item}");
                self.open = false;
            }
        }
    }

    fn on_popup_frame(&mut self, frames: u64, first_frame_ms: Option<f64>) {
        self.popup_frames = self.popup_frames.max(frames);
        if let Some(ms) = first_frame_ms {
            self.first_frames.push(ms);
            if self.first_frames.len() == 1 {
                self.frames_since_first = Some(0);
                self.rss_after = rss_bytes();
            } else if self.first_frames.len() == 2 {
                self.rss_after_pooled = rss_bytes();
            }
        }
    }

    fn tick(&mut self, ctx: &mut AppContext<'_>) {
        self.step(ctx);
    }
}

fn main() {
    App::new(GuiHost::new(Demo::default()))
        .with_title("GUIMaker Phase 8D popup")
        .with_size(1000, 600)
        .with_mode(AppMode::Flat2D)
        .with_redraw_policy(RedrawPolicy::OnDemand)
        .with_font_bytes(ROBOTO)
        .with_background_color(ferrous_engine::Color::hex("#0A0B0D"))
        .run();
}

/// The evidence capture: the window plus a 300 px margin on every side.
///
/// Returns the powershell child so the app can keep presenting frames while
/// the desktop is copied (blocking here marks the window Not Responding).
fn start_capture(ctx: &AppContext<'_>, path: &str) -> Option<std::process::Child> {
    let position = ctx.window.outer_position().ok()?;
    let size = ctx.window.outer_size();
    let x = position.x - 300;
    let y = position.y - 300;
    let w = i64::from(size.width) + 600;
    let h = i64::from(size.height) + 600;
    if let Some(parent) = std::path::Path::new(path).parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let script = format!(
        "Add-Type -AssemblyName System.Drawing; Add-Type -AssemblyName System.Windows.Forms; \
         $b=[System.Windows.Forms.Screen]::PrimaryScreen.Bounds; \
         $x=[Math]::Max($b.X,{x}); $y=[Math]::Max($b.Y,{y}); \
         $w=[Math]::Min({w}, $b.Right-$x); $h=[Math]::Min({h}, $b.Bottom-$y); \
         $bmp=New-Object System.Drawing.Bitmap $w,$h; \
         $g=[System.Drawing.Graphics]::FromImage($bmp); \
         $g.CopyFromScreen($x,$y,0,0,$bmp.Size); \
         $bmp.Save('{path}',[System.Drawing.Imaging.ImageFormat]::Png); \
         $g.Dispose(); $bmp.Dispose()"
    );
    Command::new("powershell")
        .args(["-NoProfile", "-NonInteractive", "-Command", &script])
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .ok()
}

fn rss_bytes() -> Option<u64> {
    powershell_u64(&format!(
        "(Get-Process -Id {}).WorkingSet64",
        std::process::id()
    ))
}

fn cpu_millis() -> Option<u64> {
    powershell_u64(&format!(
        "[long](Get-Process -Id {}).TotalProcessorTime.TotalMilliseconds",
        std::process::id()
    ))
}

fn powershell_u64(command: &str) -> Option<u64> {
    let output = Command::new("powershell")
        .args(["-NoProfile", "-NonInteractive", "-Command", command])
        .output()
        .ok()?;
    String::from_utf8(output.stdout).ok()?.trim().parse().ok()
}
