//! Process startup tracing.
//!
//! Records monotonic marks and spans from the first instruction of the
//! application to the first presented frame, so a slow start can be attributed
//! to a named phase instead of a guessed `eprintln!`. Spans are only recorded
//! **before** the first presented frame: an application may open a span every
//! frame, and keeping every one of those would grow the trace without bound.
//! Marks keep being recorded after the first frame so the close/teardown
//! timings still appear in the closing report.
//!
//! The trace is **off by default**. While it is off every call is a single
//! relaxed atomic load plus a branch: [`mark`] and [`span`] allocate nothing
//! and formatting closures passed to [`span_with`] are not called.
//!
//! Enable it with the `FERROUS_TRACE_STARTUP=1` environment variable or with
//! [`crate::AppConfig::with_startup_trace`]. `FERROUS_TRACE_STARTUP_JSON=1`
//! additionally prints one machine-readable line per report, which is what
//! `scripts/startup_bench.py` parses.
//!
//! ```no_run
//! use ferrous_core::startup;
//!
//! startup::begin_from_env();
//! startup::mark("app: begin");
//! {
//!     let _span = startup::span("app: load settings");
//!     // ...
//! }
//! ```
//!
//! Reports are emitted to stderr: a human table at the first presented frame
//! and another one after teardown. The JSON lines carry the
//! `startup-trace`/`startup-close` prefixes so a tool can tell them apart
//! from the table and from the engine's other logging.
//!
//! # Timestamps
//!
//! Times are measured with a monotonic clock from the first [`begin`] call of
//! the process. An application that wants the whole of `main` measured calls
//! [`begin_from_env`] as its first statement; the engine then reuses that
//! origin when `App::run` starts.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, MutexGuard, OnceLock};

#[cfg(not(target_arch = "wasm32"))]
use std::time::Instant;
#[cfg(target_arch = "wasm32")]
use web_time::Instant;

/// Version of the trace record format. Bumped when a field changes meaning.
pub const TRACE_VERSION: u32 = 1;

/// Environment variable that enables the trace (`1`, `true`, `yes` or `on`).
pub const ENABLE_ENV: &str = "FERROUS_TRACE_STARTUP";
/// Environment variable that adds the JSON lines to the table.
pub const JSON_ENV: &str = "FERROUS_TRACE_STARTUP_JSON";

static ENABLED: AtomicBool = AtomicBool::new(false);
/// Whether spans are still recorded. Cleared by the first-frame report; marks
/// keep being recorded for the close report.
static RECORD_SPANS: AtomicBool = AtomicBool::new(false);
static FIRST_REPORTED: AtomicBool = AtomicBool::new(false);
static CLOSE_REPORTED: AtomicBool = AtomicBool::new(false);

fn origin() -> &'static Instant {
    static ORIGIN: OnceLock<Instant> = OnceLock::new();
    ORIGIN.get_or_init(Instant::now)
}

fn records() -> &'static Mutex<Vec<Record>> {
    static RECORDS: OnceLock<Mutex<Vec<Record>>> = OnceLock::new();
    RECORDS.get_or_init(|| Mutex::new(Vec::new()))
}

/// One recorded point or span, in the order it was recorded.
#[derive(Debug, Clone)]
enum Record {
    /// A named instant.
    Mark { name: String, at: Instant },
    /// A named scope that has already ended.
    Span {
        name: String,
        start: Instant,
        end: Instant,
    },
}

impl Record {
    /// Latest instant the record covers, for the total.
    fn end(&self) -> Instant {
        match self {
            Self::Mark { at, .. } => *at,
            Self::Span { end, .. } => *end,
        }
    }
}

/// One mark in a [`Trace`], in milliseconds since the trace origin.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct Mark {
    /// Mark name, for example `engine: adapter`.
    pub name: String,
    /// Time since the trace origin, in milliseconds.
    pub at_ms: f64,
}

/// One completed span in a [`Trace`], in milliseconds since the trace origin.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct SpanRecord {
    /// Span name, for example `motioner: font Inter 400`.
    pub name: String,
    /// Time the span opened, in milliseconds since the trace origin.
    pub start_ms: f64,
    /// Time the span closed, in milliseconds since the trace origin.
    pub end_ms: f64,
}

impl SpanRecord {
    /// Span duration in milliseconds.
    #[must_use]
    pub fn duration_ms(&self) -> f64 {
        (self.end_ms - self.start_ms).max(0.0)
    }
}

/// A snapshot of the trace taken by [`trace`].
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct Trace {
    /// Format version, see [`TRACE_VERSION`].
    pub version: u32,
    /// Time since the origin of the last recorded mark or span end.
    pub total_ms: f64,
    /// Marks in the order they were recorded.
    pub marks: Vec<Mark>,
    /// Completed spans in the order they were recorded.
    pub spans: Vec<SpanRecord>,
}

/// Whether the trace is enabled in this process.
///
/// The check is a single relaxed atomic load, safe to call on every frame.
#[must_use]
pub fn enabled() -> bool {
    ENABLED.load(Ordering::Relaxed)
}

/// Enables the trace.
///
/// Call this as early as possible: the first call fixes the trace origin,
/// which is the zero of every reported time. The first call in a process wins
/// even when tracing was already enabled, so an application may call
/// [`begin_from_env`] at the top of `main` and the engine may call [`begin`]
/// again with its own configuration without moving the origin.
///
/// `app_requested` is the application's own switch (the engine passes its
/// `AppConfig::startup_trace` field); [`ENABLE_ENV`] enables the trace in
/// either case.
pub fn begin(app_requested: bool) {
    if !app_requested && !env_flag(ENABLE_ENV) {
        return;
    }
    let _ = origin();
    ENABLED.store(true, Ordering::Relaxed);
    RECORD_SPANS.store(true, Ordering::Relaxed);
}

/// [`begin`] driven only by [`ENABLE_ENV`].
///
/// Applications call this once at the top of `main` so the trace starts with
/// the process's first instruction.
pub fn begin_from_env() {
    begin(false);
}

/// Records a named instant.
///
/// The name is free-form; by convention the engine uses `engine: <phase>` and
/// applications use `<app>: <phase>`. Nothing is allocated and nothing is
/// recorded when the trace is disabled.
pub fn mark(name: &str) {
    if !enabled() {
        return;
    }
    push(Record::Mark {
        name: name.to_owned(),
        at: Instant::now(),
    });
}

/// Opens a named span that is recorded when the guard is dropped.
///
/// Nothing is recorded when the trace is disabled.
#[must_use]
pub fn span(name: impl Into<String>) -> Span {
    if !enabled() || !RECORD_SPANS.load(Ordering::Relaxed) {
        return Span::none();
    }
    Span::open(name.into())
}

/// [`span`] with a label built only when the trace is enabled.
///
/// Use this for labels that need formatting, so a disabled trace does not pay
/// for the `format!`:
///
/// ```no_run
/// # use ferrous_core::startup;
/// # let family = "Inter";
/// # let weight = 400;
/// let _span = startup::span_with(|| format!("app: font {family} {weight}"));
/// ```
#[must_use]
pub fn span_with(label: impl FnOnce() -> String) -> Span {
    if !enabled() || !RECORD_SPANS.load(Ordering::Relaxed) {
        return Span::none();
    }
    Span::open(label())
}

/// A running [`span`]; records its duration when dropped.
#[derive(Debug)]
pub struct Span {
    name: Option<String>,
    start: Option<Instant>,
}

impl Span {
    /// A guard that records nothing.
    fn none() -> Self {
        Self {
            name: None,
            start: None,
        }
    }

    /// A guard that records one span when dropped.
    fn open(name: String) -> Self {
        Self {
            name: Some(name),
            start: Some(Instant::now()),
        }
    }
}

impl Drop for Span {
    fn drop(&mut self) {
        if let (Some(name), Some(start)) = (self.name.take(), self.start.take()) {
            push(Record::Span {
                name,
                start,
                end: Instant::now(),
            });
        }
    }
}

/// Records `engine: first frame presented` and prints the startup report once.
///
/// The runner calls this after the first successful `frame.present()`. The
/// mark is taken before the report is formatted, so printing does not inflate
/// the measured time. Later calls do nothing.
pub fn first_frame_presented() {
    if !enabled() || FIRST_REPORTED.swap(true, Ordering::Relaxed) {
        return;
    }
    mark("engine: first frame presented");
    RECORD_SPANS.store(false, Ordering::Relaxed);
    emit("startup trace", "startup-trace", "first-present");
}

/// Prints the first-present report without recording a mark.
///
/// The engine runner marks `engine: first frame presented` as soon as
/// `frame.present()` succeeds, shows the window and records
/// `engine: window visible`, and then calls this so both marks are in the same
/// report in presentation order. Applications that only need the combined
/// mark + report call [`first_frame_presented`]. Later calls do nothing.
pub fn report_first_frame() {
    if !enabled() || FIRST_REPORTED.swap(true, Ordering::Relaxed) {
        return;
    }
    RECORD_SPANS.store(false, Ordering::Relaxed);
    emit("startup trace", "startup-trace", "first-present");
}

/// Records `engine: teardown done` and prints the closing report once.
///
/// The runner calls this after the window and the GPU state are dropped, so
/// the mark covers the whole teardown. Later calls do nothing.
pub fn teardown() {
    if !enabled() || CLOSE_REPORTED.swap(true, Ordering::Relaxed) {
        return;
    }
    mark("engine: teardown done");
    emit("startup trace: close", "startup-close", "exit");
}

/// A snapshot of everything recorded so far.
#[must_use]
pub fn trace() -> Trace {
    let origin = *origin();
    let guard = records();
    let records = lock(guard);
    let mut marks = Vec::new();
    let mut spans = Vec::new();
    let mut latest = origin;
    for record in records.iter() {
        if record.end() > latest {
            latest = record.end();
        }
        match record {
            Record::Mark { name, at } => marks.push(Mark {
                name: name.clone(),
                at_ms: at.saturating_duration_since(origin).as_secs_f64() * 1000.0,
            }),
            Record::Span { name, start, end } => spans.push(SpanRecord {
                name: name.clone(),
                start_ms: start.saturating_duration_since(origin).as_secs_f64() * 1000.0,
                end_ms: end.saturating_duration_since(origin).as_secs_f64() * 1000.0,
            }),
        }
    }
    Trace {
        version: TRACE_VERSION,
        total_ms: latest.saturating_duration_since(origin).as_secs_f64() * 1000.0,
        marks,
        spans,
    }
}

/// Clears every recorded mark and span and disables the trace.
///
/// Only tests need this: the trace is process-wide state.
#[cfg(test)]
pub(crate) fn reset() {
    ENABLED.store(false, Ordering::Relaxed);
    RECORD_SPANS.store(false, Ordering::Relaxed);
    FIRST_REPORTED.store(false, Ordering::Relaxed);
    CLOSE_REPORTED.store(false, Ordering::Relaxed);
    lock(records()).clear();
}

/// Pushes one record, ignoring a poisoned lock: a panic while tracing must
/// not turn into a second panic during unwinding.
fn push(record: Record) {
    lock(records()).push(record);
}

/// Locks the record list, recovering from a poisoned lock.
fn lock(guard: &'static Mutex<Vec<Record>>) -> MutexGuard<'static, Vec<Record>> {
    guard
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

/// Whether an environment variable holds an affirmative value.
fn env_flag(name: &str) -> bool {
    std::env::var(name).is_ok_and(|value| {
        matches!(
            value.trim().to_ascii_lowercase().as_str(),
            "1" | "true" | "yes" | "on"
        )
    })
}

/// Writes the report: a human table plus, when [`JSON_ENV`] is set, one JSON
/// line prefixed with `event` for tools.
fn emit(title: &str, event: &str, stage: &str) {
    let trace = trace();
    let mut out = format!("{title} (ms since trace start)\n");
    out.push_str("  mark                                     at       delta\n");
    let mut previous = 0.0_f64;
    for mark in &trace.marks {
        let delta = (mark.at_ms - previous).max(0.0);
        previous = mark.at_ms;
        out.push_str(&format!(
            "  {name:38} {at:9.2} {delta:11.2}\n",
            name = mark.name,
            at = mark.at_ms,
            delta = delta
        ));
    }
    for span in &trace.spans {
        out.push_str(&format!(
            "  {name:38} {start:9.2} {duration:11.2} (span)\n",
            name = span.name,
            start = span.start_ms,
            duration = span.duration_ms()
        ));
    }
    out.push_str(&format!(
        "  {name:38} {at:9.2}\n",
        name = "total",
        at = trace.total_ms
    ));
    // Write failures (a closed pipe) must not turn a diagnostic into a panic.
    let _ = std::io::Write::write_all(&mut std::io::stderr(), out.as_bytes());

    if env_flag(JSON_ENV) {
        #[derive(serde::Serialize)]
        struct Envelope<'a> {
            event: &'a str,
            stage: &'a str,
            trace: &'a Trace,
        }
        let envelope = Envelope {
            event,
            stage,
            trace: &trace,
        };
        if let Ok(json) = serde_json::to_string(&envelope) {
            let line = format!("{event} {json}\n");
            let _ = std::io::Write::write_all(&mut std::io::stderr(), line.as_bytes());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::MutexGuard;

    /// The trace is process-wide state; the tests of this module share it.
    fn test_lock() -> MutexGuard<'static, ()> {
        static LOCK: Mutex<()> = Mutex::new(());
        LOCK.lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    #[test]
    fn disabled_trace_records_nothing() {
        let _lock = test_lock();
        reset();
        assert!(!enabled());
        mark("test: ignored");
        {
            let _span = span("test: ignored span");
        }
        assert!(trace().marks.is_empty());
        assert!(trace().spans.is_empty());
    }

    #[test]
    fn disabled_trace_does_not_build_computed_labels() {
        let _lock = test_lock();
        reset();
        let called = std::cell::Cell::new(false);
        {
            let _span = span_with(|| {
                called.set(true);
                "test: label".to_owned()
            });
        }
        assert!(!called.get());
    }

    #[test]
    fn marks_are_recorded_in_order_with_monotonic_times() {
        let _lock = test_lock();
        reset();
        begin(true);
        mark("test: one");
        mark("test: two");
        let trace = trace();
        assert_eq!(trace.version, TRACE_VERSION);
        assert_eq!(trace.marks.len(), 2);
        assert_eq!(trace.marks[0].name, "test: one");
        assert_eq!(trace.marks[1].name, "test: two");
        assert!(trace.marks[0].at_ms <= trace.marks[1].at_ms);
        assert!(trace.total_ms >= trace.marks[1].at_ms);
        reset();
    }

    #[test]
    fn spans_stop_at_the_first_frame_but_close_marks_remain() {
        let _lock = test_lock();
        reset();
        begin(true);
        {
            let _span = span("test: startup span");
        }
        report_first_frame();
        {
            let _span = span("test: per-frame span (must be ignored)");
        }
        mark("test: close mark");
        let trace = trace();
        assert_eq!(trace.spans.len(), 1, "only the startup span is recorded");
        assert_eq!(trace.spans[0].name, "test: startup span");
        assert!(
            trace
                .marks
                .iter()
                .any(|mark| mark.name == "test: close mark"),
            "marks keep being recorded for the close report"
        );
        assert!(
            !trace
                .spans
                .iter()
                .any(|span| span.name.contains("per-frame")),
            "the per-frame span was not recorded"
        );
        reset();
    }

    #[test]
    fn a_dropped_span_records_its_duration() {
        let _lock = test_lock();
        reset();
        begin(true);
        {
            let _span = span("test: span");
        }
        let trace = trace();
        assert_eq!(trace.spans.len(), 1);
        let span = &trace.spans[0];
        assert_eq!(span.name, "test: span");
        assert!(span.start_ms <= span.end_ms);
        assert!(span.duration_ms() >= 0.0);
        reset();
    }

    #[test]
    fn computed_span_labels_are_built_when_enabled() {
        let _lock = test_lock();
        reset();
        begin(true);
        {
            let _span = span_with(|| "test: computed".to_owned());
        }
        assert_eq!(trace().spans[0].name, "test: computed");
        reset();
    }

    #[test]
    fn a_span_closed_after_the_report_appears_in_the_next_snapshot() {
        let _lock = test_lock();
        reset();
        begin(true);
        let guard = span("test: open");
        assert!(trace().spans.is_empty());
        drop(guard);
        assert_eq!(trace().spans.len(), 1);
        reset();
    }

    #[test]
    fn reports_are_emitted_once() {
        let _lock = test_lock();
        reset();
        begin(true);
        first_frame_presented();
        first_frame_presented();
        assert_eq!(
            trace()
                .marks
                .iter()
                .filter(|mark| mark.name == "engine: first frame presented")
                .count(),
            1
        );
        teardown();
        teardown();
        assert_eq!(
            trace()
                .marks
                .iter()
                .filter(|mark| mark.name == "engine: teardown done")
                .count(),
            1
        );
        reset();
    }

    #[test]
    fn the_trace_serializes_with_its_marks() {
        let _lock = test_lock();
        reset();
        begin(true);
        mark("test: json");
        let json = serde_json::to_string(&trace()).unwrap();
        assert!(json.contains("\"marks\""));
        assert!(json.contains("\"test: json\""));
        reset();
    }
}
