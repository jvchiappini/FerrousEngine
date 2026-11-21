//! Geometry shared by layout, hit testing and the display list.
//!
//! All values are logical pixels; layout snaps them to the physical grid at the
//! end of a frame with [`Rect::snap`].

use glam::Vec2;

/// A width/height pair in logical pixels.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Size {
    /// Width in logical pixels.
    pub width: f32,
    /// Height in logical pixels.
    pub height: f32,
}

impl Size {
    /// The zero size.
    pub const ZERO: Self = Self {
        width: 0.0,
        height: 0.0,
    };

    /// Builds a size.
    #[must_use]
    pub const fn new(width: f32, height: f32) -> Self {
        Self { width, height }
    }
}

/// An axis-aligned rectangle in logical pixels.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Rect {
    /// Left edge.
    pub x: f32,
    /// Top edge.
    pub y: f32,
    /// Width.
    pub width: f32,
    /// Height.
    pub height: f32,
}

impl Rect {
    /// The empty rectangle at the origin.
    pub const ZERO: Self = Self {
        x: 0.0,
        y: 0.0,
        width: 0.0,
        height: 0.0,
    };

    /// Builds a rectangle.
    #[must_use]
    pub const fn new(x: f32, y: f32, width: f32, height: f32) -> Self {
        Self {
            x,
            y,
            width,
            height,
        }
    }

    /// Builds a rectangle from an origin and a size.
    #[must_use]
    pub const fn from_origin_size(origin: Vec2, size: Size) -> Self {
        Self::new(origin.x, origin.y, size.width, size.height)
    }

    /// The right edge.
    #[must_use]
    #[inline]
    pub fn right(self) -> f32 {
        self.x + self.width
    }

    /// The bottom edge.
    #[must_use]
    #[inline]
    pub fn bottom(self) -> f32 {
        self.y + self.height
    }

    /// The top-left corner.
    #[must_use]
    pub fn origin(self) -> Vec2 {
        Vec2::new(self.x, self.y)
    }

    /// The size as a [`Size`].
    #[must_use]
    pub fn size(self) -> Size {
        Size::new(self.width, self.height)
    }

    /// Whether the rectangle has a positive area.
    #[must_use]
    pub fn is_empty(self) -> bool {
        self.width <= 0.0 || self.height <= 0.0
    }

    /// Whether a point lies inside the rectangle, edges included.
    #[must_use]
    #[inline]
    pub fn contains(self, point: Vec2) -> bool {
        point.x >= self.x
            && point.y >= self.y
            && point.x <= self.right()
            && point.y <= self.bottom()
    }

    /// The intersection of two rectangles, empty when they do not overlap.
    #[must_use]
    pub fn intersect(self, other: Self) -> Self {
        let x = self.x.max(other.x);
        let y = self.y.max(other.y);
        let right = self.right().min(other.right());
        let bottom = self.bottom().min(other.bottom());
        Self::new(x, y, (right - x).max(0.0), (bottom - y).max(0.0))
    }

    /// The smallest rectangle containing both.
    #[must_use]
    pub fn union(self, other: Self) -> Self {
        let x = self.x.min(other.x);
        let y = self.y.min(other.y);
        let right = self.right().max(other.right());
        let bottom = self.bottom().max(other.bottom());
        Self::new(x, y, right - x, bottom - y)
    }

    /// Translates the rectangle.
    #[must_use]
    pub fn translated(self, offset: Vec2) -> Self {
        Self::new(
            self.x + offset.x,
            self.y + offset.y,
            self.width,
            self.height,
        )
    }

    /// Snaps every edge to the physical pixel grid of `scale`.
    ///
    /// Edges are rounded first and the width is derived from them, so two
    /// adjacent rectangles share an edge and a one-pixel border does not blur.
    #[must_use]
    pub fn snap(self, scale: f32) -> Self {
        if !scale.is_finite() || scale <= 0.0 {
            return self;
        }
        let x = (self.x * scale).round() / scale;
        let y = (self.y * scale).round() / scale;
        let right = ((self.x + self.width) * scale).round() / scale;
        let bottom = ((self.y + self.height) * scale).round() / scale;
        Self::new(x, y, (right - x).max(0.0), (bottom - y).max(0.0))
    }
}

#[cfg(test)]
mod tests {
    use super::{Rect, Size};
    use glam::Vec2;

    #[test]
    fn intersection_and_union() {
        let a = Rect::new(0.0, 0.0, 10.0, 10.0);
        let b = Rect::new(5.0, 5.0, 10.0, 10.0);
        assert_eq!(a.intersect(b), Rect::new(5.0, 5.0, 5.0, 5.0));
        assert_eq!(a.union(b), Rect::new(0.0, 0.0, 15.0, 15.0));
        assert!(a.intersect(Rect::new(20.0, 0.0, 1.0, 1.0)).is_empty());
    }

    #[test]
    fn snapping_shares_edges() {
        let scale = 1.5;
        let left = Rect::new(10.0, 0.0, 13.0, 20.0).snap(scale);
        let right = Rect::new(23.0, 0.0, 13.0, 20.0).snap(scale);
        assert!((left.right() - right.x).abs() < 1e-6);
        // Every edge is on the physical grid.
        for value in [left.x, left.right(), right.x, right.right()] {
            let physical = value * scale;
            assert!((physical - physical.round()).abs() < 1e-4);
        }
    }

    #[test]
    fn contains_includes_edges() {
        let rect = Rect::new(0.0, 0.0, 4.0, 4.0);
        assert!(rect.contains(Vec2::ZERO));
        assert!(rect.contains(Vec2::new(4.0, 4.0)));
        assert!(!rect.contains(Vec2::new(4.1, 2.0)));
        assert_eq!(rect.size(), Size::new(4.0, 4.0));
    }
}
