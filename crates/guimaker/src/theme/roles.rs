//! Colour roles generated from a seed through an `OKLCH` ramp.
//!
//! A theme does not ship hand-picked hex values for its surfaces, text and
//! accent. It ships one [`ColorSeed`] (a neutral hue plus chroma and an accent
//! hue plus chroma) and this module generates a coherent palette from it:
//!
//! 1. a monotonic neutral ramp in `OKLCH` lightness;
//! 2. the accent and its hover, pressed and subtle variants;
//! 3. the success, warning, danger and info families;
//! 4. the overlay scrim, the selection fill and the focus ring.
//!
//! Because every colour is produced in `OKLCH` and converted once by
//! [`Color`], a double-encoded `sRGB` value cannot be constructed here, and the
//! contrast guarantees in `guimaker::theme` are properties of the generated
//! data rather than of a hand-edited palette.

use serde::{Deserialize, Serialize};

use super::color::Color;
use super::tokens::ThemeMode;

/// The number of steps in the generated neutral ramp.
pub const NEUTRAL_STEPS: usize = 12;

/// The lightness targets for the dark neutral ramp, darkest first.
const DARK_RAMP: [f32; NEUTRAL_STEPS] = [
    0.135, 0.160, 0.180, 0.220, 0.280, 0.340, 0.440, 0.560, 0.660, 0.760, 0.870, 0.970,
];

/// The lightness targets for the light neutral ramp, darkest first.
const LIGHT_RAMP: [f32; NEUTRAL_STEPS] = [
    0.180, 0.300, 0.400, 0.500, 0.600, 0.660, 0.720, 0.800, 0.880, 0.930, 0.970, 1.000,
];

/// The lightness targets for the high-contrast neutral ramp, darkest first.
const HIGH_CONTRAST_RAMP: [f32; NEUTRAL_STEPS] = [
    0.090, 0.110, 0.130, 0.170, 0.300, 0.450, 0.580, 0.700, 0.800, 0.900, 0.960, 1.000,
];

/// The seed a theme generates its palette from.
///
/// The neutral seed controls every surface, border and text tone; the accent
/// seed controls the accent states and the focus ring hue. Semantic colours
/// keep fixed hues so success always reads as green.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct ColorSeed {
    /// Hue of the neutral family, in `OKLCH` degrees.
    pub neutral_hue: f32,
    /// Chroma of the neutral family; keep it small for a grey.
    pub neutral_chroma: f32,
    /// Hue of the accent, in `OKLCH` degrees.
    pub accent_hue: f32,
    /// Chroma of the accent.
    pub accent_chroma: f32,
}

impl ColorSeed {
    /// Builds a seed from its four values.
    #[must_use]
    pub const fn new(
        neutral_hue: f32,
        neutral_chroma: f32,
        accent_hue: f32,
        accent_chroma: f32,
    ) -> Self {
        Self {
            neutral_hue,
            neutral_chroma,
            accent_hue,
            accent_chroma,
        }
    }

    /// The seed of the built-in dark theme: a cool grey and the blue accent.
    #[must_use]
    pub const fn dark() -> Self {
        Self::new(264.0, 0.012, 265.0, 0.19)
    }

    /// The seed of the built-in light theme.
    #[must_use]
    pub const fn light() -> Self {
        Self::new(264.0, 0.010, 265.0, 0.19)
    }

    /// The seed of the built-in high-contrast theme: pure greys and a bright
    /// accent.
    #[must_use]
    pub const fn high_contrast() -> Self {
        Self::new(264.0, 0.0, 265.0, 0.20)
    }
}

/// A neutral ramp: the surfaces, borders and text tones of a theme.
///
/// Steps are ordered from darkest to lightest. The ramp is monotonic in
/// `OKLCH` lightness, which the test suite checks for every built-in.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct NeutralRamp {
    steps: [Color; NEUTRAL_STEPS],
}

impl NeutralRamp {
    /// Generates the dark or light ramp for a seed.
    #[must_use]
    pub fn generate(seed: ColorSeed, mode: ThemeMode) -> Self {
        let targets = match mode {
            ThemeMode::Dark => DARK_RAMP,
            ThemeMode::Light => LIGHT_RAMP,
        };
        Self::from_lightness(seed, targets)
    }

    /// Generates the high-contrast ramp for a seed.
    #[must_use]
    pub fn high_contrast(seed: ColorSeed) -> Self {
        Self::from_lightness(seed, HIGH_CONTRAST_RAMP)
    }

    fn from_lightness(seed: ColorSeed, targets: [f32; NEUTRAL_STEPS]) -> Self {
        let steps =
            targets.map(|lightness| Color::oklch(lightness, seed.neutral_chroma, seed.neutral_hue));
        Self { steps }
    }

    /// The ramp colours, darkest first.
    #[must_use]
    pub fn steps(&self) -> &[Color; NEUTRAL_STEPS] {
        &self.steps
    }

    /// The `OKLCH` lightness of every step, darkest first.
    #[must_use]
    pub fn lightness(&self) -> [f32; NEUTRAL_STEPS] {
        core::array::from_fn(|index| self.steps[index].to_oklch().lightness)
    }
}

/// The semantic role of a colour in a theme.
///
/// A widget asks for a role, never for a literal colour. `Tone::Muted`, for
/// example, resolves to [`Roles::text_muted`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ColorRole {
    /// The window backdrop behind panels.
    Bg,
    /// A panel body.
    Surface,
    /// A raised surface: headers, cards, popovers.
    SurfaceRaised,
    /// A recessed surface: wells and tracks.
    SurfaceSunken,
    /// The translucent scrim behind a modal.
    Overlay,
    /// Hairline dividers and quiet outlines.
    Border,
    /// Stronger outlines for inputs and cards.
    BorderStrong,
    /// Primary text and values.
    Text,
    /// Secondary text: captions, units, timestamps.
    TextMuted,
    /// Tertiary text for large or secondary content only.
    TextFaint,
    /// Text drawn on the accent.
    TextOnAccent,
    /// The single meaning-carrying accent.
    Accent,
    /// The accent on hover.
    AccentHover,
    /// The accent while pressed.
    AccentPressed,
    /// The accent at 14 % alpha, for subtle fills.
    AccentSubtle,
    /// The keyboard focus ring.
    FocusRing,
    /// The selection fill.
    Selection,
    /// Success status.
    Success,
    /// Success at 14 % alpha.
    SuccessSubtle,
    /// Warning status.
    Warning,
    /// Warning at 14 % alpha.
    WarningSubtle,
    /// Danger status.
    Danger,
    /// Danger at 14 % alpha.
    DangerSubtle,
    /// Informational status.
    Info,
    /// Informational status at 14 % alpha.
    InfoSubtle,
}

impl ColorRole {
    /// Every role, in palette order.
    pub const ALL: [Self; 25] = [
        Self::Bg,
        Self::Surface,
        Self::SurfaceRaised,
        Self::SurfaceSunken,
        Self::Overlay,
        Self::Border,
        Self::BorderStrong,
        Self::Text,
        Self::TextMuted,
        Self::TextFaint,
        Self::TextOnAccent,
        Self::Accent,
        Self::AccentHover,
        Self::AccentPressed,
        Self::AccentSubtle,
        Self::FocusRing,
        Self::Selection,
        Self::Success,
        Self::SuccessSubtle,
        Self::Warning,
        Self::WarningSubtle,
        Self::Danger,
        Self::DangerSubtle,
        Self::Info,
        Self::InfoSubtle,
    ];

    /// The role's stable name.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Bg => "bg",
            Self::Surface => "surface",
            Self::SurfaceRaised => "surface_raised",
            Self::SurfaceSunken => "surface_sunken",
            Self::Overlay => "overlay",
            Self::Border => "border",
            Self::BorderStrong => "border_strong",
            Self::Text => "text",
            Self::TextMuted => "text_muted",
            Self::TextFaint => "text_faint",
            Self::TextOnAccent => "text_on_accent",
            Self::Accent => "accent",
            Self::AccentHover => "accent_hover",
            Self::AccentPressed => "accent_pressed",
            Self::AccentSubtle => "accent_subtle",
            Self::FocusRing => "focus_ring",
            Self::Selection => "selection",
            Self::Success => "success",
            Self::SuccessSubtle => "success_subtle",
            Self::Warning => "warning",
            Self::WarningSubtle => "warning_subtle",
            Self::Danger => "danger",
            Self::DangerSubtle => "danger_subtle",
            Self::Info => "info",
            Self::InfoSubtle => "info_subtle",
        }
    }
}

/// The resolved colour roles of a theme.
///
/// Every field is a role an app can read or override. The guarantee tests in
/// `guimaker::theme` hold for the built-in themes; an override is the app's
/// responsibility.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Roles {
    /// The window backdrop behind panels.
    pub bg: Color,
    /// A panel body.
    pub surface: Color,
    /// A raised surface: headers, cards, popovers.
    pub surface_raised: Color,
    /// A recessed surface: wells and tracks.
    pub surface_sunken: Color,
    /// The translucent scrim behind a modal.
    pub overlay: Color,
    /// Hairline dividers and quiet outlines.
    pub border: Color,
    /// Stronger outlines for inputs and cards.
    pub border_strong: Color,
    /// Primary text and values.
    pub text: Color,
    /// Secondary text: captions, units, timestamps.
    pub text_muted: Color,
    /// Tertiary text for large or secondary content only.
    pub text_faint: Color,
    /// Text drawn on the accent.
    pub text_on_accent: Color,
    /// The single meaning-carrying accent.
    pub accent: Color,
    /// The accent on hover.
    pub accent_hover: Color,
    /// The accent while pressed.
    pub accent_pressed: Color,
    /// The accent at 14 % alpha, for subtle fills.
    pub accent_subtle: Color,
    /// The keyboard focus ring.
    pub focus_ring: Color,
    /// The selection fill.
    pub selection: Color,
    /// Success status.
    pub success: Color,
    /// Success at 14 % alpha.
    pub success_subtle: Color,
    /// Warning status.
    pub warning: Color,
    /// Warning at 14 % alpha.
    pub warning_subtle: Color,
    /// Danger status.
    pub danger: Color,
    /// Danger at 14 % alpha.
    pub danger_subtle: Color,
    /// Informational status.
    pub info: Color,
    /// Informational status at 14 % alpha.
    pub info_subtle: Color,
}

/// The three palette shapes.
#[derive(Clone, Copy)]
enum Variant {
    Dark,
    Light,
    HighContrast,
}

impl Roles {
    /// Generates the roles for a dark or light theme.
    #[must_use]
    pub fn from_seed(seed: ColorSeed, mode: ThemeMode) -> Self {
        let variant = match mode {
            ThemeMode::Dark => Variant::Dark,
            ThemeMode::Light => Variant::Light,
        };
        let ramp = NeutralRamp::generate(seed, mode);
        Self::build(seed, &ramp, variant)
    }

    /// Generates the roles for the high-contrast theme.
    #[must_use]
    pub fn high_contrast(seed: ColorSeed) -> Self {
        let ramp = NeutralRamp::high_contrast(seed);
        Self::build(seed, &ramp, Variant::HighContrast)
    }

    /// Reads a role by name.
    #[must_use]
    #[allow(clippy::too_many_lines)] // reason: one arm per colour role keeps get/set exhaustive
    pub fn get(self, role: ColorRole) -> Color {
        match role {
            ColorRole::Bg => self.bg,
            ColorRole::Surface => self.surface,
            ColorRole::SurfaceRaised => self.surface_raised,
            ColorRole::SurfaceSunken => self.surface_sunken,
            ColorRole::Overlay => self.overlay,
            ColorRole::Border => self.border,
            ColorRole::BorderStrong => self.border_strong,
            ColorRole::Text => self.text,
            ColorRole::TextMuted => self.text_muted,
            ColorRole::TextFaint => self.text_faint,
            ColorRole::TextOnAccent => self.text_on_accent,
            ColorRole::Accent => self.accent,
            ColorRole::AccentHover => self.accent_hover,
            ColorRole::AccentPressed => self.accent_pressed,
            ColorRole::AccentSubtle => self.accent_subtle,
            ColorRole::FocusRing => self.focus_ring,
            ColorRole::Selection => self.selection,
            ColorRole::Success => self.success,
            ColorRole::SuccessSubtle => self.success_subtle,
            ColorRole::Warning => self.warning,
            ColorRole::WarningSubtle => self.warning_subtle,
            ColorRole::Danger => self.danger,
            ColorRole::DangerSubtle => self.danger_subtle,
            ColorRole::Info => self.info,
            ColorRole::InfoSubtle => self.info_subtle,
        }
    }

    /// Overrides a role.
    #[allow(clippy::too_many_lines)] // reason: one arm per colour role keeps get/set exhaustive
    pub fn set(&mut self, role: ColorRole, color: Color) {
        match role {
            ColorRole::Bg => self.bg = color,
            ColorRole::Surface => self.surface = color,
            ColorRole::SurfaceRaised => self.surface_raised = color,
            ColorRole::SurfaceSunken => self.surface_sunken = color,
            ColorRole::Overlay => self.overlay = color,
            ColorRole::Border => self.border = color,
            ColorRole::BorderStrong => self.border_strong = color,
            ColorRole::Text => self.text = color,
            ColorRole::TextMuted => self.text_muted = color,
            ColorRole::TextFaint => self.text_faint = color,
            ColorRole::TextOnAccent => self.text_on_accent = color,
            ColorRole::Accent => self.accent = color,
            ColorRole::AccentHover => self.accent_hover = color,
            ColorRole::AccentPressed => self.accent_pressed = color,
            ColorRole::AccentSubtle => self.accent_subtle = color,
            ColorRole::FocusRing => self.focus_ring = color,
            ColorRole::Selection => self.selection = color,
            ColorRole::Success => self.success = color,
            ColorRole::SuccessSubtle => self.success_subtle = color,
            ColorRole::Warning => self.warning = color,
            ColorRole::WarningSubtle => self.warning_subtle = color,
            ColorRole::Danger => self.danger = color,
            ColorRole::DangerSubtle => self.danger_subtle = color,
            ColorRole::Info => self.info = color,
            ColorRole::InfoSubtle => self.info_subtle = color,
        }
    }

    #[allow(clippy::too_many_lines)] // reason: the palette is one explicit table
    fn build(seed: ColorSeed, ramp: &NeutralRamp, variant: Variant) -> Self {
        let steps = ramp.steps();
        let (bg, surface, surface_sunken, surface_raised, border, border_strong) = match variant {
            Variant::Dark => (steps[0], steps[2], steps[1], steps[3], steps[4], steps[5]),
            Variant::Light => (
                steps[11], steps[10], steps[9], steps[11], steps[7], steps[4],
            ),
            Variant::HighContrast => (steps[0], steps[2], steps[1], steps[3], steps[6], steps[7]),
        };
        let (text, text_muted, text_faint) = match variant {
            Variant::Dark => (steps[11], steps[9], steps[7]),
            Variant::Light => (steps[0], steps[3], steps[4]),
            Variant::HighContrast => (steps[11], steps[9], steps[8]),
        };
        let (accent_lightness, hover_lightness, pressed_lightness, ring_lightness, ring_chroma) =
            match variant {
                Variant::Dark => (0.660, 0.700, 0.620, 0.880, 0.10),
                Variant::Light => (0.620, 0.660, 0.600, 0.380, 0.19),
                Variant::HighContrast => (0.780, 0.820, 0.740, 0.960, 0.04),
            };
        let accent = Color::oklch(accent_lightness, seed.accent_chroma, seed.accent_hue);
        let accent_hover = Color::oklch(hover_lightness, seed.accent_chroma, seed.accent_hue);
        let accent_pressed = Color::oklch(pressed_lightness, seed.accent_chroma, seed.accent_hue);
        let focus_ring = Color::oklch(ring_lightness, ring_chroma, seed.accent_hue);
        let overlay_alpha = match variant {
            Variant::Dark => 0.55,
            Variant::Light => 0.45,
            Variant::HighContrast => 0.75,
        };
        let (success, warning, danger, info) = match variant {
            Variant::Dark => (
                Color::oklch(0.660, 0.16, 150.0),
                Color::oklch(0.780, 0.15, 80.0),
                Color::oklch(0.660, 0.17, 25.0),
                Color::oklch(0.680, 0.14, 250.0),
            ),
            Variant::Light => (
                Color::oklch(0.480, 0.15, 150.0),
                Color::oklch(0.520, 0.14, 80.0),
                Color::oklch(0.500, 0.19, 25.0),
                Color::oklch(0.500, 0.16, 250.0),
            ),
            Variant::HighContrast => (
                Color::oklch(0.800, 0.17, 150.0),
                Color::oklch(0.860, 0.15, 80.0),
                Color::oklch(0.760, 0.19, 25.0),
                Color::oklch(0.800, 0.14, 250.0),
            ),
        };
        Self {
            bg,
            surface,
            surface_raised,
            surface_sunken,
            overlay: Color::lineara(0.0, 0.0, 0.0, overlay_alpha),
            border,
            border_strong,
            text,
            text_muted,
            text_faint,
            text_on_accent: steps[0],
            accent,
            accent_hover,
            accent_pressed,
            accent_subtle: accent.with_alpha(0.14),
            focus_ring,
            selection: accent.with_alpha(0.30),
            success,
            success_subtle: success.with_alpha(0.14),
            warning,
            warning_subtle: warning.with_alpha(0.14),
            danger,
            danger_subtle: danger.with_alpha(0.14),
            info,
            info_subtle: info.with_alpha(0.14),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{ColorRole, ColorSeed, NeutralRamp, Roles, NEUTRAL_STEPS};
    use crate::theme::{Color, ThemeMode};

    fn hue_difference(first: f32, second: f32) -> f32 {
        let difference = (first - second).rem_euclid(360.0);
        difference.min(360.0 - difference)
    }

    #[test]
    fn neutral_ramps_are_monotonic_in_lightness() {
        let dark = NeutralRamp::generate(ColorSeed::dark(), ThemeMode::Dark).lightness();
        let light = NeutralRamp::generate(ColorSeed::light(), ThemeMode::Light).lightness();
        let contrast = NeutralRamp::high_contrast(ColorSeed::high_contrast()).lightness();
        for ramp in [dark, light, contrast] {
            assert_eq!(ramp.len(), NEUTRAL_STEPS);
            for pair in ramp.windows(2) {
                assert!(
                    pair[1] > pair[0],
                    "ramp is not monotonic: {} then {}",
                    pair[0],
                    pair[1]
                );
            }
        }
    }

    #[test]
    fn every_role_is_reachable_and_overridable() {
        let mut roles = Roles::from_seed(ColorSeed::dark(), ThemeMode::Dark);
        for role in ColorRole::ALL {
            let before = roles.get(role);
            assert_eq!(before, roles.get(role));
        }
        let replacement = Color::hex(0x00_FF_00);
        roles.set(ColorRole::Accent, replacement);
        assert_eq!(roles.get(ColorRole::Accent), replacement);
        assert_eq!(roles.accent, replacement);
        assert_eq!(
            ColorRole::ALL.len(),
            25,
            "the role count is part of the public palette"
        );
    }

    #[test]
    fn high_contrast_is_stronger_than_dark() {
        let dark = Roles::from_seed(ColorSeed::dark(), ThemeMode::Dark);
        let contrast = Roles::high_contrast(ColorSeed::high_contrast());
        let dark_ratio = dark.text_muted.contrast_ratio(dark.surface);
        let contrast_ratio = contrast.text_muted.contrast_ratio(contrast.surface);
        assert!(
            contrast_ratio > dark_ratio,
            "high contrast muted {contrast_ratio} must beat dark {dark_ratio}"
        );
    }

    #[test]
    fn accent_seed_keeps_its_hue() {
        let seed = ColorSeed::new(264.0, 0.012, 30.0, 0.16);
        let roles = Roles::from_seed(seed, ThemeMode::Dark);
        let accent = roles.accent.to_oklch();
        assert!(
            hue_difference(accent.hue, 30.0) <= 2.0,
            "accent hue {} drifted from the seed",
            accent.hue
        );
        let neutral = roles.surface.to_oklch();
        assert!(
            neutral.chroma <= 0.02,
            "neutral chroma {} is not neutral",
            neutral.chroma
        );
    }

    #[test]
    fn subtle_variants_are_translucent() {
        let roles = Roles::from_seed(ColorSeed::dark(), ThemeMode::Dark);
        assert!(!roles.accent_subtle.is_opaque());
        assert!(!roles.selection.is_opaque());
        assert!(!roles.success_subtle.is_opaque());
        assert!(!roles.overlay.is_opaque());
        assert!((roles.overlay.alpha() - 0.55).abs() <= 1e-6);
    }
}
