//! Reactive constraint system for `ferrous_gui`.
//!
//! [`SizeExpr`] lets you describe a position or size relative to the container
//! (window or parent widget) rather than as a hard-coded pixel value.
//! [`Constraint`] bundles up to four such expressions (x, y, width, height) and
//! is applied automatically every frame by [`crate::ui::Ui::resolve_constraints`].
//!
//! ## Quick reference
//!
//! | Expression | Meaning |
//! |---|---|
//! | `SizeExpr::px(v)` | Fixed `v` pixels |
//! | `SizeExpr::pct(0.5)` | 50 % of container size |
//! | `SizeExpr::from_right(20.0)` | `container_w − widget_w − 20` |
//! | `SizeExpr::from_bottom(20.0)` | `container_h − widget_h − 20` |
//! | `SizeExpr::center()` | Centred in container |
//! | `a.add(b)` | Sum of two expressions |

/// Declarative size / position expression evaluated against a container dimension.
#[derive(Debug, Clone)]
pub enum SizeExpr {
    /// Fixed pixel value — identical to a literal `f32` coordinate today.
    Px(f32),
    /// Fraction of the container dimension (0.0 – 1.0 for 0 % – 100 %).
    Pct(f32),
    /// `container_size − widget_size − margin`: pin widget to the right edge.
    FromRight(f32),
    /// `container_size − widget_size − margin`: pin widget to the bottom edge.
    FromBottom(f32),
    /// Arithmetic sum of two expressions.
    Add(Box<SizeExpr>, Box<SizeExpr>),
    /// Centre inside container with an optional pixel offset.
    /// `Center(0.0)` = exactly centred; `Center(-10.0)` = 10 px to the left/up
    /// from centre.
    Center(f32),
}

impl SizeExpr {
    // ── constructors ──────────────────────────────────────────────────────────

    /// Fixed pixel value.
    #[inline]
    pub fn px(v: f32) -> Self {
        SizeExpr::Px(v)
    }

    /// Percentage of the container (0.0 = 0 %, 1.0 = 100 %).
    #[inline]
    pub fn pct(v: f32) -> Self {
        SizeExpr::Pct(v)
    }

    /// Pin to the right (or bottom) edge with `margin` pixels of clearance.
    #[inline]
    pub fn from_right(margin: f32) -> Self {
        SizeExpr::FromRight(margin)
    }

    /// Pin to the bottom edge with `margin` pixels of clearance.
    #[inline]
    pub fn from_bottom(margin: f32) -> Self {
        SizeExpr::FromBottom(margin)
    }

    /// Exactly centred inside the container (zero offset).
    #[inline]
    pub fn center() -> Self {
        SizeExpr::Center(0.0)
    }

    /// Centred with a pixel offset (positive = right/down, negative = left/up).
    #[inline]
    pub fn center_offset(offset: f32) -> Self {
        SizeExpr::Center(offset)
    }

    /// Arithmetic sum of `self` and `other`.
    ///
    /// ```ignore
    /// // 100 % of container width minus 16 px
    /// SizeExpr::pct(1.0).add(SizeExpr::px(-16.0))
    /// ```
    pub fn add(self, other: SizeExpr) -> Self {
        SizeExpr::Add(Box::new(self), Box::new(other))
    }

    // ── resolver ──────────────────────────────────────────────────────────────

    /// Evaluate the expression.
    ///
    /// * `container_size` — width or height of the parent container / window.
    /// * `widget_size`    — width or height of *this* widget (needed for
    ///   `FromRight`/`FromBottom`/`Center`).
    pub fn resolve(&self, container_size: f32, widget_size: f32) -> f32 {
        match self {
            SizeExpr::Px(v) => *v,
            SizeExpr::Pct(f) => f * container_size,
            SizeExpr::FromRight(margin) => container_size - widget_size - margin,
            SizeExpr::FromBottom(margin) => container_size - widget_size - margin,
            SizeExpr::Add(a, b) => {
                a.resolve(container_size, widget_size) + b.resolve(container_size, widget_size)
            }
            SizeExpr::Center(offset) => (container_size - widget_size) * 0.5 + offset,
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Constraint
// ─────────────────────────────────────────────────────────────────────────────

/// Declarative layout constraint for a widget.
///
/// Each field is `Option<SizeExpr>` — only the supplied axes are overridden
/// when constraints are resolved; the widget's own position/size is left
/// unchanged on `None` axes.
///
/// Build a constraint with the fluent API:
/// ```ignore
/// Constraint::new()
///     .x(SizeExpr::from_right(20.0))
///     .y(SizeExpr::px(12.0))
/// ```
#[derive(Debug, Clone, Default)]
pub struct Constraint {
    /// Horizontal position override.
    pub x: Option<SizeExpr>,
    /// Vertical position override.
    pub y: Option<SizeExpr>,
    /// Width override.
    pub width: Option<SizeExpr>,
    /// Height override.
    pub height: Option<SizeExpr>,
}

impl Constraint {
    /// Create an empty constraint (all `None`).
    pub fn new() -> Self {
        Self::default()
    }

    // ── fluent setters ────────────────────────────────────────────────────────

    /// Override the horizontal position.
    pub fn x(mut self, expr: SizeExpr) -> Self {
        self.x = Some(expr);
        self
    }

    /// Override the vertical position.
    pub fn y(mut self, expr: SizeExpr) -> Self {
        self.y = Some(expr);
        self
    }

    /// Override the width.
    pub fn width(mut self, expr: SizeExpr) -> Self {
        self.width = Some(expr);
        self
    }

    /// Override the height.
    pub fn height(mut self, expr: SizeExpr) -> Self {
        self.height = Some(expr);
        self
    }

    // ── shortcuts ─────────────────────────────────────────────────────────────

    /// Pin the widget to the right edge with `margin_x` clearance.
    /// `y`, `w` and `h` are convenience pixel values.
    pub fn pin_right(margin_x: f32, y: SizeExpr, w: f32, h: f32) -> Self {
        Self::new()
            .x(SizeExpr::from_right(margin_x))
            .y(y)
            .width(SizeExpr::px(w))
            .height(SizeExpr::px(h))
    }

    /// Pin the widget to the bottom edge with `margin_y` clearance.
    pub fn pin_bottom(x: SizeExpr, margin_y: f32, w: f32, h: f32) -> Self {
        Self::new()
            .x(x)
            .y(SizeExpr::from_bottom(margin_y))
            .width(SizeExpr::px(w))
            .height(SizeExpr::px(h))
    }

    /// Centre the widget horizontally inside the container.
    pub fn center_x(y: SizeExpr, w: f32, h: f32) -> Self {
        Self::new()
            .x(SizeExpr::center())
            .y(y)
            .width(SizeExpr::px(w))
            .height(SizeExpr::px(h))
    }

    /// Centre the widget both horizontally and vertically.
    pub fn center(w: f32, h: f32) -> Self {
        Self::new()
            .x(SizeExpr::center())
            .y(SizeExpr::center())
            .width(SizeExpr::px(w))
            .height(SizeExpr::px(h))
    }

    // ── resolution helper ─────────────────────────────────────────────────────

    /// Apply the constraint to a mutable rect `[x, y, w, h]` given the
    /// container / window dimensions.  Only `Some` fields are written.
    pub fn apply_to_rect(&self, rect: &mut [f32; 4], container_w: f32, container_h: f32) {
        // Resolve width/height first so they are available when resolving x/y
        // with FromRight / FromBottom / Center.
        if let Some(w_expr) = &self.width {
            rect[2] = w_expr.resolve(container_w, rect[2]);
        }
        if let Some(h_expr) = &self.height {
            rect[3] = h_expr.resolve(container_h, rect[3]);
        }
        if let Some(x_expr) = &self.x {
            rect[0] = x_expr.resolve(container_w, rect[2]);
        }
        if let Some(y_expr) = &self.y {
            rect[1] = y_expr.resolve(container_h, rect[3]);
        }
    }
}
