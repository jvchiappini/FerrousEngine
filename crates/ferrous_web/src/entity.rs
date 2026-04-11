use std::sync::{Arc, Mutex};
use wasm_bindgen::prelude::*;
use crate::commands::JsCommand;

#[wasm_bindgen]
#[derive(Clone)]
pub struct JsEntity {
    pub(crate) name: String,
    pub(crate) command_queue: Arc<Mutex<Vec<JsCommand>>>,
}

#[wasm_bindgen]
impl JsEntity {
    #[wasm_bindgen(js_name = setPosition)]
    pub fn set_position(&self, x: f32, y: f32, z: f32) -> JsEntity {
        self.command_queue.lock().unwrap().push(JsCommand::SetPosition {
            name: self.name.clone(),
            position: [x, y, z],
        });
        self.clone()
    }

    #[wasm_bindgen(js_name = setRotation)]
    pub fn set_rotation(&self, rx: f32, ry: f32, rz: f32) -> JsEntity {
        self.command_queue.lock().unwrap().push(JsCommand::SetRotation {
            name: self.name.clone(),
            rotation: [rx, ry, rz],
        });
        self.clone()
    }

    #[wasm_bindgen(js_name = setScale)]
    pub fn set_scale(&self, sx: f32, sy: f32, sz: f32) -> JsEntity {
        self.command_queue.lock().unwrap().push(JsCommand::SetScale {
            name: self.name.clone(),
            scale: [sx, sy, sz],
        });
        self.clone()
    }

    #[wasm_bindgen(js_name = setVisible)]
    pub fn set_visible(&self, visible: bool) -> JsEntity {
        self.command_queue.lock().unwrap().push(JsCommand::SetVisible {
            name: self.name.clone(),
            visible,
        });
        self.clone()
    }

    #[wasm_bindgen(js_name = setColor)]
    pub fn set_color(&self, r: f32, g: f32, b: f32) -> JsEntity {
        self.command_queue.lock().unwrap().push(JsCommand::UpdateMaterial {
            entity_name: self.name.clone(),
            r, g, b,
            metallic: 0.0,
            roughness: 0.5,
            clearcoat: 0.0,
            clearcoat_roughness: 0.0,
            opacity: 1.0,
            albedo_tex: None,
        });
        self.clone()
    }

    #[wasm_bindgen(js_name = setMaterial)]
    pub fn set_material(&self, r: f32, g: f32, b: f32, metal: f32, rough: f32) -> JsEntity {
        self.command_queue.lock().unwrap().push(JsCommand::UpdateMaterial {
            entity_name: self.name.clone(),
            r, g, b,
            metallic: metal,
            roughness: rough,
            clearcoat: 0.0,
            clearcoat_roughness: 0.0,
            opacity: 1.0,
            albedo_tex: None,
        });
        self.clone()
    }

    #[wasm_bindgen(js_name = remove)]
    pub fn remove(&self) {
        self.command_queue.lock().unwrap().push(JsCommand::RemoveEntity {
            name: self.name.clone(),
        });
    }
}
