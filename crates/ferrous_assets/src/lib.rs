//! `ferrous_assets` — CPU-side asset loading for FerrousEngine.
//!
//! This crate is intentionally free of wgpu / GPU types so it can be used in
//! tools, headless tests, and the editor without a GPU context.
//!
//! ## Modules
//!
//! | Module | Responsibility |
//! |--------|----------------|
//! | `gltf_loader` | glTF/GLB import → `AssetModel` (CPU mesh + materials) |
//! | `texture` | PNG/JPEG image loading → `Texture2d` |
//! | `font` | MSDF font atlas baking (parser, msdf_gen, atlas) |
//!
//! ## Planned (Phase 5)
//!
//! - `AssetHandle<T>` — type-safe, generation-tracked handle (16 bytes, `Copy`).
//! - `AssetServer` — background loading via rayon thread pool (native) /
//!   `wasm_bindgen_futures` (WASM).
//! - `Asset` trait with `import` + `process` stages.
//! - Hot-reload via `notify` file watcher (non-WASM only).

pub mod font;
pub mod gltf_loader;
pub mod texture;

// Exponemos la estructura principal para que sea fácil de importar
pub use font::Font;
pub use gltf_loader::{load_gltf, AssetMesh, AssetModel, RawMaterial};
pub use texture::Texture2d;

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn missing_file_returns_error() {
        let res = load_gltf(Path::new("this-file-does-not-exist.gltf"));
        assert!(res.is_err());
    }
}
