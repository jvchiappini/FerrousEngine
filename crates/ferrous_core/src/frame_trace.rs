//! Per-frame stage timing (`FERROUS_TRACE_FRAMES=1`).
//!
//! The startup trace answers "what took so long before the first frame"; this
//! module answers "where does every frame go".  The runner opens a frame,
//! brackets the stages it controls, and closes it:
//!
//! ```no_run
//! use ferrous_core::frame_trace::{self, Stage};
//!
//! let _frame = frame_trace::begin_frame();
//! {
//!     let _stage = frame_trace::stage(Stage::AppUpdate);
//!     // app.update(...)
//! }
//! ```
//!
//! The trace is **off by default**. While it is off [`begin_frame`],
//! [`stage`] and [`record_event`] are one relaxed atomic load (and one branch)
//! each; guards allocate nothing and do not lock.  With
//! `FERROUS_TRACE_FRAMES=1` every frame is sampled and one JSON line is written
//! to stderr when the process reports (the runner reports at teardown):
//!
//! ```text
//! frame-trace {"event":"frame-trace","frames":600,"stages":{...},...}
//! ```
//!
//! Each stage reports the median, p90 and p99 in milliseconds.  When the
//! `alloc-stats` feature is on, the same JSON carries allocations and touched
//! bytes per frame (median/p90).
//!
//! # Stages
//!
//! [`Stage`] is a fixed list, not a string map: the hot path indexes an array.
//! `EventHandling` is fed by [`record_event`] from the winit callbacks; the
//! other stages are opened by the runner and the renderer with [`stage`].

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Mutex, MutexGuard, OnceLock};
use std::time::Instant;

use serde_json::{json, Map, Value};

/// Environment variable that enables the per-frame trace.
pub const ENABLE_ENV: &str = "FERROUS_TRACE_FRAMES";

/// The stages measured for every frame.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(usize)]
pub enum Stage {
    /// Winit event callbacks (input, resize, close).
    EventHandling = 0,
    /// `FerrousApp::update`.
    AppUpdate,
    /// `UiTree` layout/update.
    UiLayout,
    /// Application draw commands (`draw_ui` / `draw_3d`).
    CommandCollection,
    /// Renderer frame builder and batch coalescing.
    BatchBuild,
    /// Buffer uploads and bind-group preparation.
    PrepareUpload,
    /// Render-pass encoding (draw calls).
    Encode,
    /// `queue.submit`.
    Submit,
    /// Swapchain acquire + `present`.
    PresentWait,
}

impl Stage {
    /// Every stage, in report order.
    pub const ALL: [Stage; 9] = [
        Stage::EventHandling,
        Stage::AppUpdate,
        Stage::UiLayout,
        Stage::CommandCollection,
        Stage::BatchBuild,
        Stage::PrepareUpload,
        Stage::Encode,
        Stage::Submit,
        Stage::PresentWait,
    ];

    /// Stable report name.
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Stage::EventHandling => "event handling",
            Stage::AppUpdate => "app update",
            Stage::UiLayout => "ui layout",
            Stage::CommandCollection => "render-command collection",
            Stage::BatchBuild => "batch build",
            Stage::PrepareUpload => "prepare/upload",
            Stage::Encode => "encode",
            Stage::Submit => "submit",
            Stage::PresentWait => "present/wait",
        }
    }
}

const STAGE_COUNT: usize = 9;
const MAX_FRAMES: usize = 100_000;

static ENABLED: AtomicBool = AtomicBool::new(false);
static INITIALISED: OnceLock<()> = OnceLock::new();
static EVENT_NS: AtomicU64 = AtomicU64::new(0);
static REPORTED: AtomicBool = AtomicBool::new(false);
/// Frames whose swapchain image reached `present()`.
static PRESENTED: AtomicU64 = AtomicU64::new(0);
/// Main render submits (`queue.submit` for the frame encoder).
static SUBMITS: AtomicU64 = AtomicU64::new(0);

#[derive(Debug, Clone, Copy)]
struct FrameSample {
    stages: [f32; STAGE_COUNT],
    /// Allocations made by the frame thread.
    allocations: u64,
    /// Bytes touched by the frame thread.
    allocated_bytes: u64,
    /// Allocations made by other threads while the frame was open.
    worker_allocations: u64,
    /// Bytes touched by other threads while the frame was open.
    worker_bytes: u64,
}

impl Default for FrameSample {
    fn default() -> Self {
        Self {
            stages: [0.0; STAGE_COUNT],
            allocations: 0,
            allocated_bytes: 0,
            worker_allocations: 0,
            worker_bytes: 0,
        }
    }
}

#[derive(Default)]
struct State {
    frames: Vec<FrameSample>,
    current: FrameSample,
    started: bool,
    #[cfg(feature = "alloc-stats")]
    alloc_before: Option<crate::alloc_stats::Snapshot>,
    #[cfg(feature = "alloc-stats")]
    thread_alloc_before: Option<crate::alloc_stats::Snapshot>,
}

fn state() -> &'static Mutex<State> {
    static STATE: OnceLock<Mutex<State>> = OnceLock::new();
    STATE.get_or_init(|| Mutex::new(State::default()))
}

fn lock_state() -> MutexGuard<'static, State> {
    // A panic while tracing must not poison every later report.
    state()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

fn env_flag(name: &str) -> bool {
    std::env::var_os(name).is_some_and(|value| {
        let text = value.to_string_lossy();
        matches!(
            text.trim().to_ascii_lowercase().as_str(),
            "1" | "true" | "yes" | "on"
        )
    })
}

/// Whether the per-frame trace is enabled.
#[must_use]
pub fn enabled() -> bool {
    INITIALISED.get_or_init(|| {
        ENABLED.store(env_flag(ENABLE_ENV), Ordering::Relaxed);
    });
    ENABLED.load(Ordering::Relaxed)
}

/// Opens a frame.  Dropping the guard closes and records it.
#[must_use]
pub fn begin_frame() -> FrameGuard {
    if enabled() {
        let mut state = lock_state();
        if state.started {
            finish_locked(&mut state);
        }
        state.current = FrameSample::default();
        state.current.stages[Stage::EventHandling as usize] =
            EVENT_NS.swap(0, Ordering::Relaxed) as f32 / 1_000_000.0;
        state.started = true;
        #[cfg(feature = "alloc-stats")]
        {
            state.alloc_before = Some(crate::alloc_stats::snapshot());
            state.thread_alloc_before = Some(crate::alloc_stats::thread_snapshot());
        }
    }
    FrameGuard
}

/// Closes the current frame.  The runner uses [`begin_frame`]'s guard; this is
/// public for drivers that cannot keep the guard alive across an event-loop
/// boundary.
pub fn end_frame() {
    if !enabled() {
        return;
    }
    let mut state = lock_state();
    if state.started {
        finish_locked(&mut state);
    }
}

fn finish_locked(state: &mut State) {
    if !state.started {
        return;
    }
    #[allow(unused_mut)]
    let mut sample = std::mem::take(&mut state.current);
    #[cfg(feature = "alloc-stats")]
    if let (Some(before), Some(thread_before)) =
        (state.alloc_before.take(), state.thread_alloc_before.take())
    {
        let global = crate::alloc_stats::between(before, crate::alloc_stats::snapshot());
        let thread =
            crate::alloc_stats::between(thread_before, crate::alloc_stats::thread_snapshot());
        sample.allocations = thread.allocating_calls();
        sample.allocated_bytes = thread.touched_bytes();
        // Everything the process allocated that the frame thread did not.
        sample.worker_allocations = global.allocating_calls().saturating_sub(sample.allocations);
        sample.worker_bytes = global
            .touched_bytes()
            .saturating_sub(sample.allocated_bytes);
    }
    if state.frames.len() < MAX_FRAMES {
        state.frames.push(sample);
    }
    state.started = false;
}

/// An open frame; [`Drop`] closes it on every return path.
#[derive(Debug)]
pub struct FrameGuard;

impl Drop for FrameGuard {
    fn drop(&mut self) {
        end_frame();
    }
}

/// Brackets one stage.  Dropping the guard adds the elapsed time.
#[must_use]
pub fn stage(stage: Stage) -> StageGuard {
    StageGuard {
        stage,
        start: enabled().then(Instant::now),
    }
}

/// An open stage.
#[derive(Debug)]
pub struct StageGuard {
    stage: Stage,
    start: Option<Instant>,
}

impl Drop for StageGuard {
    fn drop(&mut self) {
        if let Some(start) = self.start {
            let millis = start.elapsed().as_secs_f64() * 1000.0;
            let mut state = lock_state();
            state.current.stages[self.stage as usize] += millis as f32;
        }
    }
}

/// Brackets a winit callback; the elapsed time goes to the next frame's
/// `EventHandling` stage.  Use it first thing in `window_event`.
#[must_use]
pub fn event_guard() -> EventGuard {
    EventGuard {
        start: Instant::now(),
        enabled: enabled(),
    }
}

/// An open event callback.
#[derive(Debug)]
pub struct EventGuard {
    start: Instant,
    enabled: bool,
}

impl Drop for EventGuard {
    fn drop(&mut self) {
        if self.enabled {
            record_event(self.start);
        }
    }
}

/// Records the time an event callback took, for the next frame's
/// `EventHandling` stage.  Allocation-free and lock-free.
pub fn record_event(start: Instant) {
    if !enabled() {
        return;
    }
    let nanos = start.elapsed().as_nanos().min(u128::from(u64::MAX)) as u64;
    EVENT_NS.fetch_add(nanos, Ordering::Relaxed);
}

/// Counts a presented frame.
///
/// The runner calls this after `SurfaceTexture::present`; an idle on-demand
/// app must leave this counter unchanged.
pub fn record_present() {
    if enabled() {
        PRESENTED.fetch_add(1, Ordering::Relaxed);
    }
}

/// Counts a main GPU submit.
///
/// The runner calls this around `queue.submit` for the frame encoder; an idle
/// on-demand app must leave this counter unchanged.
pub fn record_submit() {
    if enabled() {
        SUBMITS.fetch_add(1, Ordering::Relaxed);
    }
}

/// A nearest-rank percentile (`p` in `0.0..=1.0`).
#[must_use]
pub fn percentile(values: &[f32], p: f64) -> f32 {
    if values.is_empty() {
        return 0.0;
    }
    let mut sorted = values.to_vec();
    sorted.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let rank = ((p.clamp(0.0, 1.0) * sorted.len() as f64).ceil() as usize).max(1);
    sorted[rank.min(sorted.len()) - 1]
}

fn stats(values: &[f32]) -> Value {
    json!({
        "median_ms": f64::from(percentile(values, 0.5)),
        "p90_ms": f64::from(percentile(values, 0.9)),
        "p99_ms": f64::from(percentile(values, 0.99)),
        "samples": values.len(),
    })
}

/// Writes the frame summary once, as one `frame-trace {...}` line on stderr.
///
/// `gpu_passes` is the `(pass, milliseconds)` list from the renderer; pass an
/// empty slice when timestamps are unavailable. `gpu_unsupported` names the
/// passes that could not be timed.
pub fn report(gpu_passes: &[(String, f64)], gpu_unsupported: &[String]) {
    if !enabled() || REPORTED.swap(true, Ordering::Relaxed) {
        return;
    }
    let state = lock_state();
    let frames = state.frames.len();

    let mut stages = Map::new();
    for stage in Stage::ALL {
        let values: Vec<f32> = state
            .frames
            .iter()
            .map(|frame| frame.stages[stage as usize])
            .collect();
        stages.insert(stage.name().to_owned(), stats(&values));
    }

    let allocations: Vec<f32> = state
        .frames
        .iter()
        .map(|frame| frame.allocations as f32)
        .collect();
    let bytes: Vec<f32> = state
        .frames
        .iter()
        .map(|frame| frame.allocated_bytes as f32)
        .collect();
    let worker_allocations: Vec<f32> = state
        .frames
        .iter()
        .map(|frame| frame.worker_allocations as f32)
        .collect();
    let worker_bytes: Vec<f32> = state
        .frames
        .iter()
        .map(|frame| frame.worker_bytes as f32)
        .collect();

    let mut gpu = Map::new();
    for (pass, millis) in gpu_passes {
        gpu.insert(pass.clone(), json!({ "median_ms": millis }));
    }

    let payload = json!({
        "event": "frame-trace",
        "version": 1,
        "frames": frames,
        "presented": PRESENTED.load(Ordering::Relaxed),
        "gpu_submits": SUBMITS.load(Ordering::Relaxed),
        "alloc_stats": cfg!(feature = "alloc-stats"),
        "stages": stages,
        "allocations_per_frame": {
            "median": f64::from(percentile(&allocations, 0.5)),
            "p90": f64::from(percentile(&allocations, 0.9)),
        },
        "bytes_per_frame": {
            "median": f64::from(percentile(&bytes, 0.5)),
            "p90": f64::from(percentile(&bytes, 0.9)),
        },
        "worker_allocations_per_frame": {
            "median": f64::from(percentile(&worker_allocations, 0.5)),
            "p90": f64::from(percentile(&worker_allocations, 0.9)),
        },
        "worker_bytes_per_frame": {
            "median": f64::from(percentile(&worker_bytes, 0.5)),
            "p90": f64::from(percentile(&worker_bytes, 0.9)),
        },
        "gpu_passes": gpu,
        "gpu_unsupported": gpu_unsupported,
    });
    eprintln!("frame-trace {}", payload);
}

#[cfg(test)]
fn force_enabled(value: bool) {
    INITIALISED.get_or_init(|| {});
    ENABLED.store(value, Ordering::Relaxed);
}

#[cfg(test)]
fn clear_for_test() {
    let mut state = lock_state();
    state.frames.clear();
    state.current = FrameSample::default();
    state.started = false;
    EVENT_NS.store(0, Ordering::Relaxed);
    REPORTED.store(false, Ordering::Relaxed);
    PRESENTED.store(0, Ordering::Relaxed);
    SUBMITS.store(0, Ordering::Relaxed);
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn nearest_rank_percentiles_are_stable() {
        let values = [1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0, 10.0];
        assert_eq!(percentile(&values, 0.5), 5.0);
        assert_eq!(percentile(&values, 0.9), 9.0);
        assert_eq!(percentile(&values, 0.99), 10.0);
        assert_eq!(percentile(&[], 0.5), 0.0);
    }

    #[test]
    fn stages_and_frames_are_recorded() {
        force_enabled(true);
        clear_for_test();
        {
            let _frame = begin_frame();
            {
                let _stage = stage(Stage::AppUpdate);
                std::thread::sleep(Duration::from_millis(1));
            }
        }
        let state = lock_state();
        assert_eq!(state.frames.len(), 1);
        assert!(state.frames[0].stages[Stage::AppUpdate as usize] >= 0.5);
        drop(state);
        force_enabled(false);
    }

    #[test]
    fn a_disabled_trace_records_nothing() {
        force_enabled(false);
        clear_for_test();
        {
            let _frame = begin_frame();
            let _stage = stage(Stage::AppUpdate);
        }
        assert!(lock_state().frames.is_empty());
    }
}
