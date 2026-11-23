//! The design system's tokens, colour roles and themes.
//!
//! This module is the **only** place in `guimaker` with literal design values.
//! Everything else takes tokens: `pad(Spacing::S4)`, `radius(Radius::Md)`,
//! `text(TypeRole::Body)`, `Tone::Muted`. The pixel escape hatch is explicit and
//! reported by the design lint.
//!
//! # Layers
//!
//! - [`tokens`]: spacing, typography (two densities), radii, borders, elevation
//!   and motion.
//! - [`color`]: one [`Color`] type whose constructors name their colour space,
//!   plus `OKLCH` conversion and `WCAG` contrast.
//! - [`roles`]: the 29 colour roles, generated from a [`ColorSeed`] through a
//!   monotonic `OKLCH` neutral ramp.
//! - [`Theme`]: the assembled data (`mode`, `density`, `colors`, `typography`,
//!   `radii`, `borders`, `elevation`, `motion`, `prefers_reduced_motion`),
//!   serialisable to RON.
//!
//! # Reduced motion
//!
//! [`Theme::prefers_reduced_motion`] is the media-query equivalent of CSS
//! `prefers-reduced-motion`. When it is set, [`Theme::duration`] maps every
//! transition duration to [`Duration::Instant`]: colour, opacity, transform,
//! elevation and overlay transitions snap to their end state instead of
//! animating. The caret blink is not a transition, so it keeps blinking; see
//! [`crate::motion`].
//!
//! # Built-ins and guarantees
//!
//! [`Theme::dark`], [`Theme::light`] and [`Theme::high_contrast`] are the three
//! shipped themes. The test suite pins the contrast guarantees on every text
//! role against every surface it may sit on:
//!
//! | Role | Minimum | Applies to |
//! |---|---|---|
//! | `text` | 7:1 | body text |
//! | `text_muted` | 4.5:1 | secondary text |
//! | `text_faint` | 3:1 | large or secondary text only |
//! | `text_on_accent` | 4.5:1 | labels on all three accent states |
//! | `text_on_accent_fill` | 4.5:1 | labels on all three filled accent states |
//! | `focus_ring` | 3:1 | against the four surfaces |
//!
//! A focus ring must be drawn with a surface-coloured gap (an outline offset):
//! one opaque ring colour cannot reach 3:1 against both the dark surfaces and a
//! mid-bright accent because the required luminance ranges do not overlap.
//!
//! # Theme files
//!
//! `Theme` is serde-serialisable, and RON is the on-disk format (G-0003):
//!
//! ```
//! use guimaker::theme::Theme;
//!
//! let theme = Theme::dark();
//! let text = theme.to_ron().expect("serialise a built-in theme");
//! let decoded = Theme::from_ron(&text).expect("deserialise");
//! assert_eq!(decoded.to_ron().expect("reserialise"), text);
//! ```

pub mod color;
pub mod roles;
pub mod tokens;

pub use color::{Color, Oklch};
pub use roles::{ColorRole, ColorSeed, NeutralRamp, Roles, NEUTRAL_STEPS};
pub use tokens::{
    BorderWidth, Borders, Breakpoint, Breakpoints, ContentWidth, Density, Duration, Easing,
    Elevation, ElevationLevel, ElevationSet, FontFamily, FontWeight, Motion, Radii, Radius, Shadow,
    Spacing, ThemeMode, TypeRole, TypeStyle, Typography, MONO_FAMILY, SANS_FAMILY,
};

use serde::{Deserialize, Serialize};

/// A complete theme: the data every `guimaker` view resolves its tokens
/// against.
///
/// The three built-ins are [`Theme::dark`], [`Theme::light`] and
/// [`Theme::high_contrast`]. An app that wants its own accent calls
/// [`Theme::from_seed`] and can still override any role through
/// [`Roles::set`].
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Theme {
    /// Whether the theme is dark or light. High contrast is a dark variant.
    pub mode: ThemeMode,
    /// The typography density.
    pub density: Density,
    /// The seed the palette was generated from. Informational; it is kept in
    /// theme files so a designer can regenerate the palette.
    pub seed: ColorSeed,
    /// The resolved colour roles.
    pub colors: Roles,
    /// The resolved type scale.
    pub typography: Typography,
    /// The resolved corner radii.
    pub radii: Radii,
    /// The resolved border widths.
    pub borders: Borders,
    /// The resolved elevation levels.
    pub elevation: ElevationSet,
    /// The resolved motion tokens.
    pub motion: Motion,
    /// Whether transitions snap to their end state.
    ///
    /// This is the equivalent of the `prefers-reduced-motion` media query.
    /// When set, [`Theme::duration`] returns [`Duration::Instant`] so colour,
    /// opacity, transform, elevation and overlay transitions do not animate.
    /// It is not a transition itself, so a caret still blinks.
    ///
    /// Added in phase 7, so a theme file written before the field existed still
    /// loads: a missing value takes `false`.
    #[serde(default)]
    pub prefers_reduced_motion: bool,
    /// The family chain text shaping tries after the style's own family.
    ///
    /// The chain is per theme; every type role inherits it. Glyphs the chain
    /// still cannot cover fall through to the operating system's fonts, which
    /// the text backend loads lazily. An empty chain is the default and only
    /// costs a lookup when a character the primary face lacks appears.
    #[serde(default)]
    pub font_fallbacks: Vec<crate::theme::FontFamily>,
    /// The container breakpoints responsive values resolve against.
    ///
    /// Added in phase 3, so a theme file written before the field existed still
    /// loads: missing values take `Breakpoints::default`.
    #[serde(default)]
    pub breakpoints: Breakpoints,
}

/// The interaction state of a filled (primary) action.
///
/// A filled button asks [`Theme::filled_action`] for its background and text;
/// the theme picks the matching fill role for the state so view code never
/// selects an accent colour by hand.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AccentState {
    /// The resting state.
    Rest,
    /// The pointer is over the control.
    Hover,
    /// The control is held down.
    Pressed,
}

/// The resolved colours of a filled (primary) action.
///
/// `fill` is the accent fill for the current [`AccentState`]; `text` is the
/// light neutral drawn on it. Both come from the theme, and the pair is
/// guaranteed to meet 4.5:1 in every state and every built-in theme.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FilledAction {
    /// The background fill.
    pub fill: Color,
    /// The text drawn on the fill.
    pub text: Color,
}

impl Theme {
    /// The built-in dark theme, with compact density.
    #[must_use]
    pub fn dark() -> Self {
        Self::from_seed(ColorSeed::dark(), ThemeMode::Dark, Density::Compact)
    }

    /// The built-in light theme, with compact density.
    #[must_use]
    pub fn light() -> Self {
        Self::from_seed(ColorSeed::light(), ThemeMode::Light, Density::Compact)
    }

    /// The built-in high-contrast theme, with compact density.
    ///
    /// It is a dark-mode theme with a pure-grey ramp and a brighter accent.
    #[must_use]
    pub fn high_contrast() -> Self {
        Self::high_contrast_with_seed(ColorSeed::high_contrast(), Density::Compact)
    }

    /// Builds a dark or light theme from a palette seed.
    #[must_use]
    pub fn from_seed(seed: ColorSeed, mode: ThemeMode, density: Density) -> Self {
        Self {
            mode,
            density,
            seed,
            colors: Roles::from_seed(seed, mode),
            typography: Typography::for_density(density),
            radii: Radii::default(),
            borders: Borders::default(),
            elevation: ElevationSet::for_mode(mode),
            motion: Motion::default(),
            prefers_reduced_motion: false,
            font_fallbacks: Vec::new(),
            breakpoints: Breakpoints::default(),
        }
    }

    /// Builds a high-contrast theme from a palette seed.
    #[must_use]
    pub fn high_contrast_with_seed(seed: ColorSeed, density: Density) -> Self {
        Self {
            mode: ThemeMode::Dark,
            density,
            seed,
            colors: Roles::high_contrast(seed),
            typography: Typography::for_density(density),
            radii: Radii::default(),
            borders: Borders::default(),
            elevation: ElevationSet::for_mode(ThemeMode::Dark),
            motion: Motion::default(),
            prefers_reduced_motion: false,
            font_fallbacks: Vec::new(),
            breakpoints: Breakpoints::default(),
        }
    }

    /// Returns this theme at another density.
    ///
    /// Only the type scale changes: colours, radii, borders, elevation and
    /// motion are density-independent by construction.
    #[must_use]
    pub fn with_density(mut self, density: Density) -> Self {
        self.density = density;
        self.typography = Typography::for_density(density);
        self
    }

    /// Returns this theme with the reduced-motion preference set.
    ///
    /// This is the `prefers-reduced-motion` media-query equivalent. When
    /// `reduced` is true, [`Theme::duration`] maps every transition duration to
    /// [`Duration::Instant`].
    #[must_use]
    pub fn with_reduced_motion(mut self, reduced: bool) -> Self {
        self.prefers_reduced_motion = reduced;
        self
    }

    /// Resolves a motion duration against the reduced-motion preference.
    ///
    /// Returns [`Duration::Instant`] when [`Theme::prefers_reduced_motion`] is
    /// set, so colour, opacity, transform, elevation and overlay transitions
    /// snap to their end state. Otherwise `duration` passes through unchanged.
    /// The caret blink is not a transition and is unaffected.
    #[must_use]
    pub fn duration(&self, duration: Duration) -> Duration {
        if self.prefers_reduced_motion {
            Duration::Instant
        } else {
            duration
        }
    }

    /// Reads a colour role.
    #[must_use]
    pub fn role(&self, role: ColorRole) -> Color {
        self.colors.get(role)
    }

    /// Resolves the fill and text of a filled (primary) action.
    ///
    /// This is the single hook filled buttons use: it maps [`AccentState`] to
    /// `accent_fill`, `accent_fill_hover` or `accent_fill_pressed` and always
    /// pairs it with `text_on_accent_fill`. The accent roles themselves stay
    /// reserved for focus, links, selection outlines and accent text.
    #[must_use]
    pub fn filled_action(&self, state: AccentState) -> FilledAction {
        let fill = match state {
            AccentState::Rest => ColorRole::AccentFill,
            AccentState::Hover => ColorRole::AccentFillHover,
            AccentState::Pressed => ColorRole::AccentFillPressed,
        };
        FilledAction {
            fill: self.role(fill),
            text: self.role(ColorRole::TextOnAccentFill),
        }
    }

    /// Resolves a spacing token to logical pixels.
    #[must_use]
    pub fn spacing(&self, spacing: Spacing) -> f32 {
        spacing.px()
    }

    /// Resolves a content-column width token to logical pixels.
    #[must_use]
    pub fn content_width(&self, width: ContentWidth) -> f32 {
        width.px()
    }

    /// Resolves an elevation level.
    #[must_use]
    pub fn elevation_at(&self, level: ElevationLevel) -> Elevation {
        self.elevation.get(level)
    }

    /// Serialises the theme to RON (G-0003).
    ///
    /// # Errors
    ///
    /// Returns the `ron` error when the theme cannot be serialised.
    pub fn to_ron(&self) -> Result<String, ron::Error> {
        ron::ser::to_string_pretty(self, ron::ser::PrettyConfig::default())
    }

    /// Reads a theme from RON (G-0003).
    ///
    /// # Errors
    ///
    /// Returns the `ron` error when `text` is not a valid theme.
    pub fn from_ron(text: &str) -> Result<Self, ron::error::SpannedError> {
        ron::from_str(text)
    }
}

impl Default for Theme {
    fn default() -> Self {
        Self::dark()
    }
}

#[cfg(test)]
mod tests {
    use super::{
        AccentState, Color, ColorRole, ContentWidth, Density, Duration, Spacing, Theme, TypeRole,
    };

    /// The surfaces a text role may sit on.
    const SURFACES: [ColorRole; 4] = [
        ColorRole::Bg,
        ColorRole::Surface,
        ColorRole::SurfaceRaised,
        ColorRole::SurfaceSunken,
    ];

    /// A constructor for one of the built-in themes.
    type ThemeFactory = fn() -> Theme;

    const BUILT_INS: [(&str, ThemeFactory); 3] = [
        ("dark", Theme::dark),
        ("light", Theme::light),
        ("high_contrast", Theme::high_contrast),
    ];

    fn assert_contrast(name: &str, theme: &Theme, print: bool) {
        for surface in SURFACES {
            let background = theme.role(surface);
            let text = theme.role(ColorRole::Text).contrast_ratio(background);
            let muted = theme.role(ColorRole::TextMuted).contrast_ratio(background);
            let faint = theme.role(ColorRole::TextFaint).contrast_ratio(background);
            let ring = theme.role(ColorRole::FocusRing).contrast_ratio(background);
            if print {
                println!(
                    "{name:13} {:14} text {text:5.2} muted {muted:5.2} faint {faint:5.2} ring {ring:5.2}",
                    surface.name()
                );
            }
            assert!(
                text >= 7.0,
                "{name}: text on {} is {text:.2}:1, needs 7:1",
                surface.name()
            );
            assert!(
                muted >= 4.5,
                "{name}: text_muted on {} is {muted:.2}:1, needs 4.5:1",
                surface.name()
            );
            assert!(
                faint >= 3.0,
                "{name}: text_faint on {} is {faint:.2}:1, needs 3:1",
                surface.name()
            );
            assert!(
                ring >= 3.0,
                "{name}: focus_ring on {} is {ring:.2}:1, needs 3:1",
                surface.name()
            );
            for role in [
                ColorRole::Success,
                ColorRole::Warning,
                ColorRole::Danger,
                ColorRole::Info,
            ] {
                let ratio = theme.role(role).contrast_ratio(background);
                if print {
                    println!(
                        "{name:13} {:14} {:14} {ratio:5.2}",
                        surface.name(),
                        role.name()
                    );
                }
                assert!(
                    ratio >= 4.5,
                    "{name}: {} on {} is {ratio:.2}:1, needs 4.5:1",
                    role.name(),
                    surface.name()
                );
            }
        }
        for accent in [
            ColorRole::Accent,
            ColorRole::AccentHover,
            ColorRole::AccentPressed,
        ] {
            let ratio = theme
                .role(ColorRole::TextOnAccent)
                .contrast_ratio(theme.role(accent));
            if print {
                println!(
                    "{name:13} {:14} {:14} {ratio:5.2}",
                    accent.name(),
                    "text_on_accent"
                );
            }
            assert!(
                ratio >= 4.5,
                "{name}: text_on_accent on {} is {ratio:.2}:1, needs 4.5:1",
                accent.name()
            );
        }
        for fill in [
            ColorRole::AccentFill,
            ColorRole::AccentFillHover,
            ColorRole::AccentFillPressed,
        ] {
            let ratio = theme
                .role(ColorRole::TextOnAccentFill)
                .contrast_ratio(theme.role(fill));
            if print {
                println!(
                    "{name:13} {:14} text_on_accent_fill {ratio:5.2}",
                    fill.name()
                );
            }
            assert!(
                ratio >= 4.5,
                "{name}: text_on_accent_fill on {} is {ratio:.2}:1, needs 4.5:1",
                fill.name()
            );
        }
    }

    #[test]
    fn built_in_contrast_guarantees() {
        for (name, build) in BUILT_INS {
            assert_contrast(name, &build(), false);
        }
    }

    #[test]
    fn contrast_table() {
        for (name, build) in BUILT_INS {
            println!("== {name}");
            assert_contrast(name, &build(), true);
        }
    }

    #[test]
    fn filled_action_states_and_role_count() {
        assert_eq!(
            ColorRole::ALL.len(),
            29,
            "the role count is part of the public palette"
        );
        for (name, build) in BUILT_INS {
            let theme = build();
            for (state, fill) in [
                (AccentState::Rest, ColorRole::AccentFill),
                (AccentState::Hover, ColorRole::AccentFillHover),
                (AccentState::Pressed, ColorRole::AccentFillPressed),
            ] {
                let action = theme.filled_action(state);
                assert_eq!(action.fill, theme.role(fill));
                assert_eq!(action.text, theme.role(ColorRole::TextOnAccentFill));
                let ratio = action.text.contrast_ratio(action.fill);
                assert!(
                    ratio >= 4.5,
                    "{name}: filled_action text on {} is {ratio:.2}:1, needs 4.5:1",
                    fill.name()
                );
            }
        }
    }

    #[test]
    #[allow(clippy::float_cmp)] // reason: token values are exact literals
    fn content_width_resolves_through_the_theme() {
        let theme = Theme::dark();
        assert_eq!(theme.content_width(ContentWidth::Sm), 480.0);
        assert_eq!(theme.content_width(ContentWidth::Md), 640.0);
        assert_eq!(theme.content_width(ContentWidth::Lg), 800.0);
        assert_eq!(theme.content_width(ContentWidth::Xl), 1024.0);
        assert_eq!(theme.content_width(ContentWidth::Prose), 672.0);
    }

    #[test]
    fn theme_files_round_trip() {
        for (name, build) in BUILT_INS {
            let theme = build();
            let text = theme.to_ron().expect("serialise a built-in theme");
            let decoded = Theme::from_ron(&text).expect("deserialise a built-in theme");
            // The file is the contract: colours travel as 8-bit hex, so a
            // loaded theme must re-serialise to the same canonical text.
            let reencoded = decoded.to_ron().expect("serialise the loaded theme");
            assert_eq!(reencoded, text, "{name} did not round-trip");
            for role in ColorRole::ALL {
                let before = theme.role(role).to_srgb();
                let after = decoded.role(role).to_srgb();
                for channel in 0..4 {
                    assert!(
                        (before[channel] - after[channel]).abs() <= 1.0 / 255.0,
                        "{name}: {} channel {channel} drifted by more than one 8-bit step",
                        role.name()
                    );
                }
            }
        }
    }

    #[test]
    fn reduced_motion_maps_durations_to_instant() {
        let normal = Theme::dark();
        for duration in [Duration::Fast, Duration::Base, Duration::Slow] {
            assert_eq!(normal.duration(duration), duration);
        }
        let reduced = normal.clone().with_reduced_motion(true);
        for duration in [Duration::Fast, Duration::Base, Duration::Slow] {
            assert_eq!(reduced.duration(duration), Duration::Instant);
        }
        assert_eq!(reduced.duration(Duration::Instant), Duration::Instant);
        assert!(!normal.prefers_reduced_motion);
        assert!(reduced.prefers_reduced_motion);
    }

    #[test]
    fn reduced_motion_flag_round_trips_through_ron() {
        let theme = Theme::dark().with_reduced_motion(true);
        let text = theme.to_ron().expect("serialise a reduced-motion theme");
        let decoded = Theme::from_ron(&text).expect("deserialise a reduced-motion theme");
        assert!(decoded.prefers_reduced_motion);
        // Colours travel as 8-bit hex, so the canonical file text is the
        // contract rather than exact float equality.
        let reencoded = decoded.to_ron().expect("serialise the loaded theme");
        assert_eq!(reencoded, text);
    }

    #[test]
    fn density_switch_changes_metrics_only() {
        let compact = Theme::dark();
        let comfortable = compact.clone().with_density(Density::Comfortable);
        assert_eq!(compact.mode, comfortable.mode);
        assert_eq!(compact.seed, comfortable.seed);
        assert_eq!(compact.colors, comfortable.colors);
        assert_eq!(compact.radii, comfortable.radii);
        assert_eq!(compact.borders, comfortable.borders);
        assert_eq!(compact.elevation, comfortable.elevation);
        assert_eq!(compact.motion, comfortable.motion);
        assert_eq!(compact.density, Density::Compact);
        assert_eq!(comfortable.density, Density::Comfortable);
        assert_ne!(
            compact.typography.get(TypeRole::Body),
            comfortable.typography.get(TypeRole::Body)
        );
    }

    #[test]
    fn any_role_can_be_overridden() {
        let mut theme = Theme::dark();
        let replacement = Color::hex(0xFF_00_00);
        theme.colors.set(ColorRole::Accent, replacement);
        assert_eq!(theme.role(ColorRole::Accent), replacement);
    }

    #[test]
    #[allow(clippy::float_cmp)] // reason: token values are exact literals
    fn spacing_resolves_through_the_theme() {
        let theme = Theme::dark();
        assert_eq!(theme.spacing(Spacing::S4), 16.0);
        assert_eq!(theme.spacing(Spacing::S0), 0.0);
    }

    #[test]
    fn high_contrast_is_a_dark_variant() {
        use super::ThemeMode;
        assert_eq!(Theme::high_contrast().mode, ThemeMode::Dark);
        assert_eq!(Theme::default(), Theme::dark());
    }
}
