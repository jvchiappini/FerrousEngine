//! The `FerrousApp` host adapter: about one screen of glue.
//!
//! [`GuiHost`] wraps any [`GuiApp`] (a model plus a `view`/`update` pair) and
//! implements `ferrous_engine::FerrousApp`. The mapping is deliberately thin:
//!
//! - `setup`/`update` set the viewport and scale from the window, apply the
//!   theme, advance the injected clock and paint the view into a
//!   `guimaker::Frame`. Messages produced by routing are fed straight back into
//!   [`GuiApp::update`].
//! - `draw_ui` maps the frame's display list onto the engine's `GuiBatch` with
//!   `guimaker_ferrous::encode`; the engine has already built and bound the
//!   font atlas, so the adapter never owns a GPU resource.
//! - `on_window_event` translates winit input into `guimaker::InputEvent`.
//!
//! # Input mapping
//!
//! | winit event | `guimaker::InputEvent` |
//! |---|---|
//! | `CursorMoved` | `PointerMoved` (physical -> logical) |
//! | `CursorLeft` | `PointerLeft` |
//! | `MouseInput` | `PointerDown` / `PointerUp` at the last pointer position |
//! | `MouseWheel` line delta | `Wheel { kind: Line }` |
//! | `MouseWheel` pixel delta | `Wheel { kind: Pixel }`, scaled to logical |
//! | `KeyboardInput` named/character | `KeyDown` / `KeyUp` |
//! | `KeyboardInput` text | `Text` (control characters dropped) |
//! | `Ime::Commit` | `Text` |
//! | `ModifiersChanged` | `Modifiers` |
//! | `Focused(false)` | `FocusLost` |
//! | `ScaleFactorChanged` | updates the adapter's logical-to-physical scale |
//!
//! Physical coordinates are divided by the window scale factor, so the view
//! and the routing tree always work in logical pixels; the adapter multiplies
//! back by the scale in `draw_ui`.
//!
//! # Example
//!
//! ```no_run
//! use ferrous_engine::{App, AppMode};
//! use guimaker::recipe::{start_screen, StartMsg};
//! use guimaker::theme::Theme;
//! use guimaker::view::Element;
//! use guimaker_gallery::host::{GuiApp, GuiHost};
//!
//! struct Model;
//!
//! impl GuiApp for Model {
//!     type Msg = StartMsg;
//!     fn view(&self) -> Element<Self::Msg> { start_screen(0.0) }
//!     fn update(&mut self, _msg: Self::Msg) {}
//!     fn theme(&self) -> Theme { Theme::dark() }
//! }
//!
//! App::new(GuiHost::new(Model))
//!     .with_size(1280, 800)
//!     .with_mode(AppMode::Flat2D)
//!     .with_font_bytes(include_bytes!("../../../assets/fonts/Roboto-Regular.ttf"))
//!     .run();
//! ```

use ferrous_engine::windows::{PopupId, PopupSpec};
use ferrous_engine::winit::event::{
    ElementState, Ime, MouseButton as WinitMouseButton, MouseScrollDelta, WindowEvent,
};
use ferrous_engine::winit::keyboard::{Key, ModifiersState, NamedKey};
use ferrous_engine::{AppContext, DrawContext, FerrousApp};
use glam::Vec2;
use guimaker::geom::Size;
use guimaker::input::shortcut::ShortcutKey;
use guimaker::input::{InputEvent, Modifiers, PointerButton, ScrollKind};
use guimaker::overlay::HostCapabilities;
use guimaker::theme::Theme;
use guimaker::view::Element;
use guimaker::{Frame, Ui};
use guimaker_ferrous::{FerrousTextSystem, GuiMakerRenderer};

use crate::overlay::DebugOverlay;

/// An application model `GUIMaker` can drive.
///
/// Implement this on the struct that owns the app's state. `view` must be a
/// pure function of `self`; messages from the routed input come back through
/// [`GuiApp::update`] on the following call.
pub trait GuiApp {
    /// The action type the view emits.
    type Msg: Clone;

    /// Builds the view for the current model.
    fn view(&self) -> Element<Self::Msg>;

    /// Applies one message from the view.
    fn update(&mut self, message: Self::Msg);

    /// The theme to paint with. Defaults to [`Theme::dark`].
    fn theme(&self) -> Theme {
        Theme::dark()
    }

    /// A popup painted a frame.
    ///
    /// `frames` is that popup's presented-frame count after this one;
    /// `first_frame_ms` is `Some` on the first frame, carrying the time from
    /// the host opening the window to that frame. An evidence harness records
    /// these; the default ignores them.
    fn on_popup_frame(&mut self, _frames: u64, _first_frame_ms: Option<f64>) {}

    /// Receives one popup's own presented pixels after
    /// [`ferrous_engine::windows::PopupQueue::capture_all`].
    ///
    /// The frame is read back from the popup's swapchain, never from the
    /// desktop. Default ignores it.
    fn on_popup_screenshot(&mut self, _capture: ferrous_engine::traits::PopupCapture) {}

    /// Receives the main window's requested screenshot.
    ///
    /// Default ignores it.
    fn on_screenshot(&mut self, _screenshot: ferrous_engine::Screenshot) {}

    /// Called once per host frame with the engine context.
    ///
    /// Use it for work a model drives itself (timers, a benchmark timeline)
    /// without going through a message. It runs after the view is painted and
    /// before the popups are synchronised, so a change made here applies to
    /// the next frame. The default does nothing.
    fn tick(&mut self, _ctx: &mut ferrous_engine::AppContext<'_>) {}

    /// Receives committed text for the app's focused editable control.
    ///
    /// The host calls it for every `InputEvent::Text` before the frame's
    /// routing (and for popup text too). The app forwards it to its own
    /// `TextFieldModel` while its field is focused; the default ignores it.
    fn on_text(&mut self, _text: &str) {}

    /// The injected clock instant in milliseconds.
    ///
    /// Returning `None` makes the adapter use the engine's frame clock
    /// (`AppContext::time.elapsed`, in seconds). Return an explicit value when
    /// an animation or test needs a deterministic clock.
    fn time_ms(&self) -> Option<f64> {
        None
    }

    /// What the host can do with overlays that do not fit the window.
    ///
    /// The default is [`HostCapabilities::desktop`]: the host really draws a
    /// popup-hosted overlay in its own window, so an overflowing menu is not
    /// clipped. A web build keeps [`HostCapabilities::single_window`], where
    /// overlays clamp to the canvas instead of disappearing.
    fn capabilities(&self) -> HostCapabilities {
        #[cfg(target_arch = "wasm32")]
        {
            HostCapabilities::single_window()
        }
        #[cfg(not(target_arch = "wasm32"))]
        {
            HostCapabilities::desktop()
        }
    }
}

/// The `FerrousApp` adapter around one [`GuiApp`].
pub struct GuiHost<A: GuiApp> {
    app: A,
    ui: Ui<A::Msg>,
    frame: Frame<A::Msg>,
    text: FerrousTextSystem,
    theme: Theme,
    viewport: Size,
    scale: f32,
    pointer: Vec2,
    overlay: DebugOverlay,
    /// The GUI renderer that encodes a popup-hosted overlay.
    popup_renderer: GuiMakerRenderer,
    /// The popups this host has open, one per popup-hosted overlay.
    popups: Vec<HostPopup>,
    /// How many popups have been opened since the run started.
    popup_opens: u64,
    /// When the last user gesture that can open an overlay arrived, so the
    /// time-to-first-frame is measured from the input, not from the open.
    last_input_at: Option<std::time::Instant>,
}

/// One popup the host opened for a popup-hosted overlay.
struct HostPopup {
    popup: PopupId,
    /// The overlay's stable path hash (its identity across tree rebuilds).
    path: u64,
    /// Logical position of the popup's top-left in main-window coordinates.
    origin: Vec2,
    /// Physical size the popup was last given.
    size: (u32, u32),
    /// Physical position the popup was last shown at.
    position: (i32, i32),
    /// The DPI factor of the popup's own window (it can differ from the main
    /// window's when the popup sits on another monitor).
    scale: f32,
    /// Fingerprint of the overlay's painted subtree and geometry.
    fingerprint: u64,
    /// Whether the popup is currently shown.
    shown: bool,
    /// When the host asked for it (the time-to-first-frame clock).
    opened_at: std::time::Instant,
    /// Whether its first frame has been painted.
    first_frame: bool,
    /// Milliseconds from the open to that first frame.
    first_frame_ms: Option<f64>,
    /// Presented frames (the idle-popup evidence reads this).
    frames: u64,
}

/// One popup as this frame's painter planned it.
struct PopupPlan {
    /// The overlay's stable path hash.
    path: u64,
    /// Logical top-left in main-window coordinates.
    origin: Vec2,
    /// Physical size.
    size: (u32, u32),
    /// Physical screen position.
    position: (i32, i32),
    /// The overlay subtree's content+geometry fingerprint.
    fingerprint: u64,
    /// Whether an animation inside the overlay is running.
    animating: bool,
}

/// Popup evidence collected by a [`GuiHost`].
#[derive(Clone, Debug, Default, PartialEq)]
pub struct PopupStats {
    /// Popups the host opened since the app started.
    pub opens: u64,
    /// Popup frames the host painted.
    pub frames: u64,
    /// One entry per popup: milliseconds from its open to its first frame.
    pub first_frame_ms: Vec<f64>,
    /// Popups currently on screen.
    pub visible: usize,
}

impl<A: GuiApp> GuiHost<A> {
    /// Builds an adapter around `app`.
    #[must_use]
    pub fn new(app: A) -> Self {
        let theme = app.theme();
        let ui = Ui::new(theme.clone());
        Self {
            app,
            ui,
            frame: Frame::new(),
            text: FerrousTextSystem::new(),
            theme,
            viewport: Size::ZERO,
            scale: 1.0,
            pointer: Vec2::ZERO,
            overlay: DebugOverlay::new(),
            popup_renderer: GuiMakerRenderer::new(),
            popups: Vec::new(),
            popup_opens: 0,
            last_input_at: None,
        }
    }

    /// The wrapped application.
    #[must_use]
    pub fn app(&self) -> &A {
        &self.app
    }

    /// The wrapped application, mutably.
    pub fn app_mut(&mut self) -> &mut A {
        &mut self.app
    }

    /// The persistent `GUIMaker` UI (layout, store, input state).
    #[must_use]
    pub fn ui(&self) -> &Ui<A::Msg> {
        &self.ui
    }

    /// The current logical viewport.
    #[must_use]
    pub fn viewport(&self) -> Size {
        self.viewport
    }

    /// The current logical-to-physical scale factor.
    #[must_use]
    pub fn scale(&self) -> f32 {
        self.scale
    }

    /// The last painted frame.
    #[must_use]
    pub fn frame(&self) -> &Frame<A::Msg> {
        &self.frame
    }

    /// The debug overlay state (F12 toggles it in a window).
    #[must_use]
    pub fn overlay(&self) -> &DebugOverlay {
        &self.overlay
    }

    /// The debug overlay state, mutably (a test or a menu may toggle it).
    pub fn overlay_mut(&mut self) -> &mut DebugOverlay {
        &mut self.overlay
    }

    /// The popup counters an evidence harness reads.
    #[must_use]
    pub fn popup_stats(&self) -> PopupStats {
        PopupStats {
            opens: self.popup_opens,
            frames: self.popups.iter().map(|popup| popup.frames).sum(),
            first_frame_ms: self
                .popups
                .iter()
                .filter_map(|popup| popup.first_frame_ms)
                .collect(),
            visible: self
                .frame
                .popups
                .iter()
                .filter(|batch| self.popups.iter().any(|popup| popup.path == batch.path))
                .count(),
        }
    }

    /// Opens, moves, resizes, shows or hides the popup for every overlay the
    /// painter sent to a popup window, and asks for a full repaint of them.
    ///
    /// Called from `update`, after `paint`: the frame's popup list is the plan.
    /// An overlay that closed is no longer in the list, so its popup is hidden
    /// and returned to the pool.
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)] // reason: rect money is clamped above
    fn sync_popups(&mut self, ctx: &mut AppContext<'_>) {
        let main_scale = self.scale;
        let window_origin = ctx
            .window
            .inner_position()
            .map_or((0, 0), |position| (position.x, position.y));
        let input_at = self.last_input_at.unwrap_or_else(std::time::Instant::now);
        let mut plan: Vec<PopupPlan> = Vec::new();
        for batch in &self.frame.popups {
            // A popup redraws only when its own content, size or position
            // changed, or one of its own animations is running. A hover in the
            // main window leaves this fingerprint untouched.
            let fingerprint = self.ui.overlay_fingerprint(batch.node);
            let animating = self.ui.overlay_animating(batch.node);
            // A popup uses its own monitor's DPI, learned from its window; the
            // first frame uses the main window's factor for one frame.
            let scale = self
                .popups
                .iter()
                .find(|popup| popup.path == batch.path)
                .map_or(main_scale, |popup| popup.scale);
            let width = (batch.rect.width * scale).round().max(1.0);
            let height = (batch.rect.height * scale).round().max(1.0);
            plan.push(PopupPlan {
                path: batch.path,
                origin: Vec2::new(batch.rect.x, batch.rect.y),
                size: (width as u32, height as u32),
                position: (
                    window_origin.0 + (batch.rect.x * scale).round() as i32,
                    window_origin.1 + (batch.rect.y * scale).round() as i32,
                ),
                fingerprint,
                animating,
            });
        }

        // A popup whose overlay is no longer popup-hosted (or is gone) hides.
        for host in &mut self.popups {
            if !plan.iter().any(|entry| entry.path == host.path) && host.shown {
                ctx.popups.hide(host.popup);
                host.shown = false;
            }
        }

        for PopupPlan {
            path,
            origin,
            size,
            position,
            fingerprint,
            animating,
        } in plan
        {
            if let Some(host) = self.popups.iter_mut().find(|popup| popup.path == path) {
                let mut dirty = animating || host.fingerprint != fingerprint;
                host.origin = origin;
                host.fingerprint = fingerprint;
                if host.size != size {
                    ctx.popups.resize(host.popup, size);
                    host.size = size;
                    dirty = true;
                }
                // Only touch the window when it moved or was hidden, so an
                // idle, unchanged popup produces no commands at all.
                if !host.shown {
                    ctx.popups.show(host.popup, position);
                    host.shown = true;
                    host.position = position;
                    // A re-shown popup is the pooled case: its own
                    // time-to-first-frame starts now.
                    host.opened_at = input_at;
                    host.first_frame = false;
                    dirty = true;
                } else if host.position != position {
                    ctx.popups.show(host.popup, position);
                    host.position = position;
                    dirty = true;
                }
                if dirty {
                    ctx.popups.redraw(host.popup);
                }
            } else {
                let popup = ctx.popups.open(PopupSpec {
                    position,
                    size,
                    transparent: true,
                    resizable: false,
                });
                ctx.popups.show(popup, position);
                self.popup_opens += 1;
                self.popups.push(HostPopup {
                    popup,
                    path,
                    origin,
                    size,
                    position,
                    scale: main_scale,
                    fingerprint,
                    shown: true,
                    opened_at: input_at,
                    first_frame: false,
                    first_frame_ms: None,
                    frames: 0,
                });
            }
        }
    }

    fn apply_grid(&mut self, viewport: Size, scale: f32) {
        if viewport != self.viewport || (scale - self.scale).abs() > f32::EPSILON {
            self.viewport = viewport;
            self.scale = scale;
            self.ui.set_viewport(viewport, scale);
        }
    }

    /// Passes the monitor work area to the layout, in window-local logical
    /// pixels, so a popup-hosted overlay never leaves the screen.
    fn apply_work_area(&mut self, ctx: &AppContext<'_>) {
        let area = ctx.work_area();
        self.ui.set_work_area(Some(guimaker::geom::Rect::new(
            area.x,
            area.y,
            area.width,
            area.height,
        )));
    }

    fn apply_theme(&mut self) {
        let theme = self.app.theme();
        if theme != self.theme {
            self.theme = theme.clone();
            self.ui.set_theme(theme.clone());
            self.text.set_fallback_chain(theme.font_fallbacks.clone());
        }
    }

    #[allow(clippy::cast_possible_truncation)] // reason: pointer positions fit f32 at any real window size
    fn logical(&self, x: f64, y: f64) -> Vec2 {
        Vec2::new(
            (x / f64::from(self.scale)) as f32,
            (y / f64::from(self.scale)) as f32,
        )
    }

    fn push_frame_messages(&mut self) {
        let messages = std::mem::take(&mut self.frame.messages);
        for message in messages {
            self.app.update(message);
        }
    }

    /// Translates one window event; exposed so a host can unit-test the map.
    ///
    /// The function mutates the last pointer position for cursor events but
    /// does not touch the UI; the returned events are pushed by the caller.
    pub fn translate_event(&mut self, event: &WindowEvent) -> Vec<InputEvent> {
        self.translate_event_at(event, Vec2::ZERO)
    }

    /// [`Self::translate_event`] with the window's origin in main-window
    /// coordinates added to every pointer position.
    ///
    /// A popup reports popup-local pixels; `offset` is the overlay's logical
    /// position in the main window, so the routed pointer lands exactly on the
    /// overlay `GUIMaker` laid out (hover, clicks and outside-click dismissal all
    /// work the same in both windows).
    #[allow(clippy::cast_possible_truncation)] // reason: a window scale factor is finite and small
    pub fn translate_event_at(&mut self, event: &WindowEvent, offset: Vec2) -> Vec<InputEvent> {
        let mut events = Vec::new();
        match event {
            WindowEvent::CursorMoved { position, .. } => {
                let point = self.logical(position.x, position.y) + offset;
                self.pointer = point;
                self.overlay.cursor = point;
                events.push(InputEvent::PointerMoved { position: point });
            }
            WindowEvent::CursorLeft { .. } => {
                events.push(InputEvent::PointerLeft);
            }
            WindowEvent::MouseInput { state, button, .. } => {
                let Some(button) = map_button(*button) else {
                    return events;
                };
                let position = self.pointer;
                let event = match state {
                    ElementState::Pressed => InputEvent::PointerDown { position, button },
                    ElementState::Released => InputEvent::PointerUp { position, button },
                };
                events.push(event);
            }
            WindowEvent::MouseWheel { delta, .. } => {
                let (delta, kind) = match delta {
                    MouseScrollDelta::LineDelta(x, y) => (Vec2::new(*x, *y), ScrollKind::Line),
                    MouseScrollDelta::PixelDelta(position) => {
                        (self.logical(position.x, position.y), ScrollKind::Pixel)
                    }
                };
                events.push(InputEvent::Wheel {
                    position: self.pointer,
                    delta,
                    kind,
                });
            }
            WindowEvent::KeyboardInput { event, .. } => {
                if let Some(key) = map_key(&event.logical_key) {
                    match event.state {
                        ElementState::Pressed => {
                            events.push(InputEvent::KeyDown {
                                key,
                                repeat: event.repeat,
                            });
                            if !event.repeat {
                                if let Some(text) = &event.text {
                                    if text.chars().any(|ch| !ch.is_control()) {
                                        events.push(InputEvent::Text(text.to_string()));
                                    }
                                }
                            }
                        }
                        ElementState::Released => events.push(InputEvent::KeyUp { key }),
                    }
                }
            }
            WindowEvent::ModifiersChanged(modifiers) => {
                events.push(InputEvent::Modifiers(map_modifiers(modifiers.state())));
            }
            WindowEvent::Focused(false) => events.push(InputEvent::FocusLost),
            WindowEvent::Ime(Ime::Commit(text)) => {
                if !text.is_empty() {
                    events.push(InputEvent::Text(text.clone()));
                }
            }
            WindowEvent::ScaleFactorChanged { scale_factor, .. } => {
                let scale = *scale_factor as f32;
                if scale.is_finite() && scale > 0.0 {
                    self.scale = scale;
                }
            }
            _ => {}
        }
        events
    }
}

impl<A: GuiApp> FerrousApp for GuiHost<A> {
    fn setup(&mut self, ctx: &mut AppContext<'_>) {
        let scale = window_scale(ctx);
        let viewport = logical_size(ctx, scale);
        self.apply_grid(viewport, scale);
        self.apply_work_area(ctx);
        self.ui.set_host_capabilities(self.app.capabilities());
        self.apply_theme();
    }

    fn update(&mut self, ctx: &mut AppContext<'_>) {
        let scale = window_scale(ctx);
        let viewport = logical_size(ctx, scale);
        self.apply_grid(viewport, scale);
        self.apply_work_area(ctx);
        self.apply_theme();

        let now_ms = self.app.time_ms().unwrap_or(ctx.time.elapsed * 1000.0);
        let _ = self.ui.set_time_ms(now_ms);

        let view = self.app.view();
        self.ui.paint(&view, &mut self.frame, &mut self.text);
        for command in &self.frame.display_list.commands {
            if let guimaker::display_list::DrawCommand::Text { run, .. } = command {
                ctx.request_glyphs(&run.text);
            }
        }
        self.app.tick(ctx);
        self.sync_popups(ctx);
        self.push_frame_messages();

        if self.ui.animating() {
            ctx.request_redraw();
        } else if let Some(next_ms) = self.ui.next_wake_ms(now_ms) {
            let delay = std::time::Duration::from_secs_f64((next_ms - now_ms).max(1.0) / 1000.0);
            ctx.redraw_at(std::time::Instant::now() + delay);
        }
    }

    fn draw_ui(&mut self, dc: &mut DrawContext<'_, '_>) {
        // The overlay is a debug pass: lint runs only while it is enabled and
        // the extra commands live for this submission only. The next `update`
        // repaints the normal list from scratch.
        let mut base = 0;
        if self.overlay.enabled {
            let findings = guimaker::lint::lint(&self.ui);
            let extra = self.overlay.commands(&self.ui, &findings);
            base = self.frame.display_list.commands.len();
            self.frame.display_list.commands.extend(extra);
        }
        guimaker_ferrous::encode(
            &self.frame.display_list,
            self.viewport,
            self.scale,
            dc.gui,
            dc.font,
        );
        if self.overlay.enabled {
            self.frame.display_list.commands.truncate(base);
        }
    }

    #[allow(clippy::cast_possible_truncation)] // reason: a window scale factor is finite and small
    fn draw_popup(&mut self, popup: PopupId, dc: &mut DrawContext<'_, '_>) {
        // The popup's own DPI, not the main window's: a menu on a second
        // monitor must size its text for that monitor.
        let popup_scale = dc.ctx.scale_factor as f32;
        let scale = if popup_scale.is_finite() && popup_scale > 0.0 {
            popup_scale
        } else {
            self.scale
        };
        let Some(host) = self.popups.iter_mut().find(|host| host.popup == popup) else {
            return;
        };
        host.scale = scale;
        let Some(batch) = self
            .frame
            .popups
            .iter()
            .find(|batch| batch.path == host.path)
        else {
            return;
        };
        // The overlay's commands are in main-window coordinates; the popup's
        // own origin is (0, 0), so offset them by the overlay's origin.
        let mut commands = batch.commands.clone();
        for command in &mut commands {
            command.translate(-host.origin.x, -host.origin.y);
        }
        let viewport = Size::new(batch.rect.width, batch.rect.height);
        self.popup_renderer
            .encode_commands(&commands, viewport, scale, dc.gui, dc.font);

        host.frames += 1;
        let first_frame_ms = if host.first_frame {
            None
        } else {
            host.first_frame = true;
            let ms = host.opened_at.elapsed().as_secs_f64() * 1000.0;
            host.first_frame_ms = Some(ms);
            Some(ms)
        };
        let frames = host.frames;
        self.app.on_popup_frame(frames, first_frame_ms);
    }

    fn on_popup_event(&mut self, popup: PopupId, event: &WindowEvent, ctx: &mut AppContext<'_>) {
        let gesture = match event {
            WindowEvent::MouseInput { state, .. } => *state == ElementState::Pressed,
            WindowEvent::KeyboardInput { event, .. } => event.state == ElementState::Pressed,
            _ => false,
        };
        if gesture {
            self.last_input_at = Some(std::time::Instant::now());
        }
        let Some(origin) = self
            .popups
            .iter()
            .find(|host| host.popup == popup)
            .map(|host| host.origin)
        else {
            return;
        };
        if let WindowEvent::KeyboardInput { event: key, .. } = event {
            if is_overlay_toggle(key.state, &key.logical_key) {
                self.overlay.enabled = !self.overlay.enabled;
            }
        }
        let events = self.translate_event_at(event, origin);
        if events.is_empty() {
            return;
        }
        for event in events {
            if let InputEvent::Text(text) = &event {
                self.app.on_text(text);
            }
            self.ui.push_event(event);
        }
        // Popup input does not go through the main window's event loop path, so
        // the host asks for the frame itself (the anchor and the popup both
        // change on hover, click or Escape).
        ctx.request_redraw();
    }

    fn on_popup_screenshot(&mut self, capture: ferrous_engine::traits::PopupCapture) {
        self.app.on_popup_screenshot(capture);
    }

    fn on_screenshot(&mut self, screenshot: ferrous_engine::Screenshot) {
        self.app.on_screenshot(screenshot);
    }

    fn on_exit(&mut self) {
        let stats = self.popup_stats();
        eprintln!(
            "popup-report {{\"opens\":{},\"frames\":{},\"visible\":{},\"first_frame_ms\":{:?}}}",
            stats.opens, stats.frames, stats.visible, stats.first_frame_ms
        );
    }

    #[allow(clippy::cast_precision_loss)] // reason: window dimensions sit far below 2^24
    fn on_resize(&mut self, new_size: (u32, u32), ctx: &mut AppContext<'_>) {
        let scale = window_scale(ctx);
        let viewport = Size::new(new_size.0 as f32 / scale, new_size.1 as f32 / scale);
        self.apply_grid(viewport, scale);
    }

    fn on_window_event(&mut self, event: &WindowEvent, ctx: &mut AppContext<'_>) {
        // Remember when a gesture that can open an overlay arrived: the
        // evidence clock starts there, not at the window creation.
        let gesture = match event {
            WindowEvent::MouseInput { state, .. } => *state == ElementState::Pressed,
            WindowEvent::KeyboardInput { event, .. } => event.state == ElementState::Pressed,
            _ => false,
        };
        if gesture {
            self.last_input_at = Some(std::time::Instant::now());
        }
        if let WindowEvent::KeyboardInput { event: key, .. } = event {
            if is_overlay_toggle(key.state, &key.logical_key) {
                self.overlay.enabled = !self.overlay.enabled;
                ctx.request_redraw();
            }
        }
        let events = self.translate_event(event);
        if events.is_empty() {
            return;
        }
        // A pointer move that keeps the same target, cursor, tooltip, drag and
        // scroll state changes nothing: do not ask for a frame. Every other
        // event still asks (a click, a key, a wheel, a focus change).
        let pointer_only = events
            .iter()
            .all(|event| matches!(event, InputEvent::PointerMoved { .. }));
        let needs_frame = !pointer_only || self.ui.pointer_move_changes(self.pointer);
        for event in events {
            if let InputEvent::Text(text) = &event {
                self.app.on_text(text);
            }
            self.ui.push_event(event);
        }
        if needs_frame {
            ctx.request_redraw();
        }
    }
}

#[allow(clippy::cast_possible_truncation)] // reason: a window scale factor is finite and small
fn window_scale(ctx: &AppContext<'_>) -> f32 {
    let scale = ctx.window.scale_factor() as f32;
    if scale.is_finite() && scale > 0.0 {
        scale
    } else {
        1.0
    }
}

#[allow(clippy::cast_precision_loss)] // reason: window dimensions sit far below 2^24
fn logical_size(ctx: &AppContext<'_>, scale: f32) -> Size {
    Size::new(
        ctx.window_size.0 as f32 / scale,
        ctx.window_size.1 as f32 / scale,
    )
}

fn map_button(button: WinitMouseButton) -> Option<PointerButton> {
    match button {
        WinitMouseButton::Left => Some(PointerButton::Primary),
        WinitMouseButton::Right => Some(PointerButton::Secondary),
        WinitMouseButton::Middle => Some(PointerButton::Middle),
        WinitMouseButton::Back | WinitMouseButton::Forward | WinitMouseButton::Other(_) => None,
    }
}

fn map_modifiers(state: ModifiersState) -> Modifiers {
    Modifiers {
        shift: state.shift_key(),
        control: state.control_key(),
        alt: state.alt_key(),
        super_key: state.super_key(),
    }
}

/// Whether a key event toggles the debug overlay (F12 on press).
fn is_overlay_toggle(state: ElementState, key: &Key) -> bool {
    state == ElementState::Pressed && matches!(key, Key::Named(NamedKey::F12))
}

fn map_key(key: &Key) -> Option<ShortcutKey> {
    match key {
        Key::Named(named) => Some(match named {
            NamedKey::Tab => ShortcutKey::Tab,
            NamedKey::Enter => ShortcutKey::Enter,
            NamedKey::Space => ShortcutKey::Space,
            NamedKey::Escape => ShortcutKey::Escape,
            NamedKey::Backspace => ShortcutKey::Backspace,
            NamedKey::Delete => ShortcutKey::Delete,
            NamedKey::Insert => ShortcutKey::Insert,
            NamedKey::Home => ShortcutKey::Home,
            NamedKey::End => ShortcutKey::End,
            NamedKey::PageUp => ShortcutKey::PageUp,
            NamedKey::PageDown => ShortcutKey::PageDown,
            NamedKey::ArrowLeft => ShortcutKey::ArrowLeft,
            NamedKey::ArrowRight => ShortcutKey::ArrowRight,
            NamedKey::ArrowUp => ShortcutKey::ArrowUp,
            NamedKey::ArrowDown => ShortcutKey::ArrowDown,
            NamedKey::F1 => ShortcutKey::Function(1),
            NamedKey::F2 => ShortcutKey::Function(2),
            NamedKey::F3 => ShortcutKey::Function(3),
            NamedKey::F4 => ShortcutKey::Function(4),
            NamedKey::F5 => ShortcutKey::Function(5),
            NamedKey::F6 => ShortcutKey::Function(6),
            NamedKey::F7 => ShortcutKey::Function(7),
            NamedKey::F8 => ShortcutKey::Function(8),
            NamedKey::F9 => ShortcutKey::Function(9),
            NamedKey::F10 => ShortcutKey::Function(10),
            NamedKey::F11 => ShortcutKey::Function(11),
            NamedKey::F12 => ShortcutKey::Function(12),
            _ => return None,
        }),
        Key::Character(text) => ShortcutKey::from_char(text.chars().next()?),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::{is_overlay_toggle, map_key, map_modifiers, GuiApp, GuiHost};
    use ferrous_engine::winit::event::ElementState;
    use ferrous_engine::winit::keyboard::{Key, ModifiersState, NamedKey};
    use guimaker::theme::Theme;
    use guimaker::view::{text, Element};

    #[derive(Default)]
    struct Counter {
        clicks: usize,
        theme: Theme,
    }

    impl GuiApp for Counter {
        type Msg = ();
        fn view(&self) -> Element<Self::Msg> {
            text::<()>(format!("{}", self.clicks))
        }
        fn update(&mut self, (): Self::Msg) {
            self.clicks += 1;
        }
        fn theme(&self) -> Theme {
            self.theme.clone()
        }
    }

    #[test]
    fn a_host_starts_at_a_sane_grid() {
        let host = GuiHost::new(Counter::default());
        assert!((host.scale() - 1.0).abs() < f32::EPSILON);
        assert_eq!(host.viewport(), guimaker::geom::Size::ZERO);
    }

    #[test]
    fn named_and_character_keys_map() {
        assert_eq!(
            map_key(&Key::Named(NamedKey::Escape)),
            Some(guimaker::input::shortcut::ShortcutKey::Escape)
        );
        assert_eq!(
            map_key(&Key::Character("s".into())),
            Some(guimaker::input::shortcut::ShortcutKey::Letter(b'S'))
        );
        assert_eq!(map_key(&Key::Character("\u{1F600}".into())), None);
    }

    #[test]
    fn modifiers_map_all_four_flags() {
        let mapped = map_modifiers(ModifiersState::SHIFT | ModifiersState::CONTROL);
        assert!(mapped.shift);
        assert!(mapped.control);
        assert!(!mapped.alt);
        assert!(!mapped.super_key);
    }

    #[test]
    fn f12_pressed_toggles_the_overlay_and_nothing_else_does() {
        let f12 = Key::Named(NamedKey::F12);
        assert!(is_overlay_toggle(ElementState::Pressed, &f12));
        assert!(!is_overlay_toggle(ElementState::Released, &f12));
        assert!(!is_overlay_toggle(
            ElementState::Pressed,
            &Key::Named(NamedKey::F11)
        ));
        assert!(!is_overlay_toggle(
            ElementState::Pressed,
            &Key::Character("f".into())
        ));
    }
}
