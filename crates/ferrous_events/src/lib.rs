//! Raw input events, platform input parameters and the injected clock.
//!
//! This crate stops at the boundary of the operating system. It describes what
//! the platform delivered (a pointer position, a button transition, a wheel
//! delta, a key, committed text, an IME composition, a clipboard payload, a
//! file drop, the modifier state and window focus) together with the instant
//! it happened, the double-click time and distance the platform uses, and a
//! [`Clock`] the host injects so every consumer is deterministic in tests.
//!
//! It deliberately contains **no routing, no hit testing, no hover or focus
//! state**. GUIMaker owns the gesture state machine, the spatial index, the
//! focus order, the shortcut registry and the text-editing engine
//! (`guimaker::input`); an engine-side retained UI system that wants to try
//! again must implement its own policy on top of these raw events.
//!
//! # The clock
//!
//! Nothing here reads `Instant::now()` behind the caller's back: a host that
//! wants wall-clock time uses [`SystemClock`]; a test uses [`ManualClock`] and
//! advances it by hand. Every stamped event carries `time_ms`, so gesture
//! recognition can be replayed byte for byte.

#![forbid(unsafe_code)]
#![warn(missing_docs)]

use std::path::PathBuf;
use std::time::{Duration, Instant};

use glam::Vec2;

/// A monotonic timestamp in milliseconds.
pub type TimeMs = f64;

/// The modifier keys held at the instant of an event.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct Modifiers {
    /// Shift is held.
    pub shift: bool,
    /// Control is held.
    pub control: bool,
    /// Alt (Option on macOS) is held.
    pub alt: bool,
    /// The platform key (Windows, Command) is held.
    pub super_key: bool,
}

impl Modifiers {
    /// No modifier is held.
    #[must_use]
    pub const fn none() -> Self {
        Self {
            shift: false,
            control: false,
            alt: false,
            super_key: false,
        }
    }

    /// Whether any modifier is held.
    #[must_use]
    pub const fn any(self) -> bool {
        self.shift || self.control || self.alt || self.super_key
    }
}

/// A pointer button.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum PointerButton {
    /// The primary (left) button.
    Primary,
    /// The secondary (right) button.
    Secondary,
    /// The middle (wheel click) button.
    Middle,
    /// An extra back button.
    Back,
    /// An extra forward button.
    Forward,
}

/// Whether a button or key went down or up.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ButtonState {
    /// The button or key was pressed.
    Pressed,
    /// The button or key was released.
    Released,
}

/// How a wheel delta is expressed.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ScrollKind {
    /// Discrete wheel lines (a notch, typically 1.0).
    Line,
    /// Pixel deltas from a precision touchpad or a high-resolution wheel.
    Pixel,
}

/// A platform key, independent of the keyboard layout.
pub use winit::keyboard::KeyCode;

/// An IME (input method editor) transition.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Ime {
    /// The composition text changed; it is shown underlined and not committed.
    Composition(String),
    /// The composition was committed as final text.
    Commit(String),
    /// The composition was cancelled and its text discarded.
    Cancel,
}

/// A clipboard operation the platform performed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Clipboard {
    /// The host copied the selection.
    Copy,
    /// The host cut the selection.
    Cut,
    /// The host pasted this text.
    Paste(String),
}

/// A file-drag transition from the operating system.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FileDrop {
    /// A drag entered the window.
    Entered(PathBuf),
    /// A drag is hovering the window.
    Hovered(PathBuf),
    /// A drag was dropped on the window.
    Dropped(PathBuf),
    /// A drag left the window.
    Left,
}

/// One raw input event, tagged with the instant the platform delivered it.
///
/// The enum is `Clone` but not `Copy`: text payloads own their bytes. A host
/// that needs to keep an event around clones it; a hot path forwards it.
#[derive(Clone, Debug, PartialEq)]
pub enum RawInput {
    /// The pointer moved. High-frequency moves may be coalesced by the host.
    PointerMoved {
        /// Position in logical pixels.
        position: Vec2,
        /// The instant the platform delivered the move.
        time_ms: TimeMs,
    },
    /// A pointer button changed state.
    PointerButton {
        /// Position in logical pixels.
        position: Vec2,
        /// The button.
        button: PointerButton,
        /// Whether it went down or up.
        state: ButtonState,
        /// Modifiers held at the transition.
        modifiers: Modifiers,
        /// The instant of the transition.
        time_ms: TimeMs,
    },
    /// The wheel or touchpad scrolled.
    PointerWheel {
        /// Position in logical pixels.
        position: Vec2,
        /// The delta; `positive y` scrolls up, as winit reports it.
        delta: Vec2,
        /// Lines or pixels.
        kind: ScrollKind,
        /// Modifiers held (`Shift` asks for horizontal scrolling).
        modifiers: Modifiers,
        /// The instant of the scroll.
        time_ms: TimeMs,
    },
    /// The pointer left the window.
    PointerLeft,
    /// The pointer entered the window.
    PointerEntered {
        /// Position in logical pixels.
        position: Vec2,
        /// The instant of the enter.
        time_ms: TimeMs,
    },
    /// The modifier state changed.
    ModifiersChanged(Modifiers),
    /// A key changed state.
    Key {
        /// The platform key code, layout independent.
        code: KeyCode,
        /// Whether it went down or up.
        state: ButtonState,
        /// Whether this is an auto-repeat, not the first press.
        repeat: bool,
        /// Modifiers held at the transition.
        modifiers: Modifiers,
        /// The text the platform produced for this key, if any.
        text: Option<String>,
        /// The instant of the transition.
        time_ms: TimeMs,
    },
    /// Committed text (typed, pasted or an IME commit the host forwards here).
    Text(String),
    /// An IME transition.
    Ime(Ime),
    /// The platform performed a clipboard operation.
    Clipboard(Clipboard),
    /// A file drag transition.
    FileDrop(FileDrop),
    /// The window gained or lost focus.
    FocusChanged(bool),
}

impl RawInput {
    /// The instant carried by the event, when it carries one.
    #[must_use]
    pub fn time_ms(&self) -> Option<TimeMs> {
        match self {
            Self::PointerMoved { time_ms, .. }
            | Self::PointerButton { time_ms, .. }
            | Self::PointerWheel { time_ms, .. }
            | Self::PointerEntered { time_ms, .. }
            | Self::Key { time_ms, .. } => Some(*time_ms),
            Self::PointerLeft
            | Self::ModifiersChanged(_)
            | Self::Text(_)
            | Self::Ime(_)
            | Self::Clipboard(_)
            | Self::FileDrop(_)
            | Self::FocusChanged(_) => None,
        }
    }

    /// The modifiers carried by the event, when it carries them.
    #[must_use]
    pub fn modifiers(&self) -> Option<Modifiers> {
        match self {
            Self::PointerButton { modifiers, .. }
            | Self::PointerWheel { modifiers, .. }
            | Self::Key { modifiers, .. } => Some(*modifiers),
            Self::ModifiersChanged(modifiers) => Some(*modifiers),
            _ => None,
        }
    }

    /// The pointer position carried by the event, when it carries one.
    #[must_use]
    pub fn position(&self) -> Option<Vec2> {
        match self {
            Self::PointerMoved { position, .. }
            | Self::PointerButton { position, .. }
            | Self::PointerWheel { position, .. }
            | Self::PointerEntered { position, .. } => Some(*position),
            _ => None,
        }
    }
}

/// The platform parameters a gesture recognizer needs.
///
/// The defaults are the Windows and GNOME values (500 ms / 4 logical pixels /
/// 3 logical pixels); a host may read them from its toolkit and replace them.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct InputConfig {
    /// Maximum delay between two clicks of a double or triple click.
    pub double_click_time_ms: TimeMs,
    /// Maximum pointer travel between two clicks of a multi-click.
    pub double_click_distance: f32,
    /// Pointer travel after a press before it becomes a drag.
    pub drag_threshold: f32,
}

impl Default for InputConfig {
    fn default() -> Self {
        Self {
            double_click_time_ms: 500.0,
            double_click_distance: 4.0,
            drag_threshold: 3.0,
        }
    }
}

impl InputConfig {
    /// The defaults for the current platform.
    ///
    /// The desktop values are shared; keeping this constructor named makes the
    /// injection point obvious when a host does read the real system values.
    #[must_use]
    pub fn platform_default() -> Self {
        Self::default()
    }
}

/// A source of monotonic time, injected by the host.
pub trait Clock {
    /// The current instant, in milliseconds.
    fn now_ms(&self) -> TimeMs;
}

/// A clock backed by the monotonic system timer.
#[derive(Clone, Copy, Debug)]
pub struct SystemClock {
    start: Instant,
}

impl SystemClock {
    /// Starts a clock at "now".
    #[must_use]
    pub fn new() -> Self {
        Self {
            start: Instant::now(),
        }
    }
}

impl Default for SystemClock {
    fn default() -> Self {
        Self::new()
    }
}

impl Clock for SystemClock {
    fn now_ms(&self) -> TimeMs {
        self.start.elapsed().as_secs_f64() * 1000.0
    }
}

/// A clock a test moves by hand.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ManualClock {
    now_ms: TimeMs,
}

impl ManualClock {
    /// A clock at `now_ms`.
    #[must_use]
    pub fn at(now_ms: TimeMs) -> Self {
        Self { now_ms }
    }

    /// Moves the clock forward by `delta_ms` and returns the new instant.
    pub fn advance(&mut self, delta_ms: TimeMs) -> TimeMs {
        self.now_ms += delta_ms.max(0.0);
        self.now_ms
    }

    /// Sets the clock to an absolute instant.
    pub fn set(&mut self, now_ms: TimeMs) {
        self.now_ms = now_ms;
    }
}

impl Clock for ManualClock {
    fn now_ms(&self) -> TimeMs {
        self.now_ms
    }
}

/// The platform duration, as a [`Duration`], of a config value.
#[must_use]
pub fn millis(value: TimeMs) -> Duration {
    Duration::from_secs_f64(value.max(0.0) / 1000.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_config_is_the_desktop_triplet() {
        let config = InputConfig::default();
        assert_eq!(config.double_click_time_ms, 500.0);
        assert_eq!(config.double_click_distance, 4.0);
        assert_eq!(config.drag_threshold, 3.0);
        assert_eq!(config, InputConfig::platform_default());
    }

    #[test]
    fn manual_clock_advances_only_forward() {
        let mut clock = ManualClock::at(10.0);
        assert_eq!(clock.now_ms(), 10.0);
        assert_eq!(clock.advance(5.0), 15.0);
        assert_eq!(clock.advance(-5.0), 15.0);
        clock.set(2.0);
        assert_eq!(clock.now_ms(), 2.0);
    }

    #[test]
    fn system_clock_is_monotonic() {
        let clock = SystemClock::new();
        let first = clock.now_ms();
        std::thread::sleep(Duration::from_millis(1));
        assert!(clock.now_ms() >= first);
    }

    #[test]
    fn events_expose_their_instant_and_modifiers() {
        let event = RawInput::PointerButton {
            position: Vec2::new(4.0, 5.0),
            button: PointerButton::Primary,
            state: ButtonState::Pressed,
            modifiers: Modifiers {
                shift: true,
                ..Modifiers::none()
            },
            time_ms: 12.5,
        };
        assert_eq!(event.time_ms(), Some(12.5));
        assert_eq!(event.position(), Some(Vec2::new(4.0, 5.0)));
        assert_eq!(event.modifiers().map(|m| m.shift), Some(true));
        assert_eq!(RawInput::PointerLeft.time_ms(), None);
        assert!(!Modifiers::none().any());
    }

    #[test]
    fn millis_never_goes_negative() {
        assert_eq!(millis(-1.0), Duration::ZERO);
        assert_eq!(millis(2.0), Duration::from_millis(2));
    }
}
