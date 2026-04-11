use wasm_bindgen::prelude::*;
use crate::engine::FerrousWebEngine;
use crate::commands::JsCommand;

#[wasm_bindgen]
impl FerrousWebEngine {
    #[wasm_bindgen(js_name = updateMaterial)]
    pub fn update_material(
        &self, name: String,
        r: f32, g: f32, b: f32,
        metal: f32, rough: f32,
        clearcoat: f32, clearcoat_rough: f32,
        opacity: f32,
    ) {
        self.push_command(JsCommand::UpdateMaterial {
            entity_name: name,
            r, g, b,
            metallic: metal,
            roughness: rough,
            clearcoat,
            clearcoat_roughness: clearcoat_rough,
            opacity,
            albedo_tex: None,
        });
    }

    #[wasm_bindgen(js_name = setTransform)]
    pub fn set_transform(
        &self, name: String,
        x: f32, y: f32, z: f32,
        rx: f32, ry: f32, rz: f32,
        sx: f32, sy: f32, sz: f32,
    ) {
        self.push_command(JsCommand::SetPosition { name: name.clone(), position: [x, y, z] });
        self.push_command(JsCommand::SetRotation { name: name.clone(), rotation: [rx, ry, rz] });
        self.push_command(JsCommand::SetScale { name, scale: [sx.max(0.001), sy.max(0.001), sz.max(0.001)] });
    }

    #[wasm_bindgen(js_name = setVisible)]
    pub fn set_visible(&self, name: String, visible: bool) {
        self.push_command(JsCommand::SetVisible { name, visible });
    }

    #[wasm_bindgen(js_name = loadTexture)]
    pub fn load_texture(&self, url: String) -> js_sys::Promise {
        let req_id = crate::runtime::NEXT_REQUEST_ID.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let promise = js_sys::Promise::new(&mut |resolve, _reject| {
            crate::runtime::ASSET_RESOLVERS.lock().unwrap().insert(req_id, resolve);
        });
        self.push_command(JsCommand::LoadTexture { url, request_id: req_id });
        promise
    }

    #[wasm_bindgen(js_name = loadModel)]
    pub fn load_model(&self, url: String) -> js_sys::Promise {
        let req_id = crate::runtime::NEXT_REQUEST_ID.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let promise = js_sys::Promise::new(&mut |resolve, _reject| {
            crate::runtime::ASSET_RESOLVERS.lock().unwrap().insert(req_id, resolve);
        });
        self.push_command(JsCommand::LoadModel { url, request_id: req_id });
        promise
    }
}
