//! The styling API: tokens in, resolved pixels and colours out.
//!
//! A [`Style`] holds token names, never design numbers. [`Style::resolve`]
//! turns it into a [`ResolvedStyle`] against a theme; layout and the display
//! list only see resolved values. The one deliberate escape hatch is
//! [`SizeSpec::Px`], which the design lint reports.

use crate::text::{Ellipsis, TextStyle};
use crate::theme::{
    BorderWidth, Breakpoint, Color, ColorRole, ContentWidth, Elevation, ElevationLevel, Radius,
    Spacing, Theme, TypeRole,
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

/// The pointer cursor a widget asks for.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[repr(u8)]
pub enum Cursor {
    /// The platform arrow.
    #[default]
    Default,
    /// A hand (a clickable control).
    Pointer,
    /// An I-beam (editable text).
    Text,
    /// An open hand (a draggable surface at rest).
    Grab,
    /// A closed hand (dragging).
    Grabbing,
    /// Horizontal resize.
    ResizeHorizontal,
    /// Vertical resize.
    ResizeVertical,
    /// Not allowed.
    NotAllowed,
    /// A crosshair (drawing, picking).
    Crosshair,
}

impl Cursor {
    /// A stable name for debug output.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Default => "default",
            Self::Pointer => "pointer",
            Self::Text => "text",
            Self::Grab => "grab",
            Self::Grabbing => "grabbing",
            Self::ResizeHorizontal => "resize-h",
            Self::ResizeVertical => "resize-v",
            Self::NotAllowed => "not-allowed",
            Self::Crosshair => "crosshair",
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
    /// A clamped fraction of the container, in logical pixels.
    Fluid(Fluid),
    /// The explicit pixel escape hatch; the design lint reports it.
    Px(f32),
}

impl SizeSpec {
    /// Resolves a token or pixel size; `None` for auto/hug/fill/fraction/fluid.
    #[must_use]
    pub fn length(self) -> Option<f32> {
        match self {
            Self::Token(spacing) => Some(spacing.px()),
            Self::Px(value) | Self::Fraction(value) => Some(value),
            Self::Auto | Self::Hug | Self::Fill | Self::Fluid(_) => None,
        }
    }
}

/// A fluid size: `clamp(min, fraction * container, max)`.
///
/// This is how a width or a type size scales smoothly between breakpoints
/// without a step. The fraction is a layout ratio like
/// [`SizeSpec::Fraction`], not a design value; the bounds are tokens.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Fluid {
    /// The smallest value, in logical pixels.
    pub min: Spacing,
    /// The share of the container width (or height) to aim for.
    pub fraction: f32,
    /// The largest value, in logical pixels.
    pub max: Spacing,
}

impl Fluid {
    /// A clamp of a spacing token, a container fraction and a spacing token.
    #[must_use]
    pub const fn new(min: Spacing, fraction: f32, max: Spacing) -> Self {
        Self { min, fraction, max }
    }

    /// The resolved value for a container of `container` logical pixels.
    #[must_use]
    pub fn resolve(&self, container: f32) -> f32 {
        let preferred = container * self.fraction;
        preferred.clamp(self.min.px(), self.max.px())
    }
}

/// A fluid type size: the role whose size scales between two roles.
///
/// The resolved size is `clamp(min.size, fraction * container, max.size)` and
/// the line height keeps its ratio. Weight, family and tracking come from the
/// node's [`TypeRole`]; use [`Style::text_role`] with the largest of the two.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FluidText {
    /// The smallest role.
    pub min: TypeRole,
    /// The share of the container width (or height) to aim for.
    pub fraction: f32,
    /// The largest role.
    pub max: TypeRole,
}

impl FluidText {
    /// A clamp between two type roles.
    #[must_use]
    pub const fn new(min: TypeRole, fraction: f32, max: TypeRole) -> Self {
        Self { min, fraction, max }
    }
}

/// A value that can change with the container's breakpoint class.
///
/// `.pad(resp![Xs: S4, Lg: S8])` builds one of these. A plain value converts
/// into a constant one, so `.pad(S4)` still reads the same. Resolution picks the
/// entry of the largest declared class that is at or below the container's
/// class; a constant value therefore applies at every width.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Responsive<T> {
    slots: [Option<(Breakpoint, T)>; Breakpoint::COUNT],
}

impl<T> Responsive<T> {
    /// A value with no slots yet; [`Responsive::at`] fills them.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            slots: [None, None, None, None, None],
        }
    }

    /// The same value at every width.
    #[must_use]
    pub fn single(value: T) -> Self {
        Self::new().at(Breakpoint::Xs, value)
    }

    /// The value to use from `breakpoint` upwards.
    #[must_use]
    pub fn at(mut self, breakpoint: Breakpoint, value: T) -> Self {
        self.slots[breakpoint.index()] = Some((breakpoint, value));
        self
    }

    /// The value for a container of `container_width` logical pixels.
    ///
    /// # Panics
    ///
    /// Panics when no slot was ever declared. The [`resp!`](crate::resp) macro
    /// and `From<T>` always declare one, so this only happens if
    /// [`Responsive::new`] is used directly without [`Responsive::at`].
    #[must_use]
    pub fn resolve(&self, container_width: f32, breakpoints: &crate::theme::Breakpoints) -> &T {
        let class = breakpoints.class(container_width);
        let mut best: Option<&(Breakpoint, T)> = None;
        let mut first: Option<&(Breakpoint, T)> = None;
        for entry in self.slots.iter().flatten() {
            first.get_or_insert(entry);
            if entry.0 <= class {
                best = Some(entry);
            }
        }
        let entry = best
            .or(first)
            .expect("responsive value has no declared slot");
        &entry.1
    }

    /// Whether the value is the same at every width.
    ///
    /// A constant value lets layout skip the second responsive pass; a view made
    /// only of constants lays out exactly once, as before phase 3.
    #[must_use]
    pub fn is_constant(&self) -> bool {
        self.slots.iter().flatten().count() <= 1
    }

    /// Whether any declared value matches `predicate`.
    #[must_use]
    pub fn any(&self, predicate: impl Fn(&T) -> bool) -> bool {
        self.slots
            .iter()
            .flatten()
            .any(|(_, value)| predicate(value))
    }

    /// Calls `visit` for every declared slot, in class order.
    pub fn for_each(&self, mut visit: impl FnMut(Breakpoint, &T)) {
        for (breakpoint, value) in self.slots.iter().flatten() {
            visit(*breakpoint, value);
        }
    }
}

impl<T> Default for Responsive<T>
where
    T: Default,
{
    fn default() -> Self {
        Self::single(T::default())
    }
}

impl<T> From<T> for Responsive<T> {
    fn from(value: T) -> Self {
        Self::single(value)
    }
}

/// Builds a [`Responsive`] from `Breakpoint: value` pairs.
///
/// ```
/// use guimaker::prelude::*;
/// let pad: Responsive<Spacing> = resp![Xs: Spacing::S4, Lg: Spacing::S8];
/// let theme = Theme::dark();
/// assert_eq!(*pad.resolve(600.0, &theme.breakpoints), Spacing::S4);
/// assert_eq!(*pad.resolve(1400.0, &theme.breakpoints), Spacing::S8);
/// ```
#[macro_export]
macro_rules! resp {
    ($($breakpoint:ident : $value:expr),+ $(,)?) => {{
        let mut responsive = $crate::style::Responsive::new();
        $(
            responsive = responsive.at(
                $crate::theme::Breakpoint::$breakpoint,
                ::core::convert::Into::into($value),
            );
        )+
        responsive
    }};
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
    pub direction: Responsive<Direction>,
    /// Padding on every edge.
    pub padding: Edges<Responsive<Spacing>>,
    /// Gap between children.
    pub gap: Responsive<Spacing>,
    /// Cross-axis alignment.
    pub align: Responsive<Align>,
    /// Main-axis distribution.
    pub justify: Responsive<Justify>,
    /// The node's own alignment override in its parent's cross axis.
    pub align_self: Option<Align>,
    /// Flex grow factor.
    pub grow: f32,
    /// Flex shrink factor.
    pub shrink: f32,
    /// Width rule.
    pub width: Responsive<SizeSpec>,
    /// Height rule.
    pub height: Responsive<SizeSpec>,
    /// Minimum width as a spacing token.
    pub min_width: Option<Responsive<Spacing>>,
    /// Minimum height as a spacing token.
    pub min_height: Option<Responsive<Spacing>>,
    /// Maximum width as a spacing token.
    pub max_width: Option<Responsive<Spacing>>,
    /// Maximum height as a spacing token.
    pub max_height: Option<Responsive<Spacing>>,
    /// A content column maximum (`max_width(ContentWidth::Prose)`).
    pub content_width: Option<ContentWidth>,
    /// Background colour role.
    pub bg: Option<ColorRole>,
    /// Border colour role; `None` draws no border.
    pub border: Option<ColorRole>,
    /// Border width token.
    pub border_width: BorderWidth,
    /// Corner radius token.
    pub radius: Responsive<Radius>,
    /// Shadow level.
    pub elevation: ElevationLevel,
    /// Opacity, `0.0..=1.0`.
    pub opacity: f32,
    /// Whether the node clips its children.
    pub clip: bool,
    /// Whether the node (and subtree) is laid out at all.
    pub hidden: bool,
    /// Hides the node below a breakpoint, like a CSS container query.
    pub hidden_below: Option<Breakpoint>,
    /// Hides the node from a breakpoint upwards.
    pub hidden_above: Option<Breakpoint>,
    /// The default text tone.
    pub tone: Tone,
    /// The default text role.
    pub text_role: TypeRole,
    /// A fluid type size clamped between two roles.
    pub fluid_text: Option<FluidText>,
    /// Whether digits are tabular.
    pub tabular: bool,
    /// How many lines to show.
    pub max_lines: Option<u16>,
    /// How to shorten an overflowing line.
    pub ellipsis: Ellipsis,
    /// Aspect ratio (width / height).
    pub aspect: Option<f32>,
    /// Grid columns, when the element is a grid.
    pub columns: Option<Responsive<Columns>>,
    /// Flow wrapping.
    pub wrap: bool,
    /// Scroll axes, when the element is a scroll container.
    pub scroll: Responsive<ScrollAxes>,
    /// Shrink priority in a row: 1 gives way first, 9 last; 0 is unmanaged.
    pub priority: u8,
    /// Whether the element is a hit-test target.
    pub interactive: bool,
    /// Whether the element is disabled.
    pub disabled: bool,
    /// An optional tooltip.
    pub tooltip: Option<Tooltip>,
    /// Tab order override. Positive values are visited first (ascending),
    /// negative values last (descending); `None` keeps the tree order.
    pub tab_index: Option<i32>,
    /// The pointer cursor while over the element.
    pub cursor: Cursor,
}

impl Default for Style {
    fn default() -> Self {
        Self {
            direction: Responsive::single(Direction::Row),
            padding: Edges::all(Responsive::single(Spacing::S0)),
            gap: Responsive::single(Spacing::S0),
            align: Responsive::single(Align::Start),
            justify: Responsive::single(Justify::Start),
            align_self: None,
            grow: 0.0,
            shrink: 1.0,
            width: Responsive::single(SizeSpec::Auto),
            height: Responsive::single(SizeSpec::Auto),
            min_width: None,
            min_height: None,
            max_width: None,
            max_height: None,
            content_width: None,
            bg: None,
            border: None,
            border_width: BorderWidth::Hairline,
            radius: Responsive::single(Radius::None),
            elevation: ElevationLevel::E0,
            opacity: 1.0,
            clip: false,
            hidden: false,
            hidden_below: None,
            hidden_above: None,
            tone: Tone::Default,
            text_role: TypeRole::Body,
            fluid_text: None,
            tabular: false,
            max_lines: None,
            ellipsis: Ellipsis::None,
            aspect: None,
            columns: None,
            wrap: false,
            scroll: Responsive::single(ScrollAxes::None),
            priority: 0,
            interactive: false,
            disabled: false,
            tooltip: None,
            tab_index: None,
            cursor: Cursor::Default,
        }
    }
}

impl Style {
    /// Whether any part of the style resolves differently at another width.
    ///
    /// A view whose styles are all constant lays out in one pass; this is what
    /// keeps the phase 2 single-pass cost for a non-responsive screen.
    #[must_use]
    pub fn is_responsive(&self) -> bool {
        !self.direction.is_constant()
            || !self.padding.top.is_constant()
            || !self.padding.right.is_constant()
            || !self.padding.bottom.is_constant()
            || !self.padding.left.is_constant()
            || !self.gap.is_constant()
            || !self.align.is_constant()
            || !self.justify.is_constant()
            || !self.width.is_constant()
            || !self.height.is_constant()
            || self.min_width.is_some_and(|value| !value.is_constant())
            || self.min_height.is_some_and(|value| !value.is_constant())
            || self.max_width.is_some_and(|value| !value.is_constant())
            || self.max_height.is_some_and(|value| !value.is_constant())
            || self.columns.is_some_and(|value| !value.is_constant())
            || !self.scroll.is_constant()
            || !self.radius.is_constant()
            || self.hidden_below.is_some()
            || self.hidden_above.is_some()
            || self.fluid_text.is_some()
            || self.width.any(|value| matches!(value, SizeSpec::Fluid(_)))
            || self.height.any(|value| matches!(value, SizeSpec::Fluid(_)))
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
    /// The node's own alignment override in its parent's cross axis.
    pub align_self: Option<Align>,
    /// Flex grow.
    pub grow: f32,
    /// Flex shrink.
    pub shrink: f32,
    /// Shrink priority (see [`Style::priority`]).
    pub priority: u8,
    /// Width rule.
    pub width: SizeSpec,
    /// Height rule.
    pub height: SizeSpec,
    /// The fraction a fluid width resolved from, when it is one.
    pub fluid_width: Option<f32>,
    /// The fraction a fluid height resolved from, when it is one.
    pub fluid_height: Option<f32>,
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
    /// Resolves every token against `theme` for a node whose container is
    /// `container_width` logical pixels wide.
    ///
    /// The container is the parent's content width (the viewport for the root),
    /// which is what a CSS container query measures. Fluid sizes and
    /// breakpoints both use it.
    #[must_use]
    pub fn resolve(&self, theme: &Theme, container_width: f32) -> ResolvedStyle {
        let spacing = |value: Spacing| theme.spacing(value);
        let breakpoints = &theme.breakpoints;
        let resolved_size = |rule: &Responsive<SizeSpec>| {
            let spec = *rule.resolve(container_width, breakpoints);
            match spec {
                SizeSpec::Fluid(fluid) => SizeSpec::Px(fluid.resolve(container_width)),
                other => other,
            }
        };
        let fluid_fraction =
            |rule: &Responsive<SizeSpec>| match *rule.resolve(container_width, breakpoints) {
                SizeSpec::Fluid(fluid) => Some(fluid.fraction),
                _ => None,
            };
        let type_style = theme.typography.get(self.text_role);
        let mut text_style = TextStyle::from_type(type_style, self.tabular);
        if let Some(fluid) = self.fluid_text {
            let size = (container_width * fluid.fraction).clamp(
                theme.typography.get(fluid.min).size,
                theme.typography.get(fluid.max).size,
            );
            let base = type_style.size;
            let factor = if base > 0.0 { size / base } else { 1.0 };
            text_style.size = size;
            text_style.line_height = (text_style.line_height * factor).max(size);
        }
        let class = breakpoints.class(container_width);
        let hidden_below = self
            .hidden_below
            .is_some_and(|breakpoint| class < breakpoint);
        let hidden_above = self
            .hidden_above
            .is_some_and(|breakpoint| class >= breakpoint);
        ResolvedStyle {
            direction: *self.direction.resolve(container_width, breakpoints),
            padding: Edges {
                top: spacing(*self.padding.top.resolve(container_width, breakpoints)),
                right: spacing(*self.padding.right.resolve(container_width, breakpoints)),
                bottom: spacing(*self.padding.bottom.resolve(container_width, breakpoints)),
                left: spacing(*self.padding.left.resolve(container_width, breakpoints)),
            },
            gap: spacing(*self.gap.resolve(container_width, breakpoints)),
            align: *self.align.resolve(container_width, breakpoints),
            justify: *self.justify.resolve(container_width, breakpoints),
            align_self: self.align_self,
            grow: self.grow,
            shrink: self.shrink,
            priority: self.priority,
            width: resolved_size(&self.width),
            height: resolved_size(&self.height),
            fluid_width: fluid_fraction(&self.width),
            fluid_height: fluid_fraction(&self.height),
            min_width: self
                .min_width
                .map(|value| spacing(*value.resolve(container_width, breakpoints))),
            min_height: self
                .min_height
                .map(|value| spacing(*value.resolve(container_width, breakpoints))),
            max_width: self
                .max_width
                .map(|value| spacing(*value.resolve(container_width, breakpoints)))
                .or_else(|| self.content_width.map(|width| theme.content_width(width))),
            max_height: self
                .max_height
                .map(|value| spacing(*value.resolve(container_width, breakpoints))),
            bg: self.bg.map(|role| theme.role(role)),
            border: self.border.map(|role| theme.role(role)),
            border_width: if self.border.is_some() {
                theme.borders.resolve(self.border_width)
            } else {
                0.0
            },
            radius: *self.radius.resolve(container_width, breakpoints),
            elevation_level: self.elevation,
            elevation: theme.elevation_at(self.elevation),
            opacity: self.opacity.clamp(0.0, 1.0),
            clip: self.clip || self.scroll.resolve(container_width, breakpoints).clips(),
            hidden: self.hidden || hidden_below || hidden_above,
            tone: self.tone,
            text_role: self.text_role,
            text: ResolvedText {
                style: text_style,
                color: self.tone.color(theme),
            },
            max_lines: self.max_lines,
            ellipsis: self.ellipsis,
            aspect: self.aspect,
            columns: self
                .columns
                .map(|value| *value.resolve(container_width, breakpoints)),
            wrap: self.wrap,
            scroll: *self.scroll.resolve(container_width, breakpoints),
            interactive: self.interactive,
            disabled: self.disabled,
        }
    }
}
#[cfg(test)]
mod tests {
    use super::{Columns, Edges, Fluid, Responsive, SizeSpec, Style, Tone};
    use crate::theme::{
        BorderWidth, Breakpoint, Breakpoints, ContentWidth, Radius, Spacing, Theme, TypeRole,
    };

    fn at(style: &Style, width: f32) -> super::ResolvedStyle {
        style.resolve(&Theme::dark(), width)
    }

    #[test]
    fn defaults_are_tokens_not_numbers() {
        let style = Style::default();
        let breakpoints = Breakpoints::default();
        assert_eq!(*style.padding.top.resolve(0.0, &breakpoints), Spacing::S0);
        assert_eq!(*style.gap.resolve(0.0, &breakpoints), Spacing::S0);
        assert_eq!(*style.width.resolve(0.0, &breakpoints), SizeSpec::Auto);
        assert_eq!(*style.radius.resolve(0.0, &breakpoints), Radius::None);
        assert_eq!(style.text_role, TypeRole::Body);
        assert_eq!(style.border_width, BorderWidth::Hairline);
        assert!(!style.is_responsive(), "the defaults are constant");
    }

    #[test]
    fn resolution_uses_the_theme() {
        let theme = Theme::dark();
        let style = Style {
            padding: Edges::all(Responsive::single(Spacing::S4)),
            gap: Responsive::single(Spacing::S2),
            bg: Some(crate::theme::ColorRole::Surface),
            content_width: Some(ContentWidth::Prose),
            ..Style::default()
        };
        let resolved = style.resolve(&theme, 1280.0);
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

    #[test]
    fn responsive_values_pick_the_largest_declared_class() {
        let breakpoints = Breakpoints::default();
        let value = Responsive::single(Spacing::S2)
            .at(Breakpoint::Md, Spacing::S6)
            .at(Breakpoint::Lg, Spacing::S12);
        assert_eq!(*value.resolve(360.0, &breakpoints), Spacing::S2);
        assert_eq!(*value.resolve(767.0, &breakpoints), Spacing::S2);
        assert_eq!(*value.resolve(768.0, &breakpoints), Spacing::S6);
        assert_eq!(*value.resolve(1439.0, &breakpoints), Spacing::S12);
        assert_eq!(*value.resolve(3840.0, &breakpoints), Spacing::S12);
        assert!(!value.is_constant());
        assert!(Responsive::single(Spacing::S4).is_constant());
    }

    #[test]
    fn a_value_declared_from_md_applies_at_small_widths() {
        let breakpoints = Breakpoints::default();
        let value = Responsive::new().at(Breakpoint::Md, Spacing::S8);
        // No entry at or below `Xs`, so the smallest declared entry is used.
        assert_eq!(*value.resolve(320.0, &breakpoints), Spacing::S8);
    }

    #[test]
    fn fluid_sizes_clamp_between_the_tokens() {
        let breakpoints = Breakpoints::default();
        let style = Style {
            width: Responsive::single(SizeSpec::Fluid(Fluid::new(Spacing::S24, 0.5, Spacing::S24))),
            ..Style::default()
        };
        let resolved = style.resolve(&Theme::dark(), 200.0);
        assert_eq!(resolved.width, SizeSpec::Px(96.0));
        assert_eq!(resolved.fluid_width, Some(0.5));
        assert!(style.is_responsive());
        assert_eq!(breakpoints.class(0.0), Breakpoint::Xs);
    }

    #[test]
    fn hidden_below_resolves_to_hidden() {
        let style = Style {
            hidden_below: Some(Breakpoint::Md),
            ..Style::default()
        };
        assert!(at(&style, 500.0).hidden);
        assert!(!at(&style, 800.0).hidden);
    }
}
