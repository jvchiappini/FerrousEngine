//! Lightweight representation of an editor "gizmo" (translate/rotate/scale
//! handles) and helper logic for driving object transforms.
//!
//! The core crate exposes only the abstract state and the math; rendering is
//! handled by `ferrous_renderer` and user interaction by the editor or app.
//!
//! The design is intentionally minimal so that non-editor tools can ignore
//! the module entirely; a game that never displays a gizmo still builds.

use glam::{Mat4, Vec3};

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
    /// The RGBA color used to tint this plane handle (dim version; renderer
    /// brightens it when highlighted).
    pub fn color(self) -> [f32; 3] {
        match self {
            Plane::XY => [0.2, 0.2, 1.0], // blue  — no red/green component
            Plane::XZ => [0.2, 1.0, 0.2], // green — no red/blue component
            Plane::YZ => [1.0, 0.2, 0.2], // red   — no green/blue component
        }
    }
    /// Highlighted (bright) color.
    pub fn highlight_color(self) -> [f32; 3] {
        match self {
            Plane::XY => [0.4, 0.4, 1.0],
            Plane::XZ => [0.4, 1.0, 0.4],
            Plane::YZ => [1.0, 0.4, 0.4],
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
}

impl Default for GizmoState {
    fn default() -> Self {
        Self {
            mode: GizmoMode::Translate,
            world_transform: Transform::default(),
            highlighted_axis: None,
            highlighted_plane: None,
            dragging: false,
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
