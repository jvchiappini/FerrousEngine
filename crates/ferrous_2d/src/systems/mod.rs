//! ECS preparation systems for the 2D render path.
//!
//! **Deprecated.** These functions walk ECS queries and push instances into the
//! 2D batchers, but they are **not registered** in the engine's
//! `StagedScheduler` (nor in any other scheduler), so they have no effect: the
//! live 2D path is the immediate-mode renderer. They are kept only for API
//! compatibility and will be removed in a future release. See
//! `ferrous_2d/README.md` and the `Renderer` immediate-mode entry points.

/// ECS→batcher conversion for sprites, shapes and paths. Deprecated.
pub mod batching;
pub use batching::*;
