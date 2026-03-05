//! [`ImageData`] — CPU-side image asset implementing the [`Asset`] trait.
//!
//! Unlike [`crate::texture::Texture2d`], `ImageData` holds only CPU memory
//! (width, height, RGBA8 pixel bytes) and **has no GPU dependency** — it can
//! be loaded on a background thread without a `wgpu::Device`.
//!
//! The renderer is responsible for uploading `ImageData` to the GPU when it
//! is needed.  This keeps `ferrous_assets` free of wgpu and makes headless
//! testing straightforward.
//!
//! ## Example
//!
//! ```rust,ignore
//! use ferrous_assets::{AssetServer, ImageData};
//!
//! let mut server = AssetServer::new();
//! let handle = server.load::<ImageData>("assets/textures/albedo.png");
//!
//! // later, once ready:
//! if let AssetState::Ready(img) = server.get(handle) {
//!     renderer.register_texture(img.width, img.height, &img.pixels);
//! }
//! ```

use crate::asset_trait::Asset;
use anyhow::{bail, Context, Result};
use std::path::Path;

// ---------------------------------------------------------------------------
// ImageData
// ---------------------------------------------------------------------------

/// A CPU-side RGBA8 image loaded from disk.
///
/// All pixels are guaranteed to be in RGBA8 order (4 bytes per pixel).
/// The `pixels` field has exactly `width * height * 4` bytes.
#[derive(Debug, Clone)]
pub struct ImageData {
    pub width: u32,
    pub height: u32,
    /// Raw RGBA8 pixel data, row-major, top-left origin.
    pub pixels: Vec<u8>,
}

impl ImageData {
    /// Create `ImageData` from raw RGBA8 bytes.
    ///
    /// # Panics
    ///
    /// Panics if `pixels.len() != width * height * 4`.
    pub fn from_rgba8(width: u32, height: u32, pixels: Vec<u8>) -> Self {
        assert_eq!(
            pixels.len(),
            (width as usize) * (height as usize) * 4,
            "pixels length does not match width*height*4"
        );
        Self {
            width,
            height,
            pixels,
        }
    }

    /// Number of bytes per row.
    #[inline]
    pub fn bytes_per_row(&self) -> u32 {
        self.width * 4
    }
}

impl Asset for ImageData {
    fn type_name() -> &'static str {
        "ImageData"
    }

    fn import(path: &Path) -> Result<Self> {
        let ext = path
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("")
            .to_ascii_lowercase();

        // EXR files are HDR — we decode with the `image` crate and tonemap to
        // LDR RGBA8 for storage.  For non-HDR formats we just decode normally.
        let img = image::open(path)
            .with_context(|| format!("failed to open image '{}'", path.display()))?;

        // Normalise to RGBA8.
        let rgba = img.to_rgba8();
        let (w, h) = rgba.dimensions();

        if w == 0 || h == 0 {
            bail!("image '{}' has zero-size dimensions", path.display());
        }

        let _ = ext; // reserved for per-format logic in the future

        Ok(ImageData {
            width: w,
            height: h,
            pixels: rgba.into_raw(),
        })
    }
}

// `ImageData` is just `Vec<u8>` + two `u32`s — always Send + Sync.
const _: () = {
    fn _assert_send_sync<T: Send + Sync>() {}
    fn _check() {
        _assert_send_sync::<ImageData>();
    }
};

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn type_name_is_correct() {
        assert_eq!(ImageData::type_name(), "ImageData");
    }

    #[test]
    fn import_missing_file_returns_error() {
        let res = ImageData::import(Path::new("__no_such_image__.png"));
        assert!(res.is_err());
    }

    #[test]
    fn from_rgba8_correct_length() {
        let pixels = vec![255u8; 4 * 4 * 4]; // 4×4 image
        let img = ImageData::from_rgba8(4, 4, pixels);
        assert_eq!(img.bytes_per_row(), 16);
    }

    #[test]
    #[should_panic]
    fn from_rgba8_wrong_length_panics() {
        ImageData::from_rgba8(2, 2, vec![0u8; 3]); // wrong size
    }
}
