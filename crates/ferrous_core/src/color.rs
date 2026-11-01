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

use serde::{Deserialize, Serialize};

/// Linear-space RGBA colour.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
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

    /// Construct from a CSS-style hex string: `"#RGB"`, `"#RRGGBB"`, or
    /// `"#RRGGBBAA"`.  The leading `#` is optional.
    ///
    /// **No transfer function is applied.**  Each byte is divided by 255 and
    /// stored as-is, so `"#808080"` yields `0.5019` in the linear-light
    /// fields even though sRGB `0x80` is `0.2159` in linear light.  This is
    /// historical behaviour that existing callers depend on; use
    /// [`Color::from_srgb_hex`] when the string is an sRGB colour (what a
    /// design tool shows).
    ///
    /// Invalid digits are treated as `0`; an unrecognised length returns
    /// magenta (`#FF00FF`) as a visible error sentinel.
    ///
    /// ```rust,ignore
    /// let bg   = Color::hex("#1E1E1E");   // dark editor background
    /// let blue = Color::hex("#007ACC");   // VS Code blue
    /// let semi = Color::hex("#FF000080"); // half-transparent red
    /// let wht  = Color::hex("#FFF");      // 3-digit shorthand
    /// ```
    pub fn hex(s: &str) -> Self {
        let s = s.trim_start_matches('#');
        // Helper: parse two hex chars starting at byte index `i`.
        let parse = |i: usize| -> f32 {
            u8::from_str_radix(s.get(i..i + 2).unwrap_or("00"), 16).unwrap_or(0) as f32 / 255.0
        };
        match s.len() {
            3 => {
                // Expand each nibble: '#F80' → '#FF8800'
                let expand = |c: char| -> f32 {
                    let v = c.to_digit(16).unwrap_or(0) as u8;
                    (v | (v << 4)) as f32 / 255.0
                };
                let mut chars = s.chars();
                Self::rgba(
                    expand(chars.next().unwrap_or('0')),
                    expand(chars.next().unwrap_or('0')),
                    expand(chars.next().unwrap_or('0')),
                    1.0,
                )
            }
            6 => Self::rgba(parse(0), parse(2), parse(4), 1.0),
            8 => Self::rgba(parse(0), parse(2), parse(4), parse(6)),
            _ => Self::rgba(1.0, 0.0, 1.0, 1.0), // magenta = invalid colour
        }
    }

    /// Construct from a CSS-style hex string, assuming sRGB (gamma) space.
    ///
    /// Equivalent to [`Color::hex`] followed by [`Color::srgba`]: the bytes
    /// are gamma-decoded with the **2.2 power approximation**, not the exact
    /// piecewise sRGB curve, so `"#808080"` becomes `0.2195` instead of the
    /// correct `0.2159`.  Prefer [`Color::from_srgb_hex`] for exact decoding.
    pub fn hex_srgb(s: &str) -> Self {
        let linear = Self::hex(s);
        Self::srgba(linear.r, linear.g, linear.b, linear.a)
    }

    /// Construct from a CSS-style hex string using the exact sRGB EOTF
    /// (IEC 61966-2-1), including the linear segment near black.
    ///
    /// Accepts `"#RGB"`, `"#RRGGBB"` and `"#RRGGBBAA"` (the `#` is
    /// optional) and returns a colour whose stored fields are linear light,
    /// ready for the renderer.  `"#808080"` decodes to `0.2159`, where
    /// [`Color::hex`] returns `0.5019` and [`Color::hex_srgb`] returns
    /// `0.2195`.
    ///
    /// Invalid digits are treated as `0`; an unrecognised length returns
    /// magenta (`#FF00FF`), matching [`Color::hex`].
    ///
    /// ```rust,ignore
    /// let ui = Color::from_srgb_hex("#5B8CFF");
    /// let bg = Color::from_srgb_hex("#17181C");
    /// ```
    pub fn from_srgb_hex(s: &str) -> Self {
        let c = Self::hex(s);
        Self::rgba(
            srgb_channel_to_linear(c.r),
            srgb_channel_to_linear(c.g),
            srgb_channel_to_linear(c.b),
            c.a,
        )
    }

    /// Construct from 8-bit sRGB components, decoded with the exact sRGB
    /// EOTF.  Alpha stays gamma-encoded-free: it is simply divided by 255.
    ///
    /// ```rust,ignore
    /// let accent = Color::from_srgb8(0x5B, 0x8C, 0xFF, 0xFF);
    /// ```
    pub fn from_srgb8(r: u8, g: u8, b: u8, a: u8) -> Self {
        let decode = |v: u8| srgb_channel_to_linear(v as f32 / 255.0);
        Self::rgba(decode(r), decode(g), decode(b), a as f32 / 255.0)
    }

    /// Construct from a packed `0xRRGGBBAA` hexadecimal `u32`.
    ///
    /// Identical to [`from_hex`] but named to make the byte-order explicit
    /// when the value comes from a compile-time constant.
    ///
    /// ```rust,ignore
    /// let coral = Color::from_hex_u32(0xFF6B6BFF);
    /// ```
    #[inline]
    pub fn from_hex_u32(hex: u32) -> Self {
        Self::from_hex(hex)
    }

    /// Returns `[r, g, b, a]` in linear f32 — ready to pass directly to the
    /// renderer (e.g. `dc.gui.rect` colour arrays).
    ///
    /// This is a **plain field read** identical to [`to_array`]: no gamma
    /// conversion happens here.  The fields are treated as linear light, so
    /// converting an sRGB colour means using [`Color::from_srgb_hex`],
    /// [`Color::from_srgb8`] or [`Color::srgb`] *before* storing it.  Values
    /// that arrived through [`Color::hex`] are not linearised and look
    /// washed out on an sRGB surface.
    #[inline]
    pub fn to_linear_f32(self) -> [f32; 4] {
        [self.r, self.g, self.b, self.a]
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
    #[cfg(feature = "gpu")]
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

/// sRGB electro-optical transfer function (IEC 61966-2-1) for one channel in
/// `0.0..=1.0`, including the linear segment below the `0.04045` breakpoint.
/// Inputs outside the range are clamped first.
fn srgb_channel_to_linear(v: f32) -> f32 {
    let v = if v.is_finite() {
        v.clamp(0.0, 1.0)
    } else {
        0.0
    };
    if v <= 0.040_45 {
        v / 12.92
    } else {
        ((v + 0.055) / 1.055).powf(2.4)
    }
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
        assert!(
            (c.r - expected).abs() < 1e-5,
            "r={}, expected={}",
            c.r,
            expected
        );
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

    // ── Color::hex tests ─────────────────────────────────────────────────

    #[test]
    fn hex_rrggbb_dark_gray() {
        // #1E1E1E == r=0x1E=30, g=30, b=30
        let c = Color::hex("#1E1E1E");
        let expected = 30.0_f32 / 255.0;
        assert!((c.r - expected).abs() < 1e-5, "r={}", c.r);
        assert!((c.g - expected).abs() < 1e-5, "g={}", c.g);
        assert!((c.b - expected).abs() < 1e-5, "b={}", c.b);
        assert!((c.a - 1.0).abs() < 1e-5);
    }

    #[test]
    fn hex_rrggbb_vscode_blue() {
        // #007ACC == r=0, g=0x7A=122, b=0xCC=204
        let c = Color::hex("#007ACC");
        assert!((c.r - 0.0).abs() < 1e-5, "r={}", c.r);
        assert!((c.g - 122.0 / 255.0).abs() < 1e-5, "g={}", c.g);
        assert!((c.b - 204.0 / 255.0).abs() < 1e-5, "b={}", c.b);
        assert!((c.a - 1.0).abs() < 1e-5);
    }

    #[test]
    fn hex_3digit_shorthand() {
        // #FFF == #FFFFFF
        let c = Color::hex("#FFF");
        assert!((c.r - 1.0).abs() < 1e-5);
        assert!((c.g - 1.0).abs() < 1e-5);
        assert!((c.b - 1.0).abs() < 1e-5);
        assert!((c.a - 1.0).abs() < 1e-5);
    }

    #[test]
    fn hex_rrggbbaa_semi_transparent() {
        // #FF000080 == red, alpha=0x80=128
        let c = Color::hex("#FF000080");
        assert!((c.r - 1.0).abs() < 1e-5, "r={}", c.r);
        assert!((c.g - 0.0).abs() < 1e-5, "g={}", c.g);
        assert!((c.b - 0.0).abs() < 1e-5, "b={}", c.b);
        assert!((c.a - 128.0 / 255.0).abs() < 1e-5, "a={}", c.a);
    }

    #[test]
    fn hex_invalid_returns_magenta() {
        let c = Color::hex("#ZZZZZZZ"); // 7 chars → invalid length
        assert!((c.r - 1.0).abs() < 1e-5);
        assert!((c.g - 0.0).abs() < 1e-5);
        assert!((c.b - 1.0).abs() < 1e-5);
    }

    #[test]
    fn to_linear_f32_matches_to_array() {
        let c = Color::rgba(0.1, 0.2, 0.3, 0.4);
        assert_eq!(c.to_linear_f32(), c.to_array());
    }

    #[test]
    fn exact_srgb_decode_of_mid_gray() {
        // sRGB 0x80 is 0.50196 gamma-encoded; linear light is 0.21586.
        let c = Color::from_srgb_hex("#808080");
        assert!((c.r - 0.215_861).abs() < 1e-5, "r={}", c.r);
        assert!((c.g - c.r).abs() < 1e-7 && (c.b - c.r).abs() < 1e-7);
        assert!((c.a - 1.0).abs() < 1e-6);
        // The old constructors are unchanged: hex() stores the gamma byte and
        // hex_srgb() uses the 2.2 approximation.
        assert!((Color::hex("#808080").r - 0.501_961).abs() < 1e-5);
        assert!((Color::hex_srgb("#808080").r - 0.501_961_f32.powf(2.2)).abs() < 1e-5);
    }

    #[test]
    fn exact_srgb_decode_endpoints_and_black_segment() {
        let white = Color::from_srgb_hex("#FFFFFF");
        assert!((white.r - 1.0).abs() < 1e-6);
        let black = Color::from_srgb_hex("#000000");
        assert_eq!(black.r, 0.0);
        // Below the 0.04045 breakpoint the curve is linear, not a power.
        let near_black = Color::from_srgb8(2, 2, 2, 255);
        assert!((near_black.r - (2.0 / 255.0) / 12.92).abs() < 1e-7);
        // Alpha is not gamma-decoded.
        let half = Color::from_srgb8(255, 255, 255, 128);
        assert!((half.a - 128.0 / 255.0).abs() < 1e-6);
    }

    #[test]
    fn from_srgb8_matches_from_srgb_hex() {
        let a = Color::from_srgb8(0x5B, 0x8C, 0xFF, 0x80);
        let b = Color::from_srgb_hex("#5B8CFF80");
        assert!((a.r - b.r).abs() < 1e-7);
        assert!((a.g - b.g).abs() < 1e-7);
        assert!((a.b - b.b).abs() < 1e-7);
        assert!((a.a - b.a).abs() < 1e-7);
    }
}
