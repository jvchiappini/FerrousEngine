//! RGBA colour type used throughout the engine.
//!
//! Stored as four `f32` values in linear light (0.0 – 1.0).  Common colours
//! are available as associated constants so you never have to remember the
//! exact float values.
//!
//! # Example
//! ```rust,ignore
//! use ferrous_core::Color;
//!
//! let red   = Color::RED;
//! let teal  = Color::rgb(0.0, 0.5, 0.5);
//! let glass = Color::rgba(0.2, 0.8, 1.0, 0.4);
//! let sky   = Color::from_hex(0x87CEEBFF);
//!
//! let [r, g, b, a] = sky.to_array();
//! ```

/// Linear-space RGBA colour.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Color {
    pub r: f32,
    pub g: f32,
    pub b: f32,
    pub a: f32,
}

impl Default for Color {
    fn default() -> Self {
        Self::WHITE
    }
}

impl Color {
    // ── Constructors ────────────────────────────────────────────────────────

    /// Opaque colour from red, green, blue components.
    #[inline]
    pub const fn rgb(r: f32, g: f32, b: f32) -> Self {
        Self { r, g, b, a: 1.0 }
    }

    /// Colour from all four components.
    #[inline]
    pub const fn rgba(r: f32, g: f32, b: f32, a: f32) -> Self {
        Self { r, g, b, a }
    }

    /// Construct from a packed `0xRRGGBBAA` hexadecimal value.
    ///
    /// ```rust,ignore
    /// let coral = Color::from_hex(0xFF6B6BFF);
    /// ```
    pub fn from_hex(hex: u32) -> Self {
        let r = ((hex >> 24) & 0xFF) as f32 / 255.0;
        let g = ((hex >> 16) & 0xFF) as f32 / 255.0;
        let b = ((hex >> 8) & 0xFF) as f32 / 255.0;
        let a = ((hex) & 0xFF) as f32 / 255.0;
        Self { r, g, b, a }
    }

    /// Construct from 8-bit components.
    pub fn from_rgb8(r: u8, g: u8, b: u8) -> Self {
        Self::rgb(r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0)
    }

    /// Construct from 8-bit components including alpha.
    pub fn from_rgba8(r: u8, g: u8, b: u8, a: u8) -> Self {
        Self::rgba(
            r as f32 / 255.0,
            g as f32 / 255.0,
            b as f32 / 255.0,
            a as f32 / 255.0,
        )
    }

    /// Construct from sRGB (gamma-encoded) components.
    ///
    /// Most colour pickers and design tools work in sRGB.  This constructor
    /// applies the standard gamma‑2.2 approximation so that you never need to
    /// call `.powf(2.2)` manually:
    ///
    /// ```rust,ignore
    /// // Old (error-prone):
    /// let red = Color::rgb(0.9f32.powf(2.2), 0.1f32.powf(2.2), 0.1f32.powf(2.2));
    ///
    /// // New:
    /// let red = Color::srgb(0.9, 0.1, 0.1);
    /// ```
    #[inline]
    pub fn srgb(r: f32, g: f32, b: f32) -> Self {
        Self::rgb(r.powf(2.2), g.powf(2.2), b.powf(2.2))
    }

    /// Construct from sRGB components including alpha.
    /// RGB channels are gamma-corrected; alpha is passed through unchanged.
    #[inline]
    pub fn srgba(r: f32, g: f32, b: f32, a: f32) -> Self {
        Self::rgba(r.powf(2.2), g.powf(2.2), b.powf(2.2), a)
    }

    // ── Conversions ─────────────────────────────────────────────────────────

    /// Returns `[r, g, b, a]`.
    #[inline]
    pub fn to_array(self) -> [f32; 4] {
        [self.r, self.g, self.b, self.a]
    }

    /// Returns `[r, g, b]` (alpha discarded).
    #[inline]
    pub fn to_rgb_array(self) -> [f32; 3] {
        [self.r, self.g, self.b]
    }

    /// Convert to a `wgpu::Color` for use as a clear / blend value.
    pub fn to_wgpu(self) -> wgpu::Color {
        wgpu::Color {
            r: self.r as f64,
            g: self.g as f64,
            b: self.b as f64,
            a: self.a as f64,
        }
    }

    // ── Modifiers ───────────────────────────────────────────────────────────

    /// Return a new colour with the alpha channel replaced.
    #[inline]
    pub fn with_alpha(self, a: f32) -> Self {
        Self { a, ..self }
    }

    /// Linearly interpolate towards `other` by factor `t` (0 = self, 1 = other).
    pub fn lerp(self, other: Self, t: f32) -> Self {
        Self {
            r: self.r + (other.r - self.r) * t,
            g: self.g + (other.g - self.g) * t,
            b: self.b + (other.b - self.b) * t,
            a: self.a + (other.a - self.a) * t,
        }
    }

    /// Return a brighter version by multiplying RGB by `factor`.
    pub fn brighten(self, factor: f32) -> Self {
        Self {
            r: (self.r * factor).min(1.0),
            g: (self.g * factor).min(1.0),
            b: (self.b * factor).min(1.0),
            a: self.a,
        }
    }

    // ── Palette ─────────────────────────────────────────────────────────────

    pub const WHITE: Self = Self::rgb(1.0, 1.0, 1.0);
    pub const BLACK: Self = Self::rgb(0.0, 0.0, 0.0);
    pub const TRANSPARENT: Self = Self::rgba(0.0, 0.0, 0.0, 0.0);

    pub const RED: Self = Self::rgb(1.0, 0.0, 0.0);
    pub const GREEN: Self = Self::rgb(0.0, 1.0, 0.0);
    pub const BLUE: Self = Self::rgb(0.0, 0.0, 1.0);

    pub const YELLOW: Self = Self::rgb(1.0, 1.0, 0.0);
    pub const CYAN: Self = Self::rgb(0.0, 1.0, 1.0);
    pub const MAGENTA: Self = Self::rgb(1.0, 0.0, 1.0);

    pub const ORANGE: Self = Self::rgb(1.0, 0.5, 0.0);
    pub const PURPLE: Self = Self::rgb(0.5, 0.0, 0.5);
    pub const PINK: Self = Self::rgb(1.0, 0.41, 0.71);

    pub const DARK_GRAY: Self = Self::rgb(0.25, 0.25, 0.25);
    pub const GRAY: Self = Self::rgb(0.5, 0.5, 0.5);
    pub const LIGHT_GRAY: Self = Self::rgb(0.75, 0.75, 0.75);

    pub const SKY_BLUE: Self = Self::rgb(0.53, 0.81, 0.92);
    pub const LIME: Self = Self::rgb(0.0, 0.8, 0.0);
    pub const TEAL: Self = Self::rgb(0.0, 0.5, 0.5);
    pub const NAVY: Self = Self::rgb(0.0, 0.0, 0.5);
    pub const BEIGE: Self = Self::rgb(0.96, 0.96, 0.86);
    pub const BROWN: Self = Self::rgb(0.55, 0.27, 0.07);

    /// Warm white sunlight — slightly yellow‑orange tint.
    /// Equivalent to ~5500 K colour temperature in linear space.
    pub const WARM_WHITE: Self = Self::rgb(1.0, 0.97, 0.90);
}

impl From<[f32; 4]> for Color {
    fn from(a: [f32; 4]) -> Self {
        Self::rgba(a[0], a[1], a[2], a[3])
    }
}

impl From<[f32; 3]> for Color {
    fn from(a: [f32; 3]) -> Self {
        Self::rgb(a[0], a[1], a[2])
    }
}

impl From<Color> for [f32; 4] {
    fn from(c: Color) -> Self {
        c.to_array()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hex_roundtrip() {
        let c = Color::from_hex(0xFF8000FF);
        assert!((c.r - 1.0).abs() < 0.01);
        assert!((c.g - 0.502).abs() < 0.01);
        assert!((c.b - 0.0).abs() < 0.01);
        assert!((c.a - 1.0).abs() < 0.01);
    }

    #[test]
    fn lerp_halfway() {
        let mid = Color::BLACK.lerp(Color::WHITE, 0.5);
        assert!((mid.r - 0.5).abs() < 1e-6);
    }

    // ── Phase 4.5: sRGB and WARM_WHITE tests ─────────────────────────────

    #[test]
    fn srgb_pure_white_is_linear_white() {
        let c = Color::srgb(1.0, 1.0, 1.0);
        assert!((c.r - 1.0).abs() < 1e-6);
        assert!((c.g - 1.0).abs() < 1e-6);
        assert!((c.b - 1.0).abs() < 1e-6);
        assert!((c.a - 1.0).abs() < 1e-6);
    }

    #[test]
    fn srgb_pure_black_is_linear_black() {
        let c = Color::srgb(0.0, 0.0, 0.0);
        assert_eq!(c.r, 0.0);
        assert_eq!(c.g, 0.0);
        assert_eq!(c.b, 0.0);
    }

    #[test]
    fn srgb_applies_gamma_correction() {
        // sRGB 0.5 in linear should be ~0.5^2.2 ≈ 0.2176
        let c = Color::srgb(0.5, 0.5, 0.5);
        let expected = 0.5_f32.powf(2.2);
        assert!((c.r - expected).abs() < 1e-5, "r={}, expected={}", c.r, expected);
    }

    #[test]
    fn srgba_preserves_alpha() {
        let c = Color::srgba(1.0, 1.0, 1.0, 0.5);
        assert!((c.a - 0.5).abs() < 1e-6);
        assert!((c.r - 1.0).abs() < 1e-6); // sRGB 1.0 → linear 1.0
    }

    #[test]
    fn warm_white_exists_and_is_near_white() {
        let w = Color::WARM_WHITE;
        assert!((w.r - 1.0).abs() < 1e-6);
        // g and b slightly below 1.0
        assert!(w.g > 0.9 && w.g <= 1.0, "g={}", w.g);
        assert!(w.b > 0.8 && w.b < 1.0, "b={}", w.b);
        assert!((w.a - 1.0).abs() < 1e-6);
    }
}
