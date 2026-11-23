//! Overlay placement and window hosting.
//!
//! An overlay (a menu, a submenu, a dropdown, a tooltip, a popover) is placed
//! against an anchor rectangle. The solver is pure: given the anchor, the
//! preferred side, the content size and the work area (the monitor minus the
//! taskbar, or the canvas on a single-window host), it returns a [`Placement`]
//! that
//!
//! 1. flips to the opposite side when the preferred one does not fit;
//! 2. shifts along the other axis so the overlay stays inside the work area;
//! 3. clamps and requests an internal scroll when the content is larger than
//!    the work area in either direction.
//!
//! [`Placement::hosted_in`] then decides *where* the overlay lives: in the
//! window when it fits, in a popup window when it does not, and always in the
//! window on a host without popup windows (web, single-window hosts), where the
//! solver simply clamps to the canvas.
//!
//! The solver has no engine dependency; popup windows are the host's job.

use crate::geom::{Rect, Size};

/// The side of the anchor an overlay prefers to open on.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Side {
    /// Above the anchor.
    Top,
    /// Below the anchor.
    Bottom,
    /// Left of the anchor.
    Left,
    /// Right of the anchor (submenus).
    #[default]
    Right,
}

impl Side {
    /// The opposite side.
    #[must_use]
    pub const fn flipped(self) -> Self {
        match self {
            Self::Top => Self::Bottom,
            Self::Bottom => Self::Top,
            Self::Left => Self::Right,
            Self::Right => Self::Left,
        }
    }

    /// Whether this side grows horizontally (so the cross axis is vertical).
    #[must_use]
    pub const fn is_horizontal(self) -> bool {
        matches!(self, Self::Left | Self::Right)
    }
}

/// What the host can do with an overlay that does not fit.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct HostCapabilities {
    /// The host can create borderless popup windows that own the overlay.
    pub popup_windows: bool,
}

impl HostCapabilities {
    /// A desktop host: popups are available.
    #[must_use]
    pub const fn desktop() -> Self {
        Self {
            popup_windows: true,
        }
    }

    /// A web or single-window host: overlays clamp to the canvas.
    #[must_use]
    pub const fn single_window() -> Self {
        Self {
            popup_windows: false,
        }
    }
}

impl Default for HostCapabilities {
    /// Single-window until the engine can open popup windows: a host that
    /// claimed popups without drawing them would lose every overlay that does
    /// not fit. A host opts in with [`HostCapabilities::desktop`] once it
    /// really draws the popup.
    fn default() -> Self {
        Self::single_window()
    }
}

/// Where an overlay is drawn.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OverlayHost {
    /// Inside the main window's canvas.
    InWindow,
    /// In a popup window owned by the main window.
    Popup,
}

/// The result of [`place`].
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Placement {
    /// The rect to draw in the main window (clamped, and shortened when the
    /// content must scroll). Valid when the host is [`OverlayHost::InWindow`].
    pub rect: Rect,
    /// The unclamped rect on the chosen side. Valid as the popup window rect.
    pub preferred: Rect,
    /// The side the overlay ended up on.
    pub side: Side,
    /// Whether the side differs from the requested one.
    pub flipped: bool,
    /// Whether the content had to be shortened to fit the work area.
    pub scroll: bool,
    /// The content size the caller asked for.
    pub content: Size,
}

impl Placement {
    /// Whether the unclamped placement is fully inside `area` and the content
    /// itself fits there (no scroll).
    #[must_use]
    pub fn fits_in(&self, area: Rect) -> bool {
        self.content.width <= area.width
            && self.content.height <= area.height
            && self.preferred.x >= area.x
            && self.preferred.y >= area.y
            && self.preferred.right() <= area.right()
            && self.preferred.bottom() <= area.bottom()
    }

    /// Decides whether the overlay lives in the window or in a popup.
    ///
    /// A host without popup windows always hosts in the window; the caller then
    /// draws [`Placement::rect`], which is clamped to `window`.
    #[must_use]
    pub fn hosted_in(&self, window: Rect, capabilities: HostCapabilities) -> OverlayHost {
        if capabilities.popup_windows && !self.fits_in(window) {
            OverlayHost::Popup
        } else {
            OverlayHost::InWindow
        }
    }
}

/// Places `content` against `anchor`, preferring `preferred` and staying inside
/// `work_area`.
///
/// The returned [`Placement::rect`] is always fully inside `work_area` (unless
/// the work area itself is empty) and is shortened on the overflowing axis when
/// the content cannot fit, with [`Placement::scroll`] set.
#[must_use]
#[allow(clippy::cast_sign_loss, clippy::cast_possible_truncation)]
// reason: the solver works on small pixel rects; clamping to zero is explicit.
pub fn place(anchor: Rect, preferred: Side, content: Size, work_area: Rect) -> Placement {
    let content = Size::new(content.width.max(0.0), content.height.max(0.0));
    let side = choose_side(anchor, preferred, content, work_area);
    let preferred_rect = rect_on_side(anchor, side, content);
    let flipped = side != preferred;

    // Shift along the cross axis, then clamp both axes into the work area.
    let mut rect = preferred_rect;
    if side.is_horizontal() {
        rect.y = clamp_axis(rect.y, rect.height, work_area.y, work_area.height);
        rect.x = clamp_axis(rect.x, rect.width, work_area.x, work_area.width);
    } else {
        rect.x = clamp_axis(rect.x, rect.width, work_area.x, work_area.width);
        rect.y = clamp_axis(rect.y, rect.height, work_area.y, work_area.height);
    }

    // Scroll when the content itself is larger than the work area.
    let mut scroll = false;
    if rect.height > work_area.height {
        rect.height = work_area.height.max(0.0);
        rect.y = work_area.y;
        scroll = true;
    }
    if rect.width > work_area.width {
        rect.width = work_area.width.max(0.0);
        rect.x = work_area.x;
        scroll = true;
    }

    Placement {
        rect,
        preferred: preferred_rect,
        side,
        flipped,
        scroll,
        content,
    }
}

/// Picks the preferred side, or its opposite when only the opposite fits.
fn choose_side(anchor: Rect, preferred: Side, content: Size, work_area: Rect) -> Side {
    if side_fits(anchor, preferred, content, work_area) {
        return preferred;
    }
    let opposite = preferred.flipped();
    if side_fits(anchor, opposite, content, work_area) {
        opposite
    } else {
        preferred
    }
}

/// Whether `content` placed on `side` of `anchor` fits in `work_area`.
fn side_fits(anchor: Rect, side: Side, content: Size, work_area: Rect) -> bool {
    let rect = rect_on_side(anchor, side, content);
    rect.x >= work_area.x
        && rect.y >= work_area.y
        && rect.right() <= work_area.right()
        && rect.bottom() <= work_area.bottom()
        && content.width <= work_area.width
        && content.height <= work_area.height
}

/// The unclamped rect for `content` on `side` of `anchor`.
fn rect_on_side(anchor: Rect, side: Side, content: Size) -> Rect {
    match side {
        Side::Top => Rect::new(
            anchor.x,
            anchor.y - content.height,
            content.width,
            content.height,
        ),
        Side::Bottom => Rect::new(anchor.x, anchor.bottom(), content.width, content.height),
        Side::Left => Rect::new(
            anchor.x - content.width,
            anchor.y,
            content.width,
            content.height,
        ),
        Side::Right => Rect::new(anchor.right(), anchor.y, content.width, content.height),
    }
}

/// Clamps `start` so a span of `extent` stays inside `[origin, origin + room]`.
fn clamp_axis(start: f32, extent: f32, origin: f32, room: f32) -> f32 {
    let max = origin + (room - extent).max(0.0);
    start.clamp(origin.min(max), max)
}

#[cfg(test)]
mod tests {
    #![allow(clippy::float_cmp)] // reason: exact token arithmetic in placement tests
    use super::*;

    const WORK: Rect = Rect {
        x: 0.0,
        y: 0.0,
        width: 1000.0,
        height: 800.0,
    };

    fn anchor(x: f32, y: f32, w: f32, h: f32) -> Rect {
        Rect::new(x, y, w, h)
    }

    fn size(w: f32, h: f32) -> Size {
        Size::new(w, h)
    }

    #[test]
    fn a_centred_anchor_keeps_the_preferred_side() {
        let placement = place(
            anchor(400.0, 300.0, 100.0, 30.0),
            Side::Bottom,
            size(180.0, 200.0),
            WORK,
        );
        assert_eq!(placement.side, Side::Bottom);
        assert!(!placement.flipped);
        assert!(!placement.scroll);
        assert_eq!(placement.rect, Rect::new(400.0, 330.0, 180.0, 200.0));
    }

    #[test]
    fn a_bottom_anchor_flips_the_dropdown_up() {
        let placement = place(
            anchor(400.0, 750.0, 100.0, 40.0),
            Side::Bottom,
            size(180.0, 200.0),
            WORK,
        );
        assert_eq!(placement.side, Side::Top);
        assert!(placement.flipped);
        assert_eq!(placement.rect.y, 550.0);
        assert_eq!(placement.rect.bottom(), 750.0);
    }

    #[test]
    fn a_right_edge_anchor_flips_a_submenu_left() {
        let placement = place(
            anchor(900.0, 100.0, 90.0, 30.0),
            Side::Right,
            size(200.0, 160.0),
            WORK,
        );
        assert_eq!(placement.side, Side::Left);
        assert!(placement.flipped);
        assert_eq!(placement.rect.right(), 900.0);
    }

    #[test]
    fn a_left_edge_anchor_flips_a_submenu_right() {
        let placement = place(
            anchor(10.0, 100.0, 90.0, 30.0),
            Side::Left,
            size(200.0, 160.0),
            WORK,
        );
        assert_eq!(placement.side, Side::Right);
        assert!(placement.flipped);
        assert_eq!(placement.rect.x, 100.0);
    }

    #[test]
    fn a_top_anchor_flips_the_tooltip_down() {
        let placement = place(
            anchor(400.0, 10.0, 80.0, 20.0),
            Side::Top,
            size(200.0, 120.0),
            WORK,
        );
        assert_eq!(placement.side, Side::Bottom);
        assert_eq!(placement.rect.y, 30.0);
    }

    #[test]
    fn every_corner_stays_inside_the_work_area() {
        let corners = [
            (anchor(0.0, 0.0, 60.0, 30.0), Side::Bottom),
            (anchor(940.0, 0.0, 60.0, 30.0), Side::Bottom),
            (anchor(0.0, 770.0, 60.0, 30.0), Side::Top),
            (anchor(940.0, 770.0, 60.0, 30.0), Side::Top),
        ];
        for (rect, side) in corners {
            let placement = place(rect, side, size(180.0, 120.0), WORK);
            assert!(placement.rect.x >= WORK.x);
            assert!(placement.rect.y >= WORK.y);
            assert!(placement.rect.right() <= WORK.right() + 0.001);
            assert!(placement.rect.bottom() <= WORK.bottom() + 0.001);
            assert!(!placement.scroll);
        }
    }

    #[test]
    fn a_dropdown_shifts_left_near_the_right_edge() {
        // Preferred side fits vertically but not horizontally; it stays on the
        // bottom and is shifted left instead of flipping.
        let placement = place(
            anchor(900.0, 100.0, 60.0, 30.0),
            Side::Bottom,
            size(300.0, 160.0),
            WORK,
        );
        assert_eq!(placement.side, Side::Bottom);
        assert!(!placement.flipped);
        assert_eq!(placement.rect.right(), WORK.right());
    }

    #[test]
    fn a_dropdown_taller_than_the_screen_scrolls() {
        let placement = place(
            anchor(400.0, 100.0, 100.0, 30.0),
            Side::Bottom,
            size(200.0, 1200.0),
            WORK,
        );
        assert!(placement.scroll);
        assert_eq!(placement.rect.height, WORK.height);
        assert_eq!(placement.rect.y, WORK.y);
        assert!(!placement.fits_in(WORK));
    }

    #[test]
    fn a_popup_is_chosen_only_when_it_cannot_fit() {
        let fits = place(
            anchor(400.0, 300.0, 100.0, 30.0),
            Side::Bottom,
            size(180.0, 200.0),
            WORK,
        );
        assert_eq!(
            fits.hosted_in(WORK, HostCapabilities::desktop()),
            OverlayHost::InWindow
        );

        // A near-full-width menu at the right edge: neither side fits.
        let wide = place(
            anchor(900.0, 100.0, 80.0, 30.0),
            Side::Bottom,
            size(980.0, 160.0),
            WORK,
        );
        assert_eq!(
            wide.hosted_in(WORK, HostCapabilities::desktop()),
            OverlayHost::Popup
        );
        // A single-window host draws the clamped version instead.
        assert_eq!(
            wide.hosted_in(WORK, HostCapabilities::single_window()),
            OverlayHost::InWindow
        );
        assert!(wide.rect.x >= WORK.x && wide.rect.right() <= WORK.right());
    }

    #[test]
    fn side_flips_are_involutive() {
        assert_eq!(Side::Top.flipped(), Side::Bottom);
        assert_eq!(Side::Bottom.flipped(), Side::Top);
        assert_eq!(Side::Left.flipped(), Side::Right);
        assert_eq!(Side::Right.flipped(), Side::Left);
    }
}
