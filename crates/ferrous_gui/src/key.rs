//! Thin wrapper around a key‑code enum used by the GUI widgets.
//!
//! We avoid exposing `winit::keyboard::KeyCode` directly so that the crate can
//! be compiled without bringing in `winit` at all.  The enum starts out very
//! small – just the keys the current widget set actually care about – and can
//! be expanded later as needed.

/// Represents a non‑text key pressed while a widget has focus.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum GuiKey {
    /// Backspace — delete character before the cursor.
    Backspace,
    /// Delete — delete character after the cursor.
    Delete,
    /// Arrow left — move cursor one position left.
    ArrowLeft,
    /// Arrow right — move cursor one position right.
    ArrowRight,
    /// Arrow up.
    ArrowUp,
    /// Arrow down.
    ArrowDown,
    /// Home — move cursor to beginning of line.
    Home,
    /// End — move cursor to end of line.
    End,
    /// Enter / Return key.
    Enter,
    /// Escape key.
    Escape,
    /// Tab key.
    Tab,
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
            winit::keyboard::KeyCode::Delete => GuiKey::Delete,
            winit::keyboard::KeyCode::ArrowLeft => GuiKey::ArrowLeft,
            winit::keyboard::KeyCode::ArrowRight => GuiKey::ArrowRight,
            winit::keyboard::KeyCode::ArrowUp => GuiKey::ArrowUp,
            winit::keyboard::KeyCode::ArrowDown => GuiKey::ArrowDown,
            winit::keyboard::KeyCode::Home => GuiKey::Home,
            winit::keyboard::KeyCode::End => GuiKey::End,
            winit::keyboard::KeyCode::Enter | winit::keyboard::KeyCode::NumpadEnter => GuiKey::Enter,
            winit::keyboard::KeyCode::Escape => GuiKey::Escape,
            winit::keyboard::KeyCode::Tab => GuiKey::Tab,
            _ => GuiKey::Backspace, // catch‑all; callers should not rely on this
        }
    }
}