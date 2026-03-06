//! Type-safe asset handles and state.
//!
//!
//! An [`AssetHandle<T>`] is a lightweight, `Copy` identifier (8 bytes) that
//! refers to a specific asset inside the [`crate::server::AssetServer`].
//! Handles are generation-tracked: once an asset is evicted and a new one
//! takes its slot, old handles become stale and `get()` returns
//! [`AssetState::NotFound`].
//!
//! ## Example
//!
//! ```rust,ignore
//! let handle: AssetHandle<GltfModel> = server.load("assets/player.glb");
//! match server.get(handle) {
//!     AssetState::Loading          => { /* spinner */ }
//!     AssetState::Ready(model)     => { /* use it */ }
//!     AssetState::Failed(msg)      => eprintln!("load error: {msg}"),
//!     AssetState::NotFound         => eprintln!("stale handle"),
//! }
//! ```

use std::marker::PhantomData;

// ---------------------------------------------------------------------------
// AssetHandle
// ---------------------------------------------------------------------------

/// A lightweight, type-tagged, generation-tracked reference to an asset.
///
/// Internally: 4-byte `id` (slot index in the registry) + 2-byte `generation`
/// to detect use-after-free. Total: 8 bytes — fits in a register.
///
/// The `T` phantom type prevents mixing handles of different asset types
/// without any runtime overhead.
#[derive(Debug)]
pub struct AssetHandle<T> {
    /// Slot index in the asset registry.
    pub(crate) id: u32,
    /// Generation counter — bumped every time the slot is recycled.
    pub(crate) generation: u16,
    /// Zero-size phantom to bind the handle to its asset type.
    _marker: PhantomData<fn() -> T>,
}

// Manual impls so that T does not need to be Clone/Copy/PartialEq/Hash/etc.
impl<T> Clone for AssetHandle<T> {
    fn clone(&self) -> Self {
        *self
    }
}
impl<T> Copy for AssetHandle<T> {}

impl<T> PartialEq for AssetHandle<T> {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id && self.generation == other.generation
    }
}
impl<T> Eq for AssetHandle<T> {}

impl<T> std::hash::Hash for AssetHandle<T> {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.id.hash(state);
        self.generation.hash(state);
    }
}

impl<T> AssetHandle<T> {
    /// Construct a handle from raw parts.  Only the `AssetServer` should call
    /// this — external code should obtain handles via
    /// [`crate::server::AssetServer::load`].
    ///
    /// This method must remain public because the server lives in a different
    /// crate than the type definitions; we rely on documentation rather than
    /// visibility to discourage arbitrary use.
    #[inline]
    pub fn new(id: u32, generation: u16) -> Self {
        Self {
            id,
            generation,
            _marker: PhantomData,
        }
    }

    /// Returns the internal slot index (not stable across sessions).
    #[inline]
    pub fn id(self) -> u32 {
        self.id
    }

    /// Returns the generation count (used for staleness checks).
    #[inline]
    pub fn generation(self) -> u16 {
        self.generation
    }
}

// ---------------------------------------------------------------------------
// AssetState
// ---------------------------------------------------------------------------

/// The current loading state of an asset tracked by the [`crate::server::AssetServer`].
#[derive(Debug, Clone)]
pub enum AssetState<T> {
    /// The asset has been queued but the background thread hasn't finished yet.
    Loading,
    /// The asset is fully loaded and ready to use.
    Ready(std::sync::Arc<T>),
    /// The import failed.  The string contains a human-readable error message.
    Failed(String),
    /// The handle is stale (the slot was recycled or the asset was never known).
    NotFound,
}

impl<T> AssetState<T> {
    /// Returns `true` if the asset is ready to use.
    pub fn is_ready(&self) -> bool {
        matches!(self, AssetState::Ready(_))
    }

    /// Returns `true` if loading is still in progress.
    pub fn is_loading(&self) -> bool {
        matches!(self, AssetState::Loading)
    }

    /// Unwrap the `Arc<T>`, panicking if the asset is not ready.
    pub fn unwrap(self) -> std::sync::Arc<T> {
        match self {
            AssetState::Ready(v) => v,
            other => panic!("AssetState::unwrap called on {:?}", other.variant_name()),
        }
    }

    fn variant_name(&self) -> &'static str {
        match self {
            AssetState::Loading => "Loading",
            AssetState::Ready(_) => "Ready",
            AssetState::Failed(_) => "Failed",
            AssetState::NotFound => "NotFound",
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug)]
    struct Dummy;

    #[test]
    fn handle_copy_and_eq() {
        let h1 = AssetHandle::<Dummy>::new(7, 3);
        let h2 = h1; // Copy
        assert_eq!(h1, h2);
    }

    #[test]
    fn handle_not_eq_different_generation() {
        let h1 = AssetHandle::<Dummy>::new(0, 0);
        let h2 = AssetHandle::<Dummy>::new(0, 1);
        assert_ne!(h1, h2);
    }

    #[test]
    fn state_is_ready() {
        let s: AssetState<u32> = AssetState::Ready(std::sync::Arc::new(42));
        assert!(s.is_ready());
        assert!(!s.is_loading());
    }

    #[test]
    fn state_is_loading() {
        let s: AssetState<u32> = AssetState::Loading;
        assert!(s.is_loading());
        assert!(!s.is_ready());
    }
}
