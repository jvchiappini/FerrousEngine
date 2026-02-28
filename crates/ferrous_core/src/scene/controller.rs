use std::collections::HashMap;

use crate::input::KeyCode;
use glam::Vec3;

/// Maps key codes to unit direction vectors.  A controller is attached to a
/// camera (or other actor) so the caller can customise which keys produce
/// which movement directions.  This avoids hard‑coding WASD logic everywhere.
#[derive(Debug, Clone)]
pub struct Controller {
    mappings: HashMap<KeyCode, Vec3>,
}

impl Controller {
    /// create an empty controller with no mappings
    pub fn new() -> Self {
        Self { mappings: HashMap::new() }
    }

    /// convenience helper that sets up a conventional WASD layout
    pub fn with_default_wasd() -> Self {
        let mut ctl = Self::new();
        ctl.mappings.insert(KeyCode::KeyW, Vec3::new(0.0, 0.0, 1.0));
        ctl.mappings.insert(KeyCode::KeyS, Vec3::new(0.0, 0.0, -1.0));
        ctl.mappings.insert(KeyCode::KeyA, Vec3::new(-1.0, 0.0, 0.0));
        ctl.mappings.insert(KeyCode::KeyD, Vec3::new(1.0, 0.0, 0.0));
        ctl
    }

    /// set a mapping for a specific key
    pub fn set_mapping(&mut self, key: KeyCode, dir: Vec3) {
        self.mappings.insert(key, dir);
    }

    /// compute the desired movement direction given the current input state.
    /// The returned vector is the sum of all mapped directions whose keys are
    /// pressed; it is **not** normalised so callers can easily scale by speed.
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
