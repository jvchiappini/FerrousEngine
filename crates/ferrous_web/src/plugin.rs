use ferrous_app::AppContext;
use ferrous_core::World;
use wasm_bindgen::prelude::*;
use js_sys::Function;

/// Trait for extending the WebRuntime lifecycle.
pub trait WebPlugin: Send + Sync {
    fn name(&self) -> &str;
    fn on_update(&mut self, _ctx: &mut AppContext, _dt: f32) {}
    fn on_sync_world(&mut self, _world: &World) {}
}

/// A plugin implemented in JavaScript.
pub struct JsWebPlugin {
    name: String,
    on_update: Option<Function>,
    on_sync_world: Option<Function>,
}

impl JsWebPlugin {
    pub fn new(name: String, on_update: Option<Function>, on_sync_world: Option<Function>) -> Self {
        Self {
            name,
            on_update,
            on_sync_world,
        }
    }
}

impl WebPlugin for JsWebPlugin {
    fn name(&self) -> &str {
        &self.name
    }

    fn on_update(&mut self, _ctx: &mut AppContext, dt: f32) {
        if let Some(f) = &self.on_update {
            let _ = f.call1(&JsValue::NULL, &JsValue::from_f64(dt as f64));
        }
    }

    fn on_sync_world(&mut self, _world: &World) {
        if let Some(f) = &self.on_sync_world {
            let _ = f.call0(&JsValue::NULL);
        }
    }
}

// In WASM, everything is on the main thread, so we can safely implement Send/Sync 
// for our JS-backed plugin to store it in Arc<Mutex<...>>.
unsafe impl Send for JsWebPlugin {}
unsafe impl Sync for JsWebPlugin {}

