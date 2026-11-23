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

use ferrous_engine::winit::event::{
    ElementState, Ime, MouseButton as WinitMouseButton, MouseScrollDelta, WindowEvent,
};
use ferrous_engine::winit::keyboard::{Key, ModifiersState, NamedKey};
use ferrous_engine::{AppContext, DrawContext, FerrousApp};
use glam::Vec2;
use guimaker::geom::Size;
use guimaker::input::shortcut::ShortcutKey;
use guimaker::input::{InputEvent, Modifiers, PointerButton, ScrollKind};
use guimaker::theme::Theme;
use guimaker::view::Element;
use guimaker::{Frame, Ui};
use guimaker_ferrous::FerrousTextSystem;

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

    /// The injected clock instant in milliseconds.
    ///
    /// Returning `None` makes the adapter use the engine's frame clock
    /// (`AppContext::time.elapsed`, in seconds). Return an explicit value when
    /// an animation or test needs a deterministic clock.
    fn time_ms(&self) -> Option<f64> {
        None
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

    fn apply_grid(&mut self, viewport: Size, scale: f32) {
        if viewport != self.viewport || (scale - self.scale).abs() > f32::EPSILON {
            self.viewport = viewport;
            self.scale = scale;
            self.ui.set_viewport(viewport, scale);
        }
    }

    fn apply_theme(&mut self) {
        let theme = self.app.theme();
        if theme != self.theme {
            self.theme = theme.clone();
            self.ui.set_theme(theme);
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
    #[allow(clippy::cast_possible_truncation)] // reason: a window scale factor is finite and small
    pub fn translate_event(&mut self, event: &WindowEvent) -> Vec<InputEvent> {
        let mut events = Vec::new();
        match event {
            WindowEvent::CursorMoved { position, .. } => {
                let point = self.logical(position.x, position.y);
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
        self.apply_theme();
    }

    fn update(&mut self, ctx: &mut AppContext<'_>) {
        let scale = window_scale(ctx);
        let viewport = logical_size(ctx, scale);
        self.apply_grid(viewport, scale);
        self.apply_theme();

        let now_ms = self.app.time_ms().unwrap_or(ctx.time.elapsed * 1000.0);
        let _ = self.ui.set_time_ms(now_ms);

        let view = self.app.view();
        self.ui.paint(&view, &mut self.frame, &mut self.text);
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

    #[allow(clippy::cast_precision_loss)] // reason: window dimensions sit far below 2^24
    fn on_resize(&mut self, new_size: (u32, u32), ctx: &mut AppContext<'_>) {
        let scale = window_scale(ctx);
        let viewport = Size::new(new_size.0 as f32 / scale, new_size.1 as f32 / scale);
        self.apply_grid(viewport, scale);
    }

    fn on_window_event(&mut self, event: &WindowEvent, ctx: &mut AppContext<'_>) {
        if let WindowEvent::KeyboardInput { event: key, .. } = event {
            if is_overlay_toggle(key.state, &key.logical_key) {
                self.overlay.enabled = !self.overlay.enabled;
                ctx.request_redraw();
            }
        }
        let events = self.translate_event(event);
        for event in events {
            self.ui.push_event(event);
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
