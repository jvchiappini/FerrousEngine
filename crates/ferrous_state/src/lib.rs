#![no_std]
//! Agnostic, zero-cost state and reactivity management for UI and Game engines.
//! Completely decoupled via traits to allow ECS or Local state backends.

extern crate alloc;

pub mod observable;
pub mod reactivity;

pub use observable::{Observable, Observer};
pub use reactivity::ReactivitySystem;
