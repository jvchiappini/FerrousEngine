//! Startup policy: show the window when the first frame is ready (prompt 17
//! S3b).
//!
//! The GPU driver costs ~430 ms before the first swapchain frame. Showing the
//! window before that means the user stares at a white or black rectangle, so
//! the window is created hidden and only shown when there is something real to
//! show:
//!
//! - the GPU state (instance, device, swapchain, renderer, UI font) is built on
//!   a worker, so the event loop keeps running and the window can still be
//!   shown if the driver is slow;
//! - [`StartupGate`] holds the deadline of the safety timeout: if the first
//!   frame is not presented by then, the window is shown with the app
//!   background colour so the user knows the app opened;
//! - input that arrives before the first frame is queued and replayed in order
//!   once it is up.
//!
//! `show_window_when_ready(false)` restores the S3 behaviour (show the window
//! with the background colour immediately); the timeout can be shortened with
//! `AppConfig::with_window_show_timeout` or `FERROUS_SHOW_TIMEOUT_MS`.

use std::sync::mpsc::{self, Receiver};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use winit::event::WindowEvent;
use winit::window::WindowId;

use crate::builder::AppConfig;

/// How many input events are queued before the first frame.
///
/// A user cannot produce a thousand events in the first 500 ms; the bound is a
/// guard against a stuck startup feeding an unbounded queue. At the cap the
/// oldest event is dropped: the most recent intent wins.
pub(super) const MAX_QUEUED_INPUT: usize = 512;

/// The show timeout for `config`, honouring `FERROUS_SHOW_TIMEOUT_MS`.
#[must_use]
pub(crate) fn resolved_timeout(config: &AppConfig) -> Duration {
    if let Some(value) = std::env::var_os("FERROUS_SHOW_TIMEOUT_MS") {
        if let Some(text) = value.to_str() {
            if let Ok(millis) = text.trim().parse::<u64>() {
                return Duration::from_millis(millis);
            }
        }
        log::warn!("ignoring FERROUS_SHOW_TIMEOUT_MS={value:?} (expected milliseconds)");
    }
    config.window_show_timeout
}

/// Input events waiting for the first frame, in arrival order.
#[derive(Debug)]
pub(super) struct InputQueue<T> {
    events: Vec<T>,
}

impl<T> Default for InputQueue<T> {
    fn default() -> Self {
        Self { events: Vec::new() }
    }
}

impl<T> InputQueue<T> {
    pub(super) fn push(&mut self, event: T) {
        if self.events.len() >= MAX_QUEUED_INPUT {
            self.events.remove(0);
        }
        self.events.push(event);
    }

    pub(super) fn take(&mut self) -> Vec<T> {
        std::mem::take(&mut self.events)
    }

    #[cfg(test)]
    pub(super) fn len(&self) -> usize {
        self.events.len()
    }
}

/// What the renderer worker sends back.
pub(super) struct InitResult {
    pub(super) device: crate::graphics::DeviceInit,
    pub(super) renderer: Box<ferrous_renderer::Renderer>,
}

/// The renderer build running off the event loop.
///
/// Everything that touches the window or creates the swapchain stays on the
/// event-loop thread (`GraphicsState::init_device`): DXGI swapchains are not
/// agile across threads. Only `Renderer::new` — the part that takes seconds on
/// a cold DX12 driver — runs here, so the loop keeps pumping and the safety
/// timeout can show the window meanwhile.
pub(super) struct InitJob {
    receiver: Receiver<InitResult>,
    thread: Option<JoinHandle<()>>,
}

impl InitJob {
    pub(super) fn start(device: crate::graphics::DeviceInit) -> Self {
        let (sender, receiver) = mpsc::channel();
        let thread = std::thread::Builder::new()
            .name("ferrous-renderer".to_owned())
            .spawn(move || {
                let _span = ferrous_core::startup::span("engine: renderer build (worker)");
                let (device, renderer) = device.build_renderer();
                // A closed receiver means the runner went away while the
                // driver was still working; dropping the result is right.
                let _ = sender.send(InitResult { device, renderer });
            })
            .ok();
        Self { receiver, thread }
    }

    /// The result when the worker finished; never blocks.
    pub(super) fn try_take(&mut self) -> Option<InitResult> {
        match self.receiver.try_recv() {
            Ok(result) => Some(result),
            Err(mpsc::TryRecvError::Empty) => None,
            Err(mpsc::TryRecvError::Disconnected) => {
                log::error!("the GPU initialisation worker ended without a result");
                None
            }
        }
    }
}

impl Drop for InitJob {
    fn drop(&mut self) {
        // The worker cannot be interrupted safely (it owns the driver's device
        // creation); detach it. It notices the closed channel and drops the
        // state, and the process exit reaps the thread.
        if let Some(thread) = self.thread.take() {
            drop(thread);
        }
    }
}

/// What the startup gate is waiting for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum ShowState {
    /// Hidden: the first frame has not been presented yet.
    Hidden,
    /// Shown with the first frame (the normal path).
    FirstFrame,
    /// Shown with the background colour because the first frame was late.
    Timeout,
}

/// Window show policy for the first frame.
pub(super) struct StartupGate {
    deadline: Instant,
    show_when_ready: bool,
    state: ShowState,
    /// Whether the first frame is on screen (input can be delivered).
    first_frame: bool,
    pending: InputQueue<(WindowId, WindowEvent)>,
}

impl StartupGate {
    pub(super) fn new(timeout: Duration, show_when_ready: bool, now: Instant) -> Self {
        Self {
            deadline: now + timeout,
            show_when_ready,
            state: ShowState::Hidden,
            first_frame: false,
            pending: InputQueue::default(),
        }
    }

    pub(super) fn deadline(&self) -> Instant {
        self.deadline
    }

    pub(super) fn show_when_ready(&self) -> bool {
        self.show_when_ready
    }

    #[cfg(test)]
    pub(super) fn state(&self) -> ShowState {
        self.state
    }

    pub(super) fn is_shown(&self) -> bool {
        self.state != ShowState::Hidden
    }

    /// Whether the application can receive input (first frame presented).
    pub(super) fn accepts_input(&self) -> bool {
        self.first_frame
    }

    /// Records that the window is now visible with the first frame.
    pub(super) fn mark_first_frame(&mut self) {
        self.first_frame = true;
        if self.state == ShowState::Hidden {
            self.state = ShowState::FirstFrame;
        }
    }

    /// Records the timeout show; `true` the first time only.
    pub(super) fn mark_timeout(&mut self) -> bool {
        if self.state != ShowState::Hidden {
            return false;
        }
        self.state = ShowState::Timeout;
        true
    }

    /// Records a show requested by the application (`show_when_ready(false)`).
    pub(super) fn mark_shown_early(&mut self) {
        if self.state == ShowState::Hidden {
            self.state = ShowState::Timeout;
        }
    }

    pub(super) fn queue(&mut self, window: WindowId, event: WindowEvent) {
        self.pending.push((window, event));
    }

    pub(super) fn take_queue(&mut self) -> Vec<(WindowId, WindowEvent)> {
        self.pending.take()
    }
}

#[cfg(all(test, not(target_arch = "wasm32")))]
mod tests {
    use super::*;

    #[test]
    fn queued_events_keep_their_order() {
        let mut queue = InputQueue::default();
        for index in 0..5 {
            queue.push(index);
        }
        assert_eq!(queue.take(), vec![0, 1, 2, 3, 4]);
        assert_eq!(queue.len(), 0);
    }

    #[test]
    fn a_flood_keeps_the_most_recent_events_in_order() {
        let mut queue = InputQueue::default();
        let extra = 3;
        for index in 0..(MAX_QUEUED_INPUT + extra) {
            queue.push(index);
        }
        assert_eq!(queue.len(), MAX_QUEUED_INPUT);
        let taken = queue.take();
        assert_eq!(taken.first().copied(), Some(extra));
        assert_eq!(taken.last().copied(), Some(MAX_QUEUED_INPUT + extra - 1));
        assert!(taken.windows(2).all(|pair| pair[1] == pair[0] + 1));
    }

    #[test]
    fn the_gate_reports_hidden_until_first_frame_or_timeout() {
        let now = Instant::now();
        let mut gate = StartupGate::new(Duration::from_millis(800), true, now);
        assert!(!gate.is_shown());
        assert!(!gate.accepts_input());
        assert_eq!(gate.deadline(), now + Duration::from_millis(800));
        assert!(gate.mark_timeout());
        assert!(!gate.mark_timeout(), "the timeout is reported once");
        assert!(gate.is_shown());
        assert!(
            !gate.accepts_input(),
            "the background is not the first frame"
        );
        gate.mark_first_frame();
        assert!(gate.accepts_input());
        assert_eq!(gate.state(), ShowState::Timeout);
    }

    #[test]
    fn the_first_frame_wins_when_it_arrives_first() {
        let mut gate = StartupGate::new(crate::builder::DEFAULT_SHOW_TIMEOUT, true, Instant::now());
        gate.mark_first_frame();
        assert_eq!(gate.state(), ShowState::FirstFrame);
        assert!(!gate.mark_timeout(), "a late timeout does nothing");
    }

    #[test]
    fn the_timeout_can_be_overridden_from_the_environment() {
        let config = AppConfig {
            window_show_timeout: Duration::from_millis(800),
            ..AppConfig::default()
        };
        std::env::set_var("FERROUS_SHOW_TIMEOUT_MS", "50");
        assert_eq!(resolved_timeout(&config), Duration::from_millis(50));
        std::env::remove_var("FERROUS_SHOW_TIMEOUT_MS");
        assert_eq!(resolved_timeout(&config), Duration::from_millis(800));
    }
}
