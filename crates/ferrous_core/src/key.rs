//! Simple, winit‑compatible keycode types owned by `ferrous_core`.
//!
//! This module defines `KeyCode` and `MouseButton` enums that mirror the
//! variants used by `winit` but live in our own crate.  By wrapping the
//! external types we avoid exposing `winit` in our public API, which keeps
//! downstream crates free to disable the `input` feature and compile without
//! pulling in the windowing stack.  Conversion helpers are provided when the
//! `input` feature is active.

/// Keyboard keys that can be reported to `InputState`.
///
/// The variants are deliberately conservative; only keys the engine has used
/// so far are listed.  Unknown or unhandled `winit` codes map to
/// [`KeyCode::Unknown`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum KeyCode {
    /// Fallback value for unrecognised codes.
    Unknown,

    // common movement / editing keys
    Backspace,
    Enter,
    Escape,
    Space,
    ArrowUp,
    ArrowDown,
    ArrowLeft,
    ArrowRight,

    // letter keys (used heavily in examples)
    KeyA,
    KeyB,
    KeyC,
    KeyD,
    KeyE,
    KeyF,
    KeyG,
    KeyH,
    KeyI,
    KeyJ,
    KeyK,
    KeyL,
    KeyM,
    KeyN,
    KeyO,
    KeyP,
    KeyQ,
    KeyR,
    KeyS,
    KeyT,
    KeyU,
    KeyV,
    KeyW,
    KeyX,
    KeyY,
    KeyZ,

    // number keys (top row)
    Digit0,
    Digit1,
    Digit2,
    Digit3,
    Digit4,
    Digit5,
    Digit6,
    Digit7,
    Digit8,
    Digit9,
}

/// Mouse buttons.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MouseButton {
    Left,
    Right,
    Middle,
    /// Any other platform‑specific button.  The contained value is the
    /// raw button identifier from winit.
    Other(u16),
}

// Conversions from winit exist only when the input feature is enabled.  They
// are intentionally non‑exhaustive; unmapped variants fall back to
// [`KeyCode::Unknown`].
#[cfg(feature = "input")]
impl From<winit::keyboard::KeyCode> for KeyCode {
    fn from(k: winit::keyboard::KeyCode) -> Self {
        match k {
            winit::keyboard::KeyCode::Backspace => KeyCode::Backspace,
            winit::keyboard::KeyCode::Enter => KeyCode::Enter,
            winit::keyboard::KeyCode::Escape => KeyCode::Escape,
            winit::keyboard::KeyCode::Space => KeyCode::Space,
            winit::keyboard::KeyCode::ArrowUp => KeyCode::ArrowUp,
            winit::keyboard::KeyCode::ArrowDown => KeyCode::ArrowDown,
            winit::keyboard::KeyCode::ArrowLeft => KeyCode::ArrowLeft,
            winit::keyboard::KeyCode::ArrowRight => KeyCode::ArrowRight,

            winit::keyboard::KeyCode::KeyA => KeyCode::KeyA,
            winit::keyboard::KeyCode::KeyB => KeyCode::KeyB,
            winit::keyboard::KeyCode::KeyC => KeyCode::KeyC,
            winit::keyboard::KeyCode::KeyD => KeyCode::KeyD,
            winit::keyboard::KeyCode::KeyE => KeyCode::KeyE,
            winit::keyboard::KeyCode::KeyF => KeyCode::KeyF,
            winit::keyboard::KeyCode::KeyG => KeyCode::KeyG,
            winit::keyboard::KeyCode::KeyH => KeyCode::KeyH,
            winit::keyboard::KeyCode::KeyI => KeyCode::KeyI,
            winit::keyboard::KeyCode::KeyJ => KeyCode::KeyJ,
            winit::keyboard::KeyCode::KeyK => KeyCode::KeyK,
            winit::keyboard::KeyCode::KeyL => KeyCode::KeyL,
            winit::keyboard::KeyCode::KeyM => KeyCode::KeyM,
            winit::keyboard::KeyCode::KeyN => KeyCode::KeyN,
            winit::keyboard::KeyCode::KeyO => KeyCode::KeyO,
            winit::keyboard::KeyCode::KeyP => KeyCode::KeyP,
            winit::keyboard::KeyCode::KeyQ => KeyCode::KeyQ,
            winit::keyboard::KeyCode::KeyR => KeyCode::KeyR,
            winit::keyboard::KeyCode::KeyS => KeyCode::KeyS,
            winit::keyboard::KeyCode::KeyT => KeyCode::KeyT,
            winit::keyboard::KeyCode::KeyU => KeyCode::KeyU,
            winit::keyboard::KeyCode::KeyV => KeyCode::KeyV,
            winit::keyboard::KeyCode::KeyW => KeyCode::KeyW,
            winit::keyboard::KeyCode::KeyX => KeyCode::KeyX,
            winit::keyboard::KeyCode::KeyY => KeyCode::KeyY,
            winit::keyboard::KeyCode::KeyZ => KeyCode::KeyZ,

            winit::keyboard::KeyCode::Digit0 => KeyCode::Digit0,
            winit::keyboard::KeyCode::Digit1 => KeyCode::Digit1,
            winit::keyboard::KeyCode::Digit2 => KeyCode::Digit2,
            winit::keyboard::KeyCode::Digit3 => KeyCode::Digit3,
            winit::keyboard::KeyCode::Digit4 => KeyCode::Digit4,
            winit::keyboard::KeyCode::Digit5 => KeyCode::Digit5,
            winit::keyboard::KeyCode::Digit6 => KeyCode::Digit6,
            winit::keyboard::KeyCode::Digit7 => KeyCode::Digit7,
            winit::keyboard::KeyCode::Digit8 => KeyCode::Digit8,
            winit::keyboard::KeyCode::Digit9 => KeyCode::Digit9,

            // catch‑all for keys we don't explicitly handle
            _ => KeyCode::Unknown,
        }
    }
}

#[cfg(feature = "input")]
impl From<winit::event::MouseButton> for MouseButton {
    fn from(b: winit::event::MouseButton) -> Self {
        match b {
            winit::event::MouseButton::Left => MouseButton::Left,
            winit::event::MouseButton::Right => MouseButton::Right,
            winit::event::MouseButton::Middle => MouseButton::Middle,
            winit::event::MouseButton::Back | winit::event::MouseButton::Forward => {
                // treat extra buttons as "other" with a dummy id; the
                // numeric value isn't exposed by winit.
                MouseButton::Other(0)
            }
            winit::event::MouseButton::Other(id) => MouseButton::Other(id),
        }
    }
}
