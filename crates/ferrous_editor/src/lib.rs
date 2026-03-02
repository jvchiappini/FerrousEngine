// This file exists so that wasm-pack can produce a cdylib target.
// All application logic lives in `app.rs`.

mod app;

/// wasm32 entry point called from JavaScript: `ferrous_editor.run()`.
#[cfg(target_arch = "wasm32")]
#[wasm_bindgen::prelude::wasm_bindgen]
pub fn run() {
    app::build_app().run();
}
