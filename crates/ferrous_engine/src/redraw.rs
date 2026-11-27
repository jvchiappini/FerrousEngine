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

use winit::event::WindowEvent;
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

/// Whether one window event asks the on-demand scheduler for a frame by itself.
///
/// Cursor events are opt-out: the app's `on_window_event` decides whether the
/// move changed something visible (`AppConfig::redraw_on_pointer_move`), so a
/// move that changes nothing schedules no frame. Every other event asks for
/// one frame. `Runner::window_event` and the redraw tests share this decision.
pub(crate) fn event_requests_frame(event: &WindowEvent, redraw_on_pointer_move: bool) -> bool {
    let is_cursor = matches!(
        event,
        WindowEvent::CursorMoved { .. }
            | WindowEvent::CursorEntered { .. }
            | WindowEvent::CursorLeft { .. }
    );
    !is_cursor || redraw_on_pointer_move
}

/// Whether `event` is a genuine input event.
///
/// The show-paint guard is cleared only by these: a `Focused`, `Moved`,
/// `Resized` or `ScaleFactorChanged` event can be produced by showing the window
/// itself (Windows sends focus after `ShowWindow`), and clearing the guard on it
/// would turn the OS paint into a second idle frame.
#[cfg(not(target_arch = "wasm32"))]
pub(crate) fn is_input_event(event: &WindowEvent) -> bool {
    matches!(
        event,
        WindowEvent::CursorMoved { .. }
            | WindowEvent::CursorEntered { .. }
            | WindowEvent::CursorLeft { .. }
            | WindowEvent::MouseInput { .. }
            | WindowEvent::MouseWheel { .. }
            | WindowEvent::KeyboardInput { .. }
            | WindowEvent::Ime(_)
            | WindowEvent::Touch(_)
            | WindowEvent::ModifiersChanged(_)
            | WindowEvent::DroppedFile(_)
            | WindowEvent::HoveredFile(_)
            | WindowEvent::HoveredFileCancelled
    )
}

/// Whether `event` can be a side effect of showing the window.
///
/// While the show-paint guard is armed these events must not request a frame:
/// the frame is already on the swapchain and the event changes nothing visible,
/// so asking for one makes the OS paint a redundant second frame. A `Resized`
/// or `ScaleFactorChanged` only qualifies when it is a no-op (see
/// [`settle_changes_nothing`]): a real size or scale change must still draw.
#[cfg(not(target_arch = "wasm32"))]
pub(crate) fn is_show_settling(event: &WindowEvent) -> bool {
    matches!(
        event,
        WindowEvent::Focused(_)
            | WindowEvent::Moved(_)
            | WindowEvent::Resized(_)
            | WindowEvent::ScaleFactorChanged { .. }
    )
}

/// Whether a DPI scale change is a real one.
///
/// Winit reports an unchanged factor on some platforms; reconfiguring a surface
/// for it costs a frame, so the runner compares with the cached factor first.
pub(crate) fn scale_changed(new: f64, current: f64) -> bool {
    (new - current).abs() > f64::EPSILON
}

/// Whether a show-settling event changes nothing the surface needs to redraw.
///
/// `Focused` and `Moved` never change the drawable, so they are no-op settles
/// while the guard is armed. A `Resized` is a no-op only when the physical size
/// is the one the surface already has, and a `ScaleFactorChanged` only when the
/// factor did not move: a real resize or DPI change that arrives during the
/// show settling must reconfigure the surface and draw (review of Phase 9D
/// finding 1).
#[cfg(not(target_arch = "wasm32"))]
pub(crate) fn settle_changes_nothing(
    event: &WindowEvent,
    window_size: (u32, u32),
    scale_factor: f64,
) -> bool {
    match event {
        WindowEvent::Resized(size) => (size.width, size.height) == window_size,
        WindowEvent::ScaleFactorChanged {
            scale_factor: new, ..
        } => !scale_changed(*new, scale_factor),
        _ => true,
    }
}

/// Whether the OS paint that follows showing the window may be swallowed.
///
/// The first frame is already on the swapchain when the window is shown, so the
/// paint the OS sends for the newly visible window is redundant. It may only be
/// dropped when nothing has asked for a frame since that first present: `mark`
/// is the scheduler's request count captured at the show and `current` is the
/// count when the paint arrives. A request made in the first `tick` (an
/// animation, a spinner, a `redraw_at` wake) is a real frame; winit merges
/// redraw requests, so swallowing that paint would stall the app until the next
/// input event (prompt 15 Phase 9D finding 1).
pub(crate) fn swallow_show_paint(skip: bool, mark: u64, current: u64) -> bool {
    skip && mark == current
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
    /// Monotonic count of the frames this scheduler has been asked for (an app
    /// request, an input event, a scheduled wake that fired). The runner
    /// snapshots it when the first frame is shown: if it moved before the OS
    /// sends its show paint, that paint is a real frame for the app and must
    /// not be swallowed (prompt 15 Phase 9D finding 1).
    requests: u64,
}

impl Scheduler {
    pub(crate) fn new(policy: RedrawPolicy) -> Self {
        Self {
            policy,
            requested: false,
            input: false,
            deadline: None,
            pending: false,
            requests: 0,
        }
    }

    /// How many frames have been asked for so far. Read by the runner's
    /// show-paint guard; a request made after the first present bumps it.
    pub(crate) fn requests(&self) -> u64 {
        self.requests
    }

    pub(crate) fn policy(&self) -> RedrawPolicy {
        self.policy
    }

    /// The app asked for the next frame (animation, a state change).
    pub(crate) fn request(&mut self) {
        self.requested = true;
        self.requests = self.requests.wrapping_add(1);
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
        self.requests = self.requests.wrapping_add(1);
    }

    /// An input event arrived. On-demand apps get one frame to process it.
    pub(crate) fn on_input(&mut self) {
        self.input = true;
        self.requests = self.requests.wrapping_add(1);
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

        /// The default redraw decision `Runner::window_event` makes for one
        /// event, without the app and the GPU.
        fn on_window_event(&mut self, event: &WindowEvent, redraw_on_pointer_move: bool) {
            if event_requests_frame(event, redraw_on_pointer_move) {
                self.scheduler.on_input();
            }
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
    fn a_pointer_move_the_app_does_not_forward_presents_no_frame() {
        use winit::dpi::PhysicalPosition;
        use winit::event::DeviceId;

        let start = at(0);
        let moved = WindowEvent::CursorMoved {
            device_id: DeviceId::dummy(),
            position: PhysicalPosition::new(40.0, 24.0),
        };

        // The real event goes through the same decision `Runner::window_event`
        // makes before it can deliver it to the app. With
        // `redraw_on_pointer_move == false` the move is opt-out and schedules
        // nothing, so the simulated loop presents no frame.
        let mut runner = HeadlessRunner::new(start);
        runner.on_window_event(&moved, false);
        runner.advance_to(start + Duration::from_secs(10));
        assert_eq!(
            runner.frames, 0,
            "a pointer move the app does not forward must present no frame"
        );

        // Positive control: the same event does ask for a frame when the
        // config forwards pointer moves, so the assertion above is not the
        // idle test under another name.
        let mut forwarding = HeadlessRunner::new(start);
        forwarding.on_window_event(&moved, true);
        forwarding.advance_to(start + Duration::from_secs(1));
        assert_eq!(
            forwarding.frames, 1,
            "the same move presents one frame when it is forwarded"
        );
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

    #[test]
    fn the_show_paint_is_swallowed_only_when_nothing_asked_since_the_present() {
        // The window is shown after the first present; the scheduler snapshot
        // is `mark`.
        assert!(swallow_show_paint(true, 7, 7), "the redundant OS paint");

        // An app requests a frame in its first `tick` (an animation, a
        // spinner, a scheduled wake). winit may merge that request with the OS
        // show paint into one `RedrawRequested`; that frame is real and must
        // render.
        assert!(
            !swallow_show_paint(true, 7, 8),
            "a request after the first present must not be eaten"
        );
        // Once the show paint has been consumed the flag is cleared, so a later
        // redraw is never swallowed either.
        assert!(!swallow_show_paint(false, 7, 7));
    }

    #[test]
    fn the_shows_own_focus_does_not_clear_or_move_the_show_guard() {
        // `show_ready` calls `focus_window`, so Windows delivers a `Focused`
        // event before the OS paint. It must not count as a frame request, or
        // the paint becomes a redundant second idle frame (review of Phase 9D
        // finding 1).
        let focused = WindowEvent::Focused(true);
        assert!(is_show_settling(&focused));
        assert!(!is_input_event(&focused));

        // With the request count unchanged, the OS paint is swallowed: one
        // frame in total.
        let mark = 7;
        assert!(swallow_show_paint(true, mark, mark));

        // A real input event is different: it clears the guard and moves the
        // count, so the next frame renders.
        assert!(is_input_event(&WindowEvent::CursorEntered {
            device_id: winit::event::DeviceId::dummy(),
        }));
    }

    #[test]
    fn an_app_request_in_the_first_tick_moves_the_request_count() {
        let mut scheduler = on_demand();
        let mark = scheduler.requests();
        // `request` is what `AppContext::request_redraw` does during the first
        // `update`/`tick`, after the first present and the show.
        scheduler.request();
        assert_ne!(
            scheduler.requests(),
            mark,
            "a request after the show must be visible to the guard"
        );
        assert!(!swallow_show_paint(true, mark, scheduler.requests()));
    }

    #[test]
    fn a_real_settle_resize_or_scale_still_draws_while_the_guard_is_armed() {
        let size = (1600, 900);
        let scale = 1.25;

        // The show's own focus/move never changes the drawable.
        assert!(settle_changes_nothing(
            &WindowEvent::Focused(true),
            size,
            scale
        ));
        assert!(settle_changes_nothing(
            &WindowEvent::Moved(winit::dpi::PhysicalPosition::new(120, 80)),
            size,
            scale
        ));

        // A resize the hidden window already had is the OS settling the show:
        // it may be swallowed.
        assert!(settle_changes_nothing(
            &WindowEvent::Resized(winit::dpi::PhysicalSize::new(size.0, size.1)),
            size,
            scale
        ));
        // A real resize must reconfigure the surface and draw; it must not be
        // swallowed as a settle (review of Phase 9D finding 1).
        assert!(!settle_changes_nothing(
            &WindowEvent::Resized(winit::dpi::PhysicalSize::new(1280, 720)),
            size,
            scale
        ));

        // The scale branch uses the same comparison the runner uses for
        // `ScaleFactorChanged`: equal is a settle, moved is a real frame.
        assert!(!scale_changed(scale, scale));
        assert!(scale_changed(1.5, scale));
        assert!(scale_changed(scale, 1.5));
    }
}
