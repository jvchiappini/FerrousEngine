//! Utilities for gathering simple process metrics such as CPU and memory
//! usage.  These are convenience wrappers around `sysinfo` so that
//! callers can simply write `ferrous_core::metrics::get_cpu_usage()`
//! without needing to bring a dependency into every crate.
//!
//! On **wasm32** all functions return `0.0` / `0` because the browser
//! does not expose OS-level process metrics.

// ─── Desktop implementation ────────────────────────────────────────────────
#[cfg(not(target_arch = "wasm32"))]
mod inner {
    use once_cell::sync::Lazy;
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::sync::Mutex;
    use sysinfo::{ProcessExt, System, SystemExt};

    static GLOBAL_SYS: Lazy<Mutex<System>> = Lazy::new(|| Mutex::new(System::new()));
    static LAST_REFRESH: AtomicU64 = AtomicU64::new(0);

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
        if now.saturating_sub(last) > 500 {
            sys.refresh_cpu();
            sys.refresh_memory();
            let pid = sysinfo::Pid::from(std::process::id() as usize);
            sys.refresh_process(pid);
            LAST_REFRESH.store(now, Ordering::Relaxed);
        }
        f(&mut sys)
    }

    pub fn get_cpu_usage() -> f32 {
        with_system(|sys| {
            let pid = sysinfo::Pid::from(std::process::id() as usize);
            sys.process(pid).map(|p| p.cpu_usage()).unwrap_or(0.0)
        })
    }

    pub fn get_ram_usage_bytes() -> u64 {
        with_system(|sys| {
            let pid = sysinfo::Pid::from(std::process::id() as usize);
            sys.process(pid).map(|p| p.memory()).unwrap_or(0)
        })
    }

    pub fn get_ram_usage_mb() -> f32 {
        get_ram_usage_bytes() as f32 / 1024.0 / 1024.0
    }

    pub fn get_virtual_memory_mb() -> f32 {
        get_virtual_memory_bytes() as f32 / 1024.0 / 1024.0
    }

    pub fn get_virtual_memory_bytes() -> u64 {
        with_system(|sys| {
            let pid = sysinfo::Pid::from(std::process::id() as usize);
            sys.process(pid).map(|p| p.virtual_memory()).unwrap_or(0)
        })
    }
}

// ─── wasm32 stubs ──────────────────────────────────────────────────────────
#[cfg(target_arch = "wasm32")]
mod inner {
    /// Not available in the browser — always returns `0.0`.
    pub fn get_cpu_usage() -> f32 { 0.0 }
    pub fn get_ram_usage_bytes() -> u64 { 0 }
    pub fn get_ram_usage_mb() -> f32 { 0.0 }
    pub fn get_virtual_memory_mb() -> f32 { 0.0 }
    pub fn get_virtual_memory_bytes() -> u64 { 0 }
}

// ─── Public API (uniform on all platforms) ─────────────────────────────────
pub use inner::{
    get_cpu_usage,
    get_ram_usage_bytes,
    get_ram_usage_mb,
    get_virtual_memory_bytes,
    get_virtual_memory_mb,
};
