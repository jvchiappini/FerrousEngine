//! The scroll controller: wheel, touchpad and momentum, in one place.
//!
//! Every call is `O(1)` and allocation-free. The controller owns a single
//! container's offset and velocity; the router keeps one per hovered scroll
//! container and mirrors the offset into the keyed store so layout, paint and
//! hit testing see the same number.
//!
//! Deltas follow the platform convention: a positive `y` scrolls the content
//! up (the wheel moves away from the user), so the offset decreases.

#![allow(
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::cast_possible_wrap
)] // reason: row, column and pixel values are bounded UI quantities
use glam::Vec2;

/// How a wheel delta is expressed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ScrollKind {
    /// Discrete wheel lines (one notch is typically `1.0`).
    Line,
    /// Pixel deltas from a precision touchpad or a high-resolution wheel.
    Pixel,
}

/// One scroll container's offset and momentum.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ScrollController {
    offset: Vec2,
    velocity: Vec2,
    max: Vec2,
    line_height: f32,
    friction: f32,
}

impl Default for ScrollController {
    fn default() -> Self {
        Self {
            offset: Vec2::ZERO,
            velocity: Vec2::ZERO,
            max: Vec2::ZERO,
            line_height: 32.0,
            friction: 6.0,
        }
    }
}

impl ScrollController {
    /// A controller at the top of its content.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Sets the line height a wheel notch travels.
    pub fn set_line_height(&mut self, line_height: f32) {
        self.line_height = line_height.max(1.0);
    }

    /// Sets the momentum decay rate per second.
    pub fn set_friction(&mut self, friction: f32) {
        self.friction = friction.max(0.0);
    }

    /// The current offset.
    #[must_use]
    pub fn offset(&self) -> Vec2 {
        self.offset
    }

    /// The current velocity in logical pixels per second.
    #[must_use]
    pub fn velocity(&self) -> Vec2 {
        self.velocity
    }

    /// The maximum offset (`content - viewport`, never negative).
    #[must_use]
    pub fn max(&self) -> Vec2 {
        self.max
    }

    /// Whether momentum is still moving the content.
    #[must_use]
    pub fn is_moving(&self) -> bool {
        self.velocity.length_squared() > 0.0
    }

    /// Sets the offset, clamped to the bounds; returns whether it changed.
    pub fn set_offset(&mut self, offset: Vec2) -> bool {
        let clamped = clamp(offset, self.max);
        if clamped == self.offset {
            return false;
        }
        self.offset = clamped;
        true
    }

    /// Sets the scrollable range from the viewport and content sizes.
    ///
    /// A negative or non-finite difference clamps to zero, and the offset is
    /// pulled back inside the new range.
    pub fn set_bounds(&mut self, viewport: Vec2, content: Vec2) {
        self.max = Vec2::new(
            (content.x - viewport.x).max(0.0),
            (content.y - viewport.y).max(0.0),
        );
        self.offset = clamp(self.offset, self.max);
    }

    /// Applies a wheel or touchpad delta.
    ///
    /// A line delta travels `line_height` pixels per line. With `Shift` held a
    /// vertical wheel becomes horizontal (the desktop convention). A wheel
    /// notch moves the offset immediately; a pixel delta (touchpad) moves it
    /// and also seeds the momentum velocity. Returns whether the offset moved.
    pub fn wheel(&mut self, delta: Vec2, kind: ScrollKind, shift: bool) -> bool {
        let (dx, dy) = if shift && delta.x.abs() < f32::EPSILON {
            (delta.y, 0.0)
        } else {
            (delta.x, delta.y)
        };
        let scaled = match kind {
            ScrollKind::Line => Vec2::new(dx, dy) * self.line_height,
            ScrollKind::Pixel => Vec2::new(dx, dy),
        };
        if scaled == Vec2::ZERO {
            return false;
        }
        let moved = self.set_offset(self.offset - scaled);
        if kind == ScrollKind::Pixel {
            self.velocity = -scaled * 60.0;
        }
        moved
    }

    /// Advances momentum by `dt_s` seconds; returns whether the offset moved.
    ///
    /// Exponential decay keeps the result independent of the frame rate:
    /// `velocity *= exp(-friction * dt)`. Below one logical pixel per second
    /// the motion stops. Hitting a bound zeroes that axis' velocity.
    pub fn advance(&mut self, dt_s: f32) -> bool {
        if !self.is_moving() || dt_s <= 0.0 {
            return false;
        }
        let decay = (-self.friction * dt_s).exp();
        self.velocity *= decay;
        if self.velocity.length_squared() < 1.0 {
            self.velocity = Vec2::ZERO;
            return false;
        }
        let before = self.offset;
        let next = clamp(self.offset + self.velocity * dt_s, self.max);
        if (next.x <= 0.0 && self.velocity.x < 0.0)
            || (next.x >= self.max.x && self.velocity.x > 0.0)
        {
            self.velocity.x = 0.0;
        }
        if (next.y <= 0.0 && self.velocity.y < 0.0)
            || (next.y >= self.max.y && self.velocity.y > 0.0)
        {
            self.velocity.y = 0.0;
        }
        self.offset = next;
        self.offset != before
    }

    /// Scrolls so the vertical range `y0..y1` is visible.
    pub fn ensure_visible(&mut self, y0: f32, y1: f32, viewport_height: f32) -> bool {
        if y0 < self.offset.y {
            return self.set_offset(Vec2::new(self.offset.x, y0));
        }
        if y1 > self.offset.y + viewport_height {
            let target = (y1 - viewport_height).min(self.max.y);
            return self.set_offset(Vec2::new(self.offset.x, target));
        }
        false
    }

    /// Scrolls so `row` of a uniform list is visible.
    pub fn scroll_to_index(
        &mut self,
        index: usize,
        item_height: f32,
        viewport_height: f32,
    ) -> bool {
        let y0 = index as f32 * item_height;
        self.ensure_visible(y0, y0 + item_height, viewport_height)
    }

    /// Jumps to a vertical offset.
    pub fn scroll_to(&mut self, y: f32) -> bool {
        self.set_offset(Vec2::new(self.offset.x, y))
    }

    /// Stops momentum immediately.
    pub fn stop(&mut self) {
        self.velocity = Vec2::ZERO;
    }
}

fn clamp(value: Vec2, max: Vec2) -> Vec2 {
    Vec2::new(
        value.x.clamp(0.0, max.x.max(0.0)),
        value.y.clamp(0.0, max.y.max(0.0)),
    )
}

#[cfg(test)]
mod tests {
    #![allow(clippy::float_cmp)] // reason: exact token arithmetic in tests
    use super::{ScrollController, ScrollKind};
    use glam::Vec2;

    fn controller() -> ScrollController {
        let mut controller = ScrollController::new();
        controller.set_bounds(Vec2::new(100.0, 100.0), Vec2::new(100.0, 1100.0));
        controller
    }

    #[test]
    fn wheel_lines_and_clamping() {
        let mut controller = controller();
        // A negative delta is a wheel step toward the end of the content.
        assert!(controller.wheel(Vec2::new(0.0, -1.0), ScrollKind::Line, false));
        assert_eq!(controller.offset().y, 32.0);
        assert!(controller.wheel(Vec2::new(0.0, -100.0), ScrollKind::Line, false));
        assert_eq!(controller.offset().y, 1000.0);
        assert!(!controller.wheel(Vec2::new(0.0, -1.0), ScrollKind::Line, false));
        assert!(controller.wheel(Vec2::new(0.0, 100.0), ScrollKind::Line, false));
        assert_eq!(controller.offset().y, 0.0);
        assert!(!controller.wheel(Vec2::new(0.0, 1.0), ScrollKind::Line, false));
    }

    #[test]
    fn shift_turns_a_vertical_wheel_horizontal() {
        let mut controller = ScrollController::new();
        controller.set_bounds(Vec2::new(100.0, 100.0), Vec2::new(1100.0, 100.0));
        assert!(controller.wheel(Vec2::new(0.0, -1.0), ScrollKind::Line, true));
        assert_eq!(controller.offset(), Vec2::new(32.0, 0.0));
    }

    #[test]
    fn pixel_deltas_seed_momentum_that_decays() {
        let mut controller = controller();
        controller.wheel(Vec2::new(0.0, -30.0), ScrollKind::Pixel, false);
        assert!(controller.is_moving());
        let first = controller.offset().y;
        assert!(controller.advance(1.0 / 60.0));
        assert!(controller.offset().y > first);
        for _ in 0..600 {
            if !controller.advance(1.0 / 60.0) {
                break;
            }
        }
        assert!(!controller.is_moving());
        assert_eq!(controller.velocity(), Vec2::ZERO);
    }

    #[test]
    fn momentum_stops_at_the_bottom() {
        let mut controller = controller();
        controller.set_offset(Vec2::new(0.0, 990.0));
        controller.wheel(Vec2::new(0.0, -50.0), ScrollKind::Pixel, false);
        for _ in 0..60 {
            controller.advance(1.0 / 60.0);
        }
        assert_eq!(controller.offset().y, 1000.0);
        assert!(!controller.is_moving());
    }

    #[test]
    fn reveal_above_and_below() {
        let mut controller = controller();
        assert!(controller.ensure_visible(200.0, 240.0, 100.0));
        assert_eq!(controller.offset().y, 140.0);
        assert!(controller.ensure_visible(40.0, 60.0, 100.0));
        assert_eq!(controller.offset().y, 40.0);
        assert!(!controller.ensure_visible(50.0, 60.0, 100.0));
        assert!(controller.scroll_to_index(40, 24.0, 100.0));
        assert_eq!(controller.offset().y, 884.0);
    }

    #[test]
    fn a_viewport_larger_than_the_content_cannot_scroll() {
        let mut controller = ScrollController::new();
        controller.set_bounds(Vec2::new(500.0, 500.0), Vec2::new(100.0, 100.0));
        assert_eq!(controller.max(), Vec2::ZERO);
        assert!(!controller.wheel(Vec2::new(0.0, 1.0), ScrollKind::Line, false));
        assert!(!controller.advance(0.016));
    }

    #[test]
    fn zero_deltas_are_no_ops() {
        let mut controller = controller();
        assert!(!controller.wheel(Vec2::ZERO, ScrollKind::Line, false));
        assert!(!controller.advance(0.0));
        assert!(!controller.is_moving());
    }
}
