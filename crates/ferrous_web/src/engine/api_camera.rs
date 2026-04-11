use wasm_bindgen::prelude::*;
use crate::engine::FerrousWebEngine;
use crate::commands::JsCommand;
use crate::config::CameraControlMode;

#[wasm_bindgen]
impl FerrousWebEngine {
    #[wasm_bindgen(js_name = setCamera)]
    pub fn set_camera(&self, ex: f32, ey: f32, ez: f32, tx: f32, ty: f32, tz: f32) {
        *self.camera_override.lock().unwrap() = Some(([ex, ey, ez], [tx, ty, tz]));
        self.push_command(JsCommand::SetCamera {
            eye: [ex, ey, ez],
            target: [tx, ty, tz],
        });
    }

    #[wasm_bindgen(js_name = setCameraControlMode)]
    pub fn set_camera_control_mode(&self, mode: String) -> Result<(), JsValue> {
        let parsed_mode = CameraControlMode::parse(&mode).ok_or_else(|| {
            self.report_error(
                "camera.invalid_mode",
                "Camera mode must be one of: fly, orbit, none",
            );
            JsValue::from_str("Invalid camera mode")
        })?;

        {
            let mut cfg = self.pending_config.lock().unwrap();
            cfg.control_mode = parsed_mode;
        }

        self.push_command(JsCommand::SetCameraControlMode {
            mode: parsed_mode.as_str().to_string(),
        });
        Ok(())
    }

    #[wasm_bindgen(js_name = setCameraParams)]
    pub fn set_camera_params(&self, speed: f32, sensitivity: f32) {
        let mut cfg = self.pending_config.lock().unwrap();
        cfg.move_speed = speed;
        cfg.look_sensitivity = sensitivity;
        
        self.push_command(JsCommand::SetCameraParams { speed, sensitivity });
    }

    #[wasm_bindgen(js_name = setCameraFov)]
    pub fn set_camera_fov(&self, fov_degrees: f32) {
        self.push_command(JsCommand::SetCameraFov { fov_degrees });
    }

    #[wasm_bindgen(js_name = configureCamera)]
    pub fn configure_camera(&self, ex: f32, ey: f32, ez: f32, tx: f32, ty: f32, tz: f32) {
        let mut cfg = self.pending_config.lock().unwrap();
        cfg.eye = [ex, ey, ez];
        cfg.target = [tx, ty, tz];
    }

    #[wasm_bindgen(js_name = configureControls)]
    pub fn configure_controls(&self, move_speed: f32, look_sensitivity: f32) -> Result<(), JsValue> {
        if move_speed <= 0.0 {
            self.report_error("config.invalid_move_speed", "move_speed must be > 0");
            return Err(JsValue::from_str("Invalid move_speed"));
        }
        let mut cfg = self.pending_config.lock().unwrap();
        cfg.move_speed = move_speed;
        cfg.look_sensitivity = look_sensitivity.max(0.0001);
        Ok(())
    }
}
