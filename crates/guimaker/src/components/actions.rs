//! Action components: buttons, toggles and grouped actions.
//!
//! Every action names its variant once and gets hover, press, focus,
//! selection, disabled, keyboard activation and animated colour transitions
//! from the library.
//!
//! ```
//! use guimaker::components::*;
//! use guimaker::prelude::*;
//! let save: Element<()> = button("Save", ButtonVariant::Primary, ());
//! assert_eq!(save.kind, NodeKind::Row);
//! ```

use crate::components::{
    icon_box, icon_surface, label_text, ButtonVariant, ControlSize, StateSpec,
};
use crate::icons::Icon;
use crate::style::{Align, Cursor, Justify, Tone};
use crate::theme::{ColorRole, Duration, Radius, Spacing};
use crate::view::Element;

/// A labelled action in one variant at the default size.
///
/// ```
/// use guimaker::components::*;
/// use guimaker::prelude::*;
/// let cancel: Element<()> = button("Cancel", ButtonVariant::Secondary, ());
/// assert!(cancel.action.is_some());
/// ```
#[must_use]
pub fn button<Msg: Clone>(label: &str, variant: ButtonVariant, message: Msg) -> Element<Msg> {
    button_with(label, variant, None, ControlSize::Md, false, message)
}

/// A labelled action with an optional leading icon, a size and a loading state.
///
/// A loading button is disabled and shows the refresh mark with a "Loading"
/// label, so it cannot be activated twice.
///
/// ```
/// use guimaker::components::*;
/// use guimaker::icons::Icon;
/// use guimaker::prelude::*;
/// let save: Element<()> = button_with("Save", ButtonVariant::Primary, Some(Icon::Check), ControlSize::Sm, false, ());
/// assert_eq!(save.children.len(), 2);
/// ```
#[must_use]
pub fn button_with<Msg: Clone>(
    label: &str,
    variant: ButtonVariant,
    icon: Option<Icon>,
    size: ControlSize,
    loading: bool,
    message: Msg,
) -> Element<Msg> {
    let spec = variant.spec();
    let mut row = spec.apply(
        Element::row()
            .gap(Spacing::S2)
            .align(Align::Center)
            .justify(Justify::Center)
            .pad_x(size.pad_x())
            .pad_y(Spacing::S1)
            .min_height(size.height())
            .radius(Radius::Sm)
            .cursor(Cursor::Pointer)
            .transition(Duration::Fast)
            .on_click(message),
    );
    if let Some(icon) = icon {
        row = row.child(icon_box(icon, size, spec));
    }
    if loading {
        row = row.disabled().child(icon_box(Icon::Refresh, size, spec));
        row.child(label_text("Loading", size.label_role(), spec))
    } else {
        row.child(label_text(label, size.label_role(), spec))
    }
}

/// A square icon-only action with a mandatory tooltip label.
///
/// The label is the accessible name and the tooltip, so an icon-only control
/// can never ship without one.
///
/// ```
/// use guimaker::components::*;
/// use guimaker::icons::Icon;
/// use guimaker::prelude::*;
/// let close: Element<()> = icon_button(Icon::Close, "Close", ButtonVariant::Ghost, ControlSize::Md, ());
/// assert_eq!(close.style.tooltip.as_ref().map(|t| &*t.text), Some("Close"));
/// ```
#[must_use]
pub fn icon_button<Msg: Clone>(
    icon: Icon,
    label: &'static str,
    variant: ButtonVariant,
    size: ControlSize,
    message: Msg,
) -> Element<Msg> {
    let spec = variant.spec();
    spec.apply(icon_surface::<Msg>(size))
        .radius(Radius::Sm)
        .cursor(Cursor::Pointer)
        .tooltip(label)
        .transition(Duration::Fast)
        .on_click(message)
        .child(icon_box(icon, size, spec))
}

/// A toggle action that shows its selected state and reports a click.
///
/// ```
/// use guimaker::components::*;
/// use guimaker::prelude::*;
/// let bold: Element<()> = toggle_button("Bold", true, ());
/// assert!(bold.style.selected);
/// ```
#[must_use]
pub fn toggle_button<Msg: Clone>(label: &str, selected: bool, message: Msg) -> Element<Msg> {
    let spec = StateSpec {
        bg: Some(ColorRole::Surface),
        hover_bg: Some(ColorRole::SurfaceRaised),
        pressed_bg: Some(ColorRole::SurfaceSunken),
        selected_bg: Some(ColorRole::AccentFill),
        disabled_bg: Some(ColorRole::Surface),
        border: Some(ColorRole::Border),
        hover_border: Some(ColorRole::BorderStrong),
        tone: Tone::Muted,
        hover_tone: Some(Tone::Default),
        pressed_tone: Some(Tone::Default),
        selected_tone: Some(Tone::OnAccentFill),
        disabled_tone: Some(Tone::Faint),
        elevation_hover: None,
    };
    spec.apply(
        Element::row()
            .gap(Spacing::S2)
            .align(Align::Center)
            .justify(Justify::Center)
            .pad_x(Spacing::S3)
            .pad_y(Spacing::S1)
            .min_height(ControlSize::Md.height())
            .radius(Radius::Sm)
            .cursor(Cursor::Pointer)
            .transition(Duration::Fast)
            .selected(selected)
            .on_click(message),
    )
    .child(label_text(label, ControlSize::Md.label_role(), spec))
}

/// A segmented control: a row of toggle actions with exactly one selected.
///
/// The index is the item's position in `items`; the caller keeps it in its
/// state and updates it from the message.
///
/// ```
/// use guimaker::components::*;
/// use guimaker::prelude::*;
/// let modes: Element<u8> = button_group([("List", 0u8), ("Grid", 1)], 0);
/// assert_eq!(modes.children.len(), 2);
/// ```
#[must_use]
pub fn button_group<Msg: Clone>(
    items: impl IntoIterator<Item = (&'static str, Msg)>,
    selected: usize,
) -> Element<Msg> {
    let mut row = Element::row()
        .gap(Spacing::S1)
        .pad(Spacing::S1)
        .align(Align::Center)
        .bg(ColorRole::SurfaceSunken)
        .radius(Radius::Md);
    for (index, (label, message)) in items.into_iter().enumerate() {
        row = row.child(toggle_button(label, index == selected, message).radius(Radius::Sm));
    }
    row
}

/// The primary action of a split button pair, with a menu affordance.
///
/// ```
/// use guimaker::components::*;
/// use guimaker::prelude::*;
/// let split: Element<&'static str> = split_button("Render", "render", "menu");
/// assert_eq!(split.children.len(), 2);
/// ```
#[must_use]
pub fn split_button<Msg: Clone>(label: &str, message: Msg, menu: Msg) -> Element<Msg> {
    Element::row()
        .gap(Spacing::S1)
        .align(Align::Center)
        .child(button(label, ButtonVariant::Primary, message))
        .child(icon_button(
            Icon::ChevronDown,
            "More actions",
            ButtonVariant::Primary,
            ControlSize::Md,
            menu,
        ))
}

#[cfg(test)]
mod tests {
    use super::{button, button_group, button_with, icon_button, split_button, toggle_button};
    use crate::components::{ButtonVariant, ControlSize};
    use crate::icons::Icon;
    use crate::prelude::*;

    #[test]
    fn every_action_is_interactive_and_keyboard_reachable() {
        let actions: Vec<Element<()>> = vec![
            button("Save", ButtonVariant::Primary, ()),
            button_with(
                "Export",
                ButtonVariant::Secondary,
                Some(Icon::Download),
                ControlSize::Sm,
                false,
                (),
            ),
            icon_button(
                Icon::Close,
                "Close",
                ButtonVariant::Ghost,
                ControlSize::Md,
                (),
            ),
            toggle_button("Bold", false, ()),
        ];
        for action in actions {
            assert!(action.action.is_some());
        }
        let split = split_button("Render", (), ());
        assert_eq!(split.children.len(), 2);
        assert!(split.children.iter().all(|child| child.action.is_some()));
    }

    #[test]
    fn a_loading_button_is_disabled() {
        let loading = button_with(
            "Save",
            ButtonVariant::Primary,
            None,
            ControlSize::Md,
            true,
            (),
        );
        assert!(loading.style.disabled);
        assert_eq!(loading.children.len(), 2);
    }

    #[test]
    fn a_group_marks_exactly_one_selected() {
        let group = button_group([("List", 0u8), ("Grid", 1u8)], 1);
        let selected = group
            .children
            .iter()
            .filter(|child| child.style.selected)
            .count();
        assert_eq!(selected, 1);
    }
}
