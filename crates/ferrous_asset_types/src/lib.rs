#![deny(missing_docs)]

//! `ferrous_asset_types` contains the fundamental interfaces used by the
//! asset pipeline: the [`Asset`] trait as well as the type-safe
//! [`AssetHandle`] and corresponding [`AssetState`].
//!
//! This crate has **no GPU dependencies** and can be depended on by tools,
//! unit tests, or any code that only needs the asset _types_ without the full
//! loading implementation.

pub mod asset_trait;
pub mod handle;

// Re-export the two most commonly-used items at crate root for convenience.
pub use asset_trait::Asset;
pub use handle::{AssetHandle, AssetState};
