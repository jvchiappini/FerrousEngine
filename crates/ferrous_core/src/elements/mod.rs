//! A tiny, high‑performance entity/component "world" used by the editor
//! and later by games.  The goal is to remain simpler and faster than
//! existing engines such as Bevy while still providing enough structure
//! to organize scene data.
//!
//! The implementation is intentionally unoptimised – it uses a couple of
//! hash maps under the hood – but the API and data layout make it easy to
//! swap out the storage later for something lock‑free or memory‑packed.

// elements now only contains individual component definitions.  the former
// `World` type has been migrated to the `scene` module; consumers should use
// `ferrous_core::scene::World` instead.  keeping this file allows the `cube`
// submodule to live at the same path for compatibility with older code.

pub mod cube;
