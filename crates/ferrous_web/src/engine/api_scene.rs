use wasm_bindgen::prelude::*;
use crate::engine::FerrousWebEngine;
use crate::entity::JsEntity;
use crate::commands::JsCommand;

#[wasm_bindgen]
impl FerrousWebEngine {
    #[wasm_bindgen(js_name = createScene)]
    pub fn create_scene(&self) -> u32 {
        let mut next = self.next_scene_id.lock().unwrap();
        let id = *next;
        *next += 1;
        self.push_command(JsCommand::CreateScene { scene_id: id });
        id
    }

    #[wasm_bindgen(js_name = setActiveScene)]
    pub fn set_active_scene(&self, scene_id: u32) {
        self.push_command(JsCommand::SetActiveScene { scene_id });
    }

    #[wasm_bindgen(js_name = clearWorld)]
    pub fn clear_world(&self) {
        self.push_command(JsCommand::ClearWorld);
    }

    #[wasm_bindgen(js_name = spawnEntity)]
    pub fn spawn_entity(
        &self, name: String, kind: String,
        x: f32, y: f32, z: f32,
        r: f32, g: f32, b: f32,
    ) -> JsEntity {
        self.push_command(JsCommand::SpawnEntity {
            name: name.clone(),
            kind,
            position: [x, y, z],
            color: [r, g, b],
        });
        JsEntity { name, command_queue: self.command_queue.clone() }
    }

    #[wasm_bindgen(js_name = removeEntity)]
    pub fn remove_entity(&self, name: String) {
        self.push_command(JsCommand::RemoveEntity { name });
    }

    #[wasm_bindgen(js_name = exportScene)]
    pub fn export_scene(&self) -> js_sys::Promise {
        let req_id = crate::runtime::NEXT_REQUEST_ID.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let promise = js_sys::Promise::new(&mut |resolve, _reject| {
            crate::runtime::ASSET_RESOLVERS.lock().unwrap().insert(req_id, resolve);
        });
        self.push_command(JsCommand::ExportScene { request_id: req_id });
        promise
    }

    #[wasm_bindgen(js_name = importScene)]
    pub fn import_scene(&self, json: String) {
        self.push_command(JsCommand::ImportScene { json });
    }
}
