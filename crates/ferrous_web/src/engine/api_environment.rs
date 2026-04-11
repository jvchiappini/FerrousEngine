use wasm_bindgen::prelude::*;
use crate::engine::FerrousWebEngine;
use crate::commands::JsCommand;

#[wasm_bindgen]
impl FerrousWebEngine {
    #[wasm_bindgen(js_name = setEnvironment)]
    pub fn set_environment(&self, fr: f32, fg: f32, fb: f32, density: f32) {
        self.push_command(JsCommand::SetEnvironment { fog_color: [fr, fg, fb], fog_density: density });
    }

    #[wasm_bindgen(js_name = setExposure)]
    pub fn set_exposure(&self, exposure: f32) {
        self.push_command(JsCommand::SetExposure { exposure });
    }

    #[wasm_bindgen(js_name = setBackground)]
    pub fn set_background(&self, r: f32, g: f32, b: f32) {
        self.push_command(JsCommand::SetBackground { r, g, b });
    }

    #[wasm_bindgen(js_name = setDebugMode)]
    pub fn set_debug_mode(&self, enabled: bool) {
        *self.debug_mode.lock().unwrap() = enabled;
        self.push_command(JsCommand::SetDebugMode { enabled });
    }

    #[wasm_bindgen(js_name = getMetricsJson)]
    pub fn get_metrics_json(&self) -> String {
        let m = self.metrics.lock().unwrap();
        format!("{{\"commands_processed\":{}}}", m.commands_processed)
    }

    #[wasm_bindgen(js_name = setErrorCallback)]
    pub fn set_error_callback(&self, callback: &js_sys::Function) {
        *self.error_callback.lock().unwrap() = Some(callback.clone());
    }
}
