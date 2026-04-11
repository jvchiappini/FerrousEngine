use std::collections::HashSet;
use std::sync::{Arc, Mutex};

use js_sys::Function;
use wasm_bindgen::prelude::*;

use ferrous_app::{App, Color};

use crate::commands::JsCommand;
use crate::config::{CameraControlMode, EngineConfig, EngineMetrics};
use crate::runtime::WebRuntime;

#[wasm_bindgen]
pub struct FerrousWebEngine {
    command_queue: Arc<Mutex<Vec<JsCommand>>>,
    camera_override: Arc<Mutex<Option<([f32; 3], [f32; 3])>>>,
    pending_config: Arc<Mutex<EngineConfig>>,
    error_callback: Arc<Mutex<Option<Function>>>,
    metrics: Arc<Mutex<EngineMetrics>>,
    debug_mode: Arc<Mutex<bool>>,
    enabled_plugins: Arc<Mutex<HashSet<String>>>,
    next_scene_id: Arc<Mutex<u32>>,
}

#[wasm_bindgen]
impl FerrousWebEngine {
    fn report_error(&self, code: &str, message: &str) {
        let payload = format!(
            "{{\"code\":\"{}\",\"message\":\"{}\"}}",
            code,
            message.replace('"', "\\\"")
        );
        if let Some(callback) = self.error_callback.lock().unwrap().as_ref() {
            let _ = callback.call1(&JsValue::NULL, &JsValue::from_str(&payload));
        } else {
            log::error!("[Ferrous:{}] {}", code, message);
        }
    }

    fn push_command(&self, cmd: JsCommand) {
        self.command_queue.lock().unwrap().push(cmd);
    }

    #[wasm_bindgen(constructor)]
    pub fn new() -> Self {
        console_error_panic_hook::set_once();
        let _ = fern::Dispatch::new()
            .level(log::LevelFilter::Debug)
            .chain(std::io::stderr())
            .apply();

        let mut plugins = HashSet::new();
        plugins.insert("terrain".to_string());
        plugins.insert("sky".to_string());

        Self {
            command_queue: Arc::new(Mutex::new(Vec::new())),
            camera_override: Arc::new(Mutex::new(None)),
            pending_config: Arc::new(Mutex::new(EngineConfig::default())),
            error_callback: Arc::new(Mutex::new(None)),
            metrics: Arc::new(Mutex::new(EngineMetrics::default())),
            debug_mode: Arc::new(Mutex::new(false)),
            enabled_plugins: Arc::new(Mutex::new(plugins)),
            next_scene_id: Arc::new(Mutex::new(2)),
        }
    }

    #[wasm_bindgen]
    pub fn set_error_callback(&self, callback: &Function) {
        *self.error_callback.lock().unwrap() = Some(callback.clone());
    }

    #[wasm_bindgen]
    pub fn configure_camera(
        &self,
        ex: f32,
        ey: f32,
        ez: f32,
        tx: f32,
        ty: f32,
        tz: f32,
    ) {
        let mut cfg = self.pending_config.lock().unwrap();
        cfg.eye = [ex, ey, ez];
        cfg.target = [tx, ty, tz];
    }

    #[wasm_bindgen]
    pub fn configure_controls(&self, move_speed: f32) -> Result<(), JsValue> {
        if move_speed <= 0.0 {
            self.report_error("config.invalid_move_speed", "move_speed must be > 0");
            return Err(JsValue::from_str("Invalid move_speed"));
        }
        let mut cfg = self.pending_config.lock().unwrap();
        cfg.move_speed = move_speed;
        Ok(())
    }

    #[wasm_bindgen]
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

    #[wasm_bindgen]
    pub fn configure(
        &self,
        _terrain_size: u32,
        _terrain_scale: f32,
        _brush_radius: f32,
        _brush_strength: f32,
        move_speed: f32,
    ) -> Result<(), JsValue> {
        self.configure_controls(move_speed)
    }

    #[wasm_bindgen]
    pub fn set_debug_mode(&self, enabled: bool) {
        *self.debug_mode.lock().unwrap() = enabled;
        self.push_command(JsCommand::SetDebugMode { enabled });
    }

    #[wasm_bindgen]
    pub fn get_metrics_json(&self) -> String {
        let m = self.metrics.lock().unwrap();
        format!("{{\"commands_processed\":{}}}", m.commands_processed)
    }

    #[wasm_bindgen]
    pub fn dispose(&self) {
        self.command_queue.lock().unwrap().clear();
        self.camera_override.lock().unwrap().take();
    }

    #[wasm_bindgen]
    pub fn mount_and_run(&self) -> Result<(), JsValue> {
        let runtime = WebRuntime::new(
            self.command_queue.clone(),
            self.camera_override.clone(),
            self.metrics.clone(),
            self.enabled_plugins.clone(),
            *self.pending_config.lock().unwrap(),
            *self.debug_mode.lock().unwrap(),
        );

        static FONT_BYTES: &[u8] = include_bytes!("../../../assets/fonts/Roboto-Regular.ttf");
        App::new(runtime)
            .with_background_color(Color::rgb(0.015, 0.02, 0.03))
            .with_font_bytes(FONT_BYTES)
            .with_render_quality(ferrous_core::RenderQuality::Low)
            .with_msaa(1)
            .with_mode(ferrous_app::AppMode::Game3D)
            .run();

        Ok(())
    }

    #[wasm_bindgen]
    pub fn create_scene(&self) -> u32 {
        let mut next = self.next_scene_id.lock().unwrap();
        let id = *next;
        *next += 1;
        self.push_command(JsCommand::CreateScene { scene_id: id });
        id
    }

    #[wasm_bindgen]
    pub fn set_active_scene(&self, scene_id: u32) {
        self.push_command(JsCommand::SetActiveScene { scene_id });
    }

    #[wasm_bindgen]
    pub fn set_camera(&self, ex: f32, ey: f32, ez: f32, tx: f32, ty: f32, tz: f32) {
        *self.camera_override.lock().unwrap() = Some(([ex, ey, ez], [tx, ty, tz]));
        self.push_command(JsCommand::SetCamera {
            eye: [ex, ey, ez],
            target: [tx, ty, tz],
        });
    }

    #[wasm_bindgen]
    pub fn create_box(
        &self,
        name: String,
        x: f32,
        y: f32,
        z: f32,
        sx: f32,
        sy: f32,
        sz: f32,
        r: f32,
        g: f32,
        b: f32,
    ) {
        self.push_command(JsCommand::CreateBox {
            name,
            position: [x, y, z],
            size: [sx.max(0.01), sy.max(0.01), sz.max(0.01)],
            color: [r, g, b],
        });
    }

    #[wasm_bindgen]
    pub fn create_sphere(
        &self,
        name: String,
        x: f32,
        y: f32,
        z: f32,
        radius: f32,
        segments: u32,
        r: f32,
        g: f32,
        b: f32,
    ) {
        self.push_command(JsCommand::CreateSphere {
            name,
            position: [x, y, z],
            radius: radius.max(0.01),
            segments: segments.max(3),
            color: [r, g, b],
        });
    }

    #[wasm_bindgen]
    pub fn spawn_entity(
        &self,
        name: String,
        kind: String,
        x: f32,
        y: f32,
        z: f32,
        r: f32,
        g: f32,
        b: f32,
    ) {
        self.push_command(JsCommand::SpawnEntity {
            name,
            kind,
            position: [x, y, z],
            color: [r, g, b],
        });
    }

    #[wasm_bindgen]
    pub fn set_transform(
        &self,
        name: String,
        x: f32,
        y: f32,
        z: f32,
        rx: f32,
        ry: f32,
        rz: f32,
        sx: f32,
        sy: f32,
        sz: f32,
    ) {
        self.push_command(JsCommand::SetTransform {
            name,
            position: [x, y, z],
            rotation: [rx, ry, rz],
            scale: [sx.max(0.001), sy.max(0.001), sz.max(0.001)],
        });
    }

    #[wasm_bindgen]
    pub fn add_point_light(
        &self,
        name: String,
        x: f32,
        y: f32,
        z: f32,
        r: f32,
        g: f32,
        b: f32,
        intensity: f32,
        range: f32,
    ) {
        self.push_command(JsCommand::AddPointLight {
            name,
            position: [x, y, z],
            color: [r, g, b],
            intensity,
            range: range.max(0.01),
        });
    }

    #[wasm_bindgen]
    pub fn set_directional_light(
        &self,
        dx: f32,
        dy: f32,
        dz: f32,
        r: f32,
        g: f32,
        b: f32,
        intensity: f32,
    ) {
        self.push_command(JsCommand::SetDirectionalLight {
            direction: [dx, dy, dz],
            color: [r, g, b],
            intensity,
        });
    }

    #[wasm_bindgen]
    pub fn update_material(
        &self,
        name: String,
        r: f32,
        g: f32,
        b: f32,
        metal: f32,
        rough: f32,
    ) {
        self.push_command(JsCommand::UpdateMaterial {
            entity_name: name,
            r,
            g,
            b,
            metallic: metal,
            roughness: rough,
        });
    }

    #[wasm_bindgen]
    pub fn remove_entity(&self, name: String) {
        self.push_command(JsCommand::RemoveEntity { name });
    }

    #[wasm_bindgen]
    pub fn clear_world(&self) {
        self.push_command(JsCommand::ClearWorld);
    }

    #[wasm_bindgen]
    pub fn enable_plugin(&self, name: String) {
        self.push_command(JsCommand::EnablePlugin { name });
    }

    #[wasm_bindgen]
    pub fn disable_plugin(&self, name: String) {
        self.push_command(JsCommand::DisablePlugin { name });
    }

    #[wasm_bindgen]
    pub fn create_terrain(&self) {
        self.push_command(JsCommand::LegacyCreateTerrain);
    }

    #[wasm_bindgen]
    pub fn toggle_sky(&self) {
        self.push_command(JsCommand::LegacyToggleSky);
    }
}