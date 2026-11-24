//! Phase 8E evidence: a menu hosted in a real popup window, driven by real
//! Windows input and captured from our own surfaces only.
//!
//! The Phase 8D example set `self.open = true` to "show" the menu and copied
//! the desktop, so it proved neither that input opens the menu nor that the
//! pixels were ours. This run instead:
//!
//! 1. opens the menu by clicking the real "File" anchor with `SendInput`;
//! 2. clicks the real "Rename" item, asserting the app message fired, the menu
//!    closed, the popup hid and the main window kept the foreground;
//! 3. reopens and closes with Escape, with an empty-window click and by
//!    activating a small helper window that belongs to this same binary;
//! 4. captures by reading back the main window's swapchain and every visible
//!    popup's own swapchain, then compositing them over a flat neutral
//!    background the size of the monitor work area (never the desktop);
//! 5. repeats the capture near the right and bottom work-area edges, asserting
//!    the solved menu stays fully on screen;
//! 6. measures input-to-present for new and pooled popups, RSS per popup, a
//!    computed VRAM figure, five seconds of popup idleness and the popup frame
//!    count while the main window repaints from hovering.
//!
//! The measurements run in a short-lived child of this same binary
//! (`--measure-child`): the host keys popups by `NodeId` and the popup pool
//! trims by `PopupId`, so five measurement menus and the evidence menu cannot
//! coexist in one process without the pool ever destroying the evidence
//! popup's window. The child writes `out/shots/p8e/measurement.txt`, exits,
//! and the parent runs the single-menu evidence scenarios with a valid window.
//!
//! Windows only (`SendInput` and `GetForegroundWindow`); the stub keeps
//! `cargo check --workspace --all-targets` green on other hosts.
//!
//! Run with `cargo run --release -p guimaker_gallery --example p8d_popup`.

#![allow(
    clippy::borrow_as_ptr,
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    clippy::cast_sign_loss,
    clippy::collapsible_match,
    clippy::format_push_string,
    clippy::manual_is_multiple_of,
    clippy::manual_midpoint,
    clippy::map_unwrap_or,
    clippy::single_match_else,
    clippy::too_many_lines,
    clippy::type_complexity,
    clippy::unused_self
)] // reason: one-off evidence harness: Win32 casts and one long linear scenario script

#[cfg(not(target_os = "windows"))]
fn main() {
    eprintln!("p8d_popup evidence is Windows-only");
}

#[cfg(target_os = "windows")]
fn main() {
    win::main();
}

#[cfg(target_os = "windows")]
mod win {
    use std::path::{Path, PathBuf};
    use std::process::Child;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::time::{Duration, Instant};

    use ferrous_engine::context::WorkArea;
    use ferrous_engine::traits::PopupCapture;
    use ferrous_engine::winit::application::ApplicationHandler;
    use ferrous_engine::winit::dpi::{LogicalSize, PhysicalPosition};
    use ferrous_engine::winit::event::{
        ElementState, MouseButton as WinitMouseButton, WindowEvent,
    };
    use ferrous_engine::winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
    use ferrous_engine::winit::keyboard::{Key, NamedKey};
    use ferrous_engine::winit::raw_window_handle::{HasWindowHandle, RawWindowHandle};
    use ferrous_engine::winit::window::{Window, WindowId};
    use ferrous_engine::{App, AppContext, AppMode, FerrousApp, RedrawPolicy, Screenshot};
    use glam::Vec2;
    use guimaker::components::overlays::{menu, MenuItem};
    use guimaker::components::text as label;
    use guimaker::geom::Rect;
    use guimaker::overlay::Side;
    use guimaker::prelude::*;
    use guimaker_gallery::host::{GuiApp, GuiHost};
    use guimaker_gallery::shot::{write_png, RenderedImage};

    const ROBOTO: &[u8] = include_bytes!("../../../assets/fonts/Roboto-Regular.ttf");
    const OUT: &str = "out/shots/p8e";
    const NEW_RUNS: usize = 5;
    const POOLED_RUNS: usize = 5;
    const IDLE: Duration = Duration::from_secs(5);
    const MEASURE_ANCHORS: [&str; NEW_RUNS] = [
        "p8d_anchor",
        "p8e_anchor_1",
        "p8e_anchor_2",
        "p8e_anchor_3",
        "p8e_anchor_4",
    ];
    const MEASURE_MENUS: [&str; NEW_RUNS] = [
        "p8d_menu",
        "p8e_menu_1",
        "p8e_menu_2",
        "p8e_menu_3",
        "p8e_menu_4",
    ];

    static FAILED: AtomicBool = AtomicBool::new(false);

    #[derive(Clone, Debug, PartialEq)]
    enum Msg {
        ToggleMeasure(usize),
        Pick(&'static str),
        HoverTarget,
    }

    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    enum SampleKind {
        New,
        Pooled,
    }

    #[derive(Clone, Copy, Debug)]
    struct Pending {
        kind: SampleKind,
        remaining: usize,
    }

    struct Demo {
        measure: bool,
        menus_open: [bool; NEW_RUNS],
        menu_side: Side,
        menu_gap: f32,
        picks: Vec<&'static str>,
        pending: Option<Pending>,
        new_ms: Vec<f64>,
        pooled_ms: Vec<f64>,
        main_frames: u64,
    }

    impl Default for Demo {
        fn default() -> Self {
            Self {
                measure: false,
                menus_open: [false; NEW_RUNS],
                menu_side: Side::Bottom,
                menu_gap: 6.0,
                picks: Vec::new(),
                pending: None,
                new_ms: Vec::new(),
                pooled_ms: Vec::new(),
                main_frames: 0,
            }
        }
    }

    impl Demo {
        fn is_open(&self) -> bool {
            self.menus_open.iter().any(|open| *open)
        }

        fn close_all(&mut self) {
            self.menus_open = [false; NEW_RUNS];
        }

        fn close_menu(&mut self, index: usize) {
            if let Some(open) = self.menus_open.get_mut(index) {
                *open = false;
            }
        }

        fn content(&self) -> Element<Msg> {
            Element::column()
                .width(SizeSpec::Fill)
                .pad(Spacing::S6)
                .gap(Spacing::S2)
                .child(label("Phase 8E", TypeRole::Title, Tone::Default))
                .child(label(
                    "Real input against real popup windows; captures read our surfaces only.",
                    TypeRole::Body,
                    Tone::Muted,
                ))
                .child(
                    guimaker::components::button(
                        "Hover target",
                        ButtonVariant::Ghost,
                        Msg::HoverTarget,
                    )
                    .key("p8e_hover"),
                )
        }

        fn anchor_overlay(&self, index: usize) -> Element<Msg> {
            let mut overlay = Element::overlay().child(
                guimaker::components::button(
                    "File",
                    ButtonVariant::Secondary,
                    Msg::ToggleMeasure(index),
                )
                .key(MEASURE_ANCHORS[index]),
            );
            if self.menus_open[index] {
                let items = [
                    MenuItem::item("Rename", Msg::Pick("rename")),
                    MenuItem::item("Duplicate", Msg::Pick("duplicate")),
                    MenuItem::separator(),
                    MenuItem::item("Delete", Msg::Pick("delete")),
                ];
                overlay = overlay.child(
                    menu(&items, None)
                        .key(MEASURE_MENUS[index])
                        .width(SizeSpec::Px(400.0))
                        .height(SizeSpec::Px(180.0))
                        .overlay_on(
                            0,
                            if index == 0 {
                                self.menu_side
                            } else {
                                Side::Bottom
                            },
                            if index == 0 { self.menu_gap } else { 6.0 },
                        ),
                );
            }
            overlay
        }

        fn view_single(&self) -> Element<Msg> {
            let anchor_row = Element::row()
                .width(SizeSpec::Fill)
                .justify(Justify::End)
                .child(self.anchor_overlay(0));
            Element::column()
                .width(SizeSpec::Fill)
                .height(SizeSpec::Fill)
                .bg(ColorRole::Bg)
                .pad(Spacing::S5)
                .gap(Spacing::S4)
                .child(anchor_row)
                .child(self.content())
        }

        fn view_menus(&self) -> Element<Msg> {
            let mut row = Element::row()
                .width(SizeSpec::Fill)
                .justify(Justify::End)
                .gap(Spacing::S2);
            for index in 0..NEW_RUNS {
                row = row.child(self.anchor_overlay(index));
            }
            Element::column()
                .width(SizeSpec::Fill)
                .height(SizeSpec::Fill)
                .bg(ColorRole::Bg)
                .pad(Spacing::S5)
                .gap(Spacing::S4)
                .child(row)
                .child(self.content())
        }
    }

    impl GuiApp for Demo {
        type Msg = Msg;

        fn view(&self) -> Element<Msg> {
            if self.measure {
                self.view_menus()
            } else {
                self.view_single()
            }
        }

        fn update(&mut self, message: Msg) {
            match message {
                Msg::ToggleMeasure(index) => {
                    if let Some(open) = self.menus_open.get_mut(index) {
                        *open = !*open;
                        if *open {
                            self.menu_gap = if (self.menu_gap - 6.0).abs() < f32::EPSILON {
                                6.5
                            } else {
                                6.0
                            };
                        }
                    }
                }
                Msg::Pick(item) => {
                    eprintln!("p8e: picked {item}");
                    self.picks.push(item);
                    self.close_menu(0);
                }
                Msg::HoverTarget => {}
            }
        }

        fn on_popup_frame(&mut self, _frames: u64, first_frame_ms: Option<f64>) {
            let Some(ms) = first_frame_ms else {
                return;
            };
            let Some(pending) = self.pending else {
                return;
            };
            let run = match pending.kind {
                SampleKind::New => self.new_ms.len() + 1,
                SampleKind::Pooled => self.pooled_ms.len() + 1,
            };
            let kind = match pending.kind {
                SampleKind::New => "new",
                SampleKind::Pooled => "pooled",
            };
            match pending.kind {
                SampleKind::New => self.new_ms.push(ms),
                SampleKind::Pooled => self.pooled_ms.push(ms),
            }
            eprintln!(
                "p8e run {{\"kind\":\"{kind}\",\"run\":{run},\"input_to_present_ms\":{ms:.3}}}"
            );
            let remaining = pending.remaining.saturating_sub(1);
            self.pending = (remaining > 0).then_some(Pending {
                kind: pending.kind,
                remaining,
            });
        }

        fn tick(&mut self, _ctx: &mut AppContext<'_>) {
            self.main_frames = self.main_frames.saturating_add(1);
        }
    }

    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    enum Phase {
        Startup,
        MeasureNew,
        PooledOpen,
        MeasureRest,
        PrepareSingle,
        NormalSetup,
        NormalOpen,
        Capture,
        ClickRename,
        EscapeSetup,
        OutsideSetup,
        BlurSetup,
        RightSetup,
        BottomSetup,
        IdleSetup,
        Idle,
        HoverSetup,
        Hover,
        Finalize,
        Done,
    }

    #[derive(Clone, Debug)]
    struct PopupRect {
        id: u32,
        position: (i32, i32),
        size: (u32, u32),
        inside: bool,
    }

    #[derive(Clone, Debug)]
    struct Case {
        name: String,
        work: (i32, i32, u32, u32),
        main_position: (i32, i32),
        main_size: (u32, u32),
        popups: Vec<PopupRect>,
    }

    struct Driver {
        host: GuiHost<Demo>,
        main_hwnd: isize,
        phase: Phase,
        step: u32,
        ready_at: Instant,
        wait_started: Option<Instant>,
        action_started: Instant,
        frames: u64,
        cursor: (f64, f64),
        failures: Vec<String>,
        expect_index: usize,
        pooled_failures: u32,
        new_failures: u32,
        rss_before: Option<u64>,
        rss_after: Option<u64>,
        capture_name: String,
        capture_next: Phase,
        cap_work: Option<WorkArea>,
        cap_inner: Option<(i32, i32)>,
        main_shot: Option<Screenshot>,
        capture_popups: Vec<PopupCapture>,
        cases: Vec<Case>,
        helper: Option<Child>,
        helper_seen: bool,
        helper_status: Option<String>,
        idle_start: Option<Instant>,
        idle_base_popup: u64,
        idle_base_main: u64,
        idle_popup_frames: u64,
        idle_main_frames: u64,
        hover_base_popup: u64,
        hover_base_main: u64,
        hover_index: u32,
        hover_popup_frames: u64,
        hover_main_frames: u64,
        popup_size: (u32, u32),
        last_popup: Option<(i32, i32, u32, u32)>,
        measure_child: bool,
        measure_file: PathBuf,
        measurement: Option<Measurement>,
    }

    #[derive(Clone, Debug, Default)]
    struct Measurement {
        new_ms: Vec<f64>,
        pooled_ms: Vec<f64>,
        rss_delta: u64,
    }

    impl Driver {
        fn new(host: GuiHost<Demo>) -> Self {
            Self {
                host,
                main_hwnd: 0,
                phase: Phase::Startup,
                step: 0,
                ready_at: Instant::now(),
                wait_started: None,
                action_started: Instant::now(),
                frames: 0,
                cursor: (0.0, 0.0),
                failures: Vec::new(),
                expect_index: 0,
                pooled_failures: 0,
                new_failures: 0,
                rss_before: None,
                rss_after: None,
                capture_name: String::new(),
                capture_next: Phase::Finalize,
                cap_work: None,
                cap_inner: None,
                main_shot: None,
                capture_popups: Vec::new(),
                cases: Vec::new(),
                helper: None,
                helper_seen: false,
                helper_status: None,
                idle_start: None,
                idle_base_popup: 0,
                idle_base_main: 0,
                idle_popup_frames: 0,
                idle_main_frames: 0,
                hover_base_popup: 0,
                hover_base_main: 0,
                hover_index: 0,
                hover_popup_frames: 0,
                hover_main_frames: 0,
                popup_size: (0, 0),
                last_popup: None,
                measure_child: false,
                measure_file: PathBuf::new(),
                measurement: None,
            }
        }

        fn measure_child(host: GuiHost<Demo>, file: PathBuf) -> Self {
            let mut driver = Self::new(host);
            driver.measure_child = true;
            driver.measure_file = file;
            driver
        }

        fn with_measurement(mut self, measurement: Measurement) -> Self {
            self.measurement = Some(measurement);
            self
        }

        fn fail(&mut self, message: impl Into<String>) {
            let message = message.into();
            eprintln!("p8e ASSERT FAIL: {message}");
            FAILED.store(true, Ordering::SeqCst);
            self.failures.push(message);
        }

        fn check(&mut self, condition: bool, message: impl Into<String>) {
            if !condition {
                self.fail(message);
            }
        }

        fn arm(&mut self, millis: u64) {
            self.ready_at = Instant::now() + Duration::from_millis(millis);
        }

        fn go(&mut self, phase: Phase) {
            self.phase = phase;
            self.step = 0;
            self.ready_at = Instant::now();
        }

        fn popup_frames(&self) -> u64 {
            self.host.popup_stats().frames
        }

        fn visible(&self) -> usize {
            self.host.popup_stats().visible
        }

        fn menu_open(&self) -> bool {
            self.host.app().is_open()
        }

        fn anchor_rect(&self, key: &str) -> Option<Rect> {
            let ui = self.host.ui();
            let id = ui.tree().find_key(key)?;
            Some(ui.layout().rect(id))
        }

        fn anchor_point(&self, ctx: &AppContext<'_>, key: &str) -> Option<(i32, i32)> {
            let rect = self.anchor_rect(key)?;
            let origin = ctx.window.inner_position().ok()?;
            let scale = f64::from(self.host.scale());
            Some((
                origin.x + ((f64::from(rect.x) + f64::from(rect.width) / 2.0) * scale) as i32,
                origin.y + ((f64::from(rect.y) + f64::from(rect.height) / 2.0) * scale) as i32,
            ))
        }

        fn text_point(&self, ctx: &AppContext<'_>, needle: &str) -> Option<(i32, i32)> {
            let origin = ctx.window.inner_position().ok()?;
            let scale = f64::from(self.host.scale());
            for batch in &self.host.frame().popups {
                for command in &batch.commands {
                    if let guimaker::display_list::DrawCommand::Text { rect, run, .. } = command {
                        if run.text.contains(needle) {
                            return Some((
                                origin.x
                                    + ((f64::from(rect.x) + f64::from(rect.width) / 2.0) * scale)
                                        as i32,
                                origin.y
                                    + ((f64::from(rect.y) + f64::from(rect.height) / 2.0) * scale)
                                        as i32,
                            ));
                        }
                    }
                }
            }
            None
        }

        fn empty_point(&self, ctx: &AppContext<'_>) -> (i32, i32) {
            let origin = ctx
                .window
                .inner_position()
                .map_or((0, 0), |position| (position.x, position.y));
            let scale = f64::from(self.host.scale().max(0.1));
            let viewport = self.host.viewport();
            (
                origin.0 + ((f64::from(viewport.width) * 0.14) * scale) as i32,
                origin.1 + ((f64::from(viewport.height) * 0.90) * scale) as i32,
            )
        }

        fn hover_point(&self, ctx: &AppContext<'_>) -> Option<(i32, i32)> {
            self.anchor_point(ctx, "p8e_hover")
        }

        fn pointer_in_anchor(&self) -> bool {
            let scale = f64::from(self.host.scale().max(0.1));
            let point = Vec2::new(
                (self.cursor.0 / scale) as f32,
                (self.cursor.1 / scale) as f32,
            );
            let keys = std::iter::once("p8d_anchor").chain(MEASURE_ANCHORS.iter().copied());
            for key in keys {
                if let Some(rect) = self.anchor_rect(key) {
                    if rect.contains(point) {
                        return true;
                    }
                }
            }
            false
        }

        fn force_main_foreground(&mut self) {
            if let Err(error) = force_foreground(self.main_hwnd) {
                eprintln!("p8e: could not force main foreground: {error}");
            }
        }

        fn send_click(&mut self, x: i32, y: i32) {
            if let Err(error) = click_at(x, y) {
                self.fail(format!("SendInput click at ({x},{y}) failed: {error}"));
            }
        }

        fn nudge_owned_popup(&self, ctx: &AppContext<'_>) {
            self.nudge_popup(ctx, "p8d_menu");
        }

        fn nudge_popup(&self, ctx: &AppContext<'_>, key: &str) {
            let popup = self
                .expected_popup_rect(ctx, key)
                .and_then(find_our_window)
                .or_else(|| {
                    let popup = owned_popup(self.main_hwnd);
                    (popup != 0).then_some(popup)
                });
            if let Some(popup) = popup {
                raise_window(popup);
            }
        }

        fn expected_popup_rect(
            &self,
            ctx: &AppContext<'_>,
            key: &str,
        ) -> Option<(i32, i32, u32, u32)> {
            let node = self.host.ui().tree().find_key(key)?;
            let batch = self
                .host
                .frame()
                .popups
                .iter()
                .find(|batch| batch.node == node)?;
            let origin = ctx.window.inner_position().ok()?;
            let scale = f64::from(self.host.scale().max(0.1));
            Some((
                origin.x + (f64::from(batch.rect.x) * scale) as i32,
                origin.y + (f64::from(batch.rect.y) * scale) as i32,
                (f64::from(batch.rect.width) * scale) as u32,
                (f64::from(batch.rect.height) * scale) as u32,
            ))
        }

        fn click_ours(&mut self, x: i32, y: i32) {
            self.force_main_foreground();
            let in_popup = self.last_popup.is_some_and(|(px, py, pw, ph)| {
                x >= px && x < px + pw as i32 && y >= py && y < py + ph as i32
            });
            if in_popup {
                let popup = owned_popup(self.main_hwnd);
                if popup != 0 {
                    raise_window(popup);
                }
            } else {
                raise_window(self.main_hwnd);
            }
            std::thread::sleep(Duration::from_millis(80));
            if point_is_ours(x, y) {
                let (handle, _) = window_at(x, y);
                if in_popup && handle == self.main_hwnd {
                    let popup = owned_popup(self.main_hwnd);
                    raise_window(popup);
                    std::thread::sleep(Duration::from_millis(80));
                }
                self.send_click(x, y);
                return;
            }
            for _ in 0..6 {
                self.force_main_foreground();
                let popup = if in_popup {
                    owned_popup(self.main_hwnd)
                } else {
                    self.main_hwnd
                };
                raise_window(popup);
                std::thread::sleep(Duration::from_millis(120));
                if point_is_ours(x, y) {
                    self.send_click(x, y);
                    return;
                }
            }
            let (handle, pid) = window_at(x, y);
            self.fail(format!(
                "refusing to click ({x},{y}): hwnd={handle} pid={pid} is not ours"
            ));
        }

        fn send_move(&mut self, x: i32, y: i32) {
            if let Err(error) = move_to(x, y) {
                self.fail(format!("SendInput move to ({x},{y}) failed: {error}"));
            }
        }

        fn send_escape(&mut self) {
            if let Err(error) = press_escape() {
                self.fail(format!("SendInput Escape failed: {error}"));
            }
        }

        fn place_window(&mut self, ctx: &AppContext<'_>, target: (i32, i32)) {
            let inner = ctx.window.inner_position().ok();
            let outer = ctx.window.outer_position().ok();
            if let (Some(inner), Some(outer)) = (inner, outer) {
                let dx = inner.x - outer.x;
                let dy = inner.y - outer.y;
                ctx.window
                    .set_outer_position(PhysicalPosition::new(target.0 - dx, target.1 - dy));
            } else {
                ctx.window
                    .set_outer_position(PhysicalPosition::new(target.0, target.1));
            }
        }

        fn place_normal(&mut self, ctx: &AppContext<'_>) {
            let work = ctx.work_area_screen().unwrap_or(WorkArea {
                x: 0.0,
                y: 0.0,
                width: 1920.0,
                height: 1080.0,
            });
            let size = ctx.window.inner_size();
            let x = (work.x as i32 + 40).min((work.x + work.width) as i32 - size.width as i32);
            let y = (work.y as i32 + 40).min((work.y + work.height) as i32 - size.height as i32);
            self.place_window(ctx, (x.max(work.x as i32), y.max(work.y as i32)));
        }

        fn place_right(&mut self, ctx: &AppContext<'_>) {
            let work = ctx.work_area_screen().unwrap_or(WorkArea {
                x: 0.0,
                y: 0.0,
                width: 1920.0,
                height: 1080.0,
            });
            let scale = f64::from(self.host.scale().max(0.1));
            let anchor = self
                .anchor_rect("p8d_anchor")
                .unwrap_or_else(|| Rect::new(0.0, 0.0, 1.0, 1.0));
            let anchor_right = (f64::from(anchor.right()) * scale) as i32;
            let x = (work.x + work.width) as i32 - anchor_right;
            let y = work.y as i32 + 40;
            self.place_window(ctx, (x.max(work.x as i32), y));
        }

        fn place_bottom(&mut self, ctx: &AppContext<'_>) {
            let work = ctx.work_area_screen().unwrap_or(WorkArea {
                x: 0.0,
                y: 0.0,
                width: 1920.0,
                height: 1080.0,
            });
            let scale = f64::from(self.host.scale().max(0.1));
            let anchor = self
                .anchor_rect("p8d_anchor")
                .unwrap_or_else(|| Rect::new(0.0, 0.0, 1.0, 1.0));
            let anchor_bottom = (f64::from(anchor.y + anchor.height) * scale) as i32;
            let x = work.x as i32 + 40;
            let y = (work.y + work.height) as i32 - 8 - anchor_bottom;
            self.place_window(ctx, (x, y.max(work.y as i32)));
        }

        fn begin_capture(&mut self, ctx: &mut AppContext<'_>, name: &str, next: Phase) {
            self.capture_name = name.to_string();
            self.capture_next = next;
            self.cap_work = ctx.work_area_screen();
            self.cap_inner = ctx
                .window
                .inner_position()
                .ok()
                .map(|position| (position.x, position.y));
            self.main_shot = None;
            self.capture_popups.clear();
            ctx.request_screenshot();
            ctx.popups.capture_all();
            self.action_started = Instant::now();
            self.phase = Phase::Capture;
            self.step = 0;
            self.arm(0);
        }

        fn finish_capture(&mut self, ctx: &AppContext<'_>, report: bool) {
            let Some(work) = self.cap_work.take() else {
                self.fail("capture finished without a work area");
                self.phase = self.capture_next;
                self.step = 0;
                self.arm(200);
                return;
            };
            let Some(main) = self.main_shot.take() else {
                self.fail("capture finished without the main screenshot");
                self.phase = self.capture_next;
                self.step = 0;
                self.arm(200);
                return;
            };
            let inner = self.cap_inner.unwrap_or((work.x as i32, work.y as i32));
            let width = work.width.max(1.0) as u32;
            let height = work.height.max(1.0) as u32;
            let mut canvas = vec![0u8; width as usize * height as usize * 4];
            for pixel in canvas.chunks_exact_mut(4) {
                pixel.copy_from_slice(&[32, 32, 36, 255]);
            }
            blit(
                &mut canvas,
                width,
                height,
                inner.0 - work.x as i32,
                inner.1 - work.y as i32,
                &main.rgba,
                main.width,
                main.height,
                false,
            );
            let mut rects = Vec::new();
            let captures = std::mem::take(&mut self.capture_popups);
            let capture_name = self.capture_name.clone();
            for capture in &captures {
                blit(
                    &mut canvas,
                    width,
                    height,
                    capture.position.0 - work.x as i32,
                    capture.position.1 - work.y as i32,
                    &capture.screenshot.rgba,
                    capture.size.0,
                    capture.size.1,
                    true,
                );
                let inside = inside_rect(capture.position, capture.size, work);
                if !inside {
                    self.fail(format!(
                        "{capture_name} popup rect {:?}+{:?} is outside the work area {}x{} at ({}, {})",
                        capture.position,
                        capture.size,
                        work.width,
                        work.height,
                        work.x,
                        work.y
                    ));
                }
                if self.popup_size == (0, 0) {
                    self.popup_size = capture.size;
                }
                self.last_popup = Some((
                    capture.position.0,
                    capture.position.1,
                    capture.size.0,
                    capture.size.1,
                ));
                rects.push(PopupRect {
                    id: capture.popup.0,
                    position: capture.position,
                    size: capture.size,
                    inside,
                });
            }
            if rects.is_empty() {
                self.fail(format!("{capture_name} capture had no popup frame"));
            }
            let image = RenderedImage {
                width,
                height,
                rgba: canvas,
            };
            let path = format!("{OUT}/popup-{}.png", self.capture_name);
            if let Err(error) = write_png(&path, &image) {
                self.fail(format!("writing {path} failed: {error}"));
            } else if report {
                eprintln!("p8e capture {path} {width}x{height}");
            }
            let case = Case {
                name: self.capture_name.clone(),
                work: (
                    work.x as i32,
                    work.y as i32,
                    work.width as u32,
                    work.height as u32,
                ),
                main_position: inner,
                main_size: (main.width, main.height),
                popups: rects,
            };
            let name = case.name.clone();
            if let Some(slot) = self.cases.iter_mut().find(|existing| existing.name == name) {
                *slot = case;
            } else {
                self.cases.push(case);
            }
            let _ = ctx;
            self.phase = self.capture_next;
            self.step = 0;
            self.arm(200);
        }

        fn drive(&mut self, ctx: &mut AppContext<'_>) {
            if matches!(
                self.phase,
                Phase::PooledOpen
                    | Phase::NormalOpen
                    | Phase::Capture
                    | Phase::ClickRename
                    | Phase::EscapeSetup
                    | Phase::OutsideSetup
                    | Phase::BlurSetup
                    | Phase::RightSetup
                    | Phase::BottomSetup
                    | Phase::IdleSetup
                    | Phase::HoverSetup
            ) {
                self.nudge_owned_popup(ctx);
            }
            if Instant::now() < self.ready_at {
                self.request_if_needed(ctx);
                return;
            }
            match self.phase {
                Phase::Startup => self.phase_startup(ctx),
                Phase::MeasureNew => self.phase_measure_new(ctx),
                Phase::PooledOpen => self.phase_pooled_open(ctx),
                Phase::MeasureRest => self.phase_measure_rest(ctx),
                Phase::PrepareSingle => self.phase_prepare_single(ctx),
                Phase::NormalSetup => self.phase_normal_setup(ctx),
                Phase::NormalOpen => self.phase_normal_open(ctx),
                Phase::Capture => self.phase_capture(ctx),
                Phase::ClickRename => self.phase_click_rename(ctx),
                Phase::EscapeSetup => self.phase_escape(ctx),
                Phase::OutsideSetup => self.phase_outside(ctx),
                Phase::BlurSetup => self.phase_blur(ctx),
                Phase::RightSetup => self.phase_right(ctx),
                Phase::BottomSetup => self.phase_bottom(ctx),
                Phase::IdleSetup => self.phase_idle_setup(ctx),
                Phase::Idle => self.phase_idle(ctx),
                Phase::HoverSetup => self.phase_hover_setup(ctx),
                Phase::Hover => self.phase_hover(ctx),
                Phase::Finalize => self.phase_finalize(ctx),
                Phase::Done => {}
            }
            self.request_if_needed(ctx);
        }

        fn request_if_needed(&self, ctx: &mut AppContext<'_>) {
            if !matches!(self.phase, Phase::Idle | Phase::Done) {
                ctx.request_redraw();
            }
        }

        fn timed_out(&self, limit: Duration) -> bool {
            self.action_started.elapsed() > limit
        }

        fn phase_startup(&mut self, ctx: &mut AppContext<'_>) {
            match self.step {
                0 => {
                    if self.frames > 8 && self.host.viewport().width > 0.0 {
                        self.place_normal(ctx);
                        self.force_main_foreground();
                        raise_window(self.main_hwnd);
                        self.action_started = Instant::now();
                        self.step = 1;
                        self.arm(600);
                    } else {
                        self.arm(50);
                    }
                }
                _ => {
                    self.host.app_mut().close_all();
                    if self.measure_child {
                        self.go(Phase::MeasureNew);
                    } else {
                        self.go(Phase::NormalSetup);
                    }
                    self.arm(350);
                }
            }
        }

        fn phase_measure_new(&mut self, ctx: &mut AppContext<'_>) {
            match self.step {
                0 => {
                    self.host.app_mut().close_all();
                    if self.rss_before.is_none() {
                        self.rss_before = rss_bytes();
                    }
                    self.step = 1;
                    self.arm(800);
                }
                1 => {
                    if !self.host.app().new_ms.is_empty() {
                        self.arm(300);
                        self.go(Phase::PooledOpen);
                        return;
                    }
                    let Some((x, y)) = self.anchor_point(ctx, "p8d_anchor") else {
                        self.arm(100);
                        return;
                    };
                    if !point_is_ours(x, y) {
                        let started = self.wait_started.get_or_insert_with(Instant::now);
                        if started.elapsed() > Duration::from_secs(10) {
                            self.fail(format!(
                                "measure anchor 0 at ({x},{y}) never became hit-testable"
                            ));
                            self.wait_started = None;
                            self.new_failures = self.new_failures.saturating_add(1);
                            self.step = 3;
                            self.arm(100);
                            return;
                        }
                        self.force_main_foreground();
                        raise_window(self.main_hwnd);
                        self.arm(250);
                        return;
                    }
                    self.wait_started = None;
                    self.host.app_mut().pending = Some(Pending {
                        kind: SampleKind::New,
                        remaining: 1,
                    });
                    self.force_main_foreground();
                    self.click_ours(x, y);
                    self.expect_index = 0;
                    self.action_started = Instant::now();
                    self.step = 2;
                    self.arm(60);
                }
                2 => {
                    self.nudge_popup(ctx, "p8d_menu");
                    if !self.host.app().new_ms.is_empty() {
                        self.arm(300);
                        self.go(Phase::PooledOpen);
                    } else if self.timed_out(Duration::from_secs(3)) {
                        self.new_failures = self.new_failures.saturating_add(1);
                        if self.new_failures >= 3 {
                            self.fail("new popup 0 never presented its first frame");
                            self.step = 3;
                            self.arm(120);
                        } else {
                            eprintln!("p8e: new popup 0 missed its first frame, retrying");
                            self.step = 1;
                            self.arm(400);
                        }
                    }
                }
                _ => {
                    self.arm(100);
                    self.go(Phase::PooledOpen);
                }
            }
        }

        fn phase_measure_rest(&mut self, ctx: &mut AppContext<'_>) {
            match self.step {
                0 => {
                    self.step = 1;
                    self.arm(250);
                }
                1 => {
                    if self.host.app().new_ms.len() >= NEW_RUNS {
                        self.rss_after = rss_bytes();
                        if self.measure_child {
                            self.write_measurement(ctx);
                        } else {
                            self.arm(300);
                            self.go(Phase::PrepareSingle);
                        }
                        return;
                    }
                    let index = self.host.app().new_ms.len();
                    let Some((x, y)) = self.anchor_point(ctx, MEASURE_ANCHORS[index]) else {
                        self.arm(100);
                        return;
                    };
                    if !point_is_ours(x, y) {
                        let started = self.wait_started.get_or_insert_with(Instant::now);
                        if started.elapsed() > Duration::from_secs(10) {
                            self.fail(format!(
                                "measure anchor {index} at ({x},{y}) never became hit-testable"
                            ));
                            self.wait_started = None;
                            self.new_failures = self.new_failures.saturating_add(1);
                            self.step = 3;
                            self.arm(100);
                            return;
                        }
                        self.force_main_foreground();
                        raise_window(self.main_hwnd);
                        self.arm(250);
                        return;
                    }
                    self.wait_started = None;
                    self.host.app_mut().pending = Some(Pending {
                        kind: SampleKind::New,
                        remaining: 1,
                    });
                    self.force_main_foreground();
                    self.click_ours(x, y);
                    self.expect_index = index;
                    self.action_started = Instant::now();
                    self.step = 2;
                    self.arm(60);
                }
                2 => {
                    if let Some(key) = MEASURE_MENUS.get(self.expect_index) {
                        self.nudge_popup(ctx, key);
                    }
                    if self.host.app().new_ms.len() > self.expect_index {
                        self.step = 1;
                        self.arm(160);
                    } else if self.host.app().menus_open[self.expect_index]
                        && self.timed_out(Duration::from_secs(2))
                    {
                        self.new_failures = self.new_failures.saturating_add(1);
                        self.host.app_mut().close_menu(self.expect_index);
                        if self.new_failures >= 3 {
                            self.fail(format!(
                                "new popup {} opened but never presented",
                                self.expect_index
                            ));
                            self.step = 3;
                            self.arm(200);
                        } else {
                            eprintln!(
                                "p8e: new popup {} missed its frame, retrying",
                                self.expect_index
                            );
                            self.step = 1;
                            self.arm(400);
                        }
                    } else if self.timed_out(Duration::from_secs(3)) {
                        self.new_failures = self.new_failures.saturating_add(1);
                        if self.new_failures >= 3 {
                            self.fail(format!(
                                "new popup {} never presented its first frame",
                                self.expect_index
                            ));
                            self.step = 3;
                            self.arm(120);
                        } else {
                            eprintln!(
                                "p8e: new popup {} missed its first frame, retrying",
                                self.expect_index
                            );
                            self.step = 1;
                            self.arm(400);
                        }
                    }
                }
                _ => {
                    self.rss_after = rss_bytes();
                    if self.measure_child {
                        self.write_measurement(ctx);
                    } else {
                        self.arm(200);
                        self.go(Phase::PrepareSingle);
                    }
                }
            }
        }

        fn write_measurement(&mut self, ctx: &mut AppContext<'_>) {
            let new = self
                .host
                .app()
                .new_ms
                .iter()
                .map(|value| format!("{value:.3}"))
                .collect::<Vec<_>>()
                .join(",");
            let pooled = self
                .host
                .app()
                .pooled_ms
                .iter()
                .map(|value| format!("{value:.3}"))
                .collect::<Vec<_>>()
                .join(",");
            let rss = self
                .rss_after
                .unwrap_or(0)
                .saturating_sub(self.rss_before.unwrap_or(0));
            let text = format!("new {new}\npooled {pooled}\nrss {rss}\n");
            if let Some(parent) = self.measure_file.parent() {
                let _ = std::fs::create_dir_all(parent);
            }
            match std::fs::write(&self.measure_file, text) {
                Ok(()) => eprintln!("p8e measurement written {}", self.measure_file.display()),
                Err(error) => eprintln!("p8e measurement write failed: {error}"),
            }
            ctx.request_exit();
            self.phase = Phase::Done;
        }

        fn phase_prepare_single(&mut self, ctx: &mut AppContext<'_>) {
            match self.step {
                0 => {
                    for index in 1..NEW_RUNS {
                        self.host.app_mut().close_menu(index);
                    }
                    self.action_started = Instant::now();
                    self.step = 1;
                    self.arm(150);
                }
                1 => {
                    let open = self.host.app().menus_open;
                    if self.visible() <= 1 && open[0] {
                        self.arm(250);
                        self.go(Phase::NormalSetup);
                    } else if self.timed_out(Duration::from_secs(4)) {
                        self.fail(format!(
                            "measurement menus did not settle (visible={} first_open={})",
                            self.visible(),
                            open[0]
                        ));
                        self.arm(250);
                        self.go(Phase::NormalSetup);
                    }
                }
                _ => {}
            }
            let _ = ctx;
        }

        fn phase_pooled_open(&mut self, ctx: &mut AppContext<'_>) {
            match self.step {
                0 => {
                    if self.host.app().pooled_ms.len() >= POOLED_RUNS {
                        self.arm(200);
                        self.go(Phase::MeasureRest);
                        return;
                    }
                    self.host.app_mut().close_menu(0);
                    self.action_started = Instant::now();
                    self.step = 1;
                    self.arm(120);
                }
                1 => {
                    let open = self.host.app().menus_open;
                    let others = open.iter().skip(1).filter(|value| **value).count();
                    if !open[0] && self.visible() == others {
                        let Some((x, y)) = self.anchor_point(ctx, "p8d_anchor") else {
                            self.arm(100);
                            return;
                        };
                        let index = self.host.app().pooled_ms.len();
                        self.host.app_mut().pending = Some(Pending {
                            kind: SampleKind::Pooled,
                            remaining: 1,
                        });
                        self.force_main_foreground();
                        self.click_ours(x, y);
                        self.expect_index = index;
                        self.action_started = Instant::now();
                        self.step = 2;
                        self.arm(60);
                    } else if self.timed_out(Duration::from_secs(4)) {
                        self.fail("pooled menu did not hide before reopening");
                        self.step = 2;
                        self.arm(50);
                    }
                }
                2 => {
                    self.nudge_owned_popup(ctx);
                    if self.host.app().pooled_ms.len() > self.expect_index {
                        if self.host.app().pooled_ms.len() >= POOLED_RUNS {
                            self.arm(400);
                            self.go(Phase::MeasureRest);
                        } else {
                            self.arm(120);
                            self.go(Phase::PooledOpen);
                        }
                    } else if self.timed_out(Duration::from_secs(3)) {
                        self.pooled_failures = self.pooled_failures.saturating_add(1);
                        if self.pooled_failures >= 2 {
                            self.fail(format!(
                                "pooled popup {} never presented its first frame",
                                self.expect_index
                            ));
                            self.arm(200);
                            self.go(Phase::MeasureRest);
                        } else {
                            eprintln!(
                                "p8e: pooled popup {} missed its first frame, retrying",
                                self.expect_index
                            );
                            self.host.app_mut().close_menu(0);
                            self.arm(120);
                            self.go(Phase::PooledOpen);
                        }
                    }
                }
                _ => {}
            }
        }

        fn phase_normal_setup(&mut self, ctx: &mut AppContext<'_>) {
            match self.step {
                0 => {
                    self.host.app_mut().close_all();
                    self.host.app_mut().menu_side = Side::Bottom;
                    if self.visible() == 0 && !self.menu_open() {
                        self.place_normal(ctx);
                        self.action_started = Instant::now();
                        self.step = 1;
                        self.arm(400);
                    } else if self.timed_out(Duration::from_secs(4)) {
                        self.fail("normal setup never settled");
                        self.step = 1;
                        self.arm(200);
                    }
                }
                _ => {
                    self.force_main_foreground();
                    self.go(Phase::NormalOpen);
                    self.arm(80);
                }
            }
        }

        fn phase_normal_open(&mut self, ctx: &mut AppContext<'_>) {
            match self.step {
                0 => {
                    let Some((x, y)) = self.anchor_point(ctx, "p8d_anchor") else {
                        self.arm(100);
                        return;
                    };
                    self.click_ours(x, y);
                    self.action_started = Instant::now();
                    self.step = 1;
                    self.arm(80);
                }
                1 => {
                    if self.menu_open() && self.visible() == 1 {
                        self.step = 2;
                        self.arm(250);
                    } else if self.timed_out(Duration::from_secs(3)) {
                        self.fail("normal menu never opened through the anchor click");
                        self.step = 2;
                        self.arm(50);
                    }
                }
                _ => {
                    self.begin_capture(ctx, "normal", Phase::ClickRename);
                }
            }
        }

        fn phase_capture(&mut self, ctx: &mut AppContext<'_>) {
            self.nudge_owned_popup(ctx);
            if self.main_shot.is_some() && !self.capture_popups.is_empty() {
                self.finish_capture(ctx, true);
            } else if self.timed_out(Duration::from_secs(5)) {
                self.fail(format!(
                    "{} capture timed out (main={} popups={})",
                    self.capture_name,
                    self.main_shot.is_some(),
                    self.capture_popups.len()
                ));
                self.finish_capture(ctx, false);
            } else {
                self.arm(30);
            }
        }

        fn phase_click_rename(&mut self, ctx: &mut AppContext<'_>) {
            match self.step {
                0 => {
                    let Some((x, y)) = self.text_point(ctx, "Rename") else {
                        self.fail("the popup frame has no Rename label to click");
                        self.host.app_mut().close_all();
                        self.arm(200);
                        self.go(Phase::EscapeSetup);
                        return;
                    };
                    self.force_main_foreground();
                    self.click_ours(x, y);
                    self.action_started = Instant::now();
                    self.step = 1;
                    self.arm(100);
                }
                1 => {
                    let picked = self.host.app().picks.contains(&"rename");
                    if picked && !self.menu_open() && self.visible() == 0 {
                        let foreground = get_foreground();
                        self.check(
                            foreground == self.main_hwnd,
                            format!(
                                "foreground changed after the Rename click: {foreground} != {}",
                                self.main_hwnd
                            ),
                        );
                        self.arm(200);
                        self.go(Phase::EscapeSetup);
                    } else if self.timed_out(Duration::from_secs(3)) {
                        self.fail(format!(
                            "Rename click did not close the menu (picked={picked} open={} visible={})",
                            self.menu_open(),
                            self.visible()
                        ));
                        self.host.app_mut().close_all();
                        self.arm(200);
                        self.go(Phase::EscapeSetup);
                    }
                }
                _ => {}
            }
        }

        fn phase_escape(&mut self, ctx: &mut AppContext<'_>) {
            match self.step {
                0 => {
                    let Some((x, y)) = self.anchor_point(ctx, "p8d_anchor") else {
                        self.arm(100);
                        return;
                    };
                    self.force_main_foreground();
                    self.click_ours(x, y);
                    self.action_started = Instant::now();
                    self.step = 1;
                    self.arm(80);
                }
                1 => {
                    if self.menu_open() && self.visible() == 1 {
                        self.step = 2;
                        self.arm(250);
                    } else if self.timed_out(Duration::from_secs(3)) {
                        self.fail("menu never reopened for the Escape case");
                        self.step = 2;
                        self.arm(50);
                    }
                }
                2 => {
                    self.send_escape();
                    self.action_started = Instant::now();
                    self.step = 3;
                    self.arm(100);
                }
                _ => {
                    if !self.menu_open() && self.visible() == 0 {
                        eprintln!("p8e: escape scenario closed the menu");
                        self.arm(200);
                        self.go(Phase::OutsideSetup);
                    } else if self.timed_out(Duration::from_secs(3)) {
                        self.fail(format!(
                            "Escape did not close the menu (open={} visible={})",
                            self.menu_open(),
                            self.visible()
                        ));
                        self.host.app_mut().close_all();
                        self.arm(200);
                        self.go(Phase::OutsideSetup);
                    }
                }
            }
        }

        fn phase_outside(&mut self, ctx: &mut AppContext<'_>) {
            match self.step {
                0 => {
                    let Some((x, y)) = self.anchor_point(ctx, "p8d_anchor") else {
                        self.arm(100);
                        return;
                    };
                    self.force_main_foreground();
                    self.click_ours(x, y);
                    self.action_started = Instant::now();
                    self.step = 1;
                    self.arm(80);
                }
                1 => {
                    if self.menu_open() && self.visible() == 1 {
                        self.step = 2;
                        self.arm(250);
                    } else if self.timed_out(Duration::from_secs(3)) {
                        self.fail("menu never reopened for the outside-click case");
                        self.step = 2;
                        self.arm(50);
                    }
                }
                2 => {
                    let (x, y) = self.empty_point(ctx);
                    self.click_ours(x, y);
                    self.action_started = Instant::now();
                    self.step = 3;
                    self.arm(120);
                }
                _ => {
                    if !self.menu_open() && self.visible() == 0 {
                        eprintln!("p8e: outside click closed the menu");
                        self.arm(200);
                        self.go(Phase::BlurSetup);
                    } else if self.timed_out(Duration::from_secs(3)) {
                        self.fail(format!(
                            "outside click did not close the menu (open={} visible={})",
                            self.menu_open(),
                            self.visible()
                        ));
                        self.host.app_mut().close_all();
                        self.arm(200);
                        self.go(Phase::BlurSetup);
                    }
                }
            }
        }

        fn phase_blur(&mut self, ctx: &mut AppContext<'_>) {
            match self.step {
                0 => {
                    let Some((x, y)) = self.anchor_point(ctx, "p8d_anchor") else {
                        self.arm(100);
                        return;
                    };
                    self.force_main_foreground();
                    self.click_ours(x, y);
                    self.action_started = Instant::now();
                    self.step = 1;
                    self.arm(80);
                }
                1 => {
                    if self.menu_open() && self.visible() == 1 {
                        self.spawn_helper();
                        self.step = 2;
                        self.arm(120);
                    } else if self.timed_out(Duration::from_secs(3)) {
                        self.fail("menu never reopened for the helper-window case");
                        self.step = 2;
                        self.arm(50);
                    }
                }
                2 => {
                    self.poll_helper();
                    if self.helper_seen && !self.menu_open() {
                        self.step = 3;
                        self.arm(150);
                    } else if self.timed_out(Duration::from_secs(5)) {
                        self.fail(format!(
                            "helper window did not take the foreground and dismiss the menu \
                             (seen={} open={})",
                            self.helper_seen,
                            self.menu_open()
                        ));
                        self.step = 3;
                        self.arm(50);
                    }
                }
                3 => {
                    let exited = self
                        .helper
                        .as_mut()
                        .is_none_or(|child| !matches!(child.try_wait(), Ok(None)));
                    if exited {
                        self.check(
                            self.helper_seen,
                            "helper window never became the foreground window",
                        );
                        self.check(
                            !self.menu_open() && self.visible() == 0,
                            format!(
                                "helper activation did not close the menu (open={} visible={})",
                                self.menu_open(),
                                self.visible()
                            ),
                        );
                        if self.helper_seen {
                            eprintln!(
                                "p8e: helper window activation closed the menu ({})",
                                self.helper_status
                                    .as_deref()
                                    .map(str::trim)
                                    .unwrap_or("unknown method")
                            );
                        }
                        self.helper = None;
                        let _ = std::fs::remove_file(helper_status_path());
                        self.step = 4;
                        self.arm(200);
                    } else if self.timed_out(Duration::from_secs(6)) {
                        self.fail("helper process did not exit");
                        self.helper = None;
                        self.step = 4;
                        self.arm(100);
                    }
                }
                _ => {
                    if self.visible() == 0 {
                        self.arm(200);
                        self.go(Phase::RightSetup);
                    } else if self.timed_out(Duration::from_secs(3)) {
                        self.fail("menu stayed visible after the helper case");
                        self.host.app_mut().close_all();
                        self.arm(200);
                        self.go(Phase::RightSetup);
                    }
                }
            }
            let _ = ctx;
        }

        fn spawn_helper(&mut self) {
            let status = helper_status_path();
            let _ = std::fs::remove_file(&status);
            let exe = std::env::current_exe().unwrap_or_else(|_| PathBuf::from("p8d_popup"));
            match std::process::Command::new(exe)
                .arg("--helper")
                .arg(&status)
                .spawn()
            {
                Ok(child) => self.helper = Some(child),
                Err(error) => {
                    self.fail(format!("spawning the helper window failed: {error}"));
                }
            }
        }

        fn poll_helper(&mut self) {
            if self.helper_seen {
                return;
            }
            let Some(child) = self.helper.as_ref() else {
                return;
            };
            let foreground = get_foreground();
            let mut pid = 0u32;
            let helper_is_foreground = if foreground != 0 {
                let thread = unsafe {
                    GetWindowThreadProcessId(foreground as *mut std::ffi::c_void, &mut pid)
                };
                thread != 0 && pid == child.id()
            } else {
                false
            };
            if helper_is_foreground {
                self.helper_seen = true;
                self.helper_status = std::fs::read_to_string(helper_status_path()).ok();
            }
        }

        fn phase_right(&mut self, ctx: &mut AppContext<'_>) {
            match self.step {
                0 => {
                    self.host.app_mut().close_all();
                    if self.visible() == 0 && !self.menu_open() {
                        self.host.app_mut().menu_side = Side::Right;
                        let _ = ctx
                            .window
                            .request_inner_size(LogicalSize::new(300.0, 420.0));
                        self.action_started = Instant::now();
                        self.step = 1;
                        self.arm(500);
                    } else if self.timed_out(Duration::from_secs(4)) {
                        self.fail("right-edge setup never settled");
                        self.step = 1;
                        self.arm(200);
                    }
                }
                1 => {
                    self.place_right(ctx);
                    self.action_started = Instant::now();
                    self.step = 2;
                    self.arm(500);
                }
                2 => {
                    let Some((x, y)) = self.anchor_point(ctx, "p8d_anchor") else {
                        self.arm(100);
                        return;
                    };
                    self.force_main_foreground();
                    self.click_ours(x, y);
                    self.action_started = Instant::now();
                    self.step = 3;
                    self.arm(80);
                }
                3 => {
                    if self.menu_open() && self.visible() == 1 {
                        self.step = 4;
                        self.arm(250);
                    } else if self.timed_out(Duration::from_secs(3)) {
                        self.fail("right-edge menu never opened");
                        self.step = 4;
                        self.arm(50);
                    }
                }
                _ => {
                    self.begin_capture(ctx, "right-edge", Phase::BottomSetup);
                }
            }
        }

        fn phase_bottom(&mut self, ctx: &mut AppContext<'_>) {
            match self.step {
                0 => {
                    self.host.app_mut().close_all();
                    if self.visible() == 0 && !self.menu_open() {
                        self.host.app_mut().menu_side = Side::Bottom;
                        let _ = ctx
                            .window
                            .request_inner_size(LogicalSize::new(500.0, 420.0));
                        self.action_started = Instant::now();
                        self.step = 1;
                        self.arm(500);
                    } else if self.timed_out(Duration::from_secs(4)) {
                        self.fail("right-edge popup never hid before the bottom case");
                        self.host.app_mut().close_all();
                        self.action_started = Instant::now();
                        self.step = 1;
                        self.arm(200);
                    }
                }
                1 => {
                    self.place_bottom(ctx);
                    self.action_started = Instant::now();
                    self.step = 2;
                    self.arm(500);
                }
                2 => {
                    let Some((x, y)) = self.anchor_point(ctx, "p8d_anchor") else {
                        self.arm(100);
                        return;
                    };
                    self.force_main_foreground();
                    self.click_ours(x, y);
                    self.action_started = Instant::now();
                    self.step = 3;
                    self.arm(80);
                }
                3 => {
                    if self.menu_open() && self.visible() == 1 {
                        self.step = 4;
                        self.arm(250);
                    } else if self.timed_out(Duration::from_secs(3)) {
                        self.fail("bottom-edge menu never opened");
                        self.step = 4;
                        self.arm(50);
                    }
                }
                _ => {
                    self.begin_capture(ctx, "bottom-edge", Phase::IdleSetup);
                }
            }
        }

        fn phase_idle_setup(&mut self, ctx: &mut AppContext<'_>) {
            match self.step {
                0 => {
                    self.host.app_mut().close_all();
                    if self.visible() == 0 && !self.menu_open() {
                        let _ = ctx
                            .window
                            .request_inner_size(LogicalSize::new(800.0, 480.0));
                        self.host.app_mut().menu_side = Side::Bottom;
                        self.action_started = Instant::now();
                        self.step = 1;
                        self.arm(500);
                    } else if self.timed_out(Duration::from_secs(4)) {
                        self.fail("bottom-edge popup never hid before the idle case");
                        self.action_started = Instant::now();
                        self.step = 1;
                        self.arm(200);
                    }
                }
                1 => {
                    self.place_normal(ctx);
                    let Some((x, y)) = self.anchor_point(ctx, "p8d_anchor") else {
                        self.arm(100);
                        return;
                    };
                    self.force_main_foreground();
                    self.click_ours(x, y);
                    self.action_started = Instant::now();
                    self.step = 2;
                    self.arm(80);
                }
                2 => {
                    if self.menu_open() && self.visible() == 1 {
                        self.step = 3;
                        self.arm(300);
                    } else if self.timed_out(Duration::from_secs(3)) {
                        self.fail("idle menu never opened");
                        self.step = 3;
                        self.arm(50);
                    }
                }
                3 => {
                    let (x, y) = self.empty_point(ctx);
                    self.send_move(x, y);
                    self.action_started = Instant::now();
                    self.step = 4;
                    self.arm(600);
                }
                _ => {
                    self.idle_base_popup = self.popup_frames();
                    self.idle_base_main = self.host.app().main_frames;
                    self.idle_start = Some(Instant::now());
                    ctx.redraw_at(Instant::now() + IDLE);
                    self.step = 0;
                    self.phase = Phase::Idle;
                }
            }
        }

        fn phase_idle(&mut self, _ctx: &mut AppContext<'_>) {
            let now = Instant::now();
            let Some(start) = self.idle_start else {
                self.idle_start = Some(now);
                return;
            };
            if now.duration_since(start) < IDLE {
                return;
            }
            self.idle_popup_frames = self.popup_frames().saturating_sub(self.idle_base_popup);
            self.idle_main_frames = self
                .host
                .app()
                .main_frames
                .saturating_sub(self.idle_base_main);
            self.check(
                self.idle_popup_frames == 0,
                format!(
                    "idle popup presented {} frames over five seconds",
                    self.idle_popup_frames
                ),
            );
            eprintln!(
                "p8e idle idle_popup_frames={} idle_main_frames={}",
                self.idle_popup_frames, self.idle_main_frames
            );
            self.arm(150);
            self.go(Phase::HoverSetup);
        }

        fn phase_hover_setup(&mut self, ctx: &mut AppContext<'_>) {
            match self.step {
                0 => {
                    if !self.menu_open() {
                        let Some((x, y)) = self.anchor_point(ctx, "p8d_anchor") else {
                            self.arm(100);
                            return;
                        };
                        self.force_main_foreground();
                        self.click_ours(x, y);
                        self.arm(250);
                        return;
                    }
                    let (x, y) = self.empty_point(ctx);
                    self.send_move(x, y);
                    self.step = 1;
                    self.arm(400);
                }
                _ => {
                    self.hover_base_popup = self.popup_frames();
                    self.hover_base_main = self.host.app().main_frames;
                    self.hover_index = 0;
                    self.arm(40);
                    self.go(Phase::Hover);
                }
            }
        }

        fn phase_hover(&mut self, ctx: &mut AppContext<'_>) {
            if self.hover_index < 12 {
                let point = if self.hover_index % 2 == 0 {
                    self.hover_point(ctx)
                        .unwrap_or_else(|| self.empty_point(ctx))
                } else {
                    self.empty_point(ctx)
                };
                self.send_move(point.0, point.1);
                self.hover_index += 1;
                self.arm(45);
                return;
            }
            self.hover_popup_frames = self.popup_frames().saturating_sub(self.hover_base_popup);
            self.hover_main_frames = self
                .host
                .app()
                .main_frames
                .saturating_sub(self.hover_base_main);
            self.check(
                self.hover_popup_frames == 0,
                format!(
                    "popup presented {} frames while the main window repainted from hovering",
                    self.hover_popup_frames
                ),
            );
            self.check(
                self.hover_main_frames > 0,
                "the main window did not repaint while hovering",
            );
            eprintln!(
                "p8e hover hover_popup_frames={} hover_main_frames={}",
                self.hover_popup_frames, self.hover_main_frames
            );
            self.host.app_mut().close_all();
            self.arm(200);
            self.go(Phase::Finalize);
        }

        fn phase_finalize(&mut self, ctx: &mut AppContext<'_>) {
            self.write_rects();
            self.print_summary();
            self.arm(50);
            ctx.request_exit();
            self.phase = Phase::Done;
        }

        fn write_rects(&mut self) {
            let normal = self
                .cases
                .iter()
                .find(|case| case.name == "normal")
                .or_else(|| self.cases.first())
                .cloned();
            let mut body = String::new();
            if let Some(normal) = &normal {
                body.push_str(&case_body(normal));
                body.push_str(",\n  \"cases\": {");
                for (index, case) in self.cases.iter().enumerate() {
                    if index > 0 {
                        body.push(',');
                    }
                    body.push_str(&format!("\n    \"{}\": {{{}}}", case.name, case_body(case)));
                }
                body.push_str("\n  }");
            } else {
                body.push_str("\"work_area\": {\"x\":0,\"y\":0,\"width\":0,\"height\":0},\n  \"popups\": [],\n  \"main\": {\"position\":{\"x\":0,\"y\":0},\"size\":{\"width\":0,\"height\":0}}");
            }
            let json = format!("{{\n  {body}\n}}\n");
            let path = format!("{OUT}/popup-rects.json");
            if let Some(parent) = Path::new(&path).parent() {
                let _ = std::fs::create_dir_all(parent);
            }
            if let Err(error) = std::fs::write(&path, json) {
                self.fail(format!("writing {path} failed: {error}"));
            } else {
                eprintln!("p8e rects {path}");
            }
        }

        fn print_summary(&mut self) {
            let measurement = self.measurement.clone().unwrap_or_default();
            let new_ms = if measurement.new_ms.is_empty() {
                self.host.app().new_ms.clone()
            } else {
                measurement.new_ms.clone()
            };
            let pooled_ms = if measurement.pooled_ms.is_empty() {
                self.host.app().pooled_ms.clone()
            } else {
                measurement.pooled_ms.clone()
            };
            let (new_median, new_min, new_max) = stats(&new_ms);
            let (pooled_median, pooled_min, pooled_max) = stats(&pooled_ms);
            let rss_delta = if measurement.rss_delta > 0 {
                measurement.rss_delta
            } else {
                self.rss_after
                    .unwrap_or(0)
                    .saturating_sub(self.rss_before.unwrap_or(0))
            };
            let rss_per_popup = rss_delta / u64::try_from(NEW_RUNS).unwrap_or(1).max(1);
            let quad = std::mem::size_of::<ferrous_ui_render::GuiQuad>() as u64;
            let text = std::mem::size_of::<ferrous_ui_render::TextQuad>() as u64;
            let instances = 1024 * (quad + text * 2);
            let svg = 4096 * 28 + 4096 * 4;
            let (popup_w, popup_h) = self.popup_size;
            let swapchain = u64::from(3 * popup_w.max(1) * popup_h.max(1) * 4);
            let vram = instances + svg + swapchain;
            self.check(
                new_ms.len() == NEW_RUNS,
                format!(
                    "expected {NEW_RUNS} new-popup samples, got {}",
                    new_ms.len()
                ),
            );
            self.check(
                pooled_ms.len() == POOLED_RUNS,
                format!(
                    "expected {POOLED_RUNS} pooled-popup samples, got {}",
                    pooled_ms.len()
                ),
            );
            let summary = format!(
                "{{\"new_ms\":{:?},\"new_median_ms\":{new_median:.3},\
                 \"new_min_ms\":{new_min:.3},\"new_max_ms\":{new_max:.3},\
                 \"pooled_ms\":{:?},\"pooled_median_ms\":{pooled_median:.3},\
                 \"pooled_min_ms\":{pooled_min:.3},\"pooled_max_ms\":{pooled_max:.3},\
                 \"rss_delta_bytes\":{rss_delta},\"rss_per_popup_bytes\":{rss_per_popup},\
                 \"popup_vram_bytes\":{vram},\"vram_instances\":{instances},\
                 \"vram_svg\":{svg},\"vram_swapchain\":{swapchain},\"vram_computed\":true,\
                 \"popup_physical\":[{popup_w},{popup_h}],\
                 \"idle_popup_frames\":{},\"idle_main_frames\":{},\
                 \"popup_frames_during_main_hover\":{},\"main_frames_during_hover\":{},\
                 \"failures\":{:?}}}",
                new_ms,
                pooled_ms,
                self.idle_popup_frames,
                self.idle_main_frames,
                self.hover_popup_frames,
                self.hover_main_frames,
                self.failures
            );
            eprintln!("p8e summary {summary}");
            let path = format!("{OUT}/summary.json");
            if let Some(parent) = Path::new(&path).parent() {
                let _ = std::fs::create_dir_all(parent);
            }
            if let Err(error) = std::fs::write(&path, format!("{summary}\n")) {
                self.fail(format!("writing {path} failed: {error}"));
            }
        }
    }

    impl FerrousApp for Driver {
        fn setup(&mut self, ctx: &mut AppContext<'_>) {
            self.host.setup(ctx);
            self.main_hwnd = window_hwnd(ctx.window);
        }

        fn update(&mut self, ctx: &mut AppContext<'_>) {
            self.host.update(ctx);
            self.frames = self.frames.saturating_add(1);
            self.drive(ctx);
        }

        fn draw_ui(&mut self, dc: &mut ferrous_engine::DrawContext<'_, '_>) {
            self.host.draw_ui(dc);
        }

        fn draw_popup(
            &mut self,
            popup: ferrous_engine::windows::PopupId,
            dc: &mut ferrous_engine::DrawContext<'_, '_>,
        ) {
            self.host.draw_popup(popup, dc);
        }

        fn on_popup_event(
            &mut self,
            popup: ferrous_engine::windows::PopupId,
            event: &WindowEvent,
            ctx: &mut AppContext<'_>,
        ) {
            self.host.on_popup_event(popup, event, ctx);
        }

        fn on_window_event(&mut self, event: &WindowEvent, ctx: &mut AppContext<'_>) {
            self.host.on_window_event(event, ctx);
            match event {
                WindowEvent::CursorMoved { position, .. } => {
                    self.cursor = (position.x, position.y);
                }
                WindowEvent::KeyboardInput { event: key, .. } => {
                    if key.state == ElementState::Pressed
                        && matches!(key.logical_key, Key::Named(NamedKey::Escape))
                        && self.menu_open()
                    {
                        self.host.app_mut().close_all();
                    }
                }
                WindowEvent::MouseInput {
                    state: ElementState::Pressed,
                    button: WinitMouseButton::Left,
                    ..
                } => {
                    if self.menu_open() && !self.pointer_in_anchor() {
                        self.host.app_mut().close_all();
                    }
                }
                WindowEvent::Focused(false) => {
                    if self.menu_open() {
                        self.host.app_mut().close_all();
                    }
                }
                _ => {}
            }
        }

        fn on_scale_factor_changed(
            &mut self,
            scale_factor: f64,
            new_size: (u32, u32),
            ctx: &mut AppContext<'_>,
        ) {
            self.host
                .on_scale_factor_changed(scale_factor, new_size, ctx);
        }

        fn on_resize(&mut self, new_size: (u32, u32), ctx: &mut AppContext<'_>) {
            self.host.on_resize(new_size, ctx);
        }

        fn on_screenshot(&mut self, screenshot: Screenshot) {
            self.main_shot = Some(screenshot.clone());
            self.host.on_screenshot(screenshot);
        }

        fn on_popup_screenshot(&mut self, capture: PopupCapture) {
            self.capture_popups.push(capture.clone());
            self.host.on_popup_screenshot(capture);
        }

        fn on_exit(&mut self) {
            lower_window(self.main_hwnd);
            self.host.on_exit();
        }
    }

    fn inside_rect(position: (i32, i32), size: (u32, u32), work: WorkArea) -> bool {
        let left = work.x as i32;
        let top = work.y as i32;
        let right = left + work.width as i32;
        let bottom = top + work.height as i32;
        position.0 >= left
            && position.1 >= top
            && position.0 + size.0 as i32 <= right
            && position.1 + size.1 as i32 <= bottom
    }

    #[allow(clippy::too_many_arguments, clippy::cast_possible_wrap)]
    fn blit(
        canvas: &mut [u8],
        canvas_width: u32,
        canvas_height: u32,
        offset_x: i32,
        offset_y: i32,
        source: &[u8],
        source_width: u32,
        source_height: u32,
        blend: bool,
    ) {
        let canvas_width = canvas_width as i32;
        let canvas_height = canvas_height as i32;
        let source_width = source_width as i32;
        let source_height = source_height as i32;
        if source_width <= 0 || source_height <= 0 {
            return;
        }
        for y in 0..source_height {
            let target_y = offset_y + y;
            if !(0..canvas_height).contains(&target_y) {
                continue;
            }
            for x in 0..source_width {
                let target_x = offset_x + x;
                if !(0..canvas_width).contains(&target_x) {
                    continue;
                }
                let source_index = ((y * source_width + x) * 4) as usize;
                let target_index = ((target_y * canvas_width + target_x) * 4) as usize;
                if source_index + 4 > source.len() || target_index + 4 > canvas.len() {
                    return;
                }
                let source_pixel = &source[source_index..source_index + 4];
                if blend {
                    let alpha = u32::from(source_pixel[3]);
                    for channel in 0..3 {
                        let destination = u32::from(canvas[target_index + channel]);
                        let value = u32::from(source_pixel[channel])
                            + (destination * (255 - alpha) + 127) / 255;
                        canvas[target_index + channel] = value.min(255) as u8;
                    }
                    canvas[target_index + 3] = 255;
                } else {
                    canvas[target_index..target_index + 4].copy_from_slice(source_pixel);
                    canvas[target_index + 3] = 255;
                }
            }
        }
    }

    fn case_body(case: &Case) -> String {
        let (work_x, work_y, work_width, work_height) = case.work;
        let mut popups = String::new();
        for (index, popup) in case.popups.iter().enumerate() {
            if index > 0 {
                popups.push(',');
            }
            popups.push_str(&format!(
                "{{\"id\":{},\"position\":{{\"x\":{},\"y\":{}}},\
                 \"size\":{{\"width\":{},\"height\":{}}},\"inside\":{}}}",
                popup.id,
                popup.position.0,
                popup.position.1,
                popup.size.0,
                popup.size.1,
                popup.inside
            ));
        }
        format!(
            "\"work_area\": {{\"x\":{work_x},\"y\":{work_y},\
             \"width\":{work_width},\"height\":{work_height}}},\n  \
             \"popups\": [{popups}],\n  \
             \"main\": {{\"position\":{{\"x\":{},\"y\":{}}},\
             \"size\":{{\"width\":{},\"height\":{}}}}}",
            case.main_position.0, case.main_position.1, case.main_size.0, case.main_size.1
        )
    }

    fn stats(values: &[f64]) -> (f64, f64, f64) {
        if values.is_empty() {
            return (f64::NAN, f64::NAN, f64::NAN);
        }
        let mut sorted = values.to_vec();
        sorted.sort_by(f64::total_cmp);
        let middle = sorted.len() / 2;
        let median = if sorted.len() % 2 == 0 {
            (sorted[middle - 1] + sorted[middle]) / 2.0
        } else {
            sorted[middle]
        };
        (median, sorted[0], sorted[sorted.len() - 1])
    }

    fn window_hwnd(window: &Window) -> isize {
        let Ok(handle) = window.window_handle() else {
            return 0;
        };
        match handle.as_raw() {
            RawWindowHandle::Win32(win32) => win32.hwnd.get(),
            _ => 0,
        }
    }

    fn get_foreground() -> isize {
        unsafe { GetForegroundWindow() as isize }
    }

    fn window_at(x: i32, y: i32) -> (isize, u32) {
        unsafe {
            let hwnd = WindowFromPoint(POINT { x, y });
            if hwnd.is_null() {
                return (0, 0);
            }
            let mut pid = 0u32;
            GetWindowThreadProcessId(hwnd, &mut pid);
            (hwnd as isize, pid)
        }
    }

    fn point_is_ours(x: i32, y: i32) -> bool {
        window_at(x, y).1 == std::process::id()
    }

    fn raise_window(hwnd: isize) {
        if hwnd == 0 {
            return;
        }
        let handle = hwnd as *mut std::ffi::c_void;
        let flags = SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE | SWP_NOOWNERZORDER;
        unsafe {
            SetWindowPos(handle, HWND_TOPMOST, 0, 0, 0, 0, flags);
            SetWindowPos(handle, HWND_NOTOPMOST, 0, 0, 0, 0, flags);
            ShowWindow(handle, SW_SHOW);
        }
    }

    fn owned_popup(hwnd: isize) -> isize {
        if hwnd == 0 {
            return 0;
        }
        unsafe { GetWindow(hwnd as *mut std::ffi::c_void, GW_ENABLEDPOPUP) as isize }
    }

    static OUR_WINDOWS: std::sync::Mutex<Vec<(isize, i32, i32, i32, i32, bool)>> =
        std::sync::Mutex::new(Vec::new());

    unsafe extern "system" fn enum_our_windows(hwnd: HWND, _param: LPARAM) -> BOOL {
        let mut pid = 0u32;
        GetWindowThreadProcessId(hwnd, &mut pid);
        if pid == std::process::id() {
            let mut rect = RECT {
                left: 0,
                top: 0,
                right: 0,
                bottom: 0,
            };
            if GetWindowRect(hwnd, &mut rect) != 0 {
                let visible = IsWindowVisible(hwnd) != 0;
                if let Ok(mut list) = OUR_WINDOWS.lock() {
                    list.push((
                        hwnd as isize,
                        rect.left,
                        rect.top,
                        rect.right,
                        rect.bottom,
                        visible,
                    ));
                }
            }
        }
        1
    }

    fn find_our_window(expected: (i32, i32, u32, u32)) -> Option<isize> {
        if let Ok(mut list) = OUR_WINDOWS.lock() {
            list.clear();
        }
        unsafe {
            EnumWindows(Some(enum_our_windows), 0);
        }
        let list = OUR_WINDOWS.lock().ok()?.clone();
        list.into_iter()
            .find(|(_, left, top, right, bottom, visible)| {
                *visible
                    && (*left - expected.0).abs() <= 3
                    && (*top - expected.1).abs() <= 3
                    && ((*right - *left) - expected.2 as i32).abs() <= 4
                    && ((*bottom - *top) - expected.3 as i32).abs() <= 4
            })
            .map(|(hwnd, ..)| hwnd)
    }

    fn lower_window(hwnd: isize) {
        if hwnd == 0 {
            return;
        }
        let handle = hwnd as *mut std::ffi::c_void;
        let flags = SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE | SWP_NOOWNERZORDER;
        unsafe {
            SetWindowPos(handle, HWND_NOTOPMOST, 0, 0, 0, 0, flags);
        }
    }

    fn cursor_pos() -> Option<(i32, i32)> {
        let mut point = POINT { x: 0, y: 0 };
        let ok = unsafe { GetCursorPos(&mut point) };
        (ok != 0).then_some((point.x, point.y))
    }

    fn force_foreground(hwnd: isize) -> Result<(), String> {
        if hwnd == 0 {
            return Err("no window handle".to_string());
        }
        let handle = hwnd as *mut std::ffi::c_void;
        unsafe {
            ShowWindow(handle, SW_SHOW);
            SetActiveWindow(handle);
            SetFocus(handle);
            if get_foreground() == hwnd {
                return Ok(());
            }
            let raised = SetForegroundWindow(handle);
            if raised != 0 && get_foreground() == hwnd {
                return Ok(());
            }
            let foreground = GetForegroundWindow();
            if foreground.is_null() {
                return Err(format!(
                    "SetForegroundWindow returned {raised} with no foreground"
                ));
            }
            let mut pid = 0u32;
            let foreground_thread = GetWindowThreadProcessId(foreground, &mut pid);
            let own_thread = GetWindowThreadProcessId(handle, std::ptr::null_mut());
            if foreground_thread == 0 || own_thread == 0 {
                return Err("GetWindowThreadProcessId failed".to_string());
            }
            AttachThreadInput(own_thread, foreground_thread, 1);
            let retried = SetForegroundWindow(handle);
            AttachThreadInput(own_thread, foreground_thread, 0);
            if retried != 0 && get_foreground() == hwnd {
                Ok(())
            } else {
                Err(format!(
                    "SetForegroundWindow returned {raised}, AttachThreadInput retry returned {retried}"
                ))
            }
        }
    }

    fn absolute(x: i32, y: i32) -> (i32, i32) {
        unsafe {
            let left = GetSystemMetrics(SM_XVIRTUALSCREEN);
            let top = GetSystemMetrics(SM_YVIRTUALSCREEN);
            let width = GetSystemMetrics(SM_CXVIRTUALSCREEN).max(2);
            let height = GetSystemMetrics(SM_CYVIRTUALSCREEN).max(2);
            let nx = f64::from(x - left) * 65_535.0 / f64::from(width - 1);
            let ny = f64::from(y - top) * 65_535.0 / f64::from(height - 1);
            (nx.round() as i32, ny.round() as i32)
        }
    }

    fn send_inputs(inputs: &[INPUT]) -> Result<(), String> {
        let sent = unsafe {
            SendInput(
                inputs.len() as u32,
                inputs.as_ptr(),
                std::mem::size_of::<INPUT>() as i32,
            )
        };
        if sent as usize == inputs.len() {
            Ok(())
        } else {
            let error = unsafe { GetLastError() };
            Err(format!(
                "SendInput inserted {sent}/{} (GetLastError={error})",
                inputs.len()
            ))
        }
    }

    fn mouse_input(dx: i32, dy: i32, flags: u32) -> INPUT {
        INPUT {
            r#type: INPUT_MOUSE,
            Anonymous: INPUT_0 {
                mi: MOUSEINPUT {
                    dx,
                    dy,
                    mouseData: 0,
                    dwFlags: flags,
                    time: 0,
                    dwExtraInfo: 0,
                },
            },
        }
    }

    fn move_to(x: i32, y: i32) -> Result<(), String> {
        let (dx, dy) = absolute(x, y);
        send_inputs(&[mouse_input(
            dx,
            dy,
            MOUSEEVENTF_MOVE | MOUSEEVENTF_ABSOLUTE | MOUSEEVENTF_VIRTUALDESK,
        )])
    }

    fn click_at(x: i32, y: i32) -> Result<(), String> {
        let (dx, dy) = absolute(x, y);
        let move_flags = MOUSEEVENTF_MOVE | MOUSEEVENTF_ABSOLUTE | MOUSEEVENTF_VIRTUALDESK;
        send_inputs(&[mouse_input(dx, dy, move_flags)])?;
        std::thread::sleep(Duration::from_millis(40));
        if cursor_pos().is_none_or(|(cx, cy)| (cx - x).abs() > 2 || (cy - y).abs() > 2) {
            send_inputs(&[mouse_input(dx, dy, move_flags)])?;
            std::thread::sleep(Duration::from_millis(40));
        }
        send_inputs(&[mouse_input(0, 0, MOUSEEVENTF_LEFTDOWN)])?;
        std::thread::sleep(Duration::from_millis(30));
        send_inputs(&[mouse_input(0, 0, MOUSEEVENTF_LEFTUP)])?;
        std::thread::sleep(Duration::from_millis(30));
        Ok(())
    }

    fn press_escape() -> Result<(), String> {
        let down = INPUT {
            r#type: INPUT_KEYBOARD,
            Anonymous: INPUT_0 {
                ki: KEYBDINPUT {
                    wVk: VK_ESCAPE,
                    wScan: 0,
                    dwFlags: 0,
                    time: 0,
                    dwExtraInfo: 0,
                },
            },
        };
        let up = INPUT {
            r#type: INPUT_KEYBOARD,
            Anonymous: INPUT_0 {
                ki: KEYBDINPUT {
                    wVk: VK_ESCAPE,
                    wScan: 0,
                    dwFlags: KEYEVENTF_KEYUP,
                    time: 0,
                    dwExtraInfo: 0,
                },
            },
        };
        send_inputs(&[down])?;
        std::thread::sleep(Duration::from_millis(25));
        send_inputs(&[up])?;
        Ok(())
    }

    fn rss_bytes() -> Option<u64> {
        powershell_u64(&format!(
            "(Get-Process -Id {}).WorkingSet64",
            std::process::id()
        ))
    }

    fn powershell_u64(command: &str) -> Option<u64> {
        let output = std::process::Command::new("powershell")
            .args(["-NoProfile", "-NonInteractive", "-Command", command])
            .output()
            .ok()?;
        String::from_utf8(output.stdout).ok()?.trim().parse().ok()
    }

    fn helper_status_path() -> PathBuf {
        PathBuf::from(format!("{OUT}/helper.txt"))
    }

    struct HelperWindow {
        window: Option<Window>,
        until: Instant,
        status: PathBuf,
    }

    impl ApplicationHandler for HelperWindow {
        fn resumed(&mut self, event_loop: &ActiveEventLoop) {
            if self.window.is_some() {
                return;
            }
            let attributes = Window::default_attributes()
                .with_title("GUIMaker Phase 8E helper")
                .with_inner_size(LogicalSize::new(260.0, 140.0))
                .with_position(PhysicalPosition::new(60, 60))
                .with_resizable(false);
            let Ok(window) = event_loop.create_window(attributes) else {
                let _ = std::fs::write(&self.status, "method=create-failed ok=false\n");
                event_loop.exit();
                return;
            };
            window.set_visible(true);
            window.focus_window();
            let hwnd = window_hwnd(&window);
            let method = match force_foreground(hwnd) {
                Ok(()) => "SetForegroundWindow",
                Err(error) => {
                    let _ = std::fs::write(
                        &self.status,
                        format!("method=failed ok=false error={error}\n"),
                    );
                    event_loop.exit();
                    return;
                }
            };
            let foreground = get_foreground() == hwnd;
            if let Some(parent) = self.status.parent() {
                let _ = std::fs::create_dir_all(parent);
            }
            let _ = std::fs::write(&self.status, format!("method={method} ok={foreground}\n"));
            window.request_redraw();
            self.window = Some(window);
            event_loop.set_control_flow(ControlFlow::WaitUntil(self.until));
        }

        fn window_event(
            &mut self,
            event_loop: &ActiveEventLoop,
            _window: WindowId,
            event: WindowEvent,
        ) {
            if matches!(event, WindowEvent::CloseRequested) {
                event_loop.exit();
            }
        }

        fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
            if Instant::now() >= self.until {
                event_loop.exit();
            } else {
                event_loop.set_control_flow(ControlFlow::WaitUntil(self.until));
            }
        }
    }

    fn run_helper(status: &Path) {
        if let Some(parent) = status.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let Ok(event_loop) = EventLoop::new() else {
            let _ = std::fs::write(status, "method=event-loop-failed ok=false\n");
            return;
        };
        let mut helper = HelperWindow {
            window: None,
            until: Instant::now() + Duration::from_millis(1500),
            status: status.to_path_buf(),
        };
        let _ = event_loop.run_app(&mut helper);
    }

    pub fn main() {
        let args: Vec<String> = std::env::args().collect();
        if let Some(index) = args.iter().position(|argument| argument == "--helper") {
            let status = args
                .get(index + 1)
                .map_or_else(helper_status_path, PathBuf::from);
            run_helper(&status);
            return;
        }
        if let Some(index) = args
            .iter()
            .position(|argument| argument == "--measure-child")
        {
            let file = args
                .get(index + 1)
                .map_or_else(measurement_path, PathBuf::from);
            let mut host = GuiHost::new(Demo::default());
            host.app_mut().measure = true;
            run_app(Driver::measure_child(host, file));
            if FAILED.load(Ordering::SeqCst) {
                std::process::exit(1);
            }
            return;
        }

        let measure_file = measurement_path();
        let _ = std::fs::remove_file(&measure_file);
        let measurement = match spawn_measurement_child(&measure_file) {
            Ok(()) => read_measurement(&measure_file),
            Err(error) => {
                eprintln!("p8e: measurement child failed: {error}");
                None
            }
        };
        if measurement.is_none() {
            eprintln!("p8e: continuing without child measurement numbers");
        }

        let driver = Driver::new(GuiHost::new(Demo::default()))
            .with_measurement(measurement.unwrap_or_default());
        run_app(driver);
        if FAILED.load(Ordering::SeqCst) {
            std::process::exit(1);
        }
    }

    fn run_app(driver: Driver) {
        App::new(driver)
            .with_title("GUIMaker Phase 8E popup evidence")
            .with_size(1000, 600)
            .with_mode(AppMode::Flat2D)
            .with_redraw_policy(RedrawPolicy::OnDemand)
            .with_font_bytes(ROBOTO)
            .with_background_color(ferrous_engine::Color::hex("#0A0B0D"))
            .run();
    }

    fn measurement_path() -> PathBuf {
        PathBuf::from(format!("{OUT}/measurement.txt"))
    }

    fn spawn_measurement_child(file: &Path) -> Result<(), String> {
        let exe = std::env::current_exe().map_err(|error| error.to_string())?;
        let status = std::process::Command::new(exe)
            .arg("--measure-child")
            .arg(file)
            .status()
            .map_err(|error| error.to_string())?;
        if status.success() {
            Ok(())
        } else {
            Err(format!("child exit status {status}"))
        }
    }

    fn read_measurement(file: &Path) -> Option<Measurement> {
        let text = std::fs::read_to_string(file).ok()?;
        let mut measurement = Measurement::default();
        for line in text.lines() {
            if let Some(values) = line.strip_prefix("new ") {
                measurement.new_ms = parse_list(values);
            } else if let Some(values) = line.strip_prefix("pooled ") {
                measurement.pooled_ms = parse_list(values);
            } else if let Some(value) = line.strip_prefix("rss ") {
                measurement.rss_delta = value.trim().parse().unwrap_or(0);
            }
        }
        Some(measurement)
    }

    fn parse_list(values: &str) -> Vec<f64> {
        values
            .split(',')
            .filter_map(|value| value.trim().parse().ok())
            .collect()
    }

    use windows_sys::Win32::Foundation::{GetLastError, BOOL, HWND, LPARAM, POINT, RECT};
    use windows_sys::Win32::System::Threading::AttachThreadInput;
    use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
        SendInput, SetActiveWindow, SetFocus, INPUT, INPUT_0, INPUT_KEYBOARD, INPUT_MOUSE,
        KEYBDINPUT, KEYEVENTF_KEYUP, MOUSEEVENTF_ABSOLUTE, MOUSEEVENTF_LEFTDOWN,
        MOUSEEVENTF_LEFTUP, MOUSEEVENTF_MOVE, MOUSEEVENTF_VIRTUALDESK, MOUSEINPUT, VK_ESCAPE,
    };
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        EnumWindows, GetCursorPos, GetForegroundWindow, GetSystemMetrics, GetWindow, GetWindowRect,
        GetWindowThreadProcessId, IsWindowVisible, SetForegroundWindow, SetWindowPos, ShowWindow,
        WindowFromPoint, GW_ENABLEDPOPUP, HWND_NOTOPMOST, HWND_TOPMOST, SM_CXVIRTUALSCREEN,
        SM_CYVIRTUALSCREEN, SM_XVIRTUALSCREEN, SM_YVIRTUALSCREEN, SWP_NOACTIVATE, SWP_NOMOVE,
        SWP_NOOWNERZORDER, SWP_NOSIZE, SW_SHOW,
    };
}
