//! Shared helpers for the Pure2D integration tests.
//!
//! # Why the GPU lock exists
//!
//! Every test creates its own windowless [`EngineContext`] and `Renderer`, so
//! the GPU tests agree only by accident when they run concurrently.  Each
//! renderer owns a full pipeline bank and a 32 MB particle storage buffer; a
//! cold driver shader cache multiplies that cost, and fourteen devices in
//! flight at once exhaust the adapter or the D3D shader compiler.
//!
//! The observed failure (reproduced by running `pure2d_effects` and
//! `pure2d_masks` concurrently with many test threads) is a `wgpu` validation
//! error, not a wrong pixel:
//!
//! ```text
//! wgpu error: Validation Error
//! Caused by:
//!   In Device::create_buffer, label = 'Particles Storage Buffer'
//!     Not enough memory left.
//! ```
//!
//! and, on the DX12 backend, `out of memory during compilation` while creating
//! a render pipeline.  There is no shared engine state involved, so the fix is
//! to serialize the tests instead of chasing a race: Cargo already runs test
//! binaries one at a time, and a process-wide mutex covers the threads inside
//! each binary.
//!
//! [`EngineContext`]: https://docs.rs/ferrous_gpu

#![allow(dead_code)] // each test binary only uses part of this module

use std::sync::{Mutex, MutexGuard, PoisonError};

static GPU: Mutex<()> = Mutex::new(());

/// Held for as long as the guard lives; drop it at the end of the test.
#[derive(Debug)]
#[must_use = "the GPU stays serialized only while the guard is alive"]
pub struct GpuLock {
    _guard: MutexGuard<'static, ()>,
}

/// Waits until no other test in this process is using the GPU.
///
/// # Panics
///
/// Never: a poisoned mutex (a previous test panicked) is recovered on purpose,
/// because a failed test says nothing about the adapter's health.
pub fn lock() -> GpuLock {
    GpuLock {
        _guard: GPU.lock().unwrap_or_else(PoisonError::into_inner),
    }
}
