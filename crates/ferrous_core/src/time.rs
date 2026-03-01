//! Frame timing utilities.
//!
//! `Time` is updated once per frame by the application runner and passed into
//! every callback via [`AppContext`].  All fields are read-only from game
//! code; the runner owns the `TimeClock` that produces snapshots.
//!
//! # Example
//! ```rust,ignore
//! fn update(&mut self, ctx: &mut AppContext) {
//!     // Rotate at 90°/s regardless of frame rate
//!     let angle = std::f32::consts::FRAC_PI_2 * ctx.time.delta;
//!     self.cube_rotation += angle;
//! }
//! ```

/// A snapshot of timing information for the current frame.
///
/// Passed by value into every `FerrousApp` callback.  Since it is `Copy`
/// you can store a copy locally if needed.
#[derive(Debug, Clone, Copy)]
pub struct Time {
    /// Seconds elapsed since the previous frame.  Typical values are in the
    /// range 0.008 – 0.033.  Clamped to a maximum of 0.1 to prevent
    /// spiral-of-death physics on slow frames.
    pub delta: f32,

    /// Total seconds elapsed since the application started.
    pub elapsed: f64,

    /// Number of frames rendered so far (starts at 0 for the first frame).
    pub frame_count: u64,

    /// Instantaneous frames-per-second derived from `delta`.
    pub fps: f32,
}

impl Default for Time {
    fn default() -> Self {
        Self {
            delta: 0.0,
            elapsed: 0.0,
            frame_count: 0,
            fps: 0.0,
        }
    }
}

impl Time {
    /// Returns the delta time clamped to `[0, max_dt]`.
    #[inline]
    pub fn clamped_delta(&self, max_dt: f32) -> f32 {
        self.delta.min(max_dt)
    }
}

// ─── Clock (internal, lives in the runner) ─────────────────────────────────

/// Stateful timer that accumulates time and produces [`Time`] snapshots.
///
/// The runner creates one of these at startup and calls `tick()` at the
/// beginning of every frame.
pub struct TimeClock {
    start:       std::time::Instant,
    last_tick:   std::time::Instant,
    frame_count: u64,
}

impl TimeClock {
    /// Create a new clock, starting the epoch now.
    pub fn new() -> Self {
        let now = std::time::Instant::now();
        Self {
            start:       now,
            last_tick:   now,
            frame_count: 0,
        }
    }

    /// Return the current [`Time`] snapshot without advancing the clock.
    ///
    /// Useful for callbacks (like `setup` or `on_resize`) that are not on the
    /// hot frame path — they still want valid timing data but must not advance
    /// the frame counter.
    pub fn peek(&self) -> Time {
        let now     = std::time::Instant::now();
        let raw_dt  = (now - self.last_tick).as_secs_f32();
        let delta   = raw_dt.min(0.1);
        let elapsed = (now - self.start).as_secs_f64();
        let fps     = if delta > 0.0 { 1.0 / delta } else { 0.0 };
        Time { delta, elapsed, frame_count: self.frame_count, fps }
    }

    /// Advance by one frame.  Returns the [`Time`] snapshot for this frame.
    pub fn tick(&mut self) -> Time {
        let now     = std::time::Instant::now();
        let raw_dt  = (now - self.last_tick).as_secs_f32();
        let delta   = raw_dt.min(0.1); // clamp to avoid spiral-of-death
        let elapsed = (now - self.start).as_secs_f64();
        let fps     = if delta > 0.0 { 1.0 / delta } else { 0.0 };
        let count   = self.frame_count;

        self.last_tick   = now;
        self.frame_count += 1;

        Time { delta, elapsed, frame_count: count, fps }
    }
}

impl Default for TimeClock {
    fn default() -> Self {
        Self::new()
    }
}
