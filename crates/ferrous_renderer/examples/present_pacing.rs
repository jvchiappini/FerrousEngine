//! Windowed present-mode pacing probe.
//!
//! A headless probe cannot measure present pacing: without a real surface
//! there is no swapchain and no `present`. The probe therefore has to be a
//! windowed app.
//!
//! This file cannot be a compiled example inside `ferrous_renderer`:
//! `ferrous_engine` depends on this crate, so a dev-dependency back on the
//! engine would be a Cargo cycle, and this crate does not depend on winit to
//! open a window itself. The probe is kept here as documented `no_run` code so
//! its recipe lives next to the renderer it measures. Copy it into an
//! application crate (or a scratch binary that depends on `ferrous_engine`)
//! and run it with `cargo run --release --example present_pacing`.
//!
//! Per mode, set the environment variable (it overrides `with_present_mode`):
//!
//! ```text
//! $env:FERROUS_PRESENT_MODE = "mailbox"
//! cargo run --release --example present_pacing
//! ```
//!
//! The preferred measurement path is still
//! `python scripts/perf_bench.py --scenario playback_1080p --allow-pointer`,
//! which measures the release Motioner binary with the same environment
//! variable; see `crates/ferrous_engine/docs/present.md`.
//!
//! ```rust,no_run
//! use std::time::Instant;
//!
//! use ferrous_engine::{App, AppContext, FerrousApp, PresentMode};
//!
//! const WARMUP_S: f32 = 2.0;
//! const MEASURE_S: f32 = 10.0;
//!
//! #[derive(Default)]
//! struct PacingProbe {
//!     started: Option<Instant>,
//!     frame_ms: Vec<f32>,
//! }
//!
//! impl FerrousApp for PacingProbe {
//!     fn update(&mut self, ctx: &mut AppContext<'_>) {
//!         let now = Instant::now();
//!         let started = *self.started.get_or_insert(now);
//!         let elapsed = now.duration_since(started).as_secs_f32();
//!
//!         // `frame_ms` is the previous frame's CPU wall-clock duration.
//!         if (WARMUP_S..WARMUP_S + MEASURE_S).contains(&elapsed) {
//!             self.frame_ms.push(ctx.render.frame_ms());
//!         }
//!
//!         if elapsed >= WARMUP_S + MEASURE_S {
//!             report(&mut self.frame_ms);
//!             ctx.request_exit();
//!         }
//!     }
//! }
//!
//! fn report(samples: &mut [f32]) {
//!     if samples.is_empty() {
//!         eprintln!("present_pacing: no samples");
//!         return;
//!     }
//!     samples.sort_by(|a, b| a.total_cmp(b));
//!     let p = |q: f32| samples[((samples.len() - 1) as f32 * q).round() as usize];
//!     let median = p(0.50);
//!     let fps = if median > 0.0 { 1000.0 / median } else { 0.0 };
//!     // 16.7 ms is one frame at 60 Hz; use 8.3 for a 120 Hz panel.
//!     let over_budget = samples.iter().filter(|ms| **ms > 16.7).count();
//!     eprintln!(
//!         "present_pacing: n={} fps~{fps:.1} frame_ms p50={:.2} p90={:.2} p99={:.2} over_16_7ms={over_budget}",
//!         samples.len(),
//!         median,
//!         p(0.90),
//!         p(0.99),
//!     );
//! }
//!
//! fn main() {
//!     // Disable the CPU frame cap so the present mode is the only pacer;
//!     // otherwise a 60 fps cap hides Mailbox and AutoNoVsync.
//!     App::new(PacingProbe::default())
//!         .with_title("Ferrous present pacing")
//!         .with_size(1920, 1080)
//!         .with_target_fps(None)
//!         .with_present_mode(PresentMode::Mailbox)
//!         .run();
//! }
//! ```
//!
//! Set `FERROUS_PRESENT_MODE` to override `with_present_mode`; the environment
//! wins by design.

fn main() {
    eprintln!(
        "present_pacing is a documented probe, not a compiled windowed example.\n\
         ferrous_renderer cannot depend on ferrous_engine (Cargo cycle) and has no\n\
         winit, so a surface cannot be opened here. Copy the `no_run` probe from\n\
         this file into an app crate, or use the release measurement:\n\n\
         \x20 cargo build --release -p motioner_app --bins\n\
         \x20 $env:FERROUS_PRESENT_MODE = \"mailbox\"\n\
         \x20 python scripts/perf_bench.py --scenario playback_1080p --allow-pointer --runs 3\n\n\
         See crates/ferrous_engine/docs/present.md."
    );
}
