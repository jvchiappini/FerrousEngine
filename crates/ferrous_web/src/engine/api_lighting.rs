use wasm_bindgen::prelude::*;
use crate::engine::FerrousWebEngine;
use crate::commands::JsCommand;

#[wasm_bindgen]
impl FerrousWebEngine {
    #[wasm_bindgen(js_name = addPointLight)]
    pub fn add_point_light(
        &self, name: String,
        position: Vec<f32>,
        color: Vec<f32>,
        intensity: f32, range: f32,
    ) {
        let p = [position.get(0).copied().unwrap_or(0.0), position.get(1).copied().unwrap_or(0.0), position.get(2).copied().unwrap_or(0.0)];
        let c = [color.get(0).copied().unwrap_or(1.0), color.get(1).copied().unwrap_or(1.0), color.get(2).copied().unwrap_or(1.0)];
        self.push_command(JsCommand::AddPointLight {
            name,
            position: p,
            color: c,
            intensity,
            range: range.max(0.01),
        });
    }

    #[wasm_bindgen(js_name = setDirectionalLight)]
    pub fn set_directional_light(
        &self, direction: Vec<f32>,
        color: Vec<f32>, intensity: f32,
    ) {
        let d = [direction.get(0).copied().unwrap_or(0.0), direction.get(1).copied().unwrap_or(-1.0), direction.get(2).copied().unwrap_or(0.0)];
        let c = [color.get(0).copied().unwrap_or(1.0), color.get(1).copied().unwrap_or(1.0), color.get(2).copied().unwrap_or(1.0)];
        self.push_command(JsCommand::SetDirectionalLight {
            direction: d,
            color: c,
            intensity,
        });
    }

    #[wasm_bindgen(js_name = setAmbientLight)]
    pub fn set_ambient_light(&self, color: Vec<f32>, intensity: f32) {
        let c = [color.get(0).copied().unwrap_or(0.1), color.get(1).copied().unwrap_or(0.1), color.get(2).copied().unwrap_or(0.1)];
        self.push_command(JsCommand::SetAmbientLight { color: c, intensity });
    }
}
