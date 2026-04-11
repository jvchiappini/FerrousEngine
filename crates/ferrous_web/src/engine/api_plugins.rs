use wasm_bindgen::prelude::*;
use crate::engine::FerrousWebEngine;
use crate::commands::JsCommand;

#[wasm_bindgen]
impl FerrousWebEngine {
    #[wasm_bindgen(js_name = registerPlugin)]
    pub fn register_plugin(&self, name: String, on_update: JsValue, on_sync_world: JsValue) {
        let update_fn = if on_update.is_function() { Some(on_update.unchecked_into::<js_sys::Function>()) } else { None };
        let sync_fn = if on_sync_world.is_function() { Some(on_sync_world.unchecked_into::<js_sys::Function>()) } else { None };
        
        let plugin = crate::plugin::JsWebPlugin::new(name, update_fn, sync_fn);
        self.registered_plugins.lock().unwrap().push(Box::new(plugin));
    }

    #[wasm_bindgen(js_name = enablePlugin)]
    pub fn enable_plugin(&self, name: String) {
        self.push_command(JsCommand::EnablePlugin { name });
    }

    #[wasm_bindgen(js_name = disablePlugin)]
    pub fn disable_plugin(&self, name: String) {
        self.push_command(JsCommand::DisablePlugin { name });
    }

    #[wasm_bindgen(js_name = createTerrain)]
    pub fn create_terrain(&self) {
        self.push_command(JsCommand::LegacyCreateTerrain);
    }

    #[wasm_bindgen(js_name = toggleSky)]
    pub fn toggle_sky(&self) {
        self.push_command(JsCommand::LegacyToggleSky);
    }
}
