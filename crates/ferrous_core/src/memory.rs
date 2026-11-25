//! Byte-level GPU and CPU memory inventory.
//!
//! Every renderer subsystem registers the buffers, textures and CPU-side
//! copies it keeps here, once, at creation:
//!
//! ```no_run
//! use ferrous_core::memory::{self, Subsystem};
//!
//! memory::register_gpu("2d:shape instances", 1 << 20, Subsystem::Renderer2d);
//! memory::register_cpu("font:Inter 400 pixels", 4 << 20, Subsystem::Fonts);
//! ```
//!
//! Registration is not a per-frame operation: it happens when a resource is
//! created or resized, and it replaces the byte count of the entry with the
//! same label.  [`report`] returns totals per subsystem and the largest items,
//! which is what `Renderer::memory_report` exposes to the bench harness.
//!
//! The registry is deliberately explicit instead of wrapping `wgpu::Device`:
//! the engine owns few resource-creation sites and a wrapper would infect every
//! public signature.  A resource that is not registered is a P0 instrumentation
//! gap, not a silent success; the coverage list lives in the phase report.

use std::collections::HashMap;
use std::sync::{Mutex, MutexGuard, OnceLock};

use serde::Serialize;

/// The subsystem that owns a resource.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
pub enum Subsystem {
    /// `ferrous_2d` instance, vertex, index and uniform buffers.
    Renderer2d,
    /// `ferrous_ui_render` / `ferrous_gui` GPU resources.
    Gui,
    /// Engine render passes and their intermediate targets.
    Renderer,
    /// The swapchain-sized renderer attachments: colour resolve, MSAA colour
    /// and depth.
    RenderTarget,
    /// Font atlases (GPU texture and CPU pixels/metrics).
    Fonts,
    /// Decoded images, video frames and their GPU textures.
    Assets,
    /// Popup windows: their per-popup GUI buffers and swapchains.
    Popups,
    /// Everything else owned by the engine.
    Engine,
    /// An application-owned resource registered through this API.
    Other,
}

impl Subsystem {
    /// Stable report name.
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Subsystem::Renderer2d => "renderer_2d",
            Subsystem::Gui => "gui",
            Subsystem::Renderer => "renderer",
            Subsystem::RenderTarget => "render_target",
            Subsystem::Fonts => "fonts",
            Subsystem::Assets => "assets",
            Subsystem::Popups => "popups",
            Subsystem::Engine => "engine",
            Subsystem::Other => "other",
        }
    }
}

/// One registered resource.
#[derive(Debug, Clone, Serialize)]
pub struct Entry {
    /// Human-readable owner/label.
    pub label: String,
    /// Bytes held by this resource.
    pub bytes: u64,
    /// Owning subsystem.
    pub subsystem: Subsystem,
    /// `true` for a CPU-side copy, `false` for GPU memory.
    pub cpu: bool,
}

/// Totals for one subsystem.
#[derive(Debug, Clone, Serialize)]
pub struct SubsystemTotal {
    /// Subsystem name.
    pub subsystem: String,
    /// GPU bytes.
    pub gpu_bytes: u64,
    /// CPU bytes.
    pub cpu_bytes: u64,
    /// GPU + CPU bytes.
    pub total_bytes: u64,
}

/// A memory inventory snapshot.
#[derive(Debug, Clone, Default, Serialize)]
pub struct Report {
    /// Total GPU bytes.
    pub gpu_bytes: u64,
    /// Total CPU-side bytes.
    pub cpu_bytes: u64,
    /// GPU + CPU bytes.
    pub total_bytes: u64,
    /// Totals per subsystem, largest first.
    pub by_subsystem: Vec<SubsystemTotal>,
    /// The largest entries, descending.
    pub largest: Vec<Entry>,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct Key {
    label: String,
    subsystem: Subsystem,
    cpu: bool,
}

fn entries() -> &'static Mutex<HashMap<Key, Entry>> {
    static ENTRIES: OnceLock<Mutex<HashMap<Key, Entry>>> = OnceLock::new();
    ENTRIES.get_or_init(|| Mutex::new(HashMap::new()))
}

fn lock_entries() -> MutexGuard<'static, HashMap<Key, Entry>> {
    entries()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

fn register(label: String, bytes: u64, subsystem: Subsystem, cpu: bool) {
    if bytes == 0 {
        return;
    }
    let key = Key {
        label: label.clone(),
        subsystem,
        cpu,
    };
    lock_entries().insert(
        key,
        Entry {
            label,
            bytes,
            subsystem,
            cpu,
        },
    );
}

/// Registers or updates a GPU resource.  The label is the identity: a resize
/// replaces the previous size instead of adding a second entry.
pub fn register_gpu(label: impl Into<String>, bytes: u64, subsystem: Subsystem) {
    register(label.into(), bytes, subsystem, false);
}

/// Registers or updates a CPU-side copy (atlas pixels, decoded images, glyph
/// tables, caches).
pub fn register_cpu(label: impl Into<String>, bytes: u64, subsystem: Subsystem) {
    register(label.into(), bytes, subsystem, true);
}

/// Removes an entry by label/subsystem/side.
pub fn unregister_gpu(label: &str, subsystem: Subsystem) {
    lock_entries().remove(&Key {
        label: label.to_owned(),
        subsystem,
        cpu: false,
    });
}

/// Removes a CPU entry by label/subsystem.
pub fn unregister_cpu(label: &str, subsystem: Subsystem) {
    lock_entries().remove(&Key {
        label: label.to_owned(),
        subsystem,
        cpu: true,
    });
}

/// Builds a report with the `largest_count` biggest entries.
#[must_use]
pub fn report(largest_count: usize) -> Report {
    let entries = lock_entries();
    let mut report = Report::default();
    let mut per_subsystem: HashMap<Subsystem, (u64, u64)> = HashMap::new();
    let mut all: Vec<Entry> = entries.values().cloned().collect();

    for entry in &all {
        if entry.cpu {
            report.cpu_bytes = report.cpu_bytes.saturating_add(entry.bytes);
        } else {
            report.gpu_bytes = report.gpu_bytes.saturating_add(entry.bytes);
        }
        let slot = per_subsystem.entry(entry.subsystem).or_default();
        if entry.cpu {
            slot.1 = slot.1.saturating_add(entry.bytes);
        } else {
            slot.0 = slot.0.saturating_add(entry.bytes);
        }
    }
    report.total_bytes = report.gpu_bytes.saturating_add(report.cpu_bytes);

    for (subsystem, (gpu_bytes, cpu_bytes)) in per_subsystem {
        report.by_subsystem.push(SubsystemTotal {
            subsystem: subsystem.name().to_owned(),
            gpu_bytes,
            cpu_bytes,
            total_bytes: gpu_bytes.saturating_add(cpu_bytes),
        });
    }
    report
        .by_subsystem
        .sort_by_key(|entry| std::cmp::Reverse(entry.total_bytes));

    all.sort_by_key(|entry| std::cmp::Reverse(entry.bytes));
    all.truncate(largest_count);
    report.largest = all;
    report
}

/// The report as one JSON object (for the bench's `memory-report` line).
#[must_use]
pub fn report_json(largest_count: usize) -> String {
    serde_json::to_string(&report(largest_count))
        .unwrap_or_else(|_| "{\"gpu_bytes\":0,\"cpu_bytes\":0,\"total_bytes\":0}".to_owned())
}

/// Every registered resource, sorted by label then subsystem.
///
/// [`report`] truncates its `largest` list, which is right for a summary but
/// hides a small entry behind bigger neighbours. A caller that needs one named
/// resource (a popup's offscreen target, a specific atlas) reads the full,
/// deterministic inventory here instead.
#[must_use]
pub fn inventory() -> Vec<Entry> {
    let mut all: Vec<Entry> = lock_entries().values().cloned().collect();
    all.sort_by(|a, b| {
        a.label
            .cmp(&b.label)
            .then_with(|| a.subsystem.name().cmp(b.subsystem.name()))
    });
    all
}

/// Clears the registry.  Tests only.
pub fn clear() {
    lock_entries().clear();
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_lock() -> MutexGuard<'static, ()> {
        static LOCK: Mutex<()> = Mutex::new(());
        LOCK.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    #[test]
    fn totals_and_largest_are_reported() {
        let _lock = test_lock();
        clear();
        register_gpu("a", 100, Subsystem::Renderer2d);
        register_gpu("b", 300, Subsystem::Gui);
        register_cpu("c", 50, Subsystem::Fonts);
        let report = report(2);
        assert_eq!(report.gpu_bytes, 400);
        assert_eq!(report.cpu_bytes, 50);
        assert_eq!(report.total_bytes, 450);
        assert_eq!(report.largest.len(), 2);
        assert_eq!(report.largest[0].label, "b");
        clear();
    }

    #[test]
    fn the_same_label_replaces_the_size() {
        let _lock = test_lock();
        clear();
        register_gpu("buffer", 100, Subsystem::Renderer2d);
        register_gpu("buffer", 250, Subsystem::Renderer2d);
        let report = report(10);
        assert_eq!(report.gpu_bytes, 250);
        assert_eq!(report.largest.len(), 1);
        clear();
    }

    #[test]
    fn popups_are_a_their_own_subsystem() {
        let _lock = test_lock();
        clear();
        assert_eq!(Subsystem::Popups.name(), "popups");
        register_gpu("popup:0:instances", 1024, Subsystem::Popups);
        register_gpu("popup:0:svg", 2048, Subsystem::Popups);
        let report = report(10);
        assert_eq!(report.by_subsystem.len(), 1);
        assert_eq!(report.by_subsystem[0].subsystem, "popups");
        assert_eq!(report.by_subsystem[0].gpu_bytes, 3072);
        clear();
    }

    #[test]
    fn the_inventory_keeps_small_named_entries() {
        let _lock = test_lock();
        clear();
        register_gpu("big", 1_000_000, Subsystem::Gui);
        register_gpu("popup:3:offscreen", 512, Subsystem::Popups);
        let small = report(1);
        assert_eq!(small.largest.len(), 1);
        assert_eq!(small.largest[0].label, "big");
        let inventory = inventory();
        let offscreen = inventory
            .iter()
            .find(|entry| entry.label == "popup:3:offscreen")
            .expect("the small offscreen entry is in the inventory");
        assert_eq!(offscreen.bytes, 512);
        assert_eq!(offscreen.subsystem, Subsystem::Popups);
        clear();
    }
}
