//! `Asset` trait — the common interface for all importable resources.
//!
//! Every asset type (GLTF models, textures, fonts, audio clips, …) implements
//! [`Asset`] so that the [`crate::server::AssetServer`] can drive the two-phase
//! import pipeline uniformly:
//!
//! ```text
//! File I/O  ──►  Asset::import()  ──►  Asset::process()  ──►  stored in registry
//!                  (raw bytes)          (post-process:
//!                                        mip gen, tangents…)
//! ```
//!
//! For simple assets the two phases can collapse into one.

use std::path::Path;

/// Marker + import trait for loadable assets.
///
/// Implementors must be `Send + Sync + 'static` so they can live behind an
/// `Arc<T>` inside the [`crate::server::AssetServer`] registry and be handed
/// to the background thread pool.
pub trait Asset: Send + Sync + 'static {
    /// Human-readable name used in log messages and diagnostics.
    fn type_name() -> &'static str
    where
        Self: Sized;

    /// Load raw asset data from `path` synchronously.
    ///
    /// This is called on a background thread by the `AssetServer`.
    /// The return type is `Self` for simplicity; complex two-stage pipelines
    /// can use the `process` hook.
    fn import(path: &Path) -> anyhow::Result<Self>
    where
        Self: Sized;
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    struct TextAsset(String);

    impl Asset for TextAsset {
        fn type_name() -> &'static str {
            "TextAsset"
        }

        fn import(path: &Path) -> anyhow::Result<Self> {
            let content = std::fs::read_to_string(path)?;
            Ok(TextAsset(content))
        }
    }

    #[test]
    fn type_name_returns_correct_str() {
        assert_eq!(TextAsset::type_name(), "TextAsset");
    }

    #[test]
    fn import_missing_file_returns_error() {
        let result = TextAsset::import(Path::new("no-such-file.txt"));
        assert!(result.is_err());
    }
}
