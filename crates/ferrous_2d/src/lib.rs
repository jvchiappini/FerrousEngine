//! 2D Subsystem for Ferrous Engine
//! 
//! Handles professional 2D rendering, specifically batched sprites,
//! 2D cameras, tilemaps (planned) and zero-cost abstraction for games.

pub mod components;
pub mod systems;
pub mod render;

// Re-exports
pub use components::*;
// To be implemented:
pub use systems::*;
pub use render::*;
