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
        let b = ((hex >> 8)  & 0xFF) as f32 / 255.0;
        let a = ((hex)        & 0xFF) as f32 / 255.0;
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

    pub const WHITE:       Self = Self::rgb(1.0, 1.0, 1.0);
    pub const BLACK:       Self = Self::rgb(0.0, 0.0, 0.0);
    pub const TRANSPARENT: Self = Self::rgba(0.0, 0.0, 0.0, 0.0);

    pub const RED:         Self = Self::rgb(1.0, 0.0, 0.0);
    pub const GREEN:       Self = Self::rgb(0.0, 1.0, 0.0);
    pub const BLUE:        Self = Self::rgb(0.0, 0.0, 1.0);

    pub const YELLOW:      Self = Self::rgb(1.0, 1.0, 0.0);
    pub const CYAN:        Self = Self::rgb(0.0, 1.0, 1.0);
    pub const MAGENTA:     Self = Self::rgb(1.0, 0.0, 1.0);

    pub const ORANGE:      Self = Self::rgb(1.0, 0.5, 0.0);
    pub const PURPLE:      Self = Self::rgb(0.5, 0.0, 0.5);
    pub const PINK:        Self = Self::rgb(1.0, 0.41, 0.71);

    pub const DARK_GRAY:   Self = Self::rgb(0.25, 0.25, 0.25);
    pub const GRAY:        Self = Self::rgb(0.5, 0.5, 0.5);
    pub const LIGHT_GRAY:  Self = Self::rgb(0.75, 0.75, 0.75);

    pub const SKY_BLUE:    Self = Self::rgb(0.53, 0.81, 0.92);
    pub const LIME:        Self = Self::rgb(0.0, 0.8, 0.0);
    pub const TEAL:        Self = Self::rgb(0.0, 0.5, 0.5);
    pub const NAVY:        Self = Self::rgb(0.0, 0.0, 0.5);
    pub const BEIGE:       Self = Self::rgb(0.96, 0.96, 0.86);
    pub const BROWN:       Self = Self::rgb(0.55, 0.27, 0.07);
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
}
