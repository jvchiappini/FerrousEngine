/// Orbital camera state driven by a [`ferrous_core::scene::Controller`].
///
/// `OrbitState` stores the mutable yaw/pitch angles that accumulate over time.
/// All user-tunable parameters (speed, sensitivity, orbit distance) live in
/// `camera.controller` — a [`ferrous_core::scene::Controller`] — so the user
/// never needs to touch renderer internals.
use ferrous_core::scene::Camera;
use ferrous_core::input::{InputState, MouseButton};

/// Persistent orbital camera angles.  Updated every frame by [`OrbitState::update`].
#[derive(Debug, Clone)]
pub struct OrbitState {
    pub yaw:   f32,
    pub pitch: f32,
}

impl Default for OrbitState {
    fn default() -> Self {
        Self { yaw: 0.0, pitch: 0.0 }
    }
}

impl OrbitState {
    /// Applies one frame of input to `camera`.
    ///
    /// All motion parameters are read from `camera.controller`:
    /// - **`speed`**            — translation units per second
    /// - **`mouse_sensitivity`** — radians per pixel for orbit drag
    /// - **`orbit_distance`**   — eye distance from target in orbital mode
    ///
    /// `dt` is elapsed time in seconds.
    pub fn update(&mut self, camera: &mut Camera, input: &mut InputState, dt: f32) {
        // ── WASD / arrow-key translation ──────────────────────────────────
        let move_dir = camera.controller.direction(input);
        if move_dir.length_squared() > 1e-6 {
            let forward  = (camera.target - camera.eye).normalize_or_zero();
            let right    = forward.cross(camera.up).normalize_or(glam::Vec3::X);
            let world    = (forward * move_dir.z + right * move_dir.x).normalize_or_zero();
            let disp     = world * camera.controller.speed * dt;
            camera.eye    += disp;
            camera.target += disp;
        }

        // ── Right-drag orbital rotation ───────────────────────────────────
        let sens = camera.controller.mouse_sensitivity;
        if sens > 0.0 && input.is_button_down(MouseButton::Right) {
            let (dx, dy) = input.consume_mouse_delta();
            self.yaw   -= dx * sens;
            self.pitch -= dy * sens;

            const LIMIT: f32 = std::f32::consts::FRAC_PI_2 - 0.01;
            self.pitch = self.pitch.clamp(-LIMIT, LIMIT);

            let rot    = glam::Mat3::from_euler(glam::EulerRot::YXZ, self.yaw, self.pitch, 0.0);
            let offset = rot * glam::Vec3::new(0.0, 0.0, camera.controller.orbit_distance);
            camera.eye = camera.target + offset;
        }
    }
}

