//! Lightweight scene helpers built on top of `elements`.

pub mod world;

pub use world::{Element, World};
// also expose the handle type so callers don't need to reach into the
// submodule.
pub use world::Handle;
