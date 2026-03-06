//! Thin wrapper around a key‑code enum used by the GUI widgets.
//!
//! We avoid exposing `winit::keyboard::KeyCode` directly so that the crate can
//! be compiled without bringing in `winit` at all.  The enum starts out very
//! small – just the keys the current widget set actually care about – and can
//! be expanded later as needed.

/// Represents a non‑text key pressed while a widget has focus.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum GuiKey {
    /// Backspace is used by the single‑line text input widget.
    Backspace,
    // future variants e.g. Enter, Escape, ArrowUp, etc.
}

// When the winit backend is enabled we provide a conversion from the full
// winit keycode type.  This is deliberately limited to the variants we care
// about; any unsupported key will simply map to `Backspace` if added here,
// but callers should not rely on round‑trip conversion.
#[cfg(feature = "winit-backend")]
impl From<winit::keyboard::KeyCode> for GuiKey {
    fn from(k: winit::keyboard::KeyCode) -> Self {
        match k {
            winit::keyboard::KeyCode::Backspace => GuiKey::Backspace,
            // other keys could be mapped here in future
            _ => GuiKey::Backspace, // catch‑all to satisfy exhaustiveness
        }
    }
}