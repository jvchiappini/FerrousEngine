//! Animated visual-state transitions driven by the injected clock.
//!
//! A component names per-state colour roles once; this module picks the role
//! for the current interaction state and, when the role changes, blends from
//! the colour on screen to the new one over the style's transition duration.
//! The clock is the host's injected instant, so the same inputs give the same
//! colours and a screenshot at a fixed time is reproducible.

use std::collections::HashMap;

use crate::theme::{Color, Duration, Easing};

/// The background colour slot.
pub const BACKGROUND: usize = 0;
/// The border colour slot.
pub const BORDER: usize = 1;
/// The text/icon colour slot.
pub const FOREGROUND: usize = 2;

const SLOTS: usize = 3;

#[derive(Clone, Copy, Debug)]
struct Transition {
    from: Color,
    to: Color,
    start_ms: f64,
    duration_ms: f64,
}

#[derive(Clone, Debug, Default)]
struct NodeVisual {
    slots: [Option<Transition>; SLOTS],
}

/// The per-node colour animator.
#[derive(Clone, Debug, Default)]
pub struct VisualAnimator {
    nodes: HashMap<u64, NodeVisual>,
}

impl VisualAnimator {
    /// Builds an empty animator.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// The colour for `slot` at `now_ms`, starting or advancing a transition.
    ///
    /// The first time a path is seen the colour snaps to `target`, so a cold
    /// frame never fades in from an unrelated colour.
    #[allow(clippy::cast_precision_loss)] // reason: a duration in milliseconds is far below 2^53
    pub fn color(
        &mut self,
        path: u64,
        slot: usize,
        target: Color,
        now_ms: f64,
        duration: Duration,
    ) -> Color {
        let node = self.nodes.entry(path).or_default();
        let entry = &mut node.slots[slot];
        // Before the host has advanced the injected clock there is no elapsed
        // time to blend over, so the state colour snaps. A real frame sets a
        // positive instant and the transition runs normally.
        if now_ms <= 0.0 {
            *entry = Some(Transition {
                from: target,
                to: target,
                start_ms: 0.0,
                duration_ms: 0.0,
            });
            return target;
        }
        let duration_ms = duration.ms() as f64;
        let Some(transition) = entry else {
            *entry = Some(Transition {
                from: target,
                to: target,
                start_ms: now_ms - duration_ms,
                duration_ms,
            });
            return target;
        };
        if transition.to != target {
            let current = sample(transition, now_ms);
            transition.from = current;
            transition.to = target;
            transition.start_ms = now_ms;
            transition.duration_ms = duration_ms;
        }
        sample(transition, now_ms)
    }

    /// Whether one node has a colour transition in flight at `now_ms`.
    #[must_use]
    pub fn is_animating(&self, path: u64, now_ms: f64) -> bool {
        self.nodes.get(&path).is_some_and(|node| {
            node.slots.iter().flatten().any(|transition| {
                transition.duration_ms > 0.0
                    && now_ms - transition.start_ms < transition.duration_ms
            })
        })
    }

    /// Whether any node has a colour transition in flight at `now_ms`.
    #[must_use]
    pub fn any_animating(&self, now_ms: f64) -> bool {
        self.nodes.values().any(|node| {
            node.slots.iter().flatten().any(|transition| {
                transition.duration_ms > 0.0
                    && now_ms - transition.start_ms < transition.duration_ms
            })
        })
    }

    /// Drops the state of nodes that no longer exist.
    pub fn retain(&mut self, mut keep: impl FnMut(u64) -> bool) {
        self.nodes.retain(|path, _| keep(*path));
    }
}

fn sample(transition: &Transition, now_ms: f64) -> Color {
    if transition.duration_ms <= 0.0 {
        return transition.to;
    }
    let raw = ((now_ms - transition.start_ms) / transition.duration_ms).clamp(0.0, 1.0);
    if raw >= 1.0 {
        return transition.to;
    }
    #[allow(clippy::cast_possible_truncation)] // reason: raw is clamped to 0..=1
    let eased = cubic_bezier(Easing::Standard.cubic_bezier(), raw as f32);
    lerp(transition.from, transition.to, eased)
}

/// Blends two colours in linear light.
#[must_use]
pub fn lerp(from: Color, to: Color, t: f32) -> Color {
    from.lerp(to, t)
}

/// Evaluates a CSS-style cubic bezier easing curve at `x`.
#[must_use]
pub fn cubic_bezier(points: [f32; 4], x: f32) -> f32 {
    let [x1, y1, x2, y2] = points;
    let sample_x = |t: f32| {
        let inverse = 1.0 - t;
        3.0 * inverse * inverse * t * x1 + 3.0 * inverse * t * t * x2 + t * t * t
    };
    let sample_y = |t: f32| {
        let inverse = 1.0 - t;
        3.0 * inverse * inverse * t * y1 + 3.0 * inverse * t * t * y2 + t * t * t
    };
    let mut low = 0.0;
    let mut high = 1.0;
    for _ in 0..18 {
        let mid = (low + high) * 0.5;
        if sample_x(mid) < x {
            low = mid;
        } else {
            high = mid;
        }
    }
    sample_y((low + high) * 0.5)
}

#[cfg(test)]
mod tests {
    use super::{cubic_bezier, lerp, VisualAnimator};
    use crate::theme::{Color, Duration};

    #[test]
    fn a_first_colour_snaps_and_a_change_animates() {
        let mut animator = VisualAnimator::new();
        let rest = Color::linear(0.0, 0.0, 0.0);
        let hover = Color::linear(1.0, 1.0, 1.0);
        assert_eq!(animator.color(7, 0, rest, 0.0, Duration::Fast), rest);
        assert!(!animator.any_animating(0.0));
        let mid = animator.color(7, 0, hover, 16.0, Duration::Fast);
        assert!(animator.any_animating(16.0));
        let mid = mid.to_linear()[0];
        assert!(mid > 0.0 && mid < 1.0, "midpoint was {mid}");
        assert_eq!(animator.color(7, 0, hover, 1_000.0, Duration::Fast), hover);
        assert!(!animator.any_animating(1_000.0));
    }

    #[test]
    fn easing_and_lerp_hit_their_endpoints() {
        assert!((cubic_bezier([0.2, 0.0, 0.0, 1.0], 0.0)).abs() < 1e-6);
        assert!((cubic_bezier([0.2, 0.0, 0.0, 1.0], 1.0) - 1.0).abs() < 1e-6);
        let black = Color::linear(0.0, 0.0, 0.0);
        let white = Color::linear(1.0, 1.0, 1.0);
        assert_eq!(lerp(black, white, 0.0), black);
        assert_eq!(lerp(black, white, 1.0), white);
    }
}
