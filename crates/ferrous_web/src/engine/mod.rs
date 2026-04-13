use std::collections::HashSet;
use std::sync::{Arc, Mutex};
use js_sys::Function;
use wasm_bindgen::prelude::*;
use ferrous_app::{App, Color};

use crate::commands::JsCommand;
use crate::config::{EngineConfig, EngineMetrics};
use crate::runtime::WebRuntime;
use crate::plugin::WebPlugin;

/// The primary entry point for the Ferrous 3D engine on the web.
#[wasm_bindgen]
pub struct FerrousWebEngine {
    pub(crate) command_queue: Arc<Mutex<Vec<JsCommand>>>,
    pub(crate) camera_override: Arc<Mutex<Option<([f32; 3], [f32; 3])>>>,
    pub(crate) pending_config: Arc<Mutex<EngineConfig>>,
    pub(crate) error_callback: Arc<Mutex<Option<Function>>>,
    pub(crate) metrics: Arc<Mutex<EngineMetrics>>,
    pub(crate) debug_mode: Arc<Mutex<bool>>,
    pub(crate) enabled_plugins: Arc<Mutex<HashSet<String>>>,
    pub(crate) next_scene_id: Arc<Mutex<u32>>,
    pub(crate) registered_plugins: Arc<Mutex<Vec<Box<dyn WebPlugin>>>>,
}

#[wasm_bindgen]
impl FerrousWebEngine {
    #[wasm_bindgen(constructor)]
    pub fn new() -> Self {
        let version = env!("CARGO_PKG_VERSION");
        web_sys::console::info_1(&JsValue::from_str(&format!("[FerrousWeb] engine ctor v{}", version)));

        use std::sync::Once;
        static SET_HOOK: Once = Once::new();
        SET_HOOK.call_once(|| {
            std::panic::set_hook(Box::new(|info| {
                let location = info.location().map(|l| format!("{}:{}:{}", l.file(), l.line(), l.column())).unwrap_or_else(|| "unknown".to_string());
                let payload = info.payload().downcast_ref::<&str>().copied()
                    .unwrap_or_else(|| info.payload().downcast_ref::<String>().map(|s| s.as_str()).unwrap_or("unknown panic"));
                
                let error_json = format!(
                    "{{\"code\":\"panic\",\"message\":\"{}\",\"location\":\"{}\"}}",
                    payload.replace('"', "\\\""),
                    location
                );
                
                web_sys::console::error_1(&wasm_bindgen::JsValue::from_str(&format!("[Ferrous-Panic] {}", error_json)));
            }));
        });

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
            registered_plugins: Arc::new(Mutex::new(Vec::new())),
        }
    }

    #[wasm_bindgen(js_name = mountAndRun)]
    pub fn mount_and_run(&self) -> Result<(), JsValue> {
        let version = env!("CARGO_PKG_VERSION");
        web_sys::console::info_1(&JsValue::from_str(&format!("[FerrousWeb] mountAndRun 41.0 v{}", version)));

        let runtime = WebRuntime::new(
            self.command_queue.clone(),
            self.camera_override.clone(),
            self.metrics.clone(),
            self.enabled_plugins.clone(),
            self.error_callback.clone(),
            self.registered_plugins.lock().unwrap().drain(..).collect(),
            *self.pending_config.lock().unwrap(),
            *self.debug_mode.lock().unwrap(),
        );

        // Pre-load Roboto-Regular.ttf for UI (built-in as binary)
        static FONT_BYTES: &[u8] = include_bytes!("../../../../assets/fonts/Roboto-Regular.ttf");

        App::new(runtime)
            .with_background_color(Color::rgb(0.06, 0.08, 0.12))
            .with_font_bytes(FONT_BYTES)
            .with_render_quality(ferrous_core::RenderQuality::Low)
            .with_msaa(1)
            .with_mode(ferrous_app::AppMode::Game3D)
            .run();

        Ok(())
    }

    #[wasm_bindgen(js_name = dispose)]
    pub fn dispose(&self) {
        self.command_queue.lock().unwrap().clear();
        self.camera_override.lock().unwrap().take();
    }

    pub(crate) fn push_command(&self, cmd: JsCommand) {
        self.command_queue.lock().unwrap().push(cmd);
    }

    pub(crate) fn report_error(&self, code: &str, message: &str) {
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
}

// Sub-modules containing API extensions
mod api_camera;
mod api_environment;
mod api_lighting;
mod api_materials;
mod api_primitives;
mod api_scene;
mod api_plugins;
mod api_primitives_dx;
pub mod api_shared;
