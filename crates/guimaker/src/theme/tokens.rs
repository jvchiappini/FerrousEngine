//! Design tokens: the single home of every design literal in `guimaker`.
//!
//! Spacing, typography, radii, borders, elevation and motion all live here as
//! literal values. Other modules (`theme::mod`, `theme::color`,
//! `theme::roles`) dereference tokens; they never hardcode design numbers.

use serde::{Deserialize, Serialize};

use super::color::Color;

/// The current theme mode, which selects mode-dependent tokens.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ThemeMode {
    /// Dark surfaces with light text.
    Dark,
    /// Light surfaces with dark text.
    Light,
}

/// A spacing step on a four-pixel grid.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Spacing {
    /// 0 px.
    S0,
    /// 4 px.
    S1,
    /// 8 px.
    S2,
    /// 12 px.
    S3,
    /// 16 px.
    S4,
    /// 20 px.
    S5,
    /// 24 px.
    S6,
    /// 32 px.
    S8,
    /// 40 px.
    S10,
    /// 48 px.
    S12,
    /// 64 px.
    S16,
    /// 96 px.
    S24,
}

impl Spacing {
    /// Every spacing step, in ascending order.
    pub const ALL: [Self; 12] = [
        Self::S0,
        Self::S1,
        Self::S2,
        Self::S3,
        Self::S4,
        Self::S5,
        Self::S6,
        Self::S8,
        Self::S10,
        Self::S12,
        Self::S16,
        Self::S24,
    ];

    /// The step's size in logical pixels.
    #[must_use]
    pub const fn px(self) -> f32 {
        match self {
            Self::S0 => 0.0,
            Self::S1 => 4.0,
            Self::S2 => 8.0,
            Self::S3 => 12.0,
            Self::S4 => 16.0,
            Self::S5 => 20.0,
            Self::S6 => 24.0,
            Self::S8 => 32.0,
            Self::S10 => 40.0,
            Self::S12 => 48.0,
            Self::S16 => 64.0,
            Self::S24 => 96.0,
        }
    }

    /// The step's short, stable name.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::S0 => "s0",
            Self::S1 => "s1",
            Self::S2 => "s2",
            Self::S3 => "s3",
            Self::S4 => "s4",
            Self::S5 => "s5",
            Self::S6 => "s6",
            Self::S8 => "s8",
            Self::S10 => "s10",
            Self::S12 => "s12",
            Self::S16 => "s16",
            Self::S24 => "s24",
        }
    }
}

/// A maximum content-column width for `max_width` layouts.
///
/// These are design values, not runtime measurements: a layout clamps a
/// content column to one of them so prose and forms keep a comfortable
/// measure on wide screens.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ContentWidth {
    /// 480 px, a narrow form or dialog column.
    Sm,
    /// 640 px, a single-column form or card.
    Md,
    /// 800 px, a wide panel or dashboard section.
    Lg,
    /// 1024 px, the widest content column.
    Xl,
    /// 672 px, a comfortable prose measure.
    Prose,
}

impl ContentWidth {
    /// Every content width, in the documented order.
    pub const ALL: [Self; 5] = [Self::Sm, Self::Md, Self::Lg, Self::Xl, Self::Prose];

    /// The width in logical pixels.
    #[must_use]
    pub const fn px(self) -> f32 {
        match self {
            Self::Sm => 480.0,
            Self::Md => 640.0,
            Self::Lg => 800.0,
            Self::Xl => 1024.0,
            Self::Prose => 672.0,
        }
    }

    /// The width's short, stable name.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Sm => "sm",
            Self::Md => "md",
            Self::Lg => "lg",
            Self::Xl => "xl",
            Self::Prose => "prose",
        }
    }
}

/// A font weight, mapped to a numeric font weight.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum FontWeight {
    /// Weight 400.
    Regular,
    /// Weight 500.
    Medium,
    /// Weight 600.
    SemiBold,
    /// Weight 700.
    Bold,
}

impl FontWeight {
    /// The numeric weight value.
    #[must_use]
    pub const fn value(self) -> u16 {
        match self {
            Self::Regular => 400,
            Self::Medium => 500,
            Self::SemiBold => 600,
            Self::Bold => 700,
        }
    }

    /// The weight's short, stable name.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Regular => "regular",
            Self::Medium => "medium",
            Self::SemiBold => "semibold",
            Self::Bold => "bold",
        }
    }
}

/// A font family available to the theme.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum FontFamily {
    /// The interface sans-serif family.
    Sans,
    /// The monospaced family.
    Mono,
}

impl FontFamily {
    /// The family's concrete font name.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Sans => SANS_FAMILY,
            Self::Mono => MONO_FAMILY,
        }
    }
}

/// The concrete name of the sans-serif family.
pub const SANS_FAMILY: &str = "Inter";
/// The concrete name of the monospaced family.
pub const MONO_FAMILY: &str = "JetBrains Mono";

/// A fully resolved text style for one typographic role.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct TypeStyle {
    /// Font size in logical pixels.
    pub size: f32,
    /// Font weight.
    pub weight: FontWeight,
    /// Absolute line height in logical pixels.
    pub line_height: f32,
    /// Letter spacing in em units.
    pub tracking: f32,
    /// Font family.
    pub family: FontFamily,
}

/// A semantic role in the type scale.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum TypeRole {
    /// The largest, most prominent text.
    Display,
    /// A page or screen title.
    Title,
    /// A section heading.
    Heading,
    /// A sub-section heading.
    Subheading,
    /// Default body copy.
    Body,
    /// Emphasized body copy.
    BodyStrong,
    /// A form or control label.
    Label,
    /// Small, secondary text.
    Caption,
    /// Uppercase, tracked introductory text.
    Overline,
    /// Inline monospaced text.
    Mono,
    /// A monospaced code block.
    Code,
}

impl TypeRole {
    /// Every type role, in scale order.
    pub const ALL: [Self; Self::COUNT] = [
        Self::Display,
        Self::Title,
        Self::Heading,
        Self::Subheading,
        Self::Body,
        Self::BodyStrong,
        Self::Label,
        Self::Caption,
        Self::Overline,
        Self::Mono,
        Self::Code,
    ];

    /// The number of type roles.
    pub const COUNT: usize = 11;

    /// The role's stable index in the scale.
    #[must_use]
    pub const fn index(self) -> usize {
        match self {
            Self::Display => 0,
            Self::Title => 1,
            Self::Heading => 2,
            Self::Subheading => 3,
            Self::Body => 4,
            Self::BodyStrong => 5,
            Self::Label => 6,
            Self::Caption => 7,
            Self::Overline => 8,
            Self::Mono => 9,
            Self::Code => 10,
        }
    }

    /// The role's short, stable name.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Display => "display",
            Self::Title => "title",
            Self::Heading => "heading",
            Self::Subheading => "subheading",
            Self::Body => "body",
            Self::BodyStrong => "body_strong",
            Self::Label => "label",
            Self::Caption => "caption",
            Self::Overline => "overline",
            Self::Mono => "mono",
            Self::Code => "code",
        }
    }
}

/// A UI density that scales typography.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Density {
    /// Tight spacing for dense data views.
    Compact,
    /// Relaxed spacing for comfortable reading.
    Comfortable,
}

/// The resolved type scale, one style per role.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Typography {
    styles: [TypeStyle; TypeRole::COUNT],
}

impl Typography {
    /// The type scale for a density.
    #[must_use]
    #[allow(clippy::too_many_lines)] // reason: literal type scales are intentionally explicit
    pub const fn for_density(density: Density) -> Self {
        match density {
            Density::Compact => Self {
                styles: [
                    TypeStyle {
                        size: 22.0,
                        weight: FontWeight::SemiBold,
                        line_height: 28.0,
                        tracking: 0.0,
                        family: FontFamily::Sans,
                    },
                    TypeStyle {
                        size: 15.0,
                        weight: FontWeight::SemiBold,
                        line_height: 20.0,
                        tracking: 0.0,
                        family: FontFamily::Sans,
                    },
                    TypeStyle {
                        size: 14.0,
                        weight: FontWeight::SemiBold,
                        line_height: 20.0,
                        tracking: 0.0,
                        family: FontFamily::Sans,
                    },
                    TypeStyle {
                        size: 13.0,
                        weight: FontWeight::SemiBold,
                        line_height: 18.0,
                        tracking: 0.0,
                        family: FontFamily::Sans,
                    },
                    TypeStyle {
                        size: 12.0,
                        weight: FontWeight::Regular,
                        line_height: 16.0,
                        tracking: 0.0,
                        family: FontFamily::Sans,
                    },
                    TypeStyle {
                        size: 12.0,
                        weight: FontWeight::SemiBold,
                        line_height: 16.0,
                        tracking: 0.0,
                        family: FontFamily::Sans,
                    },
                    TypeStyle {
                        size: 11.0,
                        weight: FontWeight::Medium,
                        line_height: 14.0,
                        tracking: 0.0,
                        family: FontFamily::Sans,
                    },
                    TypeStyle {
                        size: 11.0,
                        weight: FontWeight::Regular,
                        line_height: 14.0,
                        tracking: 0.0,
                        family: FontFamily::Sans,
                    },
                    TypeStyle {
                        size: 10.0,
                        weight: FontWeight::SemiBold,
                        line_height: 14.0,
                        tracking: 0.06,
                        family: FontFamily::Sans,
                    },
                    TypeStyle {
                        size: 12.0,
                        weight: FontWeight::Regular,
                        line_height: 16.0,
                        tracking: 0.0,
                        family: FontFamily::Mono,
                    },
                    TypeStyle {
                        size: 12.0,
                        weight: FontWeight::Regular,
                        line_height: 18.0,
                        tracking: 0.0,
                        family: FontFamily::Mono,
                    },
                ],
            },
            Density::Comfortable => Self {
                styles: [
                    TypeStyle {
                        size: 30.0,
                        weight: FontWeight::SemiBold,
                        line_height: 38.0,
                        tracking: -0.01,
                        family: FontFamily::Sans,
                    },
                    TypeStyle {
                        size: 20.0,
                        weight: FontWeight::SemiBold,
                        line_height: 28.0,
                        tracking: -0.005,
                        family: FontFamily::Sans,
                    },
                    TypeStyle {
                        size: 17.0,
                        weight: FontWeight::SemiBold,
                        line_height: 24.0,
                        tracking: 0.0,
                        family: FontFamily::Sans,
                    },
                    TypeStyle {
                        size: 15.0,
                        weight: FontWeight::SemiBold,
                        line_height: 22.0,
                        tracking: 0.0,
                        family: FontFamily::Sans,
                    },
                    TypeStyle {
                        size: 14.0,
                        weight: FontWeight::Regular,
                        line_height: 22.0,
                        tracking: 0.0,
                        family: FontFamily::Sans,
                    },
                    TypeStyle {
                        size: 14.0,
                        weight: FontWeight::SemiBold,
                        line_height: 22.0,
                        tracking: 0.0,
                        family: FontFamily::Sans,
                    },
                    TypeStyle {
                        size: 13.0,
                        weight: FontWeight::Medium,
                        line_height: 18.0,
                        tracking: 0.0,
                        family: FontFamily::Sans,
                    },
                    TypeStyle {
                        size: 12.0,
                        weight: FontWeight::Regular,
                        line_height: 16.0,
                        tracking: 0.0,
                        family: FontFamily::Sans,
                    },
                    TypeStyle {
                        size: 11.0,
                        weight: FontWeight::SemiBold,
                        line_height: 16.0,
                        tracking: 0.06,
                        family: FontFamily::Sans,
                    },
                    TypeStyle {
                        size: 13.0,
                        weight: FontWeight::Regular,
                        line_height: 20.0,
                        tracking: 0.0,
                        family: FontFamily::Mono,
                    },
                    TypeStyle {
                        size: 13.0,
                        weight: FontWeight::Regular,
                        line_height: 20.0,
                        tracking: 0.0,
                        family: FontFamily::Mono,
                    },
                ],
            },
        }
    }

    /// The style for a role.
    #[must_use]
    pub fn get(&self, role: TypeRole) -> TypeStyle {
        self.styles[role.index()]
    }

    /// Replace the style for a role.
    pub fn set(&mut self, role: TypeRole, style: TypeStyle) {
        self.styles[role.index()] = style;
    }

    /// Every style, in role order.
    #[must_use]
    pub fn styles(&self) -> &[TypeStyle; TypeRole::COUNT] {
        &self.styles
    }
}

/// A named corner radius token.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Radius {
    /// No rounding.
    None,
    /// Extra-small rounding.
    Xs,
    /// Small rounding.
    Sm,
    /// Medium rounding.
    Md,
    /// Large rounding.
    Lg,
    /// Extra-large rounding.
    Xl,
    /// Fully rounded (pill or circle).
    Full,
}

impl Radius {
    /// Every radius token, in ascending order.
    pub const ALL: [Self; 7] = [
        Self::None,
        Self::Xs,
        Self::Sm,
        Self::Md,
        Self::Lg,
        Self::Xl,
        Self::Full,
    ];
}

/// The resolved radius values for each token.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Radii {
    /// Radius for [`Radius::None`], in pixels.
    pub none: f32,
    /// Radius for [`Radius::Xs`], in pixels.
    pub xs: f32,
    /// Radius for [`Radius::Sm`], in pixels.
    pub sm: f32,
    /// Radius for [`Radius::Md`], in pixels.
    pub md: f32,
    /// Radius for [`Radius::Lg`], in pixels.
    pub lg: f32,
    /// Radius for [`Radius::Xl`], in pixels.
    pub xl: f32,
}

impl Default for Radii {
    fn default() -> Self {
        Self {
            none: 0.0,
            xs: 2.0,
            sm: 4.0,
            md: 6.0,
            lg: 8.0,
            xl: 12.0,
        }
    }
}

impl Radii {
    /// Resolve a radius token to pixels.
    ///
    /// [`Radius::Full`] resolves to half of `min_side`, producing a pill for
    /// rectangular shapes and a circle for square ones.
    #[must_use]
    pub fn resolve(self, radius: Radius, min_side: f32) -> f32 {
        match radius {
            Radius::None => self.none,
            Radius::Xs => self.xs,
            Radius::Sm => self.sm,
            Radius::Md => self.md,
            Radius::Lg => self.lg,
            Radius::Xl => self.xl,
            Radius::Full => min_side * 0.5,
        }
    }
}

/// A named border width token.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum BorderWidth {
    /// No border.
    None,
    /// A one-pixel hairline.
    Hairline,
    /// A two-pixel border.
    Regular,
}

/// The resolved border widths.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Borders {
    /// Width of a hairline border, in pixels.
    pub hairline: f32,
    /// Width of a regular border, in pixels.
    pub regular: f32,
}

impl Default for Borders {
    fn default() -> Self {
        Self {
            hairline: 1.0,
            regular: 2.0,
        }
    }
}

impl Borders {
    /// Resolve a border width token to pixels.
    #[must_use]
    pub fn resolve(self, width: BorderWidth) -> f32 {
        match width {
            BorderWidth::None => 0.0,
            BorderWidth::Hairline => self.hairline,
            BorderWidth::Regular => self.regular,
        }
    }
}

/// One shadow layer: color plus geometry.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Shadow {
    /// Shadow color, including its alpha.
    pub color: Color,
    /// Horizontal offset in logical pixels.
    pub offset_x: f32,
    /// Vertical offset in logical pixels.
    pub offset_y: f32,
    /// Blur radius in logical pixels.
    pub blur: f32,
    /// Spread in logical pixels.
    pub spread: f32,
}

/// A two-layer elevation: a key shadow, an ambient shadow, and a tint.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Elevation {
    /// The directional key shadow.
    pub key: Shadow,
    /// The soft, omnidirectional ambient shadow.
    pub ambient: Shadow,
    /// An overlay tint applied on top of the surface.
    pub tint: Color,
}

/// A named elevation level.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ElevationLevel {
    /// Flat, no elevation.
    E0,
    /// Subtle lift.
    E1,
    /// A raised card.
    E2,
    /// A floating panel.
    E3,
    /// A modal or overlay.
    E4,
}

impl ElevationLevel {
    /// Every elevation level, in ascending order.
    pub const ALL: [Self; 5] = [Self::E0, Self::E1, Self::E2, Self::E3, Self::E4];

    /// The level's numeric value.
    #[must_use]
    pub const fn level(self) -> u8 {
        match self {
            Self::E0 => 0,
            Self::E1 => 1,
            Self::E2 => 2,
            Self::E3 => 3,
            Self::E4 => 4,
        }
    }
}

/// The resolved elevation values for each level.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct ElevationSet {
    /// The flat level.
    pub e0: Elevation,
    /// The first elevation.
    pub e1: Elevation,
    /// The second elevation.
    pub e2: Elevation,
    /// The third elevation.
    pub e3: Elevation,
    /// The fourth elevation.
    pub e4: Elevation,
}

impl ElevationSet {
    /// The elevation set for a theme mode.
    ///
    /// The tint is transparent in light mode; shadows stay black in both
    /// modes.
    #[must_use]
    pub fn for_mode(mode: ThemeMode) -> Self {
        let tint = |alpha: f32| match mode {
            ThemeMode::Dark => Color::lineara(1.0, 1.0, 1.0, alpha),
            ThemeMode::Light => Color::TRANSPARENT,
        };
        Self {
            e0: Elevation {
                key: shadow(0.0, 0.0, 0.0, 0.0, 0.0),
                ambient: shadow(0.0, 0.0, 0.0, 0.0, 0.0),
                tint: Color::TRANSPARENT,
            },
            e1: Elevation {
                key: shadow(0.0, 2.0, 6.0, 0.0, 0.12),
                ambient: shadow(0.0, 1.0, 3.0, 0.0, 0.20),
                tint: tint(0.020),
            },
            e2: Elevation {
                key: shadow(0.0, 6.0, 18.0, 0.0, 0.14),
                ambient: shadow(0.0, 3.0, 10.0, 0.0, 0.22),
                tint: tint(0.030),
            },
            e3: Elevation {
                key: shadow(0.0, 12.0, 32.0, 0.0, 0.16),
                ambient: shadow(0.0, 6.0, 20.0, 0.0, 0.24),
                tint: tint(0.040),
            },
            e4: Elevation {
                key: shadow(0.0, 20.0, 56.0, 0.0, 0.18),
                ambient: shadow(0.0, 10.0, 32.0, 0.0, 0.28),
                tint: tint(0.050),
            },
        }
    }

    /// The elevation for a level.
    #[must_use]
    pub fn get(&self, level: ElevationLevel) -> Elevation {
        match level {
            ElevationLevel::E0 => self.e0,
            ElevationLevel::E1 => self.e1,
            ElevationLevel::E2 => self.e2,
            ElevationLevel::E3 => self.e3,
            ElevationLevel::E4 => self.e4,
        }
    }
}

/// Build a black key or ambient shadow with the given geometry and alpha.
fn shadow(offset_x: f32, offset_y: f32, blur: f32, spread: f32, alpha: f32) -> Shadow {
    Shadow {
        color: Color::lineara(0.0, 0.0, 0.0, alpha),
        offset_x,
        offset_y,
        blur,
        spread,
    }
}

/// A named animation duration.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Duration {
    /// No animation.
    Instant,
    /// A quick micro-interaction.
    Fast,
    /// A standard transition.
    Base,
    /// A slower, emphasized transition.
    Slow,
}

impl Duration {
    /// The duration in milliseconds.
    #[must_use]
    pub const fn ms(self) -> u64 {
        match self {
            Self::Instant => 0,
            Self::Fast => 90,
            Self::Base => 140,
            Self::Slow => 240,
        }
    }
}

/// A named easing curve, expressed as a cubic bezier.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Easing {
    /// Constant-rate motion.
    Linear,
    /// The default ease for most transitions.
    Standard,
    /// A slower, more expressive ease.
    Emphasized,
    /// A slight overshoot before settling.
    Overshoot,
}

impl Easing {
    /// The cubic bezier control points as `[x1, y1, x2, y2]`.
    #[must_use]
    pub const fn cubic_bezier(self) -> [f32; 4] {
        match self {
            Self::Linear => [0.0, 0.0, 1.0, 1.0],
            Self::Standard => [0.2, 0.0, 0.0, 1.0],
            Self::Emphasized => [0.05, 0.7, 0.1, 1.0],
            Self::Overshoot => [0.34, 1.56, 0.64, 1.0],
        }
    }
}

/// The resolved motion tokens.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Motion {
    /// The fast duration.
    pub fast: Duration,
    /// The base duration.
    pub base: Duration,
    /// The slow duration.
    pub slow: Duration,
    /// The standard easing.
    pub standard: Easing,
    /// The emphasized easing.
    pub emphasized: Easing,
    /// The overshoot easing.
    pub overshoot: Easing,
}

impl Default for Motion {
    fn default() -> Self {
        Self {
            fast: Duration::Fast,
            base: Duration::Base,
            slow: Duration::Slow,
            standard: Easing::Standard,
            emphasized: Easing::Emphasized,
            overshoot: Easing::Overshoot,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{
        BorderWidth, Borders, ContentWidth, Density, Duration, Easing, ElevationLevel,
        ElevationSet, FontFamily, Motion, Radii, Radius, Spacing, ThemeMode, TypeRole, Typography,
    };

    #[test]
    #[allow(clippy::float_cmp)] // reason: token values are exact literals
    fn content_widths_are_the_documented_values() {
        let expected = [
            (ContentWidth::Sm, 480.0, "sm"),
            (ContentWidth::Md, 640.0, "md"),
            (ContentWidth::Lg, 800.0, "lg"),
            (ContentWidth::Xl, 1024.0, "xl"),
            (ContentWidth::Prose, 672.0, "prose"),
        ];
        for (width, px, name) in expected {
            assert_eq!(width.px(), px);
            assert_eq!(width.name(), name);
        }
        assert_eq!(ContentWidth::ALL.len(), expected.len());
    }

    #[test]
    #[allow(clippy::float_cmp)] // reason: token values are exact literals
    fn spacing_values_are_on_the_four_pixel_grid() {
        let expected: [f32; 12] = [
            0.0, 4.0, 8.0, 12.0, 16.0, 20.0, 24.0, 32.0, 40.0, 48.0, 64.0, 96.0,
        ];
        for (step, want) in Spacing::ALL.iter().zip(expected.iter()) {
            assert_eq!(step.px(), *want);
        }
        assert_eq!(Spacing::S0.px(), 0.0);
        for step in Spacing::ALL {
            if step.px() > 0.0 {
                assert_eq!(step.px() % 4.0, 0.0);
            }
        }
    }

    #[test]
    #[allow(clippy::float_cmp)] // reason: token values are exact literals
    fn density_changes_metrics_only() {
        let compact = Typography::for_density(Density::Compact);
        let comfortable = Typography::for_density(Density::Comfortable);
        assert_ne!(compact.get(TypeRole::Body), comfortable.get(TypeRole::Body));

        let radii = Radii::default();
        assert_eq!(radii.none, 0.0);
        assert_eq!(radii.xs, 2.0);
        assert_eq!(radii.sm, 4.0);
        assert_eq!(radii.md, 6.0);
        assert_eq!(radii.lg, 8.0);
        assert_eq!(radii.xl, 12.0);

        let borders = Borders::default();
        assert_eq!(borders.hairline, 1.0);
        assert_eq!(borders.regular, 2.0);

        let motion = Motion::default();
        assert_eq!(motion.fast, Duration::Fast);
        assert_eq!(motion.base, Duration::Base);
        assert_eq!(motion.slow, Duration::Slow);
        assert_eq!(motion.standard, Easing::Standard);
        assert_eq!(motion.emphasized, Easing::Emphasized);
        assert_eq!(motion.overshoot, Easing::Overshoot);

        let mut previous = -1.0_f32;
        for step in Spacing::ALL {
            assert!(step.px() > previous);
            previous = step.px();
        }
    }

    #[test]
    fn typography_invariants() {
        for density in [Density::Compact, Density::Comfortable] {
            let typography = Typography::for_density(density);
            for role in TypeRole::ALL {
                let style = typography.get(role);
                assert!(style.size > 0.0);
                assert!(style.line_height >= style.size);
                assert!(style.tracking.is_finite());
                match role {
                    TypeRole::Mono | TypeRole::Code => {
                        assert_eq!(style.family, FontFamily::Mono);
                    }
                    _ => {
                        assert_eq!(style.family, FontFamily::Sans);
                    }
                }
            }
            assert!(typography.get(TypeRole::Overline).tracking > 0.0);
        }
    }

    #[test]
    #[allow(clippy::float_cmp)] // reason: token values are exact literals
    fn radii_resolve() {
        let radii = Radii::default();
        assert_eq!(radii.resolve(Radius::Xs, 100.0), 2.0);
        assert_eq!(radii.resolve(Radius::Sm, 100.0), 4.0);
        assert_eq!(radii.resolve(Radius::Md, 100.0), 6.0);
        assert_eq!(radii.resolve(Radius::Lg, 100.0), 8.0);
        assert_eq!(radii.resolve(Radius::Xl, 100.0), 12.0);
        assert_eq!(radii.resolve(Radius::Full, 20.0), 10.0);
        assert_eq!(radii.resolve(Radius::None, 100.0), 0.0);
    }

    #[test]
    #[allow(clippy::float_cmp)] // reason: token values are exact literals
    fn borders_resolve() {
        let borders = Borders::default();
        assert_eq!(borders.resolve(BorderWidth::None), 0.0);
        assert_eq!(borders.resolve(BorderWidth::Hairline), 1.0);
        assert_eq!(borders.resolve(BorderWidth::Regular), 2.0);
    }

    #[test]
    #[allow(clippy::float_cmp)] // reason: token values are exact literals
    fn elevation_zero_is_flat() {
        for mode in [ThemeMode::Dark, ThemeMode::Light] {
            let flat = ElevationSet::for_mode(mode).get(ElevationLevel::E0);
            assert_eq!(flat.key.color.alpha(), 0.0);
            assert_eq!(flat.ambient.color.alpha(), 0.0);
            assert_eq!(flat.tint.alpha(), 0.0);
        }
    }

    #[test]
    #[allow(clippy::float_cmp)] // reason: token values are exact literals
    fn elevation_tints_only_in_dark() {
        let dark = ElevationSet::for_mode(ThemeMode::Dark);
        let light = ElevationSet::for_mode(ThemeMode::Light);
        let mut previous_y = 0.0_f32;
        let mut previous_blur = 0.0_f32;
        for level in [
            ElevationLevel::E1,
            ElevationLevel::E2,
            ElevationLevel::E3,
            ElevationLevel::E4,
        ] {
            assert!(dark.get(level).tint.alpha() > 0.0);
            assert_eq!(light.get(level).tint.alpha(), 0.0);
            let key = dark.get(level).key;
            assert!(key.offset_y > previous_y);
            assert!(key.blur > previous_blur);
            previous_y = key.offset_y;
            previous_blur = key.blur;
        }
    }

    #[test]
    #[allow(clippy::float_cmp)] // reason: token values are exact literals
    fn motion_values() {
        assert_eq!(Duration::Instant.ms(), 0);
        assert_eq!(Duration::Fast.ms(), 90);
        assert_eq!(Duration::Base.ms(), 140);
        assert_eq!(Duration::Slow.ms(), 240);
        assert_eq!(Easing::Linear.cubic_bezier(), [0.0, 0.0, 1.0, 1.0]);
        assert_eq!(Easing::Standard.cubic_bezier(), [0.2, 0.0, 0.0, 1.0]);
        assert_eq!(Easing::Emphasized.cubic_bezier(), [0.05, 0.7, 0.1, 1.0]);
        assert_eq!(Easing::Overshoot.cubic_bezier(), [0.34, 1.56, 0.64, 1.0]);
    }

    #[test]
    fn serde_round_trips() {
        let typography = Typography::for_density(Density::Comfortable);
        let encoded = ron::to_string(&typography).expect("serialize typography");
        let decoded: Typography = ron::from_str(&encoded).expect("deserialize typography");
        assert_eq!(decoded, typography);

        let radii = Radii::default();
        let encoded = ron::to_string(&radii).expect("serialize radii");
        let decoded: Radii = ron::from_str(&encoded).expect("deserialize radii");
        assert_eq!(decoded, radii);

        let borders = Borders::default();
        let encoded = ron::to_string(&borders).expect("serialize borders");
        let decoded: Borders = ron::from_str(&encoded).expect("deserialize borders");
        assert_eq!(decoded, borders);

        let elevations = ElevationSet::for_mode(ThemeMode::Dark);
        let encoded = ron::to_string(&elevations).expect("serialize elevations");
        let decoded: ElevationSet = ron::from_str(&encoded).expect("deserialize elevations");
        // Colors travel as 8-bit hex, so the canonical file text is the
        // contract: reloading and saving again must produce the same file.
        let reencoded = ron::to_string(&decoded).expect("serialize elevations again");
        assert_eq!(reencoded, encoded);
        let before = elevations.e1.key.color.to_srgb();
        let after = decoded.e1.key.color.to_srgb();
        assert!((before[3] - after[3]).abs() <= 1.0 / 255.0);

        let motion = Motion::default();
        let encoded = ron::to_string(&motion).expect("serialize motion");
        let decoded: Motion = ron::from_str(&encoded).expect("deserialize motion");
        assert_eq!(decoded, motion);
    }
}
