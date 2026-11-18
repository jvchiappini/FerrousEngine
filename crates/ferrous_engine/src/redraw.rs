//! Frame scheduling: continuous redraw or redraw on demand.
//!
//! Every app used to redraw at `AppConfig::target_fps` forever, even when
//! nothing had changed. [`RedrawPolicy::OnDemand`] makes a frame happen only
//! when there is a reason for one:
//!
//! - input arrives (`window_event` with anything but `RedrawRequested`);
//! - the app calls [`AppContext::request_redraw`](crate::AppContext::request_redraw);
//! - a scheduled wake fires
//!   ([`AppContext::redraw_at`](crate::AppContext::redraw_at)), for animations,
//!   caret blink, hover transitions, tooltips, debounce timers or autosave;
//! - a worker wakes the loop through [`Waker`];
//! - the window is resized, its scale changes, or the OS asks for a repaint.
//!
//! While an animation is active an app simply requests the next frame from
//! every `update`; the display's vsync (`Fifo` present mode) then paces it.
//! In between, the event loop sleeps in `ControlFlow::Wait` (or `WaitUntil`
//! for the next scheduled wake) and the process costs no CPU and submits no
//! GPU work.
//!
//! [`RedrawPolicy::Continuous`] is the historical behaviour for games and
//! demos and remains the default; an application opts into on-demand drawing
//! with [`App::with_redraw_policy`](crate::App::with_redraw_policy).
//!
//! ```no_run
//! use ferrous_engine::{App, RedrawPolicy};
//!
//! // A desktop editor: draw only when something changed.
//! App::new(MyEditor)
//!     .with_redraw_policy(RedrawPolicy::OnDemand)
//!     .run();
//! # struct MyEditor;
//! # impl ferrous_engine::FerrousApp for MyEditor {}
//! ```

use winit::event_loop::EventLoopProxy;

#[cfg(not(target_arch = "wasm32"))]
pub use std::time::Instant;
#[cfg(target_arch = "wasm32")]
pub use web_time::Instant;

/// The event a [`Waker`] sends to the loop.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum EngineEvent {
    /// Wake the loop and schedule one frame.
    Redraw,
}

/// A handle a worker can use to wake the event loop.
///
/// Obtained from [`AppContext::waker`](crate::AppContext::waker) during
/// `setup` and cloned into background jobs (font bakes, image or video
/// decode, audio open, file watchers, a control server). Calling
/// [`Self::wake`] makes the loop leave its `Wait`, run `about_to_wait` and
/// present exactly one frame in [`RedrawPolicy::OnDemand`] apps. It is cheap,
/// `Send + Sync` and a no-op when the loop is gone (a window that closed
/// while a worker was finishing).
#[derive(Clone, Default)]
pub struct Waker {
    proxy: Option<EventLoopProxy<EngineEvent>>,
}

impl std::fmt::Debug for Waker {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Waker")
            .field("connected", &self.proxy.is_some())
            .finish()
    }
}

impl Waker {
    pub(crate) fn from_proxy(proxy: EventLoopProxy<EngineEvent>) -> Self {
        Self { proxy: Some(proxy) }
    }

    /// A waker that does nothing.
    ///
    /// Useful in headless tests and in apps driven by another loop.
    #[must_use]
    pub fn noop() -> Self {
        Self::default()
    }

    /// Wakes the event loop: the app gets one frame to react.
    ///
    /// A closed loop makes this a no-op, so a finishing worker never panics.
    pub fn wake(&self) {
        if let Some(proxy) = &self.proxy {
            let _ = proxy.send_event(EngineEvent::Redraw);
        }
    }
}

/// How the runner decides when to draw a frame.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum RedrawPolicy {
    /// Draw at `AppConfig::target_fps` (or as fast as possible when it is
    /// `None`). The historical behaviour; the right choice for games and
    /// demos that always animate.
    #[default]
    Continuous,
    /// Draw only when input arrives, the app asks for a frame, a scheduled
    /// wake fires, a worker wakes the loop, or the window changes. Idle apps
    /// present no frames and submit no GPU work.
    OnDemand,
}

/// What the runner should do after the current event batch.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum NextFrame {
    /// Ask the window for a redraw now.
    Draw,
    /// Sleep until the instant, then ask for a redraw.
    WaitUntil(Instant),
    /// Sleep until the next input, worker wake or event.
    Wait,
}

/// The runner's redraw state.
///
/// Kept separate from the event loop so the decision is a pure function of
/// time and flags, which the unit tests drive with a simulated clock.
#[derive(Debug)]
pub(crate) struct Scheduler {
    policy: RedrawPolicy,
    /// The app asked for the next frame.
    requested: bool,
    /// Input arrived since the last frame.
    input: bool,
    /// The earliest one-shot wake requested with `redraw_at`.
    deadline: Option<Instant>,
    /// A `RedrawRequested` was already asked for and has not run yet. Winit
    /// coalesces these, but tracking it keeps the scheduler's answer stable
    /// if `about_to_wait` runs again before the redraw.
    pending: bool,
}

impl Scheduler {
    pub(crate) fn new(policy: RedrawPolicy) -> Self {
        Self {
            policy,
            requested: false,
            input: false,
            deadline: None,
            pending: false,
        }
    }

    pub(crate) fn policy(&self) -> RedrawPolicy {
        self.policy
    }

    /// The app asked for the next frame (animation, a state change).
    pub(crate) fn request(&mut self) {
        self.requested = true;
    }

    /// Schedule a one-shot wake at `when`.
    ///
    /// Several calls keep the earliest deadline. In on-demand mode the
    /// deadline produces exactly one frame; the app schedules another one if
    /// it is still animating.
    pub(crate) fn request_at(&mut self, when: Instant) {
        self.deadline = Some(match self.deadline {
            Some(current) if current <= when => current,
            _ => when,
        });
    }

    /// An input event arrived. On-demand apps get one frame to process it.
    pub(crate) fn on_input(&mut self) {
        self.input = true;
    }

    /// A frame is starting: any redraw request that was already answered by
    /// this frame is consumed. Requests made during the frame (the animation
    /// loop) set the flags again and survive.
    pub(crate) fn begin_frame(&mut self) {
        self.pending = false;
        self.input = false;
    }

    /// The next thing the runner should do.
    ///
    /// `continuous_deadline` is the target-FPS deadline the runner computed
    /// for this frame (continuous mode only).
    pub(crate) fn next(&mut self, now: Instant, continuous_deadline: Option<Instant>) -> NextFrame {
        if self.pending {
            // A redraw is already in flight; do not ask for a second one and
            // do not spin on a deadline that is already due.
            return NextFrame::Wait;
        }

        match self.policy {
            RedrawPolicy::Continuous => {
                if let Some(deadline) = continuous_deadline {
                    if now >= deadline {
                        self.pending = true;
                        NextFrame::Draw
                    } else {
                        NextFrame::WaitUntil(deadline)
                    }
                } else {
                    self.pending = true;
                    NextFrame::Draw
                }
            }
            RedrawPolicy::OnDemand => {
                if self.input || self.requested {
                    self.input = false;
                    self.requested = false;
                    self.pending = true;
                    return NextFrame::Draw;
                }
                if self.deadline.is_some_and(|deadline| now >= deadline) {
                    self.deadline = None;
                    self.pending = true;
                    return NextFrame::Draw;
                }
                self.wait_for(now, continuous_deadline)
            }
        }
    }

    fn wait_for(&self, _now: Instant, _continuous_deadline: Option<Instant>) -> NextFrame {
        match self.deadline {
            Some(deadline) => NextFrame::WaitUntil(deadline),
            None => NextFrame::Wait,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    fn origin() -> Instant {
        static ORIGIN: std::sync::OnceLock<Instant> = std::sync::OnceLock::new();
        *ORIGIN.get_or_init(Instant::now)
    }

    fn at(ms: u64) -> Instant {
        // One fixed origin for every test instant: the scheduler never reads
        // the wall clock, so the tests must not either.
        origin() + Duration::from_millis(ms)
    }

    fn on_demand() -> Scheduler {
        Scheduler::new(RedrawPolicy::OnDemand)
    }

    /// A frame counter driven by the scheduler with a simulated clock: the
    /// headless equivalent of the runner's `about_to_wait` loop.
    struct HeadlessRunner {
        scheduler: Scheduler,
        now: Instant,
        frames: u64,
        presented_at: Vec<Instant>,
    }

    impl HeadlessRunner {
        fn new(now: Instant) -> Self {
            Self {
                scheduler: on_demand(),
                now,
                frames: 0,
                presented_at: Vec::new(),
            }
        }

        /// Advances the simulated clock to `until`, running every frame the
        /// scheduler asks for. The frame itself takes 16 ms of the clock.
        fn advance_to(&mut self, until: Instant) {
            loop {
                match self.scheduler.next(self.now, None) {
                    NextFrame::Draw => {
                        self.scheduler.begin_frame();
                        self.frames += 1;
                        self.presented_at.push(self.now);
                        self.now += Duration::from_millis(16);
                    }
                    NextFrame::WaitUntil(wake) if wake <= until => self.now = wake.max(self.now),
                    NextFrame::WaitUntil(_) | NextFrame::Wait => break,
                }
            }
            self.now = self.now.max(until);
        }
    }

    #[test]
    fn an_idle_on_demand_app_presents_no_frames_in_ten_seconds() {
        let start = at(0);
        let mut runner = HeadlessRunner::new(start);
        runner.advance_to(start + Duration::from_secs(10));
        assert_eq!(runner.frames, 0, "idle means zero presented frames");
    }

    #[test]
    fn a_redraw_at_produces_exactly_one_frame_at_the_deadline() {
        let start = at(0);
        let deadline = start + Duration::from_millis(250);
        let mut runner = HeadlessRunner::new(start);
        runner.scheduler.request_at(deadline);
        runner.advance_to(start + Duration::from_secs(10));
        assert_eq!(runner.frames, 1, "one-shot wake draws once");
        assert_eq!(runner.presented_at, vec![deadline]);
    }

    #[test]
    fn a_request_draws_one_frame() {
        let start = at(0);
        let mut runner = HeadlessRunner::new(start);
        runner.scheduler.request();
        runner.advance_to(start + Duration::from_secs(1));
        assert_eq!(runner.frames, 1);
        assert_eq!(runner.presented_at, vec![start]);
    }

    #[test]
    fn input_draws_one_frame_and_is_consumed() {
        let start = at(0);
        let mut runner = HeadlessRunner::new(start);
        runner.scheduler.on_input();
        runner.advance_to(start + Duration::from_secs(1));
        assert_eq!(runner.frames, 1);
        runner.advance_to(start + Duration::from_secs(2));
        assert_eq!(runner.frames, 1, "the input was consumed by the frame");
    }

    #[test]
    fn a_redraw_at_in_the_past_draws_immediately() {
        let start = at(0);
        let mut runner = HeadlessRunner::new(start);
        runner
            .scheduler
            .request_at(start - Duration::from_millis(1));
        runner.advance_to(start + Duration::from_secs(1));
        assert_eq!(runner.frames, 1);
        assert_eq!(runner.presented_at, vec![start]);
    }

    #[test]
    fn several_scheduled_wakes_keep_the_earliest() {
        let start = at(0);
        let mut scheduler = on_demand();
        scheduler.request_at(start + Duration::from_millis(900));
        scheduler.request_at(start + Duration::from_millis(300));
        scheduler.request_at(start + Duration::from_millis(600));
        assert_eq!(
            scheduler.next(start, None),
            NextFrame::WaitUntil(start + Duration::from_millis(300))
        );
    }

    #[test]
    fn a_frame_in_flight_is_not_requested_twice() {
        let start = at(0);
        let mut scheduler = on_demand();
        scheduler.request();
        assert_eq!(scheduler.next(start, None), NextFrame::Draw);
        assert_eq!(scheduler.next(start, None), NextFrame::Wait);
        scheduler.begin_frame();
        assert_eq!(scheduler.next(start, None), NextFrame::Wait);
    }

    #[test]
    fn an_animation_loop_requests_every_frame() {
        let start = at(0);
        let mut runner = HeadlessRunner::new(start);
        for _ in 0..10 {
            runner.scheduler.request();
            runner.advance_to(runner.now + Duration::from_millis(16));
        }
        assert_eq!(runner.frames, 10);
    }

    #[test]
    fn continuous_mode_still_draws_at_the_deadline() {
        let start = at(0);
        let mut scheduler = Scheduler::new(RedrawPolicy::Continuous);
        let deadline = start + Duration::from_millis(16);
        assert_eq!(
            scheduler.next(start, Some(deadline)),
            NextFrame::WaitUntil(deadline)
        );
        assert_eq!(scheduler.next(deadline, Some(deadline)), NextFrame::Draw);
        assert_eq!(scheduler.next(deadline, Some(deadline)), NextFrame::Wait);
        scheduler.begin_frame();
        assert_eq!(
            scheduler.next(deadline, Some(deadline + Duration::from_millis(16))),
            NextFrame::WaitUntil(deadline + Duration::from_millis(16))
        );
    }

    #[test]
    fn continuous_mode_without_a_cap_draws_immediately() {
        let start = at(0);
        let mut scheduler = Scheduler::new(RedrawPolicy::Continuous);
        assert_eq!(scheduler.next(start, None), NextFrame::Draw);
        assert_eq!(scheduler.next(start, None), NextFrame::Wait);
    }
}
