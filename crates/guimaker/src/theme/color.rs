//! Perceptual color support for the `guimaker` theme.
//!
//! The central type, [`Color`], stores linear-light `sRGB` channels with straight
//! alpha, so mixing and luminance math work without manual decoding. Constructors
//! accept `sRGB`, linear, `OKLCH`, or hex input, and [`Color::to_srgb`] is the only
//! path that encodes stored values back to `sRGB`.

// reason: the published `OKLab` and `OKLCH` matrices carry more decimal digits than
// `f32` can represent; keeping them verbatim lets the conversion be checked against
// the reference implementation.
#![allow(clippy::excessive_precision)]

use core::fmt;

use serde::de::Error as DeError;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

/// A color in linear-light `sRGB` with straight alpha.
///
/// Every constructor clamps its input, so stored channels are values in `0..=1`
/// unless the caller passes a non-finite value, which is preserved.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Color {
    /// Linear-light red channel.
    r: f32,
    /// Linear-light green channel.
    g: f32,
    /// Linear-light blue channel.
    b: f32,
    /// Straight alpha channel.
    a: f32,
}

/// A color expressed in `OKLCH`: perceptual lightness, chroma, and hue.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Oklch {
    /// Perceptual lightness in `0..=1`; `0` is black and `1` is white.
    pub lightness: f32,
    /// Chroma, where `0` is achromatic and larger values are more saturated.
    pub chroma: f32,
    /// Hue angle in degrees, normalized to `0..360`.
    pub hue: f32,
}

impl Color {
    /// Fully transparent black.
    pub const TRANSPARENT: Color = Color::lineara(0.0, 0.0, 0.0, 0.0);

    /// Opaque black.
    pub const BLACK: Color = Color::lineara(0.0, 0.0, 0.0, 1.0);

    /// Opaque white.
    pub const WHITE: Color = Color::lineara(1.0, 1.0, 1.0, 1.0);

    /// Builds an opaque color from linear-light `sRGB` channels.
    ///
    /// Out-of-range input is clamped instead of rejected.
    #[must_use]
    pub const fn linear(red: f32, green: f32, blue: f32) -> Self {
        Self::lineara(red, green, blue, 1.0)
    }

    /// Builds a color from linear-light `sRGB` channels and a straight alpha value.
    ///
    /// Out-of-range input is clamped instead of rejected.
    #[must_use]
    pub const fn lineara(red: f32, green: f32, blue: f32, alpha: f32) -> Self {
        Self {
            r: clamp_unit(red),
            g: clamp_unit(green),
            b: clamp_unit(blue),
            a: clamp_unit(alpha),
        }
    }

    /// Builds an opaque color from display-space `sRGB` channels in `0..=1`.
    ///
    /// The channels are decoded to linear light once, here. Out-of-range input is
    /// clamped instead of rejected.
    #[must_use]
    pub fn srgb(red: f32, green: f32, blue: f32) -> Self {
        Self::srgba(red, green, blue, 1.0)
    }

    /// Builds a color from display-space `sRGB` channels and a straight alpha value.
    ///
    /// The channels are decoded to linear light once, here. Out-of-range input is
    /// clamped instead of rejected.
    #[must_use]
    pub fn srgba(red: f32, green: f32, blue: f32, alpha: f32) -> Self {
        Self::lineara(
            srgb_channel_to_linear(clamp_unit(red)),
            srgb_channel_to_linear(clamp_unit(green)),
            srgb_channel_to_linear(clamp_unit(blue)),
            alpha,
        )
    }

    /// Builds an opaque color from `OKLCH` coordinates.
    ///
    /// Hue is normalized into `0..360` and chroma is reduced until the result fits
    /// inside the `sRGB` gamut.
    #[must_use]
    pub fn oklch(lightness: f32, chroma: f32, hue: f32) -> Self {
        Self::oklcha(lightness, chroma, hue, 1.0)
    }

    /// Builds a color from `OKLCH` coordinates and a straight alpha value.
    ///
    /// Hue is normalized into `0..360` and chroma is reduced until the result fits
    /// inside the `sRGB` gamut.
    #[must_use]
    pub fn oklcha(lightness: f32, chroma: f32, hue: f32, alpha: f32) -> Self {
        let mapped_lightness = clamp_unit(lightness);
        let mapped_chroma = if chroma > 0.0 { chroma } else { 0.0 };
        let normalized_hue = if hue.is_finite() {
            hue.rem_euclid(360.0)
        } else {
            0.0
        };
        let (red, green, blue) =
            gamut_mapped_linear(mapped_lightness, mapped_chroma, normalized_hue);
        Self {
            r: red,
            g: green,
            b: blue,
            a: clamp_unit(alpha),
        }
    }

    /// Builds an opaque color from a `0xRRGGBB` word.
    #[must_use]
    pub fn hex(rgb: u32) -> Self {
        Self::srgb(
            channel_from_byte(byte_from_word(rgb, 16)),
            channel_from_byte(byte_from_word(rgb, 8)),
            channel_from_byte(byte_from_word(rgb, 0)),
        )
    }

    /// Builds a color from a `0xRRGGBBAA` word.
    #[must_use]
    pub fn hex_alpha(rgba: u32) -> Self {
        Self::srgba(
            channel_from_byte(byte_from_word(rgba, 24)),
            channel_from_byte(byte_from_word(rgba, 16)),
            channel_from_byte(byte_from_word(rgba, 8)),
            channel_from_byte(byte_from_word(rgba, 0)),
        )
    }

    /// Returns the straight alpha channel in `0..=1`.
    #[must_use]
    pub fn alpha(self) -> f32 {
        self.a
    }

    /// Returns this color with its alpha channel replaced.
    ///
    /// The new alpha is clamped into `0..=1`.
    #[must_use]
    pub fn with_alpha(self, alpha: f32) -> Self {
        Self {
            a: clamp_unit(alpha),
            ..self
        }
    }

    /// Decodes the stored linear channels to display-space `sRGB`.
    ///
    /// Returns `[red, green, blue, alpha]`. This is the only place where stored
    /// linear values are re-encoded, so it should be used only at the rendering
    /// boundary.
    #[must_use]
    pub fn to_srgb(self) -> [f32; 4] {
        [
            linear_channel_to_srgb(clamp_unit(self.r)),
            linear_channel_to_srgb(clamp_unit(self.g)),
            linear_channel_to_srgb(clamp_unit(self.b)),
            self.a,
        ]
    }

    /// Blends this colour to `other` in linear light by `t` in `0.0..=1.0`.
    ///
    /// ```
    /// use guimaker::theme::Color;
    /// let black = Color::linear(0.0, 0.0, 0.0);
    /// let white = Color::linear(1.0, 1.0, 1.0);
    /// assert_eq!(black.lerp(white, 1.0), white);
    /// ```
    #[must_use]
    pub fn lerp(self, other: Self, t: f32) -> Self {
        let from = self.to_linear();
        let to = other.to_linear();
        let mix = |a: f32, b: f32| a + (b - a) * t;
        Self::lineara(
            mix(from[0], to[0]),
            mix(from[1], to[1]),
            mix(from[2], to[2]),
            mix(from[3], to[3]),
        )
    }

    /// Returns the stored linear channels as `[red, green, blue, alpha]`.
    ///
    /// The values are returned exactly as stored; no transfer function is applied.
    #[must_use]
    pub fn to_linear(self) -> [f32; 4] {
        [self.r, self.g, self.b, self.a]
    }

    /// Converts the stored linear channels to `OKLCH`.
    ///
    /// Hue is reported as `0` when chroma is negligible.
    #[must_use]
    pub fn to_oklch(self) -> Oklch {
        linear_to_oklch(self.r, self.g, self.b)
    }

    /// Computes the `WCAG` relative luminance from the stored linear channels.
    ///
    /// Alpha is ignored.
    #[must_use]
    pub fn relative_luminance(self) -> f32 {
        let red = clamp_unit(self.r);
        let green = clamp_unit(self.g);
        let blue = clamp_unit(self.b);
        0.2126 * red + 0.7152 * green + 0.0722 * blue
    }

    /// Computes the `WCAG` contrast ratio against `other`.
    ///
    /// Alpha is ignored. The result is clamped to at least `1.0`, so identical colors
    /// yield `1.0` and black against white yields `21.0`.
    #[must_use]
    pub fn contrast_ratio(self, other: Color) -> f32 {
        let first = self.relative_luminance();
        let second = other.relative_luminance();
        let (lighter, darker) = if first >= second {
            (first, second)
        } else {
            (second, first)
        };
        ((lighter + 0.05) / (darker + 0.05)).max(1.0)
    }

    /// Returns `true` when the alpha channel is fully opaque.
    #[must_use]
    pub fn is_opaque(self) -> bool {
        self.a >= 1.0
    }
}

// Clamps a channel into `0..=1`. Non-finite values pass through unchanged and are
// dealt with by the byte encoder, which also never panics.
const fn clamp_unit(value: f32) -> f32 {
    if value < 0.0 {
        0.0
    } else if value > 1.0 {
        1.0
    } else {
        value
    }
}

// Applies the `sRGB` electro-optical transfer function to a non-negative channel.
fn srgb_channel_to_linear(value: f32) -> f32 {
    if value <= 0.040_45 {
        value / 12.92
    } else {
        ((value + 0.055) / 1.055).powf(2.4)
    }
}

// Applies the inverse `sRGB` transfer function to a linear channel.
fn linear_channel_to_srgb(value: f32) -> f32 {
    if value <= 0.003_130_8 {
        12.92 * value
    } else {
        1.055 * value.powf(1.0 / 2.4) - 0.055
    }
}

fn linear_to_oklab(red: f32, green: f32, blue: f32) -> (f32, f32, f32) {
    let long_mix = 0.412_221_470_8 * red + 0.536_332_536_3 * green + 0.051_445_992_9 * blue;
    let medium_mix = 0.211_903_498_2 * red + 0.680_699_545_1 * green + 0.107_396_956_6 * blue;
    let short_mix = 0.088_302_461_9 * red + 0.281_718_837_6 * green + 0.629_978_700_5 * blue;
    let long_root = long_mix.powf(1.0 / 3.0);
    let medium_root = medium_mix.powf(1.0 / 3.0);
    let short_root = short_mix.powf(1.0 / 3.0);
    let lightness =
        0.210_454_255_3 * long_root + 0.793_617_785_0 * medium_root - 0.004_072_046_8 * short_root;
    let red_green =
        1.977_998_495_1 * long_root - 2.428_592_205_0 * medium_root + 0.450_593_709_9 * short_root;
    let blue_yellow =
        0.025_904_037_1 * long_root + 0.782_771_766_2 * medium_root - 0.808_675_766_0 * short_root;
    (lightness, red_green, blue_yellow)
}

fn oklab_to_linear(lightness: f32, red_green: f32, blue_yellow: f32) -> (f32, f32, f32) {
    let long_mix = lightness + 0.396_337_777_4 * red_green + 0.215_803_757_3 * blue_yellow;
    let medium_mix = lightness - 0.105_561_345_8 * red_green - 0.063_854_172_8 * blue_yellow;
    let short_mix = lightness - 0.089_484_177_5 * red_green - 1.291_485_548_0 * blue_yellow;
    let long_cube = long_mix * long_mix * long_mix;
    let medium_cube = medium_mix * medium_mix * medium_mix;
    let short_cube = short_mix * short_mix * short_mix;
    let red =
        4.076_741_662_1 * long_cube - 3.307_711_591_3 * medium_cube + 0.230_969_929_2 * short_cube;
    let green =
        -1.268_438_004_6 * long_cube + 2.609_757_401_1 * medium_cube - 0.341_319_396_5 * short_cube;
    let blue =
        -0.004_196_086_3 * long_cube - 0.703_418_614_7 * medium_cube + 1.707_614_701_0 * short_cube;
    (red, green, blue)
}

fn oklch_to_linear(lightness: f32, chroma: f32, hue_degrees: f32) -> (f32, f32, f32) {
    let angle = hue_degrees.to_radians();
    let red_green = chroma * angle.cos();
    let blue_yellow = chroma * angle.sin();
    oklab_to_linear(lightness, red_green, blue_yellow)
}

fn linear_to_oklch(red: f32, green: f32, blue: f32) -> Oklch {
    let (lightness, red_green, blue_yellow) = linear_to_oklab(red, green, blue);
    let chroma = red_green.hypot(blue_yellow);
    let hue = if chroma < 1e-7 {
        0.0
    } else {
        blue_yellow.atan2(red_green).to_degrees().rem_euclid(360.0)
    };
    Oklch {
        lightness,
        chroma,
        hue,
    }
}

fn channel_in_gamut(value: f32) -> bool {
    (-1e-4..=1.0 + 1e-4).contains(&value)
}

fn gamut_mapped_linear(lightness: f32, chroma: f32, hue: f32) -> (f32, f32, f32) {
    let (red, green, blue) = oklch_to_linear(lightness, chroma, hue);
    if channel_in_gamut(red) && channel_in_gamut(green) && channel_in_gamut(blue) {
        return (clamp_unit(red), clamp_unit(green), clamp_unit(blue));
    }
    let mut low = 0.0;
    let mut high = chroma;
    for _ in 0..32 {
        let probe = low + 0.5 * (high - low);
        let (search_red, search_green, search_blue) = oklch_to_linear(lightness, probe, hue);
        if channel_in_gamut(search_red)
            && channel_in_gamut(search_green)
            && channel_in_gamut(search_blue)
        {
            low = probe;
        } else {
            high = probe;
        }
    }
    let (mapped_red, mapped_green, mapped_blue) = oklch_to_linear(lightness, low, hue);
    (
        clamp_unit(mapped_red),
        clamp_unit(mapped_green),
        clamp_unit(mapped_blue),
    )
}

fn channel_from_byte(byte: u8) -> f32 {
    f32::from(byte) / 255.0
}

fn byte_from_word(word: u32, shift: u32) -> u8 {
    u8::try_from((word >> shift) & 0xFF).unwrap_or(u8::MAX)
}

#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
// reason: the channel is clamped into 0..=1 and scaled to 0..=255 before the cast.
fn channel_to_byte(value: f32) -> u8 {
    let scaled = clamp_unit(value) * 255.0;
    scaled.round() as u8
}

impl Serialize for Color {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.collect_str(self)
    }
}

impl<'de> Deserialize<'de> for Color {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let text = String::deserialize(deserializer)?;
        Self::from_hex_str(&text).ok_or_else(|| {
            <D::Error as DeError>::custom("expected a hex color like #RRGGBB or #RRGGBBAA")
        })
    }
}

impl Color {
    // Parses an optional `#` plus exactly 6 or 8 ASCII hex digits.
    fn from_hex_str(text: &str) -> Option<Self> {
        let digits = text.strip_prefix('#').unwrap_or(text);
        let length = digits.len();
        if length != 6 && length != 8 {
            return None;
        }
        let red_byte = parse_hex_byte(digits.get(0..2)?)?;
        let green_byte = parse_hex_byte(digits.get(2..4)?)?;
        let blue_byte = parse_hex_byte(digits.get(4..6)?)?;
        let red = channel_from_byte(red_byte);
        let green = channel_from_byte(green_byte);
        let blue = channel_from_byte(blue_byte);
        if length == 6 {
            return Some(Self::srgb(red, green, blue));
        }
        let alpha_byte = parse_hex_byte(digits.get(6..8)?)?;
        Some(Self::srgba(red, green, blue, channel_from_byte(alpha_byte)))
    }
}

fn parse_hex_byte(pair: &str) -> Option<u8> {
    if !pair.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return None;
    }
    u8::from_str_radix(pair, 16).ok()
}

impl fmt::Display for Color {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let [red, green, blue, alpha] = self.to_srgb();
        write!(
            formatter,
            "#{:02X}{:02X}{:02X}",
            channel_to_byte(red),
            channel_to_byte(green),
            channel_to_byte(blue)
        )?;
        if !self.is_opaque() {
            write!(formatter, "{:02X}", channel_to_byte(alpha))?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::Color;

    fn hue_difference(first: f32, second: f32) -> f32 {
        let difference = (first - second).rem_euclid(360.0);
        difference.min(360.0 - difference)
    }

    #[test]
    fn srgb_to_linear_and_back_round_trips() {
        let probes = [0.0_f32, 0.02, 0.040_45, 0.1, 0.5, 0.9, 1.0];
        for value in probes {
            let round_trip = Color::srgb(value, value, value).to_srgb()[0];
            assert!(
                (round_trip - value).abs() <= 1e-5,
                "channel {value} came back as {round_trip}"
            );
        }
    }

    #[test]
    fn linear_channel_round_trips() {
        let linear = Color::linear(0.1, 0.25, 0.9).to_linear();
        assert!((linear[0] - 0.1).abs() <= 1e-6, "red {}", linear[0]);
        assert!((linear[1] - 0.25).abs() <= 1e-6, "green {}", linear[1]);
        assert!((linear[2] - 0.9).abs() <= 1e-6, "blue {}", linear[2]);
    }

    #[test]
    fn oklch_round_trips() {
        let samples = [
            (0.66_f32, 0.10_f32, 265.0_f32),
            (0.18, 0.012, 264.0),
            (0.97, 0.01, 90.0),
        ];
        for (lightness, chroma, hue) in samples {
            let result = Color::oklch(lightness, chroma, hue).to_oklch();
            assert!(
                (result.lightness - lightness).abs() <= 0.003,
                "lightness {lightness} came back as {}",
                result.lightness
            );
            assert!(
                (result.chroma - chroma).abs() <= 0.003,
                "chroma {chroma} came back as {}",
                result.chroma
            );
            assert!(
                hue_difference(result.hue, hue) <= 0.5,
                "hue {hue} came back as {}",
                result.hue
            );
        }
    }

    #[test]
    fn hex_constructors_match_srgb() {
        let srgb = Color::hex(0x5B_8CFF).to_srgb();
        let red = srgb[0];
        let green = srgb[1];
        let blue = srgb[2];
        assert!(
            (red - f32::from(0x5B_u8) / 255.0).abs() <= 1.0 / 255.0,
            "red {red}"
        );
        assert!(
            (green - f32::from(0x8C_u8) / 255.0).abs() <= 1.0 / 255.0,
            "green {green}"
        );
        assert!(
            (blue - f32::from(0xFF_u8) / 255.0).abs() <= 1.0 / 255.0,
            "blue {blue}"
        );
        assert!((srgb[3] - 1.0).abs() <= 1e-6, "alpha {}", srgb[3]);
        let translucent = Color::hex_alpha(0x1122_3344).to_srgb();
        assert!(
            (translucent[3] - f32::from(0x44_u8) / 255.0).abs() <= 1e-6,
            "alpha {}",
            translucent[3]
        );
    }

    #[test]
    fn contrast_black_white_is_21() {
        let ratio = Color::BLACK.contrast_ratio(Color::WHITE);
        assert!((ratio - 21.0).abs() <= 0.01, "ratio {ratio}");
        let reversed = Color::WHITE.contrast_ratio(Color::BLACK);
        assert!((reversed - 21.0).abs() <= 0.01, "reversed {reversed}");
    }

    #[test]
    fn contrast_is_one_for_identical_colors() {
        let color = Color::srgb(0.3, 0.6, 0.9);
        assert!((color.contrast_ratio(color) - 1.0).abs() <= 1e-6);
        let transparent = Color::TRANSPARENT;
        assert!((transparent.contrast_ratio(transparent) - 1.0).abs() <= 1e-6);
    }

    #[test]
    fn relative_luminance_endpoints() {
        assert!(Color::BLACK.relative_luminance().abs() <= 1e-6);
        let white = Color::WHITE.relative_luminance();
        assert!((white - 1.0).abs() <= 1e-6, "white luminance {white}");
    }

    #[test]
    fn alpha_is_preserved_by_constructors() {
        let srgb_alpha = Color::srgba(0.1, 0.2, 0.3, 0.25).alpha();
        assert!((srgb_alpha - 0.25).abs() <= 1e-6, "srgb alpha {srgb_alpha}");
        let linear_alpha = Color::lineara(0.1, 0.2, 0.3, 0.5).alpha();
        assert!(
            (linear_alpha - 0.5).abs() <= 1e-6,
            "linear alpha {linear_alpha}"
        );
        let oklch_alpha = Color::oklcha(0.6, 0.1, 120.0, 0.75).alpha();
        assert!(
            (oklch_alpha - 0.75).abs() <= 1e-6,
            "oklch alpha {oklch_alpha}"
        );
        let hex_alpha = Color::hex_alpha(0x1122_3344).alpha();
        let expected = f32::from(0x44_u8) / 255.0;
        assert!(
            (hex_alpha - expected).abs() <= 1e-6,
            "hex alpha {hex_alpha}"
        );
    }

    #[test]
    fn out_of_gamut_oklch_stays_inside_srgb() {
        let color = Color::oklch(0.7, 0.4, 30.0);
        for channel in color.to_linear() {
            assert!((0.0..=1.0).contains(&channel), "linear channel {channel}");
        }
        for channel in color.to_srgb() {
            assert!((0.0..=1.0).contains(&channel), "srgb channel {channel}");
        }
    }

    #[test]
    fn hue_is_normalized() {
        let hue = Color::oklch(0.6, 0.1, -30.0).to_oklch().hue;
        assert!(hue_difference(hue, 330.0) <= 0.5, "hue came back as {hue}");
    }

    #[test]
    fn serde_round_trips() {
        let opaque = Color::srgb(0.2, 0.4, 0.8);
        let encoded = ron::to_string(&opaque).expect("serialize opaque color");
        let decoded: Color = ron::from_str(&encoded).expect("deserialize opaque color");
        let expected = opaque.to_srgb();
        let actual = decoded.to_srgb();
        assert!(
            (actual[0] - expected[0]).abs() <= 1.0 / 255.0,
            "opaque red {}",
            actual[0]
        );
        assert!(
            (actual[1] - expected[1]).abs() <= 1.0 / 255.0,
            "opaque green {}",
            actual[1]
        );
        assert!(
            (actual[2] - expected[2]).abs() <= 1.0 / 255.0,
            "opaque blue {}",
            actual[2]
        );

        let translucent = Color::srgba(0.1, 0.5, 0.9, 0.5);
        let encoded = ron::to_string(&translucent).expect("serialize translucent color");
        let decoded: Color = ron::from_str(&encoded).expect("deserialize translucent color");
        let expected = translucent.to_srgb();
        let actual = decoded.to_srgb();
        assert!(
            (actual[0] - expected[0]).abs() <= 1.0 / 255.0,
            "translucent red {}",
            actual[0]
        );
        assert!(
            (actual[3] - expected[3]).abs() <= 1.0 / 255.0,
            "translucent alpha {}",
            actual[3]
        );
    }

    #[test]
    fn bad_hex_is_rejected() {
        let result = ron::from_str::<Color>("\"#12\"");
        assert!(result.is_err());
    }
}
