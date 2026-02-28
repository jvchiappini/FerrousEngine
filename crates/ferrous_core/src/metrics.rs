//! Utilities for gathering simple process metrics such as CPU and memory
//! usage.  These are convenience wrappers around `sysinfo` so that
//! callers can simply write `ferrous_core::metrics::get_cpu_usage()`
//! without needing to bring a dependency into every crate.

use once_cell::sync::Lazy;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;
use sysinfo::{ProcessExt, System, SystemExt};

// We keep a single `System` instance rather than recreating one every
// call.  `sysinfo` computes CPU usage as a delta between two
// consecutive refreshes, so if you always build a fresh `System` the
// value will constantly be zero.  A shared `Mutex` is sufficient since
// our library will be polled from a single thread (the main loop), but
// the API is thread‑safe regardless.

static GLOBAL_SYS: Lazy<Mutex<System>> = Lazy::new(|| Mutex::new(System::new()));
static LAST_REFRESH: AtomicU64 = AtomicU64::new(0);

/// Helper to lock and refresh the process info before invoking the
/// closure.
fn with_system<F, R>(f: F) -> R
where
    F: FnOnce(&mut System) -> R,
{
    let mut sys = GLOBAL_SYS.lock().unwrap();
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis() as u64;
    let last = LAST_REFRESH.load(Ordering::Relaxed);

    // limit refresh to 500ms to avoid thrashing OS calls 3x per frame
    if now.saturating_sub(last) > 500 {
        sys.refresh_cpu();
        sys.refresh_memory();
        let pid = sysinfo::Pid::from(std::process::id() as usize);
        sys.refresh_process(pid);
        LAST_REFRESH.store(now, Ordering::Relaxed);
    }
    // we avoid iterating over all system processes which consumes huge RAM and CPU.
    f(&mut sys)
}

/// Returns the current process CPU usage as a percentage of a single
/// logical core.  The first invocation after startup may still return
/// `0.0`, but subsequent calls will reflect recent CPU activity.
pub fn get_cpu_usage() -> f32 {
    with_system(|sys| {
        let pid = sysinfo::Pid::from(std::process::id() as usize);
        sys.process(pid).map(|p| p.cpu_usage()).unwrap_or(0.0)
    })
}

/// Return resident (physical) memory used by the current process in
/// **bytes**.  This calls `sys.refresh_process` on every invocation so
/// it is cheap enough to poll once per frame for in‑engine diagnostics.
pub fn get_ram_usage_bytes() -> u64 {
    with_system(|sys| {
        let pid = sysinfo::Pid::from(std::process::id() as usize);
        // `sysinfo` returns the resident memory size in **bytes** on
        // Windows (and on most other platforms).  earlier versions of
        // this code assumed kilobytes and multiplied by 1024, which
        // produced values in the hundreds of gigabytes.  simply return
        // the raw value here and convert to MB in the caller if
        // desired.
        sys.process(pid).map(|p| p.memory()).unwrap_or(0)
    })
}

/// Same as [`get_ram_usage_bytes`], but returns a floating point value
/// in **megabytes**.  This convenience is useful when the caller just
/// wants to display the number directly in a UI.
pub fn get_ram_usage_mb() -> f32 {
    get_ram_usage_bytes() as f32 / 1024.0 / 1024.0
}

/// Virtual memory (address space) in megabytes.
pub fn get_virtual_memory_mb() -> f32 {
    get_virtual_memory_bytes() as f32 / 1024.0 / 1024.0
}

/// Return the virtual memory size (address space) of the current
/// process in bytes.  This is often larger than resident memory and
/// includes swapped/unused pages.
pub fn get_virtual_memory_bytes() -> u64 {
    with_system(|sys| {
        let pid = sysinfo::Pid::from(std::process::id() as usize);
        // same as above: `virtual_memory()` is already in bytes.
        sys.process(pid).map(|p| p.virtual_memory()).unwrap_or(0)
    })
}
