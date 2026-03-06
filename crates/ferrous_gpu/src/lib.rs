//! `ferrous_gpu` — wgpu device/queue initialisation for FerrousEngine.
//!
//! This crate owns [`EngineContext`], the central container for the wgpu
//! `Instance`, `Adapter`, `Device`, and `Queue`.  It is the only crate in
//! the workspace that unconditionally depends on `wgpu`, keeping all other
//! crates free of GPU dependencies unless they opt in.
//!
//! `ferrous_core` re-exports `EngineContext` under `#[cfg(feature = "gpu")]`
//! for backward compatibility.

mod context;

pub use context::{ContextError, EngineContext};
