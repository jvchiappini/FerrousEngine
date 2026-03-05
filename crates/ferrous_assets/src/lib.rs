//! `ferrous_assets` — CPU-side asset loading for FerrousEngine.
//!
//! This crate is intentionally free of wgpu / GPU types (except for
//! the legacy `Texture2d` helper which still takes a `wgpu::Device`) so
//! it can be used in tools, headless tests, and the editor without a full
//! GPU context.
//!
//! ## Modules
//!
//! | Module           | Responsibility                                              |
//! |------------------|-------------------------------------------------------------|
//! | `gltf_loader`    | glTF/GLB import → `AssetModel` (CPU mesh + materials)       |
//! | `texture`        | PNG/JPEG → `Texture2d` (GPU upload, legacy)                 |
//! | `font`           | MSDF font atlas baking (parser, msdf_gen, atlas)            |
//! | `handle`         | `AssetHandle<T>`, `AssetState<T>` — type-safe handle system |
//! | `asset_trait`    | `Asset` trait — `import` + `type_name`                      |
//! | `server`         | `AssetServer` — non-blocking loader + hot-reload            |
//! | `gltf_importer`  | `GltfModel: Asset` — wraps `load_gltf`                      |
//! | `image_importer` | `ImageData: Asset` — CPU-side RGBA8 image                   |
//!
//! ## Phase 5 — Asset Pipeline (implemented)
//!
//! - `AssetHandle<T>` — type-safe, generation-tracked handle (8 bytes, `Copy`).
//! - `AssetServer` — background loading via rayon thread pool (native) /
//!   synchronous inline load (wasm32).
//! - `Asset` trait with `import` + `type_name`.
//! - Hot-reload via `notify` file watcher (non-wasm32 only).
//! - `GltfModel: Asset` and `ImageData: Asset` importers.

// ── existing modules ────────────────────────────────────────────────────────
pub mod font;
pub mod gltf_loader;
pub mod texture;

// ── Phase 5: asset pipeline ─────────────────────────────────────────────────
pub mod asset_trait;
pub mod gltf_importer;
pub mod handle;
pub mod image_importer;
pub mod server;

// ── re-exports: legacy API (unchanged) ──────────────────────────────────────
pub use font::Font;
pub use gltf_loader::{load_gltf, AssetMesh, AssetModel, RawMaterial};
pub use texture::Texture2d;

// ── re-exports: Phase 5 API ──────────────────────────────────────────────────
pub use asset_trait::Asset;
pub use gltf_importer::GltfModel;
pub use handle::{AssetHandle, AssetState};
pub use image_importer::ImageData;
pub use server::AssetServer;

/// Convenience prelude — glob-import this in game/editor code.
pub mod prelude {
    pub use crate::{Asset, AssetHandle, AssetServer, AssetState, GltfModel, ImageData};
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn missing_file_returns_error() {
        let res = load_gltf(Path::new("this-file-does-not-exist.gltf"));
        assert!(res.is_err());
    }

    #[test]
    fn asset_server_can_be_constructed() {
        let _server = AssetServer::new();
    }

    #[test]
    fn image_data_type_name() {
        assert_eq!(ImageData::type_name(), "ImageData");
    }

    #[test]
    fn gltf_model_type_name() {
        assert_eq!(GltfModel::type_name(), "GltfModel");
    }
}
