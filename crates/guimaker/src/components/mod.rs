//! The component library: every control, finished.
//!
//! A component is a pure function of its data and its message: it returns an
//! `Element` and never reads interaction state. Hover, press, focus, selection
//! and disabled are resolved by the paint layer from the keyed store, so the
//! same builder gives the same tree on every frame. See [`StateSpec`] for the
//! per-state roles a component names once.

pub mod actions;
pub mod containers;
pub mod display;
pub mod inputs;
pub mod overlays;

pub use actions::*;
pub use containers::*;
pub use display::*;
pub use inputs::*;
pub use overlays::*;

use std::sync::Arc;

use crate::icons::Icon;
use crate::style::{Align, Tone};
use crate::theme::{ColorRole, Radius, Spacing, TypeRole};
use crate::view::{text as view_text, Element};

/// The three control sizes the design system ships.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ControlSize {
    /// A dense toolbar control.
    Sm,
    /// The default control.
    #[default]
    Md,
    /// A prominent form or dialog control.
    Lg,
}

impl ControlSize {
    /// The minimum height token.
    #[must_use]
    pub const fn height(self) -> Spacing {
        match self {
            Self::Sm => Spacing::S8,
            Self::Md => Spacing::S10,
            Self::Lg => Spacing::S12,
        }
    }

    /// The icon box token, derived from the label role.
    ///
    /// A control's icon is sized by the same type role its label uses, so a
    /// Small control's icon is 12 px and a Large one's is 16 px. The mapping
    /// lives in [`TypeRole::icon_size`](crate::theme::TypeRole::icon_size).
    #[must_use]
    pub const fn icon(self) -> Spacing {
        self.label_role().icon_size()
    }

    /// The horizontal padding token.
    #[must_use]
    pub const fn pad_x(self) -> Spacing {
        match self {
            Self::Sm => Spacing::S3,
            Self::Md => Spacing::S4,
            Self::Lg => Spacing::S5,
        }
    }

    /// The label role.
    ///
    /// The three sizes step the type role as well as the box: Small reads
    /// Caption, Medium Label and Large Body. [`Self::icon`] follows.
    #[must_use]
    pub const fn label_role(self) -> TypeRole {
        match self {
            Self::Sm => TypeRole::Caption,
            Self::Md => TypeRole::Label,
            Self::Lg => TypeRole::Body,
        }
    }
}

/// The action variants of a button.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ButtonVariant {
    /// The one filled, primary action of a group.
    #[default]
    Primary,
    /// A raised secondary action.
    Secondary,
    /// A borderless low-emphasis action.
    Ghost,
    /// A destructive action.
    Danger,
    /// An inline text action.
    Link,
}

impl ButtonVariant {
    /// The per-state roles this variant uses.
    #[must_use]
    pub const fn spec(self) -> StateSpec {
        match self {
            Self::Primary => StateSpec {
                bg: Some(ColorRole::AccentFill),
                hover_bg: Some(ColorRole::AccentFillHover),
                pressed_bg: Some(ColorRole::AccentFillPressed),
                selected_bg: None,
                disabled_bg: Some(ColorRole::Surface),
                border: Some(ColorRole::Border),
                hover_border: None,
                tone: Tone::OnAccentFill,
                hover_tone: None,
                pressed_tone: None,
                selected_tone: None,
                disabled_tone: Some(Tone::Faint),
                elevation_hover: None,
            },
            Self::Secondary => StateSpec {
                bg: Some(ColorRole::SurfaceRaised),
                hover_bg: Some(ColorRole::Surface),
                pressed_bg: Some(ColorRole::SurfaceSunken),
                selected_bg: None,
                disabled_bg: Some(ColorRole::SurfaceRaised),
                border: Some(ColorRole::Border),
                hover_border: Some(ColorRole::BorderStrong),
                tone: Tone::Default,
                hover_tone: None,
                pressed_tone: None,
                selected_tone: None,
                disabled_tone: Some(Tone::Faint),
                elevation_hover: None,
            },
            Self::Ghost => StateSpec {
                bg: None,
                hover_bg: Some(ColorRole::Surface),
                pressed_bg: Some(ColorRole::SurfaceSunken),
                selected_bg: Some(ColorRole::AccentSubtle),
                disabled_bg: None,
                border: None,
                hover_border: None,
                tone: Tone::Muted,
                hover_tone: Some(Tone::Default),
                pressed_tone: Some(Tone::Default),
                selected_tone: Some(Tone::Default),
                disabled_tone: Some(Tone::Faint),
                elevation_hover: None,
            },
            Self::Danger => StateSpec {
                bg: Some(ColorRole::Surface),
                hover_bg: Some(ColorRole::SurfaceRaised),
                pressed_bg: Some(ColorRole::SurfaceSunken),
                selected_bg: None,
                disabled_bg: Some(ColorRole::Surface),
                border: Some(ColorRole::Danger),
                hover_border: Some(ColorRole::Danger),
                tone: Tone::Danger,
                hover_tone: None,
                pressed_tone: None,
                selected_tone: None,
                disabled_tone: Some(Tone::Faint),
                elevation_hover: None,
            },
            Self::Link => StateSpec {
                bg: None,
                hover_bg: Some(ColorRole::AccentSubtle),
                pressed_bg: Some(ColorRole::AccentSubtle),
                selected_bg: None,
                disabled_bg: None,
                border: None,
                hover_border: None,
                tone: Tone::Accent,
                hover_tone: Some(Tone::Accent),
                pressed_tone: Some(Tone::Accent),
                selected_tone: None,
                disabled_tone: Some(Tone::Faint),
                elevation_hover: None,
            },
        }
    }
}

/// A semantic status tone and its 14 % wash.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Status {
    /// Informational.
    Info,
    /// A success.
    Success,
    /// A warning.
    Warning,
    /// A failure or destructive state.
    Danger,
}

impl Status {
    /// The saturated semantic role.
    #[must_use]
    pub const fn role(self) -> ColorRole {
        match self {
            Self::Info => ColorRole::Info,
            Self::Success => ColorRole::Success,
            Self::Warning => ColorRole::Warning,
            Self::Danger => ColorRole::Danger,
        }
    }

    /// The 14 % semantic wash.
    #[must_use]
    pub const fn subtle(self) -> ColorRole {
        match self {
            Self::Info => ColorRole::InfoSubtle,
            Self::Success => ColorRole::SuccessSubtle,
            Self::Warning => ColorRole::WarningSubtle,
            Self::Danger => ColorRole::DangerSubtle,
        }
    }

    /// The text tone a status reads in.
    #[must_use]
    pub const fn tone(self) -> Tone {
        match self {
            Self::Info => Tone::Info,
            Self::Success => Tone::Success,
            Self::Warning => Tone::Warning,
            Self::Danger => Tone::Danger,
        }
    }
}

/// The per-state roles a component names once.
///
/// A component applies a spec to its surface and its labels; the paint layer
/// picks the role for the current state and blends with the previous colour on
/// the injected clock.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct StateSpec {
    /// The rest background role.
    pub bg: Option<ColorRole>,
    /// The hover background role.
    pub hover_bg: Option<ColorRole>,
    /// The pressed background role.
    pub pressed_bg: Option<ColorRole>,
    /// The selected/checked background role.
    pub selected_bg: Option<ColorRole>,
    /// The disabled background role.
    pub disabled_bg: Option<ColorRole>,
    /// The rest border role.
    pub border: Option<ColorRole>,
    /// The hover border role.
    pub hover_border: Option<ColorRole>,
    /// The rest text tone.
    pub tone: Tone,
    /// The hover text tone.
    pub hover_tone: Option<Tone>,
    /// The pressed text tone.
    pub pressed_tone: Option<Tone>,
    /// The selected/checked text tone.
    pub selected_tone: Option<Tone>,
    /// The disabled text tone.
    pub disabled_tone: Option<Tone>,
    /// The elevation a surface lifts to on hover.
    pub elevation_hover: Option<crate::theme::ElevationLevel>,
}

impl StateSpec {
    /// A spec with only a tone.
    #[must_use]
    pub const fn tone(tone: Tone) -> Self {
        Self {
            bg: None,
            hover_bg: None,
            pressed_bg: None,
            selected_bg: None,
            disabled_bg: None,
            border: None,
            hover_border: None,
            tone,
            hover_tone: None,
            pressed_tone: None,
            selected_tone: None,
            disabled_tone: None,
            elevation_hover: None,
        }
    }

    /// A spec for a surface with a background and border.
    #[must_use]
    pub const fn surface(bg: ColorRole, border: Option<ColorRole>, tone: Tone) -> Self {
        Self {
            bg: Some(bg),
            hover_bg: None,
            pressed_bg: None,
            selected_bg: None,
            disabled_bg: None,
            border,
            hover_border: None,
            tone,
            hover_tone: None,
            pressed_tone: None,
            selected_tone: None,
            disabled_tone: None,
            elevation_hover: None,
        }
    }

    /// Applies the background, border and tone roles to an element.
    #[must_use]
    pub fn apply<Msg>(self, mut element: Element<Msg>) -> Element<Msg> {
        if let Some(role) = self.bg {
            element = element.bg(role);
        }
        if let Some(role) = self.border {
            element = element.border(role);
        }
        element = element.tone(self.tone);
        if let Some(role) = self.hover_bg {
            element = element.hover_bg(role);
        }
        if let Some(role) = self.pressed_bg {
            element = element.pressed_bg(role);
        }
        if let Some(role) = self.selected_bg {
            element = element.selected_bg(role);
        }
        if let Some(role) = self.disabled_bg {
            element = element.disabled_bg(role);
        }
        if let Some(role) = self.hover_border {
            element = element.hover_border(role);
        }
        if let Some(tone) = self.hover_tone {
            element = element.hover_tone(tone);
        }
        if let Some(tone) = self.pressed_tone {
            element = element.pressed_tone(tone);
        }
        if let Some(tone) = self.selected_tone {
            element = element.selected_tone(tone);
        }
        if let Some(tone) = self.disabled_tone {
            element = element.disabled_tone(tone);
        }
        if let Some(level) = self.elevation_hover {
            element = element.hover_elevation(level);
        }
        element
    }

    /// Applies only the text tone roles, for a label or icon child.
    #[must_use]
    pub fn apply_tone<Msg>(self, mut element: Element<Msg>) -> Element<Msg> {
        element = element.tone(self.tone);
        if let Some(tone) = self.hover_tone {
            element = element.hover_tone(tone);
        }
        if let Some(tone) = self.pressed_tone {
            element = element.pressed_tone(tone);
        }
        if let Some(tone) = self.selected_tone {
            element = element.selected_tone(tone);
        }
        if let Some(tone) = self.disabled_tone {
            element = element.disabled_tone(tone);
        }
        element
    }
}

/// An icon element at a control size, in a state-aware tone.
#[must_use]
pub fn icon_box<Msg>(icon: Icon, size: ControlSize, spec: StateSpec) -> Element<Msg> {
    spec.apply_tone(
        Element::<Msg>::icon(icon.path())
            .width(crate::style::SizeSpec::Token(size.icon()))
            .height(crate::style::SizeSpec::Token(size.icon()))
            .align_self(Align::Center),
    )
}

/// A text label at a type role, in a state-aware tone.
#[must_use]
pub fn label_text<Msg>(
    content: impl Into<Arc<str>>,
    role: TypeRole,
    spec: StateSpec,
) -> Element<Msg> {
    spec.apply_tone(view_text::<Msg>(content).text_role(role))
}

/// A square icon-only surface, sized to a control.
#[must_use]
pub fn icon_surface<Msg>(size: ControlSize) -> Element<Msg> {
    Element::row()
        .align(Align::Center)
        .justify(crate::style::Justify::Center)
        .width(crate::style::SizeSpec::Token(size.height()))
        .height(crate::style::SizeSpec::Token(size.height()))
        .radius(Radius::Sm)
}

#[cfg(test)]
mod tests {
    use super::{ButtonVariant, ControlSize, StateSpec, Status};
    use crate::style::Tone;
    use crate::theme::ColorRole;
    use crate::view::Element;

    #[test]
    fn sizes_are_ordered_and_on_the_token_grid() {
        assert!(ControlSize::Sm.height().px() < ControlSize::Md.height().px());
        assert!(ControlSize::Md.height().px() < ControlSize::Lg.height().px());
        assert_eq!(ControlSize::Md.icon(), crate::theme::Spacing::S4);
    }

    #[test]
    fn variants_name_theme_roles_only() {
        let primary = ButtonVariant::Primary.spec();
        assert_eq!(primary.bg, Some(ColorRole::AccentFill));
        assert_eq!(primary.hover_bg, Some(ColorRole::AccentFillHover));
        assert_eq!(ButtonVariant::Danger.spec().tone, Tone::Danger);
        assert_eq!(Status::Success.role(), ColorRole::Success);
    }

    #[test]
    fn a_spec_applies_its_tone_to_a_label() {
        let spec = StateSpec::tone(Tone::Muted);
        let label: Element<()> = spec.apply_tone(crate::view::text("x"));
        assert_eq!(label.style.tone, Tone::Muted);
        assert_eq!(label.style.bg, None);
    }
}
