//! Launch snapshot at runtime (prompt 17 S3b).
//!
//! [`LaunchState`] owns what the runner needs while the CPU-painted snapshot is
//! on screen: the painted child window, the input queued for the application,
//! and the capture requested for the next launch.

use std::path::PathBuf;
use std::thread::JoinHandle;

use winit::event::WindowEvent;
use winit::window::WindowId;

use crate::snapshot::{LaunchSnapshot, SnapshotImage};

/// How many input events a launch keeps before the handoff.
///
/// A user cannot produce a thousand events during the ~400 ms the GPU takes;
/// the bound is a guard against a stuck launch feeding an unbounded queue. At
/// the cap the oldest event is dropped: the most recent intent wins.
pub(super) const MAX_QUEUED_INPUT: usize = 512;

/// Input events waiting for the handoff, in arrival order.
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

/// The launch snapshot's state for the life of the window.
pub(super) struct LaunchState {
    directory: PathBuf,
    /// Content key of the screen the snapshot currently describes.
    key: String,
    /// The painted child, while it covers the window.
    surface: Option<crate::window_style::SnapshotSurface>,
    /// Events the application has not seen yet.
    pending: InputQueue<(WindowId, WindowEvent)>,
    /// The content key last written this run (one capture per key).
    captured: Option<String>,
    /// A capture asked for on the frame being built.
    capture_key: Option<String>,
    /// Writer threads; joined when the runner goes away so the file lands.
    writers: Vec<JoinHandle<()>>,
    /// The geometry the painted snapshot describes (width, height, scale).
    painted: Option<(u32, u32, f64)>,
    /// The content key of the file painted at launch, if any: a run that shows
    /// the file already has it on disk and does not capture it again.
    painted_key: Option<String>,
}

impl LaunchState {
    pub(super) fn new(config: &LaunchSnapshot) -> Self {
        Self {
            directory: config.directory.clone(),
            key: config.key.clone(),
            surface: None,
            pending: InputQueue::default(),
            captured: None,
            capture_key: None,
            writers: Vec::new(),
            painted: None,
            painted_key: None,
        }
    }

    /// The geometry the visible snapshot was painted for, if any.
    pub(super) fn painted_geometry(&self) -> Option<(u32, u32, f64)> {
        self.painted
    }

    /// The content key in force.
    pub(super) fn key(&self) -> &str {
        &self.key
    }

    /// Whether the CPU-painted snapshot is on screen.
    pub(super) fn shows_snapshot(&self) -> bool {
        self.surface.is_some()
    }

    /// Paints `image` over `window` (still hidden). `false` when the platform
    /// has no painter, in which case the caller shows the background colour.
    pub(super) fn paint(&mut self, window: &winit::window::Window, image: SnapshotImage) -> bool {
        let identity = image.identity().clone();
        match crate::window_style::SnapshotSurface::show(window, image) {
            Some(surface) => {
                self.surface = Some(surface);
                self.painted_key = Some(self.key.clone());
                self.painted = Some((
                    identity.width,
                    identity.height,
                    f64::from_bits(identity.scale),
                ));
                true
            }
            None => false,
        }
    }

    /// Paints the child now that the window is visible.
    pub(super) fn repaint(&self) {
        if let Some(surface) = &self.surface {
            surface.repaint();
        }
    }

    /// Removes the snapshot: the window shows the GPU from now on.
    ///
    /// Returns whether there was one to remove.
    pub(super) fn handoff(&mut self) -> bool {
        self.surface.take().is_some()
    }

    /// Removes the snapshot because the window changed (resize, DPI change):
    /// it no longer describes the screen, so the background-colour path takes
    /// over until the first GPU frame.
    pub(super) fn drop_snapshot(&mut self) -> bool {
        self.handoff()
    }

    /// Queues `event` for the application.
    pub(super) fn queue(&mut self, window: WindowId, event: WindowEvent) {
        self.pending.push((window, event));
    }

    /// The queued events, in order.
    pub(super) fn take_queue(&mut self) -> Vec<(WindowId, WindowEvent)> {
        self.pending.take()
    }

    /// Records a capture request for `key` (the current key when `None`).
    ///
    /// Returns `false` when this key was already written this run.
    pub(super) fn request_capture(&mut self, key: Option<String>) -> bool {
        let key = key.unwrap_or_else(|| self.key.clone());
        self.key = key.clone();
        if self.captured.as_deref() == Some(key.as_str())
            || self.capture_key.as_deref() == Some(key.as_str())
            || self.painted_key.as_deref() == Some(key.as_str())
        {
            return false;
        }
        self.capture_key = Some(key);
        true
    }

    /// Forgets a capture that could not be made, so the next frame retries.
    pub(super) fn cancel_capture(&mut self) {
        self.capture_key = None;
    }

    /// Stores `image` on a worker; the write is joined at teardown.
    pub(super) fn store(&mut self, image: SnapshotImage) {
        let Some(key) = self.capture_key.take() else {
            return;
        };
        self.captured = Some(key);
        if let Some(writer) = crate::snapshot::write_async(self.directory.clone(), image) {
            self.writers.push(writer);
        }
    }
}

impl Drop for LaunchState {
    fn drop(&mut self) {
        // A capture requested on the closing frame must reach the disk: the
        // process is about to end and the writer is otherwise killed.
        for writer in self.writers.drain(..) {
            let _ = writer.join();
        }
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
    fn a_key_is_captured_once_per_run() {
        let directory = std::env::temp_dir().join("ferrous-launch-state-test");
        let config = LaunchSnapshot {
            directory,
            key: "start".to_owned(),
        };
        let mut launch = LaunchState::new(&config);
        assert!(launch.request_capture(None));
        assert!(!launch.request_capture(None));
        assert!(launch.request_capture(Some("other".to_owned())));
        assert_eq!(launch.key(), "other");
    }
}
