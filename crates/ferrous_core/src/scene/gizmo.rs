//! Lightweight representation of an editor "gizmo" (translate/rotate/scale
//! handles) and helper logic for driving object transforms.
#![cfg(feature = "ecs")]
//!
//! The core crate exposes only the abstract state and the math; rendering is
//! handled by `ferrous_renderer` and user interaction by the editor or app.
//!
//! The design is intentionally minimal so that non-editor tools can ignore
//! the module entirely; a game that never displays a gizmo still builds.

use glam::{Mat4, Vec3};

// ---------------------------------------------------------------------------
// GizmoStyle
// ---------------------------------------------------------------------------

/// Per-axis color pair (normal + highlighted).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct AxisColors {
    /// Color when the axis is idle. RGB, each component in `[0, 1]`.
    pub normal: [f32; 3],
    /// Color when the axis is hovered or actively dragged.
    pub highlighted: [f32; 3],
}

impl AxisColors {
    pub const fn new(normal: [f32; 3], highlighted: [f32; 3]) -> Self {
        Self {
            normal,
            highlighted,
        }
    }
}

/// Per-plane color pair (normal + highlighted).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PlaneColors {
    /// Fill / outline color when the plane handle is idle. RGBA.
    pub normal: [f32; 4],
    /// Fill / outline color when the plane handle is hovered or dragged. RGBA.
    pub highlighted: [f32; 4],
}

impl PlaneColors {
    pub const fn new(normal: [f32; 4], highlighted: [f32; 4]) -> Self {
        Self {
            normal,
            highlighted,
        }
    }
}

/// Visual style for a single gizmo instance.
///
/// All fields have sensible defaults (Blender-like colours, arrows enabled,
/// standard proportions).  Override any field before calling
/// [`AppContext::update_gizmo`] to get a completely custom look.
///
/// # Example
/// ```rust,ignore
/// let mut gizmo = GizmoState::default();
/// gizmo.style.arm_length  = 2.0;
/// gizmo.style.show_arrows = false;
/// gizmo.style.show_planes = false;
/// gizmo.style.x_axis.normal      = [1.0, 0.0, 0.0];
/// gizmo.style.x_axis.highlighted = [1.0, 1.0, 0.0];
/// ```
#[derive(Debug, Clone, PartialEq)]
pub struct GizmoStyle {
    // ── Geometry ────────────────────────────────────────────────────────────
    /// How far each axis arm extends from the origin, in world units.
    /// Default: `1.5`
    pub arm_length: f32,

    /// Fraction of `arm_length` at which the plane square handles begin.
    /// Default: `0.25`
    pub plane_offset_ratio: f32,

    /// Fraction of `arm_length` that defines the side length of a plane square.
    /// Default: `0.22`
    pub plane_size_ratio: f32,

    // ── Arrows ──────────────────────────────────────────────────────────────
    /// Draw an arrowhead at the tip of each axis arm.  Default: `true`
    pub show_arrows: bool,

    /// Half-angle of each arrowhead cone, in degrees.  Default: `20.0`
    pub arrow_half_angle_deg: f32,

    /// Length of the arrowhead as a fraction of `arm_length`.  Default: `0.12`
    pub arrow_length_ratio: f32,

    // ── Plane handles ───────────────────────────────────────────────────────
    /// Draw the small square plane-move handles.  Default: `true`
    pub show_planes: bool,

    // ── Axis colours ────────────────────────────────────────────────────────
    /// X-axis (red) colors.
    pub x_axis: AxisColors,
    /// Y-axis (green) colors.
    pub y_axis: AxisColors,
    /// Z-axis (blue) colors.
    pub z_axis: AxisColors,

    // ── Plane colours ───────────────────────────────────────────────────────
    /// XY-plane colors.
    pub xy_plane: PlaneColors,
    /// XZ-plane colors.
    pub xz_plane: PlaneColors,
    /// YZ-plane colors.
    pub yz_plane: PlaneColors,
}

impl Default for GizmoStyle {
    fn default() -> Self {
        Self {
            arm_length: 1.5,
            plane_offset_ratio: 0.25,
            plane_size_ratio: 0.22,

            show_arrows: true,
            arrow_half_angle_deg: 20.0,
            arrow_length_ratio: 0.12,

            show_planes: true,

            x_axis: AxisColors::new([1.0, 0.2, 0.2], [1.0, 1.0, 0.0]),
            y_axis: AxisColors::new([0.2, 1.0, 0.2], [1.0, 1.0, 0.0]),
            z_axis: AxisColors::new([0.2, 0.4, 1.0], [1.0, 1.0, 0.0]),

            xy_plane: PlaneColors::new([0.2, 0.2, 1.0, 0.5], [0.4, 0.4, 1.0, 0.8]),
            xz_plane: PlaneColors::new([0.2, 1.0, 0.2, 0.5], [0.4, 1.0, 0.4, 0.8]),
            yz_plane: PlaneColors::new([1.0, 0.2, 0.2, 0.5], [1.0, 0.4, 0.4, 0.8]),
        }
    }
}

impl GizmoStyle {
    /// Computed plane offset in world units.
    #[inline]
    pub fn plane_offset(&self) -> f32 {
        self.arm_length * self.plane_offset_ratio
    }
    /// Computed plane square side length in world units.
    #[inline]
    pub fn plane_size(&self) -> f32 {
        self.arm_length * self.plane_size_ratio
    }
    /// Computed arrowhead length in world units.
    #[inline]
    pub fn arrow_length(&self) -> f32 {
        self.arm_length * self.arrow_length_ratio
    }

    /// Return the idle color of `axis`.
    pub fn axis_color(&self, axis: Axis) -> [f32; 3] {
        match axis {
            Axis::X => self.x_axis.normal,
            Axis::Y => self.y_axis.normal,
            Axis::Z => self.z_axis.normal,
        }
    }
    /// Return the highlighted color of `axis`.
    pub fn axis_highlight(&self, axis: Axis) -> [f32; 3] {
        match axis {
            Axis::X => self.x_axis.highlighted,
            Axis::Y => self.y_axis.highlighted,
            Axis::Z => self.z_axis.highlighted,
        }
    }

    /// Return the idle RGBA color of `plane`.
    pub fn plane_color(&self, plane: Plane) -> [f32; 4] {
        match plane {
            Plane::XY => self.xy_plane.normal,
            Plane::XZ => self.xz_plane.normal,
            Plane::YZ => self.yz_plane.normal,
        }
    }
    /// Return the highlighted RGBA color of `plane`.
    pub fn plane_highlight(&self, plane: Plane) -> [f32; 4] {
        match plane {
            Plane::XY => self.xy_plane.highlighted,
            Plane::XZ => self.xz_plane.highlighted,
            Plane::YZ => self.yz_plane.highlighted,
        }
    }
}

/// Which operation the user is currently performing with the gizmo.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GizmoMode {
    /// Move the target along world axes.
    Translate,
    /// Rotate the target about one of the world axes.
    Rotate,
    /// Scale the target uniformly or non‑uniformly.
    Scale,
}

/// One of the three principal axes (used both for highlighting and
/// constraining operations).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Axis {
    X,
    Y,
    Z,
}

/// One of the three principal planes — used for the small square plane-move
/// handles that sit between two axis arms.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Plane {
    /// Move in the XY plane (constrained perpendicular to Z).
    XY,
    /// Move in the XZ plane (constrained perpendicular to Y).
    XZ,
    /// Move in the YZ plane (constrained perpendicular to X).
    YZ,
}

impl Plane {
    /// The two world-space unit vectors that span this plane.
    pub fn axes(self) -> (Vec3, Vec3) {
        match self {
            Plane::XY => (Vec3::X, Vec3::Y),
            Plane::XZ => (Vec3::X, Vec3::Z),
            Plane::YZ => (Vec3::Y, Vec3::Z),
        }
    }
}

/// Mutable state for a single gizmo instance.
///
/// The editor keeps one of these around and updates it each frame based on
/// mouse/keyboard input.  The renderer only needs the `world_transform` and
/// `mode` fields in order to draw the correct handles; it is not involved in
/// the interaction logic.
#[derive(Debug, Clone)]
pub struct GizmoState {
    /// Current mode (translate/rotate/scale).
    pub mode: GizmoMode,

    /// The world transform of the gizmo's origin.  Typically this matches the
    /// transform of the selected object, but it can be moved independently for
    /// things like pivot adjustment.
    pub world_transform: Transform,

    /// Local-space offset from the entity's position to the rotation pivot.
    /// `Vec3::ZERO` means the pivot sits exactly on the entity's origin.
    /// The effective world-space pivot is always `entity_position + pivot_offset`,
    /// so moving the entity automatically moves the pivot with it.
    pub pivot_offset: Vec3,

    /// Optional axis that is currently highlighted (e.g. under the mouse
    /// cursor) or actively being dragged.  `None` means "no constraint".
    pub highlighted_axis: Option<Axis>,

    /// Optional plane handle that is currently highlighted or being dragged.
    /// Mutually exclusive with `highlighted_axis`.
    pub highlighted_plane: Option<Plane>,

    /// Whether a drag operation is in progress.  The editor toggles this when
    /// the user presses/releases the mouse button; the value itself is not
    /// needed by the renderer.
    pub dragging: bool,

    /// Visual style — controls colors, arm length, arrowheads, plane squares.
    /// All fields have sensible defaults; override any of them to customize.
    pub style: GizmoStyle,
}

impl Default for GizmoState {
    fn default() -> Self {
        Self {
            mode: GizmoMode::Translate,
            world_transform: Transform::default(),
            pivot_offset: Vec3::ZERO,
            highlighted_axis: None,
            highlighted_plane: None,
            dragging: false,
            style: GizmoStyle::default(),
        }
    }
}

impl GizmoState {
    /// Update `world_transform` from an arbitrary object transform.  Call this
    /// each frame before drawing so that the gizmo follows the target.
    pub fn update_world_transform(&mut self, t: Transform) {
        self.world_transform = t;
    }

    /// Full model matrix (position + rotation + scale) of the entity.
    pub fn world_matrix(&self) -> Mat4 {
        self.world_transform.matrix()
    }

    /// Translation-only matrix — strips the entity's scale and rotation so the
    /// gizmo handles always have a fixed size and are aligned to world axes.
    /// Use this when queuing a [`GizmoDraw`] so the handles don't grow with
    /// the object.
    pub fn position_matrix(&self) -> Mat4 {
        Mat4::from_translation(self.world_transform.position)
    }

    /// Effective world-space pivot used for rotation.
    ///
    /// Always equal to `entity_position + entity_rotation * pivot_offset`.
    #[inline]
    pub fn effective_pivot(&self) -> Vec3 {
        self.world_transform.position + self.world_transform.rotation * self.pivot_offset
    }
}

/// Small helper that converts an `Axis` to the corresponding unit vector.
pub fn axis_vector(axis: Axis) -> Vec3 {
    match axis {
        Axis::X => Vec3::X,
        Axis::Y => Vec3::Y,
        Axis::Z => Vec3::Z,
    }
}

// ---------------------------------------------------------------------------
// `Transform` is re-exported from the `transform` module so callers don't have
// to import two paths when they want to use both types in a sketch.
//
use crate::transform::Transform;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_state_is_translate() {
        let g = GizmoState::default();
        assert_eq!(g.mode, GizmoMode::Translate);
        assert_eq!(g.highlighted_axis, None);
    }

    #[test]
    fn axis_vector_matches() {
        assert_eq!(axis_vector(Axis::X), Vec3::X);
        assert_eq!(axis_vector(Axis::Y), Vec3::Y);
        assert_eq!(axis_vector(Axis::Z), Vec3::Z);
    }
}
