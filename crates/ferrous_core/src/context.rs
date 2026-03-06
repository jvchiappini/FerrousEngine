// Backward-compatibility shim.
// EngineContext now lives in `ferrous_gpu`; this module re-exports it so
// that existing `ferrous_core::context::EngineContext` paths keep compiling.
// Deprecated — import from `ferrous_gpu` directly in new code.
pub use ferrous_gpu::{ContextError, EngineContext};
