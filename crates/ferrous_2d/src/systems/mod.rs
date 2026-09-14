//! ECS preparation systems for the 2D render path.
//!
//! These functions walk ECS queries and push instances into the 2D batchers.
//! They are **not registered** in the engine's `StagedScheduler`; the live 2D
//! path is the immediate-mode renderer. See `ferrous_2d/README.md`.

/// ECS→batcher conversion for sprites, shapes and paths.
pub mod batching;
pub use batching::*;
