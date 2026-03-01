//! Keyboard and mouse input state.
//!
//! `InputState` is driven by the application runner (which feeds winit events
//! into it) and consumed by game logic.  It tracks:
//!
//! - Which keys / buttons are **currently held** (`is_key_down`, `is_button_down`)
//! - Which keys / buttons were **pressed this frame** (`just_pressed`)
//! - Which keys / buttons were **released this frame** (`just_released`)
//! - Mouse cursor position and per-frame delta
//! - Mouse scroll delta
//!
//! Call [`InputState::end_frame`] once per frame (after processing all winit
//! events) to promote the current frame's new events into the "just" sets and
//! clear the per-frame deltas.  The runner handles this automatically.
//!
//! # Example
//! ```rust,ignore
//! fn update(&mut self, ctx: &mut AppContext) {
//!     if ctx.input.just_pressed(KeyCode::Space) {
//!         self.jump();
//!     }
//!     if ctx.input.is_key_down(KeyCode::KeyW) {
//!         self.move_forward(ctx.time.delta);
//!     }
//!     let (dx, dy) = ctx.input.mouse_delta();
//!     self.camera_yaw   += dx * 0.003;
//!     self.camera_pitch += dy * 0.003;
//! }
//! ```

use std::collections::HashSet;

pub use winit::event::MouseButton;
pub use winit::keyboard::KeyCode;

/// Full keyboard + mouse state for one frame.
#[derive(Default, Clone)]
pub struct InputState {
    // ── Keyboard ─────────────────────────────────────────────────────────
    /// Keys held down this frame.
    keys_down:     HashSet<KeyCode>,
    /// Keys that transitioned from up → down this frame.
    keys_pressed:  HashSet<KeyCode>,
    /// Keys that transitioned from down → up this frame.
    keys_released: HashSet<KeyCode>,

    // ── Mouse buttons ─────────────────────────────────────────────────────
    buttons_down:     HashSet<MouseButton>,
    buttons_pressed:  HashSet<MouseButton>,
    buttons_released: HashSet<MouseButton>,

    // ── Mouse position & movement ──────────────────────────────────────────
    mouse_pos:         (f64, f64),
    mouse_delta:       (f32, f32),
    /// Accumulated scroll this frame (lines, positive = up / zoom-in).
    scroll_delta:      (f32, f32),
}

impl InputState {
    /// Creates a fresh, empty input state.
    pub fn new() -> Self {
        Default::default()
    }

    // ─── Called by the event loop ──────────────────────────────────────────

    /// Update keyboard state from a winit `KeyboardInput` event.
    pub fn update_key(&mut self, key: KeyCode, pressed: bool) {
        if pressed {
            if !self.keys_down.contains(&key) {
                self.keys_pressed.insert(key);
            }
            self.keys_down.insert(key);
        } else {
            if self.keys_down.contains(&key) {
                self.keys_released.insert(key);
            }
            self.keys_down.remove(&key);
        }
    }

    /// Update mouse button state from a winit event.
    pub fn update_mouse_button(&mut self, button: MouseButton, pressed: bool) {
        if pressed {
            if !self.buttons_down.contains(&button) {
                self.buttons_pressed.insert(button);
            }
            self.buttons_down.insert(button);
        } else {
            if self.buttons_down.contains(&button) {
                self.buttons_released.insert(button);
            }
            self.buttons_down.remove(&button);
        }
    }

    /// Update cursor position and accumulate delta.
    pub fn set_mouse_position(&mut self, x: f64, y: f64) {
        let (px, py) = self.mouse_pos;
        self.mouse_delta.0 += (x - px) as f32;
        self.mouse_delta.1 += (y - py) as f32;
        self.mouse_pos = (x, y);
    }

    /// Accumulate a scroll event (typically from `MouseWheel`).
    ///
    /// `dx` is horizontal scroll, `dy` is vertical (positive = up).
    pub fn add_scroll(&mut self, dx: f32, dy: f32) {
        self.scroll_delta.0 += dx;
        self.scroll_delta.1 += dy;
    }

    /// Must be called **once per frame, after all events have been processed**.
    /// Clears the per-frame "just pressed / released" sets and resets deltas.
    pub fn end_frame(&mut self) {
        self.keys_pressed.clear();
        self.keys_released.clear();
        self.buttons_pressed.clear();
        self.buttons_released.clear();
        self.mouse_delta = (0.0, 0.0);
        self.scroll_delta = (0.0, 0.0);
    }

    // ─── Queries ───────────────────────────────────────────────────────────

    // --- keyboard ---------------------------------------------------------

    /// Returns `true` while the key is held.
    #[inline]
    pub fn is_key_down(&self, key: KeyCode) -> bool {
        self.keys_down.contains(&key)
    }

    /// Alias for [`is_key_down`] — matches winit naming style.
    #[inline]
    pub fn is_key_pressed(&self, key: KeyCode) -> bool {
        self.keys_down.contains(&key)
    }

    /// Returns `true` during the **one frame** the key was first pressed.
    #[inline]
    pub fn just_pressed(&self, key: KeyCode) -> bool {
        self.keys_pressed.contains(&key)
    }

    /// Returns `true` during the **one frame** the key was released.
    #[inline]
    pub fn just_released(&self, key: KeyCode) -> bool {
        self.keys_released.contains(&key)
    }

    /// Returns `true` if any key is currently held down.
    pub fn any_key_down(&self) -> bool {
        !self.keys_down.is_empty()
    }

    // --- mouse buttons ----------------------------------------------------

    /// Returns `true` while the button is held.
    #[inline]
    pub fn is_button_down(&self, button: MouseButton) -> bool {
        self.buttons_down.contains(&button)
    }

    /// Returns `true` during the **one frame** the button was first pressed.
    #[inline]
    pub fn button_just_pressed(&self, button: MouseButton) -> bool {
        self.buttons_pressed.contains(&button)
    }

    /// Returns `true` during the **one frame** the button was released.
    #[inline]
    pub fn button_just_released(&self, button: MouseButton) -> bool {
        self.buttons_released.contains(&button)
    }

    // --- mouse position & movement ----------------------------------------

    /// Current cursor position in window coordinates.
    #[inline]
    pub fn mouse_position(&self) -> (f64, f64) {
        self.mouse_pos
    }

    /// Mouse movement since last `end_frame()` call (non-consuming).
    #[inline]
    pub fn mouse_delta(&self) -> (f32, f32) {
        self.mouse_delta
    }

    /// Retrieve and immediately reset the mouse delta.
    ///
    /// Useful when the same `InputState` is shared between camera and UI —
    /// the first consumer calls this, the second sees zero.
    pub fn consume_mouse_delta(&mut self) -> (f32, f32) {
        let d = self.mouse_delta;
        self.mouse_delta = (0.0, 0.0);
        d
    }

    /// Scroll delta since last `end_frame()` call.  Positive Y = scroll up.
    #[inline]
    pub fn scroll_delta(&self) -> (f32, f32) {
        self.scroll_delta
    }

    /// Vertical scroll this frame (positive = up / zoom in).
    #[inline]
    pub fn scroll_y(&self) -> f32 {
        self.scroll_delta.1
    }
}
