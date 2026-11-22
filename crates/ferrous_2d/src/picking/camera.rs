//! The 2D camera the picker and the host share.
//!
//! Pure2D draws in world units where one unit at zoom 1 is one logical pixel on
//! the axis, with `y` down (the same convention `ferrous_renderer`'s ortho
//! projection uses for the immediate 2D path). This camera is the inverse
//! mapping used by picking, marquee and zoom-to-cursor; keeping it here means a
//! host never writes its own screen/world maths.
//!
//! All values are logical pixels. A viewport is the drawable rectangle in
//! screen space; the camera maps its centre to the viewport centre.

use glam::Vec2;

use ferrous_core::spatial::Aabb;

/// A 2D orthographic camera: centre, zoom and viewport.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PickCamera {
    /// The world point at the viewport centre.
    pub center: Vec2,
    /// Zoom; larger is closer.
    pub zoom: f32,
    /// The viewport size in logical pixels.
    pub viewport: Vec2,
}

impl PickCamera {
    /// A camera looking at the origin at zoom 1.
    #[must_use]
    pub fn new(viewport: Vec2) -> Self {
        Self {
            center: Vec2::ZERO,
            zoom: 1.0,
            viewport,
        }
    }

    /// Sets the viewport and keeps the centre.
    pub fn set_viewport(&mut self, viewport: Vec2) {
        self.viewport = viewport.max(Vec2::splat(1.0));
    }

    /// A screen point (top-left origin) to world space.
    #[must_use]
    pub fn screen_to_world(&self, screen: Vec2) -> Vec2 {
        (screen - self.viewport * 0.5) / self.zoom.max(1e-6) + self.center
    }

    /// A world point to screen space (top-left origin).
    #[must_use]
    pub fn world_to_screen(&self, world: Vec2) -> Vec2 {
        (world - self.center) * self.zoom.max(1e-6) + self.viewport * 0.5
    }

    /// Screen units per world unit (the zoom).
    #[must_use]
    pub fn scale(&self) -> f32 {
        self.zoom.max(1e-6)
    }

    /// Converts a screen-space tolerance (pixels) to world units.
    #[must_use]
    pub fn screen_to_world_length(&self, length: f32) -> f32 {
        length / self.scale()
    }

    /// The world rectangle visible in the viewport.
    #[must_use]
    pub fn visible_world(&self) -> Aabb {
        let half = self.viewport * 0.5 / self.scale();
        Aabb {
            min: self.center - half,
            max: self.center + half,
        }
    }

    /// Zooms while keeping the world point under `cursor` fixed.
    ///
    /// `cursor` is a screen point. The zoom is clamped to `min..=max`.
    pub fn zoom_to_cursor(&mut self, cursor: Vec2, zoom: f32, min: f32, max: f32) {
        let before = self.screen_to_world(cursor);
        self.zoom = zoom.clamp(min.max(1e-6), max.max(min));
        let after = self.screen_to_world(cursor);
        self.center += before - after;
    }

    /// Pans by a screen-space delta (drag direction).
    pub fn pan(&mut self, delta_screen: Vec2) {
        self.center -= delta_screen / self.scale();
    }

    /// Fits `bounds` in the viewport with `padding` screen pixels.
    pub fn fit(&mut self, bounds: Aabb, padding: f32) {
        if bounds.is_empty() {
            return;
        }
        let size = bounds.size().max(Vec2::splat(1.0));
        let available = (self.viewport - Vec2::splat(padding * 2.0)).max(Vec2::splat(1.0));
        self.zoom = (available.x / size.x).min(available.y / size.y).max(1e-6);
        self.center = bounds.center();
    }

    /// The screen-to-world mapping as a matrix (useful for picking tools).
    #[must_use]
    pub fn screen_to_world_matrix(&self) -> glam::Affine2 {
        let scale = self.scale();
        glam::Affine2::from_scale_angle_translation(
            Vec2::splat(1.0 / scale),
            0.0,
            self.center - self.viewport * 0.5 / scale,
        )
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::float_cmp)] // reason: exact pixel arithmetic in tests

    use super::PickCamera;
    use ferrous_core::spatial::Aabb;
    use glam::Vec2;

    #[test]
    fn screen_and_world_round_trip() {
        let mut camera = PickCamera::new(Vec2::new(800.0, 600.0));
        camera.center = Vec2::new(100.0, 50.0);
        camera.zoom = 2.0;
        for screen in [
            Vec2::ZERO,
            Vec2::new(800.0, 0.0),
            Vec2::new(400.0, 300.0),
            Vec2::new(123.0, 456.0),
        ] {
            let world = camera.screen_to_world(screen);
            assert!((camera.world_to_screen(world) - screen).length() < 1e-4);
        }
        assert_eq!(
            camera.screen_to_world(Vec2::new(400.0, 300.0)),
            camera.center
        );
    }

    #[test]
    fn zoom_to_cursor_keeps_the_point_fixed() {
        let mut camera = PickCamera::new(Vec2::new(800.0, 600.0));
        let cursor = Vec2::new(600.0, 200.0);
        let before = camera.screen_to_world(cursor);
        camera.zoom_to_cursor(cursor, 3.0, 0.1, 8.0);
        let after = camera.screen_to_world(cursor);
        assert!((before - after).length() < 1e-4);
        camera.zoom_to_cursor(cursor, 100.0, 0.1, 8.0);
        assert_eq!(camera.zoom, 8.0);
    }

    #[test]
    fn pan_and_fit() {
        let mut camera = PickCamera::new(Vec2::new(800.0, 600.0));
        camera.zoom = 2.0;
        camera.pan(Vec2::new(20.0, 40.0));
        assert_eq!(camera.center, Vec2::new(-10.0, -20.0));
        let bounds = Aabb {
            min: Vec2::new(-100.0, -50.0),
            max: Vec2::new(100.0, 50.0),
        };
        camera.fit(bounds, 20.0);
        assert_eq!(camera.center, bounds.center());
        assert!((camera.zoom - 3.8).abs() < 1e-5);
        assert_eq!(camera.visible_world().center(), bounds.center());
    }

    #[test]
    fn matrices_and_tolerances_agree() {
        let mut camera = PickCamera::new(Vec2::new(640.0, 480.0));
        camera.center = Vec2::new(12.0, -8.0);
        camera.zoom = 1.5;
        let matrix = camera.screen_to_world_matrix();
        let point = Vec2::new(100.0, 200.0);
        assert!((matrix.transform_point2(point) - camera.screen_to_world(point)).length() < 1e-3);
        assert!((camera.screen_to_world_length(3.0) - 2.0).abs() < 1e-6);
        assert_eq!(camera.scale(), 1.5);
    }
}
