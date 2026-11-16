//! On-disk cache of baked font atlases.
//!
//! A bake is the expensive half of font registration (the MSDF pass over every
//! glyph), and it depends only on the font bytes and the character set.  This
//! module stores the resulting [`BakedAtlas`] so the next process — or the next
//! registration in this one — can skip the bake and upload directly.
//!
//! Files are named after the [`FontKey`], written atomically (temporary file
//! plus rename), bounded in total size with least-recently-used eviction, and
//! validated on read: a corrupt, truncated or stale file is deleted and the
//! atlas is baked again.  Nothing here panics on bad input.

use std::fs;
use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use crate::baked::{BakedAtlas, FontKey};

/// Default cache bound: 64 MiB, about twenty `latin_western` faces.
pub const DEFAULT_MAX_BYTES: u64 = 64 * 1024 * 1024;

/// Extension of a cached atlas file.
const EXTENSION: &str = "atlas";

/// How an application wants its baked atlases cached.
///
/// Pass it to the engine through `AppConfig::with_font_cache`; the engine builds
/// an [`AtlasCache`] from it.  The default is disabled so a library consumer
/// does not write to disk without asking.
#[derive(Clone, Debug)]
pub struct FontCacheConfig {
    /// Whether the cache is used at all.
    pub enabled: bool,
    /// Directory holding the atlas files; `None` means
    /// [`default_directory`] for [`Self::app_name`].
    pub directory: Option<PathBuf>,
    /// Application name used by the default directory.
    pub app_name: String,
    /// Total size bound in bytes; the least recently used files are evicted.
    pub max_bytes: u64,
}

impl FontCacheConfig {
    /// A cache for `app_name` under the per-OS default directory.
    #[must_use]
    pub fn for_app(app_name: impl Into<String>) -> Self {
        Self {
            enabled: true,
            directory: None,
            app_name: app_name.into(),
            max_bytes: DEFAULT_MAX_BYTES,
        }
    }

    /// Uses `directory` instead of the per-OS default.
    #[must_use]
    pub fn with_directory(mut self, directory: impl Into<PathBuf>) -> Self {
        self.directory = Some(directory.into());
        self
    }

    /// Changes the size bound.
    #[must_use]
    pub fn with_max_bytes(mut self, max_bytes: u64) -> Self {
        self.max_bytes = max_bytes;
        self
    }

    /// A disabled cache.
    #[must_use]
    pub fn disabled() -> Self {
        Self {
            enabled: false,
            directory: None,
            app_name: String::new(),
            max_bytes: DEFAULT_MAX_BYTES,
        }
    }
}

impl Default for FontCacheConfig {
    fn default() -> Self {
        Self::disabled()
    }
}

/// The per-OS cache directory for `app_name`.
///
/// - Windows: `%LOCALAPPDATA%\<app>\cache\fonts`
/// - macOS: `~/Library/Caches/<app>/fonts`
/// - Linux: `$XDG_CACHE_HOME/<app>/fonts`, else `~/.cache/<app>/fonts`
/// - wasm32: `None` (no filesystem)
#[must_use]
pub fn default_directory(app_name: &str) -> Option<PathBuf> {
    #[cfg(target_arch = "wasm32")]
    {
        let _ = app_name;
        None
    }
    #[cfg(target_os = "windows")]
    {
        std::env::var_os("LOCALAPPDATA").map(|base| {
            PathBuf::from(base)
                .join(app_name)
                .join("cache")
                .join("fonts")
        })
    }
    #[cfg(target_os = "macos")]
    {
        std::env::var_os("HOME").map(|home| {
            PathBuf::from(home)
                .join("Library")
                .join("Caches")
                .join(app_name)
                .join("fonts")
        })
    }
    #[cfg(all(
        not(target_os = "windows"),
        not(target_os = "macos"),
        not(target_arch = "wasm32")
    ))]
    {
        std::env::var_os("XDG_CACHE_HOME")
            .map(PathBuf::from)
            .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".cache")))
            .map(|base| base.join(app_name).join("fonts"))
    }
}

/// A directory of baked atlases, one file per [`FontKey`].
#[derive(Clone, Debug)]
pub struct AtlasCache {
    root: PathBuf,
    max_bytes: u64,
}

impl AtlasCache {
    /// A cache rooted at `root` with the default size bound.
    ///
    /// The directory is created by [`Self::store`], not here, so constructing a
    /// cache never has a side effect.
    #[must_use]
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self {
            root: root.into(),
            max_bytes: DEFAULT_MAX_BYTES,
        }
    }

    /// [`Self::new`] with an explicit size bound.
    #[must_use]
    pub fn with_max_bytes(mut self, max_bytes: u64) -> Self {
        self.max_bytes = max_bytes;
        self
    }

    /// Builds the cache a configuration asks for, or `None` when it is disabled
    /// or the platform has no cache directory.
    #[must_use]
    pub fn from_config(config: &FontCacheConfig) -> Option<Self> {
        if !config.enabled {
            return None;
        }
        let root = match &config.directory {
            Some(directory) => directory.clone(),
            None => default_directory(&config.app_name)?,
        };
        Some(Self {
            root,
            max_bytes: config.max_bytes,
        })
    }

    /// The directory holding the atlas files.
    #[must_use]
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// The file one key is stored in.
    #[must_use]
    pub fn path_for(&self, key: FontKey) -> PathBuf {
        self.root.join(format!("{}.{EXTENSION}", key.hex()))
    }

    /// Loads the atlas for `key`, or `None` when it is missing or unusable.
    ///
    /// A file that fails to decode, was written by another version, or whose
    /// stored key does not match is removed and reported as a miss.  A hit
    /// marks the file as recently used (for eviction) and returns the atlas.
    #[must_use]
    pub fn load(&self, key: FontKey) -> Option<BakedAtlas> {
        let path = self.path_for(key);
        let bytes = fs::read(&path).ok()?;
        match BakedAtlas::decode(&bytes) {
            Ok(atlas) if atlas.key == key => {
                self.mark_used(&path);
                Some(atlas)
            }
            Ok(_) | Err(_) => {
                // Corrupt, stale or a hash collision: drop it and rebuild.
                let _ = fs::remove_file(&path);
                None
            }
        }
    }

    /// Stores `atlas` for `key` and evicts the least recently used files until
    /// the cache fits the configured size bound.
    ///
    /// The write is atomic (temporary file plus rename) and failures are
    /// silent: the cache is an optimisation, never a requirement.
    pub fn store(&self, key: FontKey, atlas: &BakedAtlas) {
        if fs::create_dir_all(&self.root).is_err() {
            return;
        }
        let path = self.path_for(key);
        let temporary = self
            .root
            .join(format!("{}.{}.tmp", key.hex(), std::process::id()));
        let Ok(mut file) = fs::File::create(&temporary) else {
            return;
        };
        if file.write_all(&atlas.encode()).is_err() {
            let _ = fs::remove_file(&temporary);
            return;
        }
        let _ = file.sync_all();
        drop(file);
        if fs::rename(&temporary, &path).is_err() {
            let _ = fs::remove_file(&temporary);
            return;
        }
        self.evict();
    }

    /// Marks `path` as used now; best effort, never fails the load.
    fn mark_used(&self, path: &Path) {
        if let Ok(file) = fs::OpenOptions::new().write(true).open(path) {
            let _ = file.set_modified(SystemTime::now());
        }
    }

    /// Removes the oldest atlas files until the total size fits the bound.
    ///
    /// The newest file (usually the one just written) goes last, so a single
    /// atlas larger than the bound is kept rather than thrashing.
    fn evict(&self) {
        if self.max_bytes == 0 {
            return;
        }
        let Ok(entries) = fs::read_dir(&self.root) else {
            return;
        };
        let mut files: Vec<(SystemTime, u64, PathBuf)> = entries
            .flatten()
            .filter_map(|entry| {
                let path = entry.path();
                if path.extension().and_then(|extension| extension.to_str()) != Some(EXTENSION) {
                    return None;
                }
                let metadata = entry.metadata().ok()?;
                let modified = metadata.modified().unwrap_or(SystemTime::UNIX_EPOCH);
                Some((modified, metadata.len(), path))
            })
            .collect();
        let mut total: u64 = files.iter().map(|(_, length, _)| *length).sum();
        if total <= self.max_bytes {
            return;
        }
        files.sort_by_key(|(modified, _, _)| *modified);
        let newest = files.len().saturating_sub(1);
        for (index, (_, length, path)) in files.into_iter().enumerate() {
            if index == newest || total <= self.max_bytes {
                break;
            }
            if fs::remove_file(&path).is_ok() {
                total = total.saturating_sub(length);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::FontParser;

    const ROBOTO: &[u8] = include_bytes!("../../../assets/fonts/Roboto-Regular.ttf");

    /// A unique temporary directory per test.
    fn temp_root(name: &str) -> PathBuf {
        let root =
            std::env::temp_dir().join(format!("ferrous-font-cache-{}-{name}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        root
    }

    fn bake() -> BakedAtlas {
        let parser = FontParser::new(ROBOTO.to_vec()).unwrap();
        BakedAtlas::build(&parser, &crate::charset::ascii()).unwrap()
    }

    #[test]
    fn a_disabled_config_builds_no_cache() {
        assert!(AtlasCache::from_config(&FontCacheConfig::disabled()).is_none());
    }

    #[test]
    fn an_explicit_directory_is_used_verbatim() {
        let root = temp_root("explicit");
        let config = FontCacheConfig::for_app("Test").with_directory(&root);
        let cache = AtlasCache::from_config(&config).unwrap();
        assert_eq!(cache.root(), root.as_path());
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn a_hit_is_byte_for_byte_the_cold_bake() {
        let root = temp_root("hit");
        let cache = AtlasCache::new(&root);
        let cold = bake();
        assert!(cache.load(cold.key).is_none(), "nothing stored yet");

        cache.store(cold.key, &cold);
        let hit = cache.load(cold.key).expect("the stored atlas loads");
        assert_eq!(hit, cold, "a hit must equal the cold bake");
        assert_eq!(hit.pixels, cold.pixels);
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn a_corrupt_file_is_ignored_and_replaced() {
        let root = temp_root("corrupt");
        let cache = AtlasCache::new(&root);
        let atlas = bake();
        fs::create_dir_all(&root).unwrap();
        fs::write(cache.path_for(atlas.key), b"garbage").unwrap();
        assert!(cache.load(atlas.key).is_none());
        assert!(
            !cache.path_for(atlas.key).exists(),
            "the unusable file is removed"
        );

        cache.store(atlas.key, &atlas);
        assert_eq!(cache.load(atlas.key).unwrap(), atlas);
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn a_truncated_file_is_ignored() {
        let root = temp_root("truncated");
        let cache = AtlasCache::new(&root);
        let atlas = bake();
        fs::create_dir_all(&root).unwrap();
        let encoded = atlas.encode();
        fs::write(cache.path_for(atlas.key), &encoded[..encoded.len() / 3]).unwrap();
        assert!(cache.load(atlas.key).is_none());
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn a_file_with_another_generator_version_is_ignored() {
        let root = temp_root("version");
        let cache = AtlasCache::new(&root);
        let atlas = bake();
        fs::create_dir_all(&root).unwrap();
        let mut encoded = atlas.encode();
        encoded[6..10].copy_from_slice(&(crate::baked::ATLAS_GENERATOR_VERSION + 1).to_le_bytes());
        fs::write(cache.path_for(atlas.key), &encoded).unwrap();
        assert!(cache.load(atlas.key).is_none());
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn the_cache_is_bounded_and_evicts_the_oldest_file() {
        let root = temp_root("evict");
        let parser = FontParser::new(ROBOTO.to_vec()).unwrap();
        let ascii = BakedAtlas::build(&parser, &crate::charset::ascii()).unwrap();
        let western = BakedAtlas::build(&parser, &crate::charset::latin_western()).unwrap();
        // Room for the bigger atlas plus a little, not for both.
        let western_bytes = western.encode().len() as u64;
        let cache = AtlasCache::new(&root).with_max_bytes(western_bytes + 1);

        cache.store(ascii.key, &ascii);
        // The file system may clamp the modification time to a coarse tick;
        // make the ordering explicit for the test.
        let old = SystemTime::now() - std::time::Duration::from_secs(60);
        fs::OpenOptions::new()
            .write(true)
            .open(cache.path_for(ascii.key))
            .unwrap()
            .set_modified(old)
            .unwrap();
        cache.store(western.key, &western);

        assert!(
            !cache.path_for(ascii.key).exists(),
            "the older atlas is evicted"
        );
        assert!(cache.path_for(western.key).exists());
        let _ = fs::remove_dir_all(&root);
    }
}
