//! [`AssetServer`] — non-blocking background asset loader.
//!
//! ## Overview
//!
//! The `AssetServer` is the central registry for all runtime assets.
//! It tracks every asset by a type-safe [`crate::handle::AssetHandle<T>`] and
//! drives the two-phase `import → ready` pipeline:
//!
//! ```text
//!  caller          AssetServer                   background thread
//!    │                  │                                │
//!    │  load("foo.glb") │                                │
//!    │─────────────────►│  spawn / enqueue work          │
//!    │◄─ handle ────────│─────────────────────────────── ► import()
//!    │                  │◄── (send Arc<T> via channel) ──│
//!    │  get(handle) ────►│ drain inbox → update registry │
//!    │◄─ Ready(arc) ────│                                │
//! ```
//!
//! On **native targets** the import runs on a `rayon` thread pool.
//! On **wasm32** the import runs *synchronously on the calling thread* (there
//! is no background threading for file I/O in the browser; a proper wasm
//! implementation would use `wasm_bindgen_futures` + `fetch`, but that
//! requires JS bindings we don't have yet, so we keep it simple).
//!
//! ## Path deduplication
//!
//! If `load("foo.glb")` is called twice before the first load finishes, the
//! second call returns the **same handle** and no duplicate work is done.
//!
//! ## Hot-reload (desktop only)
//!
//! Calling [`AssetServer::watch`] on a handle registers a `notify` file watcher
//! for the underlying path.  When the file changes on disk the asset is
//! transparently re-imported and the next call to `get()` returns a freshly
//! loaded `Arc<T>`.  Old `Arc<T>` values held by callers remain valid until
//! they are dropped.

use crate::asset_trait::Asset;
use crate::handle::{AssetHandle, AssetState};
use anyhow::Result;
use std::any::{Any, TypeId};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

// ---------------------------------------------------------------------------
// Internal storage
// ---------------------------------------------------------------------------

/// Type-erased slot in the asset registry.
struct AssetSlot {
    generation: u16,
    /// `None` while loading, `Some(Ok(Arc<dyn Any>))` when ready,
    /// `Some(Err(msg))` on failure.
    value: SlotState,
}

enum SlotState {
    Loading,
    Ready(Arc<dyn Any + Send + Sync>),
    Failed(String),
}

// ---------------------------------------------------------------------------
// Inbox — channel used by background threads to deliver finished assets
// ---------------------------------------------------------------------------

struct InboxItem {
    slot_id: u32,
    generation: u16,
    result: Result<Arc<dyn Any + Send + Sync>, String>,
}

// ---------------------------------------------------------------------------
// Hot-reload watch data (desktop only)
// ---------------------------------------------------------------------------

#[cfg(not(target_arch = "wasm32"))]
struct WatchEntry {
    path: PathBuf,
    slot_id: u32,
    generation: u16,
    /// Closure that re-runs import and returns an erased Arc.
    reimport: Box<dyn Fn(&Path) -> Result<Arc<dyn Any + Send + Sync>, String> + Send + Sync>,
}

// ---------------------------------------------------------------------------
// AssetServer
// ---------------------------------------------------------------------------

/// Central asset registry with background loading and optional hot-reload.
///
/// Obtain a handle via [`AssetServer::load`]; poll state via [`AssetServer::get`].
///
/// Register this as an ECS resource so all systems can access it:
/// ```rust,ignore
/// world.ecs.resources.insert(AssetServer::new());
/// ```
pub struct AssetServer {
    /// All allocated slots.  Shared with background threads via `Arc<Mutex<_>>`.
    slots: Arc<Mutex<Vec<AssetSlot>>>,
    /// Maps (TypeId, canonicalized path) → slot index for deduplication.
    path_index: HashMap<(TypeId, PathBuf), u32>,
    /// Background threads deliver completed assets through this channel.
    inbox_tx: std::sync::mpsc::SyncSender<InboxItem>,
    inbox_rx: std::sync::mpsc::Receiver<InboxItem>,

    /// Desktop-only: file watcher entries.
    #[cfg(not(target_arch = "wasm32"))]
    watch_entries: Vec<WatchEntry>,
    /// Desktop-only: `notify` watcher handle (kept alive for the duration of
    /// the `AssetServer`).
    #[cfg(not(target_arch = "wasm32"))]
    _watcher: Option<Box<dyn notify::Watcher + Send>>,
    /// Desktop-only: channel used by the `notify` watcher to signal changes.
    #[cfg(not(target_arch = "wasm32"))]
    watch_rx: Option<std::sync::mpsc::Receiver<notify::Result<notify::Event>>>,
}

impl AssetServer {
    /// Create a new, empty `AssetServer`.
    pub fn new() -> Self {
        let (tx, rx) = std::sync::mpsc::sync_channel::<InboxItem>(256);

        #[cfg(not(target_arch = "wasm32"))]
        let (watch_tx, watch_rx) = std::sync::mpsc::channel();

        #[cfg(not(target_arch = "wasm32"))]
        let watcher: Option<Box<dyn notify::Watcher + Send>> = {
            use notify::Watcher;
            match notify::RecommendedWatcher::new(
                watch_tx,
                notify::Config::default()
                    .with_poll_interval(std::time::Duration::from_secs(1)),
            ) {
                Ok(w) => Some(Box::new(w)),
                Err(e) => {
                    eprintln!("[AssetServer] failed to create file watcher: {e}");
                    None
                }
            }
        };

        Self {
            slots: Arc::new(Mutex::new(Vec::new())),
            path_index: HashMap::new(),
            inbox_tx: tx,
            inbox_rx: rx,
            #[cfg(not(target_arch = "wasm32"))]
            watch_entries: Vec::new(),
            #[cfg(not(target_arch = "wasm32"))]
            _watcher: watcher,
            #[cfg(not(target_arch = "wasm32"))]
            watch_rx: Some(watch_rx),
        }
    }

    // -----------------------------------------------------------------------
    // load
    // -----------------------------------------------------------------------

    /// Begin loading `path` as asset type `T`.
    ///
    /// Returns immediately with a handle.  If the same path was already
    /// requested (and is still loading or ready), returns the existing handle.
    ///
    /// On **native** the import runs on a `rayon` thread.
    /// On **wasm32** the import runs synchronously right now.
    pub fn load<T: Asset>(&mut self, path: impl AsRef<Path>) -> AssetHandle<T> {
        let path = path.as_ref();
        let canonical = match std::fs::canonicalize(path) {
            Ok(p) => p,
            Err(_) => path.to_path_buf(), // best-effort if file doesn't exist yet
        };
        let key = (TypeId::of::<T>(), canonical.clone());

        // Deduplication: if the same path is already tracked return existing handle.
        if let Some(&slot_id) = self.path_index.get(&key) {
            let gen = self.slots.lock().unwrap()[slot_id as usize].generation;
            return AssetHandle::new(slot_id, gen);
        }

        // Allocate a new slot in the Loading state.
        let slot_id = {
            let mut slots = self.slots.lock().unwrap();
            let id = slots.len() as u32;
            slots.push(AssetSlot {
                generation: 0,
                value: SlotState::Loading,
            });
            id
        };
        self.path_index.insert(key, slot_id);

        let handle = AssetHandle::<T>::new(slot_id, 0);

        // Kick off the import.
        self.spawn_import::<T>(slot_id, 0, path.to_path_buf());

        handle
    }

    // -----------------------------------------------------------------------
    // get
    // -----------------------------------------------------------------------

    /// Poll the state of `handle`.
    ///
    /// This also drains the inbox from completed background imports — call it
    /// regularly (e.g. once per frame) to keep the registry up-to-date.
    pub fn get<T: Asset + 'static>(&mut self, handle: AssetHandle<T>) -> AssetState<T> {
        self.drain_inbox();

        let slots = self.slots.lock().unwrap();
        let id = handle.id() as usize;
        if id >= slots.len() {
            return AssetState::NotFound;
        }
        let slot = &slots[id];
        if slot.generation != handle.generation() {
            return AssetState::NotFound;
        }
        match &slot.value {
            SlotState::Loading => AssetState::Loading,
            SlotState::Failed(msg) => AssetState::Failed(msg.clone()),
            SlotState::Ready(arc) => match arc.clone().downcast::<T>() {
                Ok(typed) => AssetState::Ready(typed),
                Err(_) => AssetState::Failed("type mismatch in asset slot".into()),
            },
        }
    }

    // -----------------------------------------------------------------------
    // watch (desktop only)
    // -----------------------------------------------------------------------

    /// Register a file watcher for `handle`'s underlying path so it is
    /// re-imported automatically when the file changes on disk.
    ///
    /// Only available on non-wasm32 targets.
    #[cfg(not(target_arch = "wasm32"))]
    pub fn watch<T: Asset + 'static>(&mut self, handle: AssetHandle<T>) {
        // Find the path for this slot.
        let slot_id = handle.id();
        let path = match self
            .path_index
            .iter()
            .find(|((tid, _), &sid)| *tid == TypeId::of::<T>() && sid == slot_id)
        {
            Some(((_, p), _)) => p.clone(),
            None => {
                eprintln!("[AssetServer] watch(): handle {slot_id} not found in path index");
                return;
            }
        };

        let reimport: Box<dyn Fn(&Path) -> Result<Arc<dyn Any + Send + Sync>, String> + Send + Sync> =
            Box::new(|p: &Path| {
                T::import(p)
                    .map(|v| Arc::new(v) as Arc<dyn Any + Send + Sync>)
                    .map_err(|e| e.to_string())
            });

        let gen = self.slots.lock().unwrap()[slot_id as usize].generation;

        self.watch_entries.push(WatchEntry {
            path: path.clone(),
            slot_id,
            generation: gen,
            reimport,
        });

        if let Some(watcher) = self._watcher.as_mut() {
            #[allow(unused_imports)]
            use notify::Watcher;
            if let Err(e) = watcher.watch(&path, notify::RecursiveMode::NonRecursive) {
                eprintln!("[AssetServer] could not watch '{path}': {e}", path = path.display());
            }
        }
    }

    // -----------------------------------------------------------------------
    // tick — call once per frame to process hot-reload events
    // -----------------------------------------------------------------------

    /// Process pending hot-reload file change events.
    ///
    /// Call this once per frame (or whenever convenient).  On wasm32 this is
    /// a no-op.
    #[cfg(not(target_arch = "wasm32"))]
    pub fn tick(&mut self) {
        use notify::EventKind;

        let rx = match self.watch_rx.as_ref() {
            Some(rx) => rx,
            None => return,
        };

        // Drain all pending filesystem events.
        while let Ok(event) = rx.try_recv() {
            let event = match event {
                Ok(e) => e,
                Err(_) => continue,
            };

            // Only care about content modifications.
            if !matches!(
                event.kind,
                EventKind::Modify(_) | EventKind::Create(_)
            ) {
                continue;
            }

            for changed_path in &event.paths {
                // Find matching watch entries.
                for entry in &mut self.watch_entries {
                    if &entry.path != changed_path {
                        continue;
                    }

                    let new_gen = entry.generation.wrapping_add(1);
                    entry.generation = new_gen;

                    let path = changed_path.clone();
                    let reimport = &entry.reimport;
                    let result = reimport(&path);

                    let mut slots = self.slots.lock().unwrap();
                    if let Some(slot) = slots.get_mut(entry.slot_id as usize) {
                        slot.generation = new_gen;
                        slot.value = match result {
                            Ok(arc) => SlotState::Ready(arc),
                            Err(msg) => SlotState::Failed(msg),
                        };
                    }
                }
            }
        }
    }

    #[cfg(target_arch = "wasm32")]
    pub fn tick(&mut self) {
        self.drain_inbox();
    }

    // -----------------------------------------------------------------------
    // Internal helpers
    // -----------------------------------------------------------------------

    /// Drain completed imports from the inbox into the registry.
    fn drain_inbox(&mut self) {
        while let Ok(item) = self.inbox_rx.try_recv() {
            let mut slots = self.slots.lock().unwrap();
            if let Some(slot) = slots.get_mut(item.slot_id as usize) {
                if slot.generation == item.generation {
                    slot.value = match item.result {
                        Ok(arc) => SlotState::Ready(arc),
                        Err(msg) => SlotState::Failed(msg),
                    };
                }
            }
        }
    }

    /// Kick off the background import for `T` at `path`.
    fn spawn_import<T: Asset>(&self, slot_id: u32, generation: u16, path: PathBuf) {
        #[cfg(not(target_arch = "wasm32"))]
        {
            let tx = self.inbox_tx.clone();
            rayon::spawn(move || {
                let result = T::import(&path)
                    .map(|v| Arc::new(v) as Arc<dyn Any + Send + Sync>)
                    .map_err(|e| e.to_string());
                let _ = tx.send(InboxItem {
                    slot_id,
                    generation,
                    result,
                });
            });
        }

        #[cfg(target_arch = "wasm32")]
        {
            // On wasm32 we import synchronously and write directly into the slot.
            let result = T::import(&path)
                .map(|v| Arc::new(v) as Arc<dyn Any + Send + Sync>)
                .map_err(|e| e.to_string());
            let _ = self.inbox_tx.send(InboxItem {
                slot_id,
                generation,
                result,
            });
        }
    }

    /// Total number of tracked asset slots (loaded + loading + failed).
    pub fn slot_count(&self) -> usize {
        self.slots.lock().unwrap().len()
    }
}

impl Default for AssetServer {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    /// Trivial in-memory asset for testing.
    #[derive(Debug)]
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
    fn load_missing_file_yields_failed() {
        let mut server = AssetServer::new();
        let handle = server.load::<TextAsset>("this-path-definitely-does-not-exist.txt");

        // Give the background thread a moment to complete.
        std::thread::sleep(std::time::Duration::from_millis(50));

        match server.get(handle) {
            AssetState::Failed(_) => {} // expected
            AssetState::Loading => panic!("still loading after 50 ms"),
            other => panic!("unexpected state: {other:?}"),
        }
    }

    #[test]
    fn load_existing_file_yields_ready() {
        // Write a temp file.
        let dir = std::env::temp_dir();
        let path = dir.join("ferrous_asset_server_test.txt");
        std::fs::write(&path, "hello ferrous").unwrap();

        let mut server = AssetServer::new();
        let handle = server.load::<TextAsset>(&path);

        // Poll until ready or timeout.
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
        loop {
            match server.get(handle) {
                AssetState::Ready(arc) => {
                    assert_eq!(arc.0, "hello ferrous");
                    break;
                }
                AssetState::Loading if std::time::Instant::now() < deadline => {
                    std::thread::sleep(std::time::Duration::from_millis(10));
                }
                other => panic!("unexpected state: {other:?}"),
            }
        }

        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn load_same_path_twice_gives_same_handle() {
        let dir = std::env::temp_dir();
        let path = dir.join("ferrous_dedup_test.txt");
        std::fs::write(&path, "dedup").unwrap();

        let mut server = AssetServer::new();
        let h1 = server.load::<TextAsset>(&path);
        let h2 = server.load::<TextAsset>(&path);
        assert_eq!(h1, h2, "second load should return the same handle");
        assert_eq!(server.slot_count(), 1, "only one slot should be allocated");

        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn stale_handle_returns_not_found() {
        let mut server = AssetServer::new();
        // Manufacture a handle that points to a non-existent slot.
        let handle = AssetHandle::<TextAsset>::new(999, 0);
        assert!(matches!(server.get(handle), AssetState::NotFound));
    }
}
