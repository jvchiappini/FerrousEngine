//! [`GltfModel`] — `Asset` implementation that wraps the existing
//! synchronous `load_gltf` loader.
//!
//! Using this importer via the `AssetServer` gives you non-blocking loads on
//! native targets (the import runs on a `rayon` thread) while remaining
//! compatible with the existing CPU-side `AssetModel` data format.
//!
//! ## Example
//!
//! ```rust,ignore
//! use ferrous_assets::{AssetServer, GltfModel};
//!
//! let mut server = AssetServer::new();
//! let handle = server.load::<GltfModel>("assets/scene.glb");
//!
//! // later, each frame:
//! match server.get(handle) {
//!     AssetState::Ready(model) => spawn_entities(&model),
//!     AssetState::Loading      => draw_spinner(),
//!     AssetState::Failed(msg)  => eprintln!("error: {msg}"),
//!     AssetState::NotFound     => {}
//! }
//! ```

use ferrous_asset_types::Asset;
use crate::gltf_loader::{self, AssetMesh, AssetModel, RawMaterial};
use std::path::Path;

/// A complete, CPU-side GLTF/GLB model.
///
/// This is a thin newtype around [`AssetModel`] that implements the [`Asset`]
/// trait so the `AssetServer` can manage its lifetime.  The underlying data
/// is identical to what `load_gltf` returned directly in Phase 4.
pub struct GltfModel(pub AssetModel);

impl GltfModel {
    /// Access the inner `AssetModel`.
    #[inline]
    pub fn inner(&self) -> &AssetModel {
        &self.0
    }

    /// Access meshes directly.
    #[inline]
    pub fn meshes(&self) -> &[AssetMesh] {
        &self.0.meshes
    }

    /// Access materials directly.
    #[inline]
    pub fn materials(&self) -> &[RawMaterial] {
        &self.0.materials
    }

    /// Access images (width, height, RGBA8 pixels) directly.
    #[inline]
    pub fn images(&self) -> &[(u32, u32, Vec<u8>)] {
        &self.0.images
    }
}

impl Asset for GltfModel {
    fn type_name() -> &'static str {
        "GltfModel"
    }

    fn import(path: &Path) -> anyhow::Result<Self> {
        let model = gltf_loader::load_gltf(path)?;
        Ok(GltfModel(model))
    }
}

// `GltfModel` contains only `Vec<_>` and plain data — it is `Send + Sync`.
// We assert this explicitly to catch regressions early.
const _: () = {
    fn _assert_send_sync<T: Send + Sync>() {}
    fn _check() {
        _assert_send_sync::<GltfModel>();
    }
};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn type_name_is_correct() {
        assert_eq!(GltfModel::type_name(), "GltfModel");
    }

    #[test]
    fn import_missing_returns_error() {
        let res = GltfModel::import(Path::new("__does_not_exist__.glb"));
        assert!(res.is_err());
    }
}
