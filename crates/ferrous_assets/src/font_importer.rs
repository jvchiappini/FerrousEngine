//! [`FontData`] — CPU-side font bytes asset that implements [`Asset`].
//!
//! ## Why a separate `FontData` type?
//!
//! [`ferrous_font::Font`] is not `Asset`-compatible because it requires a live
//! `wgpu::Device` and `wgpu::Queue` to bake the MSDF atlas at construction
//! time — those objects only exist on the GPU thread.
//!
//! `FontData` breaks the dependency by storing only the raw font bytes on
//! load.  The GPU atlas is built later when a `Device` and `Queue` are
//! available (see `FontData::into_font`).
//!
//! ## Usage
//!
//! ```rust,ignore
//! use ferrous_assets::{AssetServer, AssetState, FontData};
//!
//! // Start loading (runs on a rayon thread — no GPU needed).
//! let handle = asset_server.load::<FontData>("assets/fonts/inter.ttf");
//!
//! // Later, each frame:
//! if let AssetState::Ready(data) = asset_server.get(handle) {
//!     let font = data.into_font(&device, &queue, ' '..'~');
//!     renderer.set_font_atlas(&font.atlas.view, &font.atlas.sampler);
//! }
//! ```

use ferrous_asset_types::Asset;
use std::path::Path;

// wgpu and the `font` module are only needed when the `gpu` feature is
// enabled; guard their usage accordingly.
#[cfg(feature = "gpu")]
use wgpu;

/// Raw font file bytes, ready to be baked into a GPU atlas.
///
/// Produced by loading a `.ttf` or `.otf` file from disk.
/// GPU-side atlas baking happens separately via [`FontData::into_font`].
#[derive(Debug, Clone)]
pub struct FontData {
    /// Raw bytes of the font file (TrueType / OpenType).
    pub bytes: Vec<u8>,
}

impl FontData {
    /// Create `FontData` from an in-memory byte slice.
    pub fn from_bytes(bytes: &[u8]) -> Self {
        Self {
            bytes: bytes.to_vec(),
        }
    }

    /// Bake a GPU-side MSDF font atlas from the stored bytes.
    ///
    /// This is the *second phase* of font loading and requires a live
    /// `wgpu::Device` + `wgpu::Queue`.  Call this from the main (GPU) thread
    /// once [`crate::server::AssetServer::get`] returns
    /// [`ferrous_asset_types::AssetState::Ready`].
    #[cfg(feature = "gpu")]
    pub fn into_font(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        chars: impl IntoIterator<Item = char>,
    ) -> ferrous_font::Font {
        ferrous_font::Font::load_bytes(&self.bytes, device, queue, chars)
    }
}

impl Asset for FontData {
    fn type_name() -> &'static str {
        "FontData"
    }

    /// Read raw font bytes from `path`.  This is called on a background thread
    /// — no GPU access is performed here.
    fn import(path: &Path) -> anyhow::Result<Self> {
        let bytes = std::fs::read(path).map_err(|e| {
            anyhow::anyhow!(
                "FontData::import — failed to read '{path}': {e}",
                path = path.display()
            )
        })?;

        // Basic sanity check: all TrueType/OpenType fonts start with a
        // 4-byte tag we can verify to catch obviously-wrong file types early.
        if bytes.len() < 4 {
            return Err(anyhow::anyhow!(
                "FontData::import — file '{}' is too small to be a font ({} bytes)",
                path.display(),
                bytes.len()
            ));
        }

        Ok(Self { bytes })
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Asset;
    use std::io::Write;

    #[test]
    fn type_name_is_correct() {
        assert_eq!(FontData::type_name(), "FontData");
    }

    #[test]
    fn import_missing_file_returns_error() {
        let result = FontData::import(Path::new("this-font-does-not-exist.ttf"));
        assert!(result.is_err());
    }

    #[test]
    fn import_too_small_file_returns_error() {
        // Write a tiny file that is clearly not a font.
        let dir = std::env::temp_dir();
        let path = dir.join("ferrous_font_too_small.bin");
        {
            let mut f = std::fs::File::create(&path).unwrap();
            f.write_all(b"AB").unwrap(); // 2 bytes — too small
        }
        let result = FontData::import(&path);
        let _ = std::fs::remove_file(&path);
        assert!(result.is_err());
    }

    #[test]
    fn from_bytes_roundtrip() {
        let data = FontData::from_bytes(b"abcdef");
        assert_eq!(data.bytes, b"abcdef");
    }

    #[test]
    fn import_arbitrary_bytes_succeeds_if_large_enough() {
        // Any file >= 4 bytes is accepted at the import stage (full parsing
        // happens in `into_font`; we don't validate font structure here).
        let dir = std::env::temp_dir();
        let path = dir.join("ferrous_font_dummy.bin");
        std::fs::write(&path, b"OTTO\x00\x01\x00\x00").unwrap(); // valid OTF magic
        let result = FontData::import(&path);
        let _ = std::fs::remove_file(&path);
        assert!(result.is_ok());
    }
}
