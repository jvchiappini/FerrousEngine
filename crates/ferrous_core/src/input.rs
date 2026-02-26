use std::collections::HashSet;

/// Re-exported key and mouse enums from `winit` for convenience.
// winit 0.30 renamed its keyboard module; `KeyCode` now lives under
// `winit::keyboard`.  We keep the simple alias so callers don't need to
// know about the upstream change.
pub use winit::event::MouseButton;
pub use winit::keyboard::KeyCode;

/// State of the keyboard and mouse at a given moment.
///
/// The `Engine` (or, in our current case, the editor) is responsible for
/// driving this structure by feeding it the events coming from `winit`.
/// Once populated, the rest of the engine can query the state using the
/// convenience helpers defined below.
#[derive(Default)]
pub struct InputState {
    keys_down: HashSet<KeyCode>,
    mouse_buttons: HashSet<MouseButton>,
    mouse_pos: (f64, f64),
    /// movement since last time `consume_mouse_delta` was called
    mouse_delta: (f32, f32),
}

impl InputState {
    /// Creates a fresh, empty input state.
    pub fn new() -> Self {
        Default::default()
    }

    /// Called by the event loop when a keyboard event arrives.
    pub fn update_key(&mut self, key: KeyCode, pressed: bool) {
        if pressed {
            self.keys_down.insert(key);
        } else {
            self.keys_down.remove(&key);
        }
    }

    /// Returns true if the given key is currently pressed down.
    pub fn is_key_pressed(&self, key: KeyCode) -> bool {
        self.keys_down.contains(&key)
    }

    /// Called by the event loop when a mouse button event arrives.
    pub fn update_mouse_button(&mut self, button: MouseButton, pressed: bool) {
        if pressed {
            self.mouse_buttons.insert(button);
        } else {
            self.mouse_buttons.remove(&button);
        }
    }

    /// Returns true if the given mouse button is currently held.
    pub fn is_button_down(&self, button: MouseButton) -> bool {
        self.mouse_buttons.contains(&button)
    }

    /// Update the current mouse cursor position (window coordinates).
    pub fn set_mouse_position(&mut self, x: f64, y: f64) {
        let (px, py) = self.mouse_pos;
        self.mouse_pos = (x, y);
        self.mouse_delta = ((x - px) as f32, (y - py) as f32);
    }

    /// Retrieve the last recorded mouse position.
    pub fn mouse_position(&self) -> (f64, f64) {
        self.mouse_pos
    }

    /// Retrieve and reset the mouse movement delta (in pixels) since the
    /// last call. This is useful for applying camera rotations.
    pub fn consume_mouse_delta(&mut self) -> (f32, f32) {
        let d = self.mouse_delta;
        self.mouse_delta = (0.0, 0.0);
        d
    }
}

// simple unit tests for the input state implementation
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn key_tracking() {
        let mut state = InputState::new();
        assert!(!state.is_key_pressed(KeyCode::A));
        state.update_key(KeyCode::A, true);
        assert!(state.is_key_pressed(KeyCode::A));
        state.update_key(KeyCode::A, false);
        assert!(!state.is_key_pressed(KeyCode::A));
    }

    #[test]
    fn mouse_tracking() {
        let mut state = InputState::new();
        assert!(!state.is_button_down(MouseButton::Left));
        state.update_mouse_button(MouseButton::Left, true);
        assert!(state.is_button_down(MouseButton::Left));
        state.update_mouse_button(MouseButton::Left, false);
        assert!(!state.is_button_down(MouseButton::Left));
        state.set_mouse_position(10.0, 20.0);
        assert_eq!(state.mouse_position(), (10.0, 20.0));
        // delta should reflect movement
        state.set_mouse_position(15.0, 25.0);
        assert_eq!(state.consume_mouse_delta(), (5.0, 5.0));
        // consumption resets
        assert_eq!(state.consume_mouse_delta(), (0.0, 0.0));
    }
}
