use std::collections::HashMap;

use crate::input::KeyCode;
use glam::Vec3;

/// Maps keys to movement directions and stores all camera-motion parameters.
///
/// This is the **user-facing** configuration struct. Place it on a `Camera`
/// (inside `ferrous_core`) and the renderer will pick up every setting
/// automatically — no renderer code needs to be touched.
///
/// # Example
/// ```rust,ignore
/// use ferrous_core::scene::Controller;
/// use ferrous_core::input::KeyCode;
/// use glam::Vec3;
///
/// let mut controller = Controller::new();
/// // Custom key layout
/// controller.bind(KeyCode::ArrowUp,    Vec3::new(0.0, 0.0,  1.0));
/// controller.bind(KeyCode::ArrowDown,  Vec3::new(0.0, 0.0, -1.0));
/// controller.bind(KeyCode::ArrowLeft,  Vec3::new(-1.0, 0.0, 0.0));
/// controller.bind(KeyCode::ArrowRight, Vec3::new( 1.0, 0.0, 0.0));
/// // Slow, precise movement
/// controller.speed = 2.0;
/// controller.mouse_sensitivity = 0.002;
/// ```
#[derive(Debug, Clone)]
pub struct Controller {
    /// Key → camera-space unit direction mappings.
    mappings: HashMap<KeyCode, Vec3>,

    /// Translation speed in world-units per second.
    ///
    /// Default: `5.0`
    pub speed: f32,

    /// Mouse drag sensitivity for orbital / look rotation (radians per pixel).
    ///
    /// Default: `0.005`
    pub mouse_sensitivity: f32,

    /// Initial distance from `camera.target` in orbital mode.
    ///
    /// Default: `5.0`
    pub orbit_distance: f32,
}

impl Controller {
    /// Creates an empty controller with no key bindings and default parameters.
    pub fn new() -> Self {
        Self {
            mappings:          HashMap::new(),
            speed:             5.0,
            mouse_sensitivity: 0.005,
            orbit_distance:    5.0,
        }
    }

    /// Convenience constructor — WASD layout with default parameters.
    pub fn with_default_wasd() -> Self {
        let mut ctl = Self::new();
        ctl.mappings.insert(KeyCode::KeyW, Vec3::new( 0.0, 0.0,  1.0));
        ctl.mappings.insert(KeyCode::KeyS, Vec3::new( 0.0, 0.0, -1.0));
        ctl.mappings.insert(KeyCode::KeyA, Vec3::new(-1.0, 0.0,  0.0));
        ctl.mappings.insert(KeyCode::KeyD, Vec3::new( 1.0, 0.0,  0.0));
        ctl
    }

    /// Binds `key` to a camera-space unit direction vector.
    ///
    /// Use positive Z for "forward", positive X for "right".
    /// Calling this method with the same key twice overwrites the old binding.
    pub fn bind(&mut self, key: KeyCode, dir: Vec3) {
        self.mappings.insert(key, dir);
    }

    /// Alias for [`bind`] — kept for backward compatibility.
    #[inline]
    pub fn set_mapping(&mut self, key: KeyCode, dir: Vec3) {
        self.bind(key, dir);
    }

    /// Removes the binding for `key`, if any.
    pub fn unbind(&mut self, key: KeyCode) {
        self.mappings.remove(&key);
    }

    /// Removes all key bindings.
    pub fn clear_bindings(&mut self) {
        self.mappings.clear();
    }

    /// Returns the combined movement direction for the keys currently held.
    ///
    /// The result is the **sum** of bound directions whose keys are pressed.
    /// It is intentionally **not** normalised — multiply by [`speed`] and `dt`
    /// to get the displacement for a frame.
    pub fn direction(&self, input: &crate::input::InputState) -> Vec3 {
        let mut out = Vec3::ZERO;
        for (key, dir) in &self.mappings {
            if input.is_key_pressed(*key) {
                out += *dir;
            }
        }
        out
    }
}
