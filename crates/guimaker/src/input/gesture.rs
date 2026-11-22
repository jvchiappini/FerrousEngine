//! Pointer gestures: a pure state machine fed by raw pointer events and an
//! injected clock.
//!
//! The recognizer never reads the clock and never touches the tree. Callers
//! pass the instant of every transition, so a recorded sequence replays to the
//! same gestures byte for byte. `O(1)` time and no allocation per event.
//!
//! Recognized: single, double and triple click (platform time and distance),
//! long press, drag (with a start threshold), drag move and end, and hover
//! intent (a stationary pointer for a delay). A drag that starts cancels the
//! click: releasing after a drag never emits a click.

use glam::Vec2;

use crate::input::PointerButton;

/// What the recognizer decided happened.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Gesture {
    /// A completed click; `count` is 1, 2 or 3.
    Click {
        /// The button that was released.
        button: PointerButton,
        /// 1 for a click, 2 for a double click, 3 for a triple click.
        count: u8,
        /// The release position.
        position: Vec2,
    },
    /// A press held longer than the long-press delay without moving.
    LongPress {
        /// The held button.
        button: PointerButton,
        /// The press position.
        position: Vec2,
    },
    /// Pointer travel passed the drag threshold.
    DragStart {
        /// The dragging button.
        button: PointerButton,
        /// The press position.
        position: Vec2,
    },
    /// The pointer moved while dragging.
    DragMove {
        /// The dragging button.
        button: PointerButton,
        /// The current position.
        position: Vec2,
        /// Position delta since the previous move.
        delta: Vec2,
        /// Total travel since the press.
        total: Vec2,
    },
    /// The button was released after a drag.
    DragEnd {
        /// The dragging button.
        button: PointerButton,
        /// The release position.
        position: Vec2,
        /// Total travel from the press to the release.
        total: Vec2,
    },
    /// The pointer stayed still over a target for the hover delay.
    HoverIntent {
        /// The stationary position.
        position: Vec2,
    },
}

/// The gesture tunables.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GestureConfig {
    /// The platform double-click time.
    pub multi_click_ms: f64,
    /// The platform double-click distance.
    pub multi_click_distance: f32,
    /// Pointer travel after a press that turns it into a drag.
    pub drag_threshold: f32,
    /// Hold time that turns a still press into a long press.
    pub long_press_ms: f64,
    /// Stationary time before a hover intent is reported.
    pub hover_intent_ms: f64,
    /// Maximum travel that still counts as stationary for hover intent.
    pub hover_move_tolerance: f32,
}

impl Default for GestureConfig {
    fn default() -> Self {
        Self {
            multi_click_ms: 500.0,
            multi_click_distance: 4.0,
            drag_threshold: 3.0,
            long_press_ms: 500.0,
            hover_intent_ms: 400.0,
            hover_move_tolerance: 1.0,
        }
    }
}

impl GestureConfig {
    /// The defaults rewritten with the platform's double-click values.
    #[must_use]
    pub fn from_platform(
        double_click_ms: f64,
        double_click_distance: f32,
        drag_threshold: f32,
    ) -> Self {
        Self {
            multi_click_ms: double_click_ms,
            multi_click_distance: double_click_distance,
            drag_threshold,
            ..Self::default()
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct ClickRecord {
    button: PointerButton,
    position: Vec2,
    time_ms: f64,
    count: u8,
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct PressRecord {
    button: PointerButton,
    position: Vec2,
    time_ms: f64,
    last_move: Vec2,
    dragging: bool,
    long_pressed: bool,
}

/// The pointer gesture recognizer.
#[derive(Clone, Debug)]
pub struct GestureRecognizer {
    config: GestureConfig,
    press: Option<PressRecord>,
    last_click: Option<ClickRecord>,
    last_position: Vec2,
    last_move_ms: f64,
    hover_seen: bool,
    hover_reported: bool,
    seen: bool,
}

impl GestureRecognizer {
    /// Builds a recognizer with the given tunables.
    #[must_use]
    pub fn new(config: GestureConfig) -> Self {
        Self {
            config,
            press: None,
            last_click: None,
            last_position: Vec2::ZERO,
            last_move_ms: f64::NEG_INFINITY,
            hover_seen: false,
            hover_reported: false,
            seen: false,
        }
    }

    /// The active tunables.
    #[must_use]
    pub fn config(&self) -> GestureConfig {
        self.config
    }

    /// Replaces the tunables; the current press is kept.
    pub fn set_config(&mut self, config: GestureConfig) {
        self.config = config;
    }

    /// Whether a button is currently held.
    #[must_use]
    pub fn is_pressed(&self) -> bool {
        self.press.is_some()
    }

    /// Whether the held button has become a drag.
    #[must_use]
    pub fn is_dragging(&self) -> bool {
        self.press.is_some_and(|press| press.dragging)
    }

    /// The click count of the last completed click.
    #[must_use]
    pub fn click_count(&self) -> u8 {
        self.last_click.map_or(0, |click| click.count)
    }

    /// The pointer moved to `position` at `now_ms`.
    ///
    /// Returns a [`Gesture::DragMove`] once the press has passed the drag
    /// threshold, and the first one also starts the drag.
    pub fn pointer_moved(&mut self, position: Vec2, now_ms: f64) -> Option<Gesture> {
        self.seen = true;
        self.hover_seen = true;
        if position.distance(self.last_position) > self.config.hover_move_tolerance {
            self.hover_reported = false;
        }
        self.last_position = position;
        self.last_move_ms = now_ms;
        let mut press = self.press?;
        let total = position - press.position;
        let delta = position - press.last_move;
        press.last_move = position;
        if !press.dragging {
            if total.length() < self.config.drag_threshold {
                return None;
            }
            press.dragging = true;
            self.press = Some(press);
            return Some(Gesture::DragStart {
                button: press.button,
                position: press.position,
            });
        }
        self.press = Some(press);
        Some(Gesture::DragMove {
            button: press.button,
            position,
            delta,
            total,
        })
    }

    /// A button went down at `position` at `now_ms`.
    ///
    /// A press that breaks the multi-click time or distance window starts a new
    /// click sequence.
    pub fn pointer_down(&mut self, position: Vec2, button: PointerButton, now_ms: f64) {
        self.seen = true;
        self.last_position = position;
        self.last_move_ms = now_ms;
        if let Some(click) = self.last_click {
            let elapsed = now_ms - click.time_ms;
            let distance = position.distance(click.position);
            if click.button != button
                || elapsed > self.config.multi_click_ms
                || distance > self.config.multi_click_distance
            {
                self.last_click = None;
            }
        }
        self.press = Some(PressRecord {
            button,
            position,
            time_ms: now_ms,
            last_move: position,
            dragging: false,
            long_pressed: false,
        });
        self.hover_seen = true;
        self.hover_reported = false;
    }

    /// A button went up at `position` at `now_ms`.
    ///
    /// Returns a click (with its count) when the release lands inside the
    /// multi-click distance and the press never became a drag, or a
    /// [`Gesture::DragEnd`] when it did.
    pub fn pointer_up(
        &mut self,
        position: Vec2,
        button: PointerButton,
        now_ms: f64,
    ) -> Option<Gesture> {
        self.last_position = position;
        self.hover_seen = true;
        self.hover_reported = false;
        let press = self.press.take()?;
        if press.button != button {
            return None;
        }
        let total = position - press.position;
        if press.dragging {
            self.last_click = None;
            return Some(Gesture::DragEnd {
                button,
                position,
                total,
            });
        }
        if total.length() > self.config.multi_click_distance {
            self.last_click = None;
            return None;
        }
        let count = match self.last_click {
            Some(click)
                if click.button == button
                    && now_ms - click.time_ms <= self.config.multi_click_ms
                    && position.distance(click.position) <= self.config.multi_click_distance =>
            {
                click.count.saturating_add(1).min(3)
            }
            _ => 1,
        };
        self.last_click = Some(ClickRecord {
            button,
            position,
            time_ms: now_ms,
            count,
        });
        Some(Gesture::Click {
            button,
            count,
            position,
        })
    }

    /// Advances timers: long press and hover intent.
    pub fn tick(&mut self, now_ms: f64) -> Option<Gesture> {
        if let Some(mut press) = self.press {
            if !press.dragging
                && !press.long_pressed
                && now_ms - press.time_ms >= self.config.long_press_ms
            {
                press.long_pressed = true;
                self.press = Some(press);
                return Some(Gesture::LongPress {
                    button: press.button,
                    position: press.position,
                });
            }
        }
        if self.seen
            && self.press.is_none()
            && self.hover_seen
            && !self.hover_reported
            && now_ms - self.last_move_ms >= self.config.hover_intent_ms
        {
            self.hover_reported = true;
            return Some(Gesture::HoverIntent {
                position: self.last_position,
            });
        }
        None
    }

    /// The pointer left the window; every in-flight gesture is cancelled.
    pub fn pointer_left(&mut self) {
        self.press = None;
        self.hover_seen = false;
        self.hover_reported = false;
        self.seen = false;
    }

    /// Forgets the multi-click sequence (a click elsewhere, a focus change).
    pub fn reset_clicks(&mut self) {
        self.last_click = None;
    }

    /// Whether a click sequence is in progress.
    #[must_use]
    pub fn has_click_sequence(&self) -> bool {
        self.last_click.is_some()
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::float_cmp)] // reason: exact token arithmetic in tests
    use super::{Gesture, GestureConfig, GestureRecognizer};
    use crate::input::PointerButton;
    use glam::Vec2;

    fn recognizer() -> GestureRecognizer {
        GestureRecognizer::new(GestureConfig::default())
    }

    #[test]
    fn single_double_and_triple_click() {
        let mut gesture = recognizer();
        let point = Vec2::new(10.0, 10.0);
        gesture.pointer_down(point, PointerButton::Primary, 0.0);
        assert_eq!(
            gesture.pointer_up(point, PointerButton::Primary, 40.0),
            Some(Gesture::Click {
                button: PointerButton::Primary,
                count: 1,
                position: point
            })
        );
        gesture.pointer_down(point, PointerButton::Primary, 200.0);
        assert_eq!(
            gesture.pointer_up(point, PointerButton::Primary, 240.0),
            Some(Gesture::Click {
                button: PointerButton::Primary,
                count: 2,
                position: point
            })
        );
        gesture.pointer_down(point, PointerButton::Primary, 400.0);
        assert_eq!(
            gesture.pointer_up(point, PointerButton::Primary, 420.0),
            Some(Gesture::Click {
                button: PointerButton::Primary,
                count: 3,
                position: point
            })
        );
        assert_eq!(gesture.click_count(), 3);
    }

    #[test]
    fn slow_or_far_clicks_start_a_new_sequence() {
        let mut gesture = recognizer();
        let point = Vec2::new(10.0, 10.0);
        gesture.pointer_down(point, PointerButton::Primary, 0.0);
        gesture.pointer_up(point, PointerButton::Primary, 10.0);
        gesture.pointer_down(point, PointerButton::Primary, 900.0);
        let click = gesture.pointer_up(point, PointerButton::Primary, 910.0);
        assert!(matches!(click, Some(Gesture::Click { count: 1, .. })));

        gesture.pointer_down(point, PointerButton::Primary, 1000.0);
        gesture.pointer_up(Vec2::new(40.0, 10.0), PointerButton::Primary, 1010.0);
        assert!(!gesture.has_click_sequence());
    }

    #[test]
    fn a_drag_cancels_the_click() {
        let mut gesture = recognizer();
        let start = Vec2::new(10.0, 10.0);
        gesture.pointer_down(start, PointerButton::Primary, 0.0);
        assert_eq!(
            gesture.pointer_moved(Vec2::new(20.0, 10.0), 10.0),
            Some(Gesture::DragStart {
                button: PointerButton::Primary,
                position: start
            })
        );
        let moved = gesture.pointer_moved(Vec2::new(30.0, 10.0), 20.0);
        assert!(matches!(moved, Some(Gesture::DragMove { .. })));
        let end = gesture.pointer_up(Vec2::new(30.0, 10.0), PointerButton::Primary, 30.0);
        assert!(matches!(end, Some(Gesture::DragEnd { .. })));
        assert!(!gesture.has_click_sequence());
    }

    #[test]
    fn a_short_move_is_still_a_click() {
        let mut gesture = recognizer();
        let start = Vec2::new(10.0, 10.0);
        gesture.pointer_down(start, PointerButton::Primary, 0.0);
        assert!(gesture.pointer_moved(Vec2::new(11.0, 11.0), 10.0).is_none());
        assert!(matches!(
            gesture.pointer_up(Vec2::new(11.0, 11.0), PointerButton::Primary, 20.0),
            Some(Gesture::Click { count: 1, .. })
        ));
    }

    #[test]
    fn long_press_fires_once() {
        let mut gesture = recognizer();
        gesture.pointer_down(Vec2::ZERO, PointerButton::Secondary, 0.0);
        assert!(gesture.tick(400.0).is_none());
        assert_eq!(
            gesture.tick(500.0),
            Some(Gesture::LongPress {
                button: PointerButton::Secondary,
                position: Vec2::ZERO
            })
        );
        assert!(gesture.tick(600.0).is_none());
    }

    #[test]
    fn hover_intent_fires_after_a_still_delay() {
        let mut gesture = recognizer();
        gesture.pointer_moved(Vec2::new(5.0, 5.0), 0.0);
        assert!(gesture.tick(100.0).is_none());
        assert_eq!(
            gesture.tick(400.0),
            Some(Gesture::HoverIntent {
                position: Vec2::new(5.0, 5.0)
            })
        );
        assert!(gesture.tick(900.0).is_none());
    }

    #[test]
    fn movement_rearms_hover_intent() {
        let mut gesture = recognizer();
        gesture.pointer_moved(Vec2::new(5.0, 5.0), 0.0);
        assert!(gesture.tick(400.0).is_some());
        gesture.pointer_moved(Vec2::new(40.0, 5.0), 410.0);
        assert!(gesture.tick(700.0).is_none());
        assert!(gesture.tick(810.0).is_some());
    }

    #[test]
    fn pointer_left_cancels_everything() {
        let mut gesture = recognizer();
        gesture.pointer_down(Vec2::ZERO, PointerButton::Primary, 0.0);
        gesture.pointer_moved(Vec2::new(50.0, 0.0), 10.0);
        gesture.pointer_left();
        assert!(!gesture.is_pressed());
        assert!(!gesture.is_dragging());
        assert!(gesture
            .pointer_up(Vec2::new(50.0, 0.0), PointerButton::Primary, 20.0)
            .is_none());
    }
}
