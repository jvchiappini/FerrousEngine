use ferrous_app::{AppContext, Quat, Vec3};
use ferrous_core::input::{KeyCode, MouseButton};

use crate::config::{CameraControlMode, EngineConfig};

pub struct CameraController {
    eye: Vec3,
    target: Vec3,
    yaw: f32,
    pitch: f32,
    orbit_distance: f32,
    move_speed: f32,
    look_sensitivity: f32,
    mode: CameraControlMode,
}

impl CameraController {
    pub fn new(config: EngineConfig) -> Self {
        let mut controller = Self {
            eye: Vec3::from_array(config.eye),
            target: Vec3::from_array(config.target),
            yaw: 0.0,
            pitch: 0.0,
            orbit_distance: 1.0,
            move_speed: config.move_speed,
            look_sensitivity: config.look_sensitivity,
            mode: config.control_mode,
        };
        controller.sync_orientation_from_target();
        controller
    }

    pub fn eye(&self) -> Vec3 {
        self.eye
    }

    pub fn target(&self) -> Vec3 {
        self.target
    }

    pub fn mode(&self) -> CameraControlMode {
        self.mode
    }

    pub fn set_mode(&mut self, mode: CameraControlMode) {
        self.mode = mode;
    }

    pub fn set_move_speed(&mut self, move_speed: f32) {
        self.move_speed = move_speed;
    }

    pub fn set_look_sensitivity(&mut self, look_sensitivity: f32) {
        self.look_sensitivity = look_sensitivity.max(0.0001);
    }

    pub fn set_camera(&mut self, eye: [f32; 3], target: [f32; 3]) {
        self.eye = Vec3::from_array(eye);
        self.target = Vec3::from_array(target);
        self.sync_orientation_from_target();
    }

    pub fn update_from_input(&mut self, ctx: &AppContext) {
        match self.mode {
            CameraControlMode::None => {}
            CameraControlMode::Fly => self.update_fly(ctx),
            CameraControlMode::Orbit => self.update_orbit(ctx),
        }
    }

    fn sync_orientation_from_target(&mut self) {
        let mut forward = self.target - self.eye;
        if forward.length_squared() < 0.0001 {
            forward = Vec3::new(0.0, 0.0, -1.0);
        } else {
            forward = forward.normalize();
        }

        self.yaw = forward.x.atan2(-forward.z);
        self.pitch = forward.y.asin().clamp(-1.45, 1.45);
        self.orbit_distance = (self.target - self.eye).length().max(0.1);
    }

    fn update_fly(&mut self, ctx: &AppContext) {
        let dt = ctx.time.delta;

        if ctx.input.is_button_down(MouseButton::Right) {
            let delta = ctx.input.mouse_delta();
            self.yaw -= delta.0 * self.look_sensitivity;
            self.pitch = (self.pitch - delta.1 * self.look_sensitivity).clamp(-1.45, 1.45);
        }

        let mut move_local = Vec3::ZERO;
        if ctx.input.is_key_down(KeyCode::KeyW) {
            move_local.z -= 1.0;
        }
        if ctx.input.is_key_down(KeyCode::KeyS) {
            move_local.z += 1.0;
        }
        if ctx.input.is_key_down(KeyCode::KeyA) {
            move_local.x -= 1.0;
        }
        if ctx.input.is_key_down(KeyCode::KeyD) {
            move_local.x += 1.0;
        }
        if ctx.input.is_key_down(KeyCode::Space) {
            move_local.y += 1.0;
        }
        if ctx.input.is_key_down(KeyCode::ShiftLeft) {
            move_local.y -= 1.0;
        }

        let rotation = Quat::from_rotation_y(self.yaw) * Quat::from_rotation_x(self.pitch);
        if move_local.length_squared() > 0.0 {
            let world_move = rotation * move_local.normalize();
            self.eye += world_move * self.move_speed * dt;
        }

        let forward = rotation * Vec3::new(0.0, 0.0, -1.0);
        self.target = self.eye + forward;
    }

    fn update_orbit(&mut self, ctx: &AppContext) {
        if ctx.input.is_button_down(MouseButton::Right) {
            let delta = ctx.input.mouse_delta();
            self.yaw -= delta.0 * self.look_sensitivity;
            self.pitch = (self.pitch - delta.1 * self.look_sensitivity).clamp(-1.45, 1.45);
        }

        if ctx.input.is_key_down(KeyCode::KeyW) {
            self.orbit_distance = (self.orbit_distance - 0.2).max(0.5);
        }
        if ctx.input.is_key_down(KeyCode::KeyS) {
            self.orbit_distance = (self.orbit_distance + 0.2).min(500.0);
        }

        let rotation = Quat::from_rotation_y(self.yaw) * Quat::from_rotation_x(self.pitch);
        let forward = rotation * Vec3::new(0.0, 0.0, -1.0);
        self.eye = self.target - forward * self.orbit_distance;
    }
}
