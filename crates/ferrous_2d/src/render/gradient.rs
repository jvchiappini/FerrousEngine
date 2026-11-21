//! CPU-side encoding of 2D gradients for the Pure2D shaders.
//!
//! The Pure2D pipelines carry a flat gradient table inside the camera uniform
//! ([`crate::render::Uniform2d::gradients`]).  A table is a sequence of
//! `vec4` slots:
//!
//! ```text
//! header     [kind, stop_count, first_stop_slot, 0]
//! geometry   [start.x, start.y, end.x, end.y]      (linear)
//!            [center.x, center.y, radius, 0]       (radial)
//! stop       [r, g, b, a]                          (linear, straight alpha)
//! offset     [offset, 0, 0, 0]
//! ```
//!
//! `kind` is `0` for linear and `1` for radial.  `first_stop_slot` is an
//! absolute slot index.  Stops are sorted by offset before encoding, which is
//! what the shader assumes.
//!
//! Build a table once per frame and upload it with
//! `Renderer2d::set_gradient_table` / `Renderer::set_2d_gradient_table`.  An
//! instance references an entry with [`ShapeInstance::with_gradient`] (or the
//! text equivalent); `0` keeps the solid colour.
//!
//! ```no_run
//! use ferrous_2d::render::{Gradient2d, GradientTable2d};
//! use glam::Vec2;
//!
//! let gradient = Gradient2d::linear(
//!     Vec2::new(0.0, 0.0),
//!     Vec2::new(200.0, 0.0),
//!     &[(0.0, [1.0, 0.0, 0.0, 1.0]), (1.0, [0.0, 0.0, 1.0, 1.0])],
//! )
//! .expect("two stops are valid");
//! let base = GradientTable2d::new()
//!     .push(&gradient)
//!     .expect("two stops fit");
//! # let _ = base;
//! ```
//!
//! [`ShapeInstance::with_gradient`]: crate::render::ShapeInstance::with_gradient

use glam::Vec2;

/// Number of `vec4` slots in the per-view gradient table.
///
/// A two-stop gradient needs six slots, so a table holds at least 85 minimal
/// gradients (fewer when the stops are longer).  Gradients that do not fit
/// are reported by [`GradientTable2d::push`] returning `None`.
pub const GRADIENT_TABLE_SLOTS: usize = 512;

/// Most stops a [`Gradient2d`] may carry.
pub const MAX_GRADIENT_STOPS: usize = 16;

/// Geometry of a gradient in the coordinate space of the drawn instance.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum GradientKind2d {
    /// Projection onto the `start` → `end` segment; `t` is `0` at `start`
    /// and `1` at `end`.
    Linear {
        /// First point of the axis.
        start: Vec2,
        /// Last point of the axis.
        end: Vec2,
    },
    /// Distance from `center`, normalised by `radius`.
    Radial {
        /// Centre of the ramp.
        center: Vec2,
        /// Distance at which the last stop is reached.
        radius: f32,
    },
}

/// One colour stop of a [`Gradient2d`].
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GradientStop2d {
    /// Position along the gradient, clamped to `0.0..=1.0` at build time.
    pub offset: f32,
    /// Straight (not premultiplied) linear RGBA, `0.0..=1.0` per channel.
    pub color: [f32; 4],
}

impl GradientStop2d {
    /// Builds a stop from an offset and a straight linear RGBA colour.
    #[must_use]
    pub const fn new(offset: f32, color: [f32; 4]) -> Self {
        Self { offset, color }
    }
}

/// A linear or radial colour ramp with two to [`MAX_GRADIENT_STOPS`] stops.
///
/// The constructor sorts the stops by offset (stable, so two stops at the same
/// offset keep their order and produce a hard edge) and clamps offsets into
/// `0.0..=1.0`.
#[derive(Debug, Clone, PartialEq)]
pub struct Gradient2d {
    /// Axis or centre of the ramp.
    pub kind: GradientKind2d,
    /// Stops in ascending offset order.
    pub stops: Vec<GradientStop2d>,
}

impl Gradient2d {
    /// Builds a linear gradient, clamping and sorting the stops.
    ///
    /// Returns `None` when `stops` holds fewer than 2 or more than
    /// [`MAX_GRADIENT_STOPS`] entries, or when a value is not finite.
    #[must_use]
    pub fn linear(start: Vec2, end: Vec2, stops: &[(f32, [f32; 4])]) -> Option<Self> {
        Self::new(GradientKind2d::Linear { start, end }, stops)
    }

    /// Builds a radial gradient, clamping and sorting the stops.
    ///
    /// Returns `None` for an invalid stop list or a non-positive/non-finite
    /// radius.
    #[must_use]
    pub fn radial(center: Vec2, radius: f32, stops: &[(f32, [f32; 4])]) -> Option<Self> {
        if !radius.is_finite() || radius <= 0.0 {
            return None;
        }
        Self::new(GradientKind2d::Radial { center, radius }, stops)
    }

    /// Builds a gradient around `kind` from `(offset, straight linear RGBA)`
    /// pairs.
    #[must_use]
    pub fn new(kind: GradientKind2d, stops: &[(f32, [f32; 4])]) -> Option<Self> {
        if stops.len() < 2 || stops.len() > MAX_GRADIENT_STOPS {
            return None;
        }
        let mut encoded = Vec::with_capacity(stops.len());
        for &(offset, color) in stops {
            if !offset.is_finite() {
                return None;
            }
            if color.iter().any(|channel| !channel.is_finite()) {
                return None;
            }
            encoded.push(GradientStop2d {
                offset: offset.clamp(0.0, 1.0),
                color,
            });
        }
        encoded.sort_by(|a, b| a.offset.total_cmp(&b.offset));
        Some(Self {
            kind,
            stops: encoded,
        })
    }

    /// Number of `vec4` slots this gradient occupies in a table.
    const fn slots(&self) -> usize {
        2 + self.stops.len() * 2
    }
}

/// A per-frame gradient table ready to upload into the 2D camera uniform.
///
/// Push the frame's gradients in the order the instances will reference them;
/// each successful push returns the `base` slot to pass to
/// [`ShapeInstance::with_gradient`](crate::render::ShapeInstance::with_gradient).
/// Clearing and refilling the same table every frame reuses its allocation.
#[derive(Debug, Default, Clone)]
pub struct GradientTable2d {
    slots: Vec<[f32; 4]>,
}

impl GradientTable2d {
    /// Creates an empty table.
    #[must_use]
    pub const fn new() -> Self {
        Self { slots: Vec::new() }
    }

    /// Drops every entry, keeping the allocation.
    pub fn clear(&mut self) {
        self.slots.clear();
    }

    /// Appends `gradient` and returns its base slot, or `None` when the table
    /// has no room left.
    pub fn push(&mut self, gradient: &Gradient2d) -> Option<u32> {
        let needed = gradient.slots();
        if self.slots.len() + needed > GRADIENT_TABLE_SLOTS {
            return None;
        }
        let base = u32::try_from(self.slots.len()).ok()?;
        let count = u32::try_from(gradient.stops.len()).ok()?;
        let kind = match gradient.kind {
            GradientKind2d::Linear { .. } => 0.0,
            GradientKind2d::Radial { .. } => 1.0,
        };
        // Both counts are bounded by `GRADIENT_TABLE_SLOTS` (512) and
        // `MAX_GRADIENT_STOPS` (16), so the conversion is exact.
        #[allow(clippy::cast_precision_loss)]
        let header = [kind, count as f32, (base + 2) as f32, 0.0];
        self.slots.push(header);
        self.slots.push(match gradient.kind {
            GradientKind2d::Linear { start, end } => [start.x, start.y, end.x, end.y],
            GradientKind2d::Radial { center, radius } => [center.x, center.y, radius, 0.0],
        });
        for stop in &gradient.stops {
            self.slots.push(stop.color);
            self.slots.push([stop.offset, 0.0, 0.0, 0.0]);
        }
        Some(base)
    }

    /// The encoded table, one `vec4` per entry.
    #[must_use]
    pub fn slots(&self) -> &[[f32; 4]] {
        &self.slots
    }

    /// `true` when no gradient has been pushed.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.slots.is_empty()
    }
}

/// Places a gradient base into an instance reference (base + 1), so `0` keeps
/// the solid colour.
#[must_use]
pub const fn gradient_reference(base: u32) -> u32 {
    base + 1
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encodes_linear_and_radial_headers() {
        let mut table = GradientTable2d::new();
        let red = [1.0, 0.0, 0.0, 1.0];
        let blue = [0.0, 0.0, 1.0, 1.0];
        let linear = Gradient2d::linear(
            Vec2::ZERO,
            Vec2::new(100.0, 0.0),
            &[(0.0, red), (1.0, blue)],
        )
        .unwrap();
        let base = table.push(&linear).unwrap();
        assert_eq!(base, 0);
        assert_eq!(table.slots()[0], [0.0, 2.0, 2.0, 0.0]);
        assert_eq!(table.slots()[1], [0.0, 0.0, 100.0, 0.0]);
        assert_eq!(table.slots()[2], red);
        assert_eq!(table.slots()[5][0], 1.0);

        let radial =
            Gradient2d::radial(Vec2::new(5.0, 6.0), 30.0, &[(0.0, red), (1.0, blue)]).unwrap();
        let second = table.push(&radial).unwrap();
        assert_eq!(second, 6);
        assert_eq!(table.slots()[6][0], 1.0);
        assert_eq!(table.slots()[7], [5.0, 6.0, 30.0, 0.0]);
    }

    #[test]
    fn sorts_and_clamps_stops() {
        let gradient = Gradient2d::linear(
            Vec2::ZERO,
            Vec2::X,
            &[
                (2.0, [0.0, 1.0, 0.0, 1.0]),
                (-1.0, [1.0, 0.0, 0.0, 1.0]),
                (0.5, [0.0, 0.0, 1.0, 1.0]),
            ],
        )
        .unwrap();
        let offsets: Vec<f32> = gradient.stops.iter().map(|stop| stop.offset).collect();
        assert_eq!(offsets, [0.0, 0.5, 1.0]);
    }

    #[test]
    fn rejects_invalid_gradients() {
        assert!(Gradient2d::linear(Vec2::ZERO, Vec2::X, &[(0.0, [0.0; 4])]).is_none());
        let stops = [(0.0, [0.0; 4]); MAX_GRADIENT_STOPS + 1];
        assert!(Gradient2d::linear(Vec2::ZERO, Vec2::X, &stops).is_none());
        assert!(Gradient2d::radial(Vec2::ZERO, 0.0, &[(0.0, [0.0; 4]), (1.0, [0.0; 4])]).is_none());
    }

    #[test]
    fn reports_a_full_table() {
        let mut table = GradientTable2d::new();
        let gradient =
            Gradient2d::radial(Vec2::ZERO, 1.0, &[(0.0, [0.0; 4]), (1.0, [1.0; 4])]).unwrap();
        for _ in 0..GRADIENT_TABLE_SLOTS / 6 {
            assert!(table.push(&gradient).is_some());
        }
        // One slot is left, so another gradient cannot fit.
        assert!(table.push(&gradient).is_none());
    }
}
