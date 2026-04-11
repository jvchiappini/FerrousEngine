use wasm_bindgen::prelude::*;
use crate::engine::FerrousWebEngine;
use crate::commands::JsCommand;

#[wasm_bindgen]
impl FerrousWebEngine {
    #[wasm_bindgen(js_name = addPointLight)]
    pub fn add_point_light(
        &self, name: String,
        x: f32, y: f32, z: f32,
        r: f32, g: f32, b: f32,
        intensity: f32, range: f32,
    ) {
        self.push_command(JsCommand::AddPointLight {
            name,
            position: [x, y, z],
            color: [r, g, b],
            intensity,
            range: range.max(0.01),
        });
    }

    #[wasm_bindgen(js_name = setDirectionalLight)]
    pub fn set_directional_light(
        &self, dx: f32, dy: f32, dz: f32,
        r: f32, g: f32, b: f32, intensity: f32,
    ) {
        self.push_command(JsCommand::SetDirectionalLight {
            direction: [dx, dy, dz],
            color: [r, g, b],
            intensity,
        });
    }

    #[wasm_bindgen(js_name = setAmbientLight)]
    pub fn set_ambient_light(&self, r: f32, g: f32, b: f32, intensity: f32) {
        self.push_command(JsCommand::SetAmbientLight { color: [r, g, b], intensity });
    }
}
