//! Overlay enter/exit motion and the caret blink clock.
//!
//! Everything here runs on the host's injected clock: no function reads
//! `Instant::now()`, so the same instants give the same frames.
//!
//! An overlay (a popover, menu, dialog or toast) is a transition: it eases
//! between its hidden and shown state, and [`Theme::prefers_reduced_motion`]
//! makes it snap to the end state instead. A caret blink is **not** a
//! transition; it keeps blinking even when motion is reduced.
//!
//! [`Theme::prefers_reduced_motion`]: crate::theme::Theme::prefers_reduced_motion

use crate::theme::{Duration, Easing, Spacing};
use crate::visual::cubic_bezier;

/// The caret half-cycle in milliseconds.
///
/// A caret is visible for one half-cycle and hidden for the next, so the full
/// on/off period is one second.
pub const CARET_BLINK_MS: f64 = 500.0;

/// The design-system overlay entrance scale: a panel enters at 0.98 and
/// settles at 1.0.
const ENTER_SCALE_FROM: f32 = 0.98;

/// Whether the caret is in its visible half-cycle at `now_ms`.
#[must_use]
pub fn caret_visible(now_ms: f64) -> bool {
    now_ms.rem_euclid(2.0 * CARET_BLINK_MS) < CARET_BLINK_MS
}

/// The next half-cycle boundary strictly after `now_ms`.
///
/// The host uses it to schedule the next repaint so the caret keeps blinking
/// while the rest of the screen is idle.
#[must_use]
pub fn next_caret_wake_ms(now_ms: f64) -> f64 {
    let phase = now_ms.rem_euclid(CARET_BLINK_MS);
    now_ms + (CARET_BLINK_MS - phase)
}

/// An overlay's open/closed state on the injected clock.
///
/// The struct stores the instants at which the overlay was opened or closed
/// and derives its eased progress from them. The caller passes the clock, the
/// duration and the reduced-motion preference on every query, so a frame can be
/// reproduced from its inputs alone.
#[derive(Clone, Copy, Debug, Default)]
pub struct EnterExit {
    opened_at: Option<f64>,
    closed_at: Option<f64>,
}

impl EnterExit {
    /// A new overlay in the closed state.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Marks the overlay as opening at `now_ms`.
    pub fn open(&mut self, now_ms: f64) {
        self.opened_at = Some(now_ms);
        self.closed_at = None;
    }

    /// Marks the overlay as closing at `now_ms`.
    pub fn close(&mut self, now_ms: f64) {
        self.closed_at = Some(now_ms);
    }

    /// Whether the overlay was opened and has not been closed since.
    #[must_use]
    pub fn is_open(&self) -> bool {
        self.opened_at.is_some() && self.closed_at.is_none()
    }

    /// The eased progress of the current enter or exit, in `0..=1`.
    ///
    /// Before the first [`EnterExit::open`] it is 0. After an open it eases
    /// from 0 to 1 over `duration`; after a close it eases from 1 back to 0.
    /// When `reduced` is set, or `duration` is [`Duration::Instant`], it snaps
    /// to the end state (1 when open, 0 when closed).
    #[must_use]
    pub fn progress(&self, now_ms: f64, duration: Duration, reduced: bool) -> f32 {
        let Some(opened_at) = self.opened_at else {
            return 0.0;
        };
        let duration_ms = millis(duration);
        if reduced || duration_ms <= 0.0 {
            return if self.closed_at.is_some() { 0.0 } else { 1.0 };
        }
        if let Some(closed_at) = self.closed_at {
            return 1.0 - eased_fraction(now_ms - closed_at, duration_ms);
        }
        eased_fraction(now_ms - opened_at, duration_ms)
    }

    /// The overlay opacity, which is exactly its [`EnterExit::progress`].
    #[must_use]
    pub fn opacity(&self, now_ms: f64, duration: Duration, reduced: bool) -> f32 {
        self.progress(now_ms, duration, reduced)
    }

    /// The overlay scale: it enters at 0.98 and settles at 1.0.
    #[must_use]
    pub fn scale(&self, now_ms: f64, duration: Duration, reduced: bool) -> f32 {
        (1.0 - ENTER_SCALE_FROM).mul_add(self.progress(now_ms, duration, reduced), ENTER_SCALE_FROM)
    }

    /// The toast slide offset in logical pixels.
    ///
    /// It starts one [`Spacing::S2`] step below its resting position (positive
    /// is downward) and slides to 0 as the overlay enters.
    #[must_use]
    pub fn slide_y(&self, now_ms: f64, duration: Duration, reduced: bool) -> f32 {
        (1.0 - self.progress(now_ms, duration, reduced)) * Spacing::S2.px()
    }

    /// Whether an enter or exit is still running at `now_ms`.
    #[must_use]
    pub fn is_animating(&self, now_ms: f64, duration: Duration, reduced: bool) -> bool {
        self.next_wake_ms(now_ms, duration, reduced).is_some()
    }

    /// The instant the current enter or exit ends, or `None` when settled.
    ///
    /// Reduced motion and [`Duration::Instant`] never animate, so they return
    /// `None`.
    #[must_use]
    pub fn next_wake_ms(&self, now_ms: f64, duration: Duration, reduced: bool) -> Option<f64> {
        let opened_at = self.opened_at?;
        let duration_ms = millis(duration);
        if reduced || duration_ms <= 0.0 {
            return None;
        }
        let start = self.closed_at.unwrap_or(opened_at);
        let end = start + duration_ms;
        (now_ms < end).then_some(end)
    }
}

#[allow(clippy::cast_precision_loss)] // reason: a duration in milliseconds is far below 2^53
fn millis(duration: Duration) -> f64 {
    duration.ms() as f64
}

fn eased_fraction(elapsed_ms: f64, duration_ms: f64) -> f32 {
    if duration_ms <= 0.0 {
        return 1.0;
    }
    let raw = (elapsed_ms / duration_ms).clamp(0.0, 1.0);
    if raw <= 0.0 {
        return 0.0;
    }
    if raw >= 1.0 {
        return 1.0;
    }
    #[allow(clippy::cast_possible_truncation)] // reason: raw is clamped to 0..=1
    cubic_bezier(Easing::Standard.cubic_bezier(), raw as f32)
}

#[cfg(test)]
mod tests {
    use super::{caret_visible, next_caret_wake_ms, EnterExit};
    use crate::theme::{Duration, Spacing};

    #[test]
    #[allow(clippy::float_cmp)] // reason: eased endpoints are exact
    fn open_eases_from_zero_to_one() {
        let mut overlay = EnterExit::new();
        assert_eq!(overlay.progress(0.0, Duration::Base, false), 0.0);
        overlay.open(100.0);
        assert_eq!(overlay.progress(100.0, Duration::Base, false), 0.0);
        let mid = overlay.progress(170.0, Duration::Base, false);
        assert!((0.0..1.0).contains(&mid), "midpoint was {mid}");
        assert_eq!(overlay.progress(240.0, Duration::Base, false), 1.0);
    }

    #[test]
    #[allow(clippy::float_cmp)] // reason: eased endpoints are exact
    fn close_runs_back_to_zero() {
        let mut overlay = EnterExit::new();
        overlay.open(0.0);
        assert_eq!(overlay.progress(140.0, Duration::Base, false), 1.0);
        overlay.close(140.0);
        assert_eq!(overlay.progress(140.0, Duration::Base, false), 1.0);
        let mid = overlay.progress(210.0, Duration::Base, false);
        assert!((0.0..1.0).contains(&mid), "midpoint was {mid}");
        assert_eq!(overlay.progress(280.0, Duration::Base, false), 0.0);
    }

    #[test]
    #[allow(clippy::float_cmp)] // reason: snapped values are exact
    fn reduced_motion_snaps_to_the_end_state() {
        let mut overlay = EnterExit::new();
        overlay.open(100.0);
        assert_eq!(overlay.progress(100.0, Duration::Slow, true), 1.0);
        assert_eq!(overlay.scale(100.0, Duration::Slow, true), 1.0);
        assert_eq!(overlay.slide_y(100.0, Duration::Slow, true), 0.0);
        assert!(!overlay.is_animating(100.0, Duration::Slow, true));
        assert_eq!(overlay.next_wake_ms(100.0, Duration::Slow, true), None);
        overlay.close(200.0);
        assert_eq!(overlay.progress(200.0, Duration::Slow, true), 0.0);
    }

    #[test]
    fn scale_stays_between_the_entrance_scale_and_one() {
        let mut overlay = EnterExit::new();
        overlay.open(0.0);
        for offset in [0.0, 35.0, 70.0, 105.0, 140.0] {
            let scale = overlay.scale(offset, Duration::Base, false);
            assert!((0.98..=1.0).contains(&scale), "scale was {scale}");
        }
    }

    #[test]
    #[allow(clippy::float_cmp)] // reason: the wake instant is exact
    fn next_wake_reports_the_end_of_the_transition() {
        let mut overlay = EnterExit::new();
        overlay.open(100.0);
        assert!(overlay.is_animating(150.0, Duration::Base, false));
        assert_eq!(
            overlay.next_wake_ms(150.0, Duration::Base, false),
            Some(240.0)
        );
        assert!(!overlay.is_animating(240.0, Duration::Base, false));
        assert_eq!(overlay.next_wake_ms(240.0, Duration::Base, false), None);
    }

    #[test]
    fn slide_starts_one_step_below_and_settles() {
        let mut overlay = EnterExit::new();
        overlay.open(0.0);
        assert!((overlay.slide_y(0.0, Duration::Base, false) - Spacing::S2.px()).abs() < 1e-6);
        assert!(overlay.slide_y(140.0, Duration::Base, false).abs() < 1e-6);
    }

    #[test]
    #[allow(clippy::float_cmp)] // reason: caret boundaries are exact multiples of 500
    fn caret_blinks_on_a_one_second_period() {
        assert!(caret_visible(0.0));
        assert!(caret_visible(499.0));
        assert!(!caret_visible(500.0));
        assert!(!caret_visible(999.0));
        assert!(caret_visible(1_000.0));
        assert_eq!(next_caret_wake_ms(0.0), 500.0);
        assert_eq!(next_caret_wake_ms(200.0), 500.0);
        assert_eq!(next_caret_wake_ms(500.0), 1_000.0);
        assert_eq!(next_caret_wake_ms(750.0), 1_000.0);
        assert_eq!(next_caret_wake_ms(2_500.0), 3_000.0);
    }
}
