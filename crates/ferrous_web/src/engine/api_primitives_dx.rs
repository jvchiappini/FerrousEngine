use wasm_bindgen::prelude::*;
use serde::{Deserialize, Serialize};
use crate::engine::FerrousWebEngine;
use crate::entity::JsEntity;
use crate::commands::JsCommand;

#[derive(Serialize, Deserialize)]
pub struct Vector3Config {
    pub x: f32,
    pub y: f32,
    pub z: f32,
}

#[derive(Serialize, Deserialize)]
pub struct ColorConfig {
    pub r: f32,
    pub g: f32,
    pub b: f32,
}

#[derive(Serialize, Deserialize)]
pub struct BoxConfig {
    pub name: String,
    pub position: Option<Vector3Config>,
    pub size: Option<Vector3Config>,
    pub color: Option<ColorConfig>,
}

#[wasm_bindgen]
impl FerrousWebEngine {
    #[wasm_bindgen(js_name = createBoxDX)]
    pub fn create_box_dx(&self, config_val: JsValue) -> Result<JsEntity, JsValue> {
        let config: BoxConfig = serde_wasm_bindgen::from_value(config_val)?;
        
        let pos = config.position.unwrap_or(Vector3Config { x: 0.0, y: 0.0, z: 0.0 });
        let size = config.size.unwrap_or(Vector3Config { x: 1.0, y: 1.0, z: 1.0 });
        let color = config.color.unwrap_or(ColorConfig { r: 1.0, g: 1.0, b: 1.0 });

        let name = config.name;
        
        self.push_command(JsCommand::CreateBox {
            name: name.clone(),
            position: [pos.x, pos.y, pos.z],
            size: [size.x.max(0.01), size.y.max(0.01), size.z.max(0.01)],
            color: [color.r, color.g, color.b],
        });
        Ok(JsEntity { name, command_queue: self.command_queue.clone() })
    }
}
