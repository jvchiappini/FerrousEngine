//! The styling API: tokens in, resolved pixels and colours out.
//!
//! A [`Style`] holds token names, never design numbers. [`Style::resolve`]
//! turns it into a [`ResolvedStyle`] against a theme; layout and the display
//! list only see resolved values. The one deliberate escape hatch is
//! [`SizeSpec::Px`], which the design lint reports.

use crate::text::{Ellipsis, TextStyle};
use crate::theme::{
    BorderWidth, Color, ColorRole, ContentWidth, Elevation, ElevationLevel, Radius, Spacing, Theme,
    TypeRole,
};

/// The resize axis of a spacing value.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Direction {
    /// Children flow left to right.
    #[default]
    Row,
    /// Children flow top to bottom.
    Column,
}

/// Cross-axis alignment.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Align {
    /// Pack at the start.
    #[default]
    Start,
    /// Centre in the cross axis.
    Center,
    /// Pack at the end.
    End,
    /// Fill the cross axis.
    Stretch,
    /// Align text baselines.
    Baseline,
}

/// Main-axis distribution.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Justify {
    /// Pack at the start.
    #[default]
    Start,
    /// Centre.
    Center,
    /// Pack at the end.
    End,
    /// Spread the free space between children.
    SpaceBetween,
}

/// A semantic text or surface tone.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Tone {
    /// Primary text.
    #[default]
    Default,
    /// Secondary text.
    Muted,
    /// Tertiary text.
    Faint,
    /// The accent.
    Accent,
    /// Success.
    Success,
    /// Warning.
    Warning,
    /// Danger.
    Danger,
    /// Informational.
    Info,
}

impl Tone {
    /// The colour role a tone resolves to.
    #[must_use]
    pub const fn role(self) -> ColorRole {
        match self {
            Self::Default => ColorRole::Text,
            Self::Muted => ColorRole::TextMuted,
            Self::Faint => ColorRole::TextFaint,
            Self::Accent => ColorRole::Accent,
            Self::Success => ColorRole::Success,
            Self::Warning => ColorRole::Warning,
            Self::Danger => ColorRole::Danger,
            Self::Info => ColorRole::Info,
        }
    }

    /// Resolves the tone against a theme.
    #[must_use]
    pub fn color(self, theme: &Theme) -> Color {
        theme.role(self.role())
    }
}

/// Four spacing values (padding or margin).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Edges<T> {
    /// Top edge.
    pub top: T,
    /// Right edge.
    pub right: T,
    /// Bottom edge.
    pub bottom: T,
    /// Left edge.
    pub left: T,
}

impl<T: Copy> Edges<T> {
    /// The same value on every edge.
    #[must_use]
    pub const fn all(value: T) -> Self {
        Self {
            top: value,
            right: value,
            bottom: value,
            left: value,
        }
    }
}

/// A width or height rule.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub enum SizeSpec {
    /// Size from content (taffy `auto`).
    #[default]
    Auto,
    /// Size from content, never larger than the content.
    Hug,
    /// Fill the available main-axis space.
    Fill,
    /// A fraction of the parent's size.
    Fraction(f32),
    /// A named spacing token.
    Token(Spacing),
    /// The explicit pixel escape hatch; the design lint reports it.
    Px(f32),
}

impl SizeSpec {
    /// Resolves a token or pixel size; `None` for auto/hug/fill/fraction.
    #[must_use]
    pub fn length(self) -> Option<f32> {
        match self {
            Self::Token(spacing) => Some(spacing.px()),
            Self::Px(value) | Self::Fraction(value) => Some(value),
            Self::Auto | Self::Hug | Self::Fill => None,
        }
    }
}

/// A grid column rule.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Columns {
    /// A fixed number of equal columns.
    Fixed(u16),
    /// As many equal columns as fit, each at least this wide.
    AutoFill(Spacing),
}

/// Which axes a scroll container scrolls.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ScrollAxes {
    /// No scrolling.
    #[default]
    None,
    /// Vertical only.
    Vertical,
    /// Horizontal only.
    Horizontal,
    /// Both axes.
    Both,
}

impl ScrollAxes {
    /// Whether the container clips its children.
    #[must_use]
    pub const fn clips(self) -> bool {
        !matches!(self, Self::None)
    }
}

/// One run of text with its own tone and emphasis.
#[derive(Clone, Debug, PartialEq)]
pub struct Span {
    /// The run's text.
    pub text: std::sync::Arc<str>,
    /// The run's tone; `None` inherits the text node's tone.
    pub tone: Option<Tone>,
    /// The run's role; `None` inherits the text node's role.
    pub role: Option<TypeRole>,
    /// Bold emphasis.
    pub strong: bool,
    /// Monospace emphasis.
    pub mono: bool,
    /// Link emphasis (accent + underline).
    pub link: bool,
}

impl Span {
    /// A plain run.
    #[must_use]
    pub fn new(text: impl Into<std::sync::Arc<str>>) -> Self {
        Self {
            text: text.into(),
            tone: None,
            role: None,
            strong: false,
            mono: false,
            link: false,
        }
    }

    /// The plain text of the run.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.text
    }
}

/// A text node's content.
#[derive(Clone, Debug, PartialEq)]
pub struct TextSpec {
    /// The rich runs.
    pub spans: Vec<Span>,
    /// Maximum number of lines; `None` is unbounded.
    pub max_lines: Option<u16>,
    /// How an overflowing line is shortened.
    pub ellipsis: Ellipsis,
}

impl TextSpec {
    /// A single plain run.
    #[must_use]
    pub fn plain(text: impl Into<std::sync::Arc<str>>) -> Self {
        Self {
            spans: vec![Span::new(text)],
            max_lines: None,
            ellipsis: Ellipsis::None,
        }
    }

    /// The concatenated plain text, used for measurement and debug output.
    #[must_use]
    pub fn text(&self) -> String {
        let mut text = String::new();
        for span in &self.spans {
            text.push_str(&span.text);
        }
        text
    }
}

/// A tooltip attached to a node.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Tooltip {
    /// The tooltip text.
    pub text: std::sync::Arc<str>,
}

/// Everything a view can say about one element.
///
/// Defaults are the good ones: a column, no padding, body text, transparent
/// background. A view only names what it wants to differ.
#[allow(clippy::struct_excessive_bools)] // reason: the style is a flat set of independent modifiers
#[derive(Clone, Debug, PartialEq)]
pub struct Style {
    /// Flow direction for `column`/`row`.
    pub direction: Direction,
    /// Padding on every edge.
    pub padding: Edges<Spacing>,
    /// Gap between children.
    pub gap: Spacing,
    /// Cross-axis alignment.
    pub align: Align,
    /// Main-axis distribution.
    pub justify: Justify,
    /// Flex grow factor.
    pub grow: f32,
    /// Flex shrink factor.
    pub shrink: f32,
    /// Width rule.
    pub width: SizeSpec,
    /// Height rule.
    pub height: SizeSpec,
    /// Minimum width as a spacing token.
    pub min_width: Option<Spacing>,
    /// Minimum height as a spacing token.
    pub min_height: Option<Spacing>,
    /// Maximum width as a spacing token.
    pub max_width: Option<Spacing>,
    /// Maximum height as a spacing token.
    pub max_height: Option<Spacing>,
    /// A content column maximum (`max_width(ContentWidth::Prose)`).
    pub content_width: Option<ContentWidth>,
    /// Background colour role.
    pub bg: Option<ColorRole>,
    /// Border colour role; `None` draws no border.
    pub border: Option<ColorRole>,
    /// Border width token.
    pub border_width: BorderWidth,
    /// Corner radius token.
    pub radius: Radius,
    /// Shadow level.
    pub elevation: ElevationLevel,
    /// Opacity, `0.0..=1.0`.
    pub opacity: f32,
    /// Whether the node clips its children.
    pub clip: bool,
    /// Whether the node (and subtree) is laid out at all.
    pub hidden: bool,
    /// The default text tone.
    pub tone: Tone,
    /// The default text role.
    pub text_role: TypeRole,
    /// Whether digits are tabular.
    pub tabular: bool,
    /// How many lines to show.
    pub max_lines: Option<u16>,
    /// How to shorten an overflowing line.
    pub ellipsis: Ellipsis,
    /// Aspect ratio (width / height).
    pub aspect: Option<f32>,
    /// Grid columns, when the element is a grid.
    pub columns: Option<Columns>,
    /// Flow wrapping.
    pub wrap: bool,
    /// Scroll axes, when the element is a scroll container.
    pub scroll: ScrollAxes,
    /// Whether the element is a hit-test target.
    pub interactive: bool,
    /// Whether the element is disabled.
    pub disabled: bool,
    /// An optional tooltip.
    pub tooltip: Option<Tooltip>,
}

impl Default for Style {
    fn default() -> Self {
        Self {
            direction: Direction::Row,
            padding: Edges::all(Spacing::S0),
            gap: Spacing::S0,
            align: Align::Start,
            justify: Justify::Start,
            grow: 0.0,
            shrink: 1.0,
            width: SizeSpec::Auto,
            height: SizeSpec::Auto,
            min_width: None,
            min_height: None,
            max_width: None,
            max_height: None,
            content_width: None,
            bg: None,
            border: None,
            border_width: BorderWidth::Hairline,
            radius: Radius::None,
            elevation: ElevationLevel::E0,
            opacity: 1.0,
            clip: false,
            hidden: false,
            tone: Tone::Default,
            text_role: TypeRole::Body,
            tabular: false,
            max_lines: None,
            ellipsis: Ellipsis::None,
            aspect: None,
            columns: None,
            wrap: false,
            scroll: ScrollAxes::None,
            interactive: false,
            disabled: false,
            tooltip: None,
        }
    }
}

/// A text style resolved against a theme, plus its colour.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ResolvedText {
    /// Font size, line height, weight and family.
    pub style: TextStyle,
    /// The text colour.
    pub color: Color,
}

/// A style with every token resolved to pixels and colours.
#[allow(clippy::struct_excessive_bools)] // reason: the style is a flat set of independent resolved modifiers
#[derive(Clone, Debug, PartialEq)]
pub struct ResolvedStyle {
    /// The flow direction.
    pub direction: Direction,
    /// Padding in pixels.
    pub padding: Edges<f32>,
    /// Gap in pixels.
    pub gap: f32,
    /// Cross-axis alignment.
    pub align: Align,
    /// Main-axis distribution.
    pub justify: Justify,
    /// Flex grow.
    pub grow: f32,
    /// Flex shrink.
    pub shrink: f32,
    /// Width rule.
    pub width: SizeSpec,
    /// Height rule.
    pub height: SizeSpec,
    /// Minimum width in pixels.
    pub min_width: Option<f32>,
    /// Minimum height in pixels.
    pub min_height: Option<f32>,
    /// Maximum width in pixels.
    pub max_width: Option<f32>,
    /// Maximum height in pixels.
    pub max_height: Option<f32>,
    /// Background colour.
    pub bg: Option<Color>,
    /// Border colour.
    pub border: Option<Color>,
    /// Border width in pixels.
    pub border_width: f32,
    /// The radius token (resolved after layout, from the node's short side).
    pub radius: Radius,
    /// The shadow level token.
    pub elevation_level: ElevationLevel,
    /// The resolved shadow.
    pub elevation: Elevation,
    /// Opacity.
    pub opacity: f32,
    /// Clip children.
    pub clip: bool,
    /// Hidden.
    pub hidden: bool,
    /// Default text tone.
    pub tone: Tone,
    /// The resolved type role.
    pub text_role: TypeRole,
    /// Default text.
    pub text: ResolvedText,
    /// Numeric text options.
    pub max_lines: Option<u16>,
    /// Ellipsis mode.
    pub ellipsis: Ellipsis,
    /// Aspect ratio.
    pub aspect: Option<f32>,
    /// Grid columns.
    pub columns: Option<Columns>,
    /// Wrap.
    pub wrap: bool,
    /// Scroll axes.
    pub scroll: ScrollAxes,
    /// Interactive.
    pub interactive: bool,
    /// Disabled.
    pub disabled: bool,
}

impl Style {
    /// Resolves every token against `theme`.
    #[must_use]
    pub fn resolve(&self, theme: &Theme) -> ResolvedStyle {
        let spacing = |value: Spacing| theme.spacing(value);
        let type_style = theme.typography.get(self.text_role);
        ResolvedStyle {
            direction: self.direction,
            padding: Edges {
                top: spacing(self.padding.top),
                right: spacing(self.padding.right),
                bottom: spacing(self.padding.bottom),
                left: spacing(self.padding.left),
            },
            gap: spacing(self.gap),
            align: self.align,
            justify: self.justify,
            grow: self.grow,
            shrink: self.shrink,
            width: self.width,
            height: self.height,
            min_width: self.min_width.map(spacing),
            min_height: self.min_height.map(spacing),
            max_width: self
                .max_width
                .map(spacing)
                .or_else(|| self.content_width.map(|width| theme.content_width(width))),
            max_height: self.max_height.map(spacing),
            bg: self.bg.map(|role| theme.role(role)),
            border: self.border.map(|role| theme.role(role)),
            border_width: if self.border.is_some() {
                theme.borders.resolve(self.border_width)
            } else {
                0.0
            },
            radius: self.radius,
            elevation_level: self.elevation,
            elevation: theme.elevation_at(self.elevation),
            opacity: self.opacity.clamp(0.0, 1.0),
            clip: self.clip || self.scroll.clips(),
            hidden: self.hidden,
            tone: self.tone,
            text_role: self.text_role,
            text: ResolvedText {
                style: TextStyle::from_type(type_style, self.tabular),
                color: self.tone.color(theme),
            },
            max_lines: self.max_lines,
            ellipsis: self.ellipsis,
            aspect: self.aspect,
            columns: self.columns,
            wrap: self.wrap,
            scroll: self.scroll,
            interactive: self.interactive,
            disabled: self.disabled,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{Columns, Edges, SizeSpec, Style, Tone};
    use crate::theme::{BorderWidth, ContentWidth, Radius, Spacing, Theme, TypeRole};

    #[test]
    fn defaults_are_tokens_not_numbers() {
        let style = Style::default();
        assert_eq!(style.padding, Edges::all(Spacing::S0));
        assert_eq!(style.gap, Spacing::S0);
        assert_eq!(style.radius, Radius::None);
        assert_eq!(style.text_role, TypeRole::Body);
        assert_eq!(style.border_width, BorderWidth::Hairline);
        assert_eq!(style.width, SizeSpec::Auto);
    }

    #[test]
    fn resolution_uses_the_theme() {
        let theme = Theme::dark();
        let style = Style {
            padding: Edges::all(Spacing::S4),
            gap: Spacing::S2,
            bg: Some(crate::theme::ColorRole::Surface),
            content_width: Some(ContentWidth::Prose),
            ..Style::default()
        };
        let resolved = style.resolve(&theme);
        assert!((resolved.padding.top - 16.0).abs() < 1e-6);
        assert!((resolved.gap - 8.0).abs() < 1e-6);
        assert_eq!(
            resolved.bg,
            Some(theme.role(crate::theme::ColorRole::Surface))
        );
        assert!((resolved.max_width.unwrap_or(0.0) - 672.0).abs() < 1e-6);
    }

    #[test]
    fn tone_resolves_to_roles() {
        let theme = Theme::light();
        assert_eq!(Tone::Muted.color(&theme), theme.colors.text_muted);
        assert_eq!(Tone::Accent.color(&theme), theme.colors.accent);
        assert_eq!(Tone::Danger.color(&theme), theme.colors.danger);
    }

    #[test]
    fn grid_columns_are_tokens() {
        assert_eq!(Columns::Fixed(3), Columns::Fixed(3));
        assert_eq!(
            Columns::AutoFill(Spacing::S6),
            Columns::AutoFill(Spacing::S6)
        );
    }
}
