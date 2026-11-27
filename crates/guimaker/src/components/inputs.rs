//! Input components: fields, choices, sliders and pickers.
//!
//! Every input names its state once, emits one message and lets the paint layer
//! resolve hover, press, focus, selection and disabled. The builders never read
//! the store, so a view is a pure function of its data.
//!
//! ```
//! use guimaker::prelude::*;
//! let field: Element<()> = text_field(Some("Name"), "Jane", "Jane", false, None, false, None, None, None, None, ());
//! assert!(!field.children.is_empty());
//! ```

use crate::components::{icon_box, icon_surface, label_text, ControlSize, StateSpec};
use crate::icons::Icon;
use crate::input::text_editor::{
    Clipboard, EditorKey, EditorOutcome, Modifiers, NullClipboard, TextEditor,
};
use crate::style::{Align, Cursor, Direction, Justify, SizeSpec, Tone};
use crate::theme::{ColorRole, Duration, Radius, Spacing, TypeRole};
use crate::view::{text, Element};

fn field_spec(error: bool) -> StateSpec {
    let border = if error {
        ColorRole::Danger
    } else {
        ColorRole::Border
    };
    let hover_border = if error {
        ColorRole::Danger
    } else {
        ColorRole::BorderStrong
    };
    StateSpec {
        bg: Some(ColorRole::SurfaceSunken),
        hover_bg: Some(ColorRole::Surface),
        pressed_bg: Some(ColorRole::SurfaceSunken),
        selected_bg: None,
        disabled_bg: Some(ColorRole::SurfaceSunken),
        border: Some(border),
        hover_border: Some(hover_border),
        tone: Tone::Default,
        hover_tone: None,
        pressed_tone: None,
        selected_tone: None,
        disabled_tone: Some(Tone::Faint),
        elevation_hover: None,
    }
}

fn choice_spec() -> StateSpec {
    StateSpec {
        bg: None,
        hover_bg: Some(ColorRole::Surface),
        pressed_bg: Some(ColorRole::SurfaceSunken),
        selected_bg: Some(ColorRole::AccentSubtle),
        disabled_bg: None,
        border: None,
        hover_border: None,
        tone: Tone::Default,
        hover_tone: Some(Tone::Default),
        pressed_tone: Some(Tone::Default),
        selected_tone: Some(Tone::Default),
        disabled_tone: Some(Tone::Faint),
        elevation_hover: None,
    }
}

fn action_spec() -> StateSpec {
    StateSpec {
        bg: None,
        hover_bg: Some(ColorRole::SurfaceRaised),
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
    }
}

fn checked_box_spec() -> StateSpec {
    StateSpec {
        bg: Some(ColorRole::AccentFill),
        hover_bg: Some(ColorRole::AccentFillHover),
        pressed_bg: Some(ColorRole::AccentFillPressed),
        selected_bg: Some(ColorRole::AccentFill),
        disabled_bg: Some(ColorRole::SurfaceSunken),
        border: Some(ColorRole::AccentFill),
        hover_border: Some(ColorRole::AccentFillHover),
        tone: Tone::OnAccentFill,
        hover_tone: None,
        pressed_tone: None,
        selected_tone: Some(Tone::OnAccentFill),
        disabled_tone: Some(Tone::Faint),
        elevation_hover: None,
    }
}

fn unchecked_box_spec() -> StateSpec {
    StateSpec {
        bg: Some(ColorRole::Surface),
        hover_bg: Some(ColorRole::SurfaceRaised),
        pressed_bg: Some(ColorRole::SurfaceSunken),
        selected_bg: None,
        disabled_bg: Some(ColorRole::SurfaceSunken),
        border: Some(ColorRole::Border),
        hover_border: Some(ColorRole::BorderStrong),
        tone: Tone::Muted,
        hover_tone: None,
        pressed_tone: None,
        selected_tone: None,
        disabled_tone: Some(Tone::Faint),
        elevation_hover: None,
    }
}

fn switch_track_spec() -> StateSpec {
    StateSpec {
        bg: Some(ColorRole::SurfaceSunken),
        hover_bg: Some(ColorRole::Surface),
        pressed_bg: Some(ColorRole::SurfaceSunken),
        selected_bg: Some(ColorRole::AccentFill),
        disabled_bg: Some(ColorRole::SurfaceSunken),
        border: Some(ColorRole::Border),
        hover_border: Some(ColorRole::BorderStrong),
        tone: Tone::Default,
        hover_tone: None,
        pressed_tone: None,
        selected_tone: Some(Tone::OnAccentFill),
        disabled_tone: Some(Tone::Faint),
        elevation_hover: None,
    }
}

fn slider_track_spec() -> StateSpec {
    StateSpec {
        bg: Some(ColorRole::SurfaceSunken),
        hover_bg: Some(ColorRole::Surface),
        pressed_bg: Some(ColorRole::SurfaceSunken),
        selected_bg: Some(ColorRole::SurfaceSunken),
        disabled_bg: Some(ColorRole::SurfaceSunken),
        border: Some(ColorRole::Border),
        hover_border: Some(ColorRole::BorderStrong),
        tone: Tone::Default,
        hover_tone: None,
        pressed_tone: None,
        selected_tone: None,
        disabled_tone: Some(Tone::Faint),
        elevation_hover: None,
    }
}

fn field_surface<Msg: Clone>(
    spec: StateSpec,
    size: ControlSize,
    disabled: bool,
    selected: bool,
    message: Msg,
) -> Element<Msg> {
    let mut surface = spec.apply(
        Element::row()
            .gap(Spacing::S2)
            .align(Align::Center)
            .width(SizeSpec::Fill)
            .pad_x(size.pad_x())
            .pad_y(Spacing::S1)
            .min_height(size.height())
            .radius(Radius::Sm)
            .cursor(Cursor::Pointer)
            .transition(Duration::Fast)
            .on_click(message),
    );
    if selected {
        // A host-rendered focus state is an accent outline; the library's own
        // keyboard focus ring is drawn on top when the surface is focused.
        surface = surface.border(ColorRole::Accent);
    }
    if disabled {
        surface.disabled()
    } else {
        surface
    }
}

fn icon_action<Msg: Clone>(
    icon: Icon,
    label: &'static str,
    size: ControlSize,
    spec: StateSpec,
    message: Msg,
) -> Element<Msg> {
    spec.apply(icon_surface::<Msg>(size))
        .radius(Radius::Xs)
        .cursor(Cursor::Pointer)
        .tooltip(label)
        .transition(Duration::Fast)
        .on_click(message)
        .child(icon_box(icon, size, spec))
}

fn safe_selection(value: &str, selection: Option<(usize, usize)>) -> Option<(usize, usize)> {
    let (start, end) = selection?;
    let start = start.min(value.len());
    let end = end.min(value.len());
    if start >= end || !value.is_char_boundary(start) || !value.is_char_boundary(end) {
        return None;
    }
    Some((start, end))
}

fn field_value<Msg: Clone>(
    value: &str,
    placeholder: &str,
    selection: Option<(usize, usize)>,
    focused: bool,
    size: ControlSize,
    spec: StateSpec,
) -> Element<Msg> {
    let caret = focused.then_some(value.len());
    if value.is_empty() {
        let mut text = text::<Msg>(placeholder)
            .text_role(size.label_role())
            .tone(Tone::Faint)
            .grow();
        if let Some(caret) = caret {
            text = text.caret(caret);
        }
        return text;
    }
    let safe = safe_selection(value, selection);
    let mut field = spec
        .apply_tone(text::<Msg>(value).text_role(size.label_role()))
        .grow();
    if let Some((start, end)) = safe {
        field = field.selection(start, end);
    }
    if let Some(caret) = caret {
        field = field.caret(caret);
    }
    field
}

fn labelled_field<Msg: Clone>(
    label: Option<&str>,
    surface: Element<Msg>,
    error: Option<&str>,
    helper: Option<&str>,
) -> Element<Msg> {
    let mut column = Element::column().gap(Spacing::S1).width(SizeSpec::Fill);
    if let Some(label) = label {
        column = column.child(label_text::<Msg>(
            label,
            TypeRole::Caption,
            StateSpec::tone(Tone::Muted),
        ));
    }
    column = column.child(surface);
    if let Some(error) = error {
        return column.child(
            text::<Msg>(error)
                .text_role(TypeRole::Caption)
                .tone(Tone::Danger),
        );
    }
    if let Some(helper) = helper {
        column = column.child(
            text::<Msg>(helper)
                .text_role(TypeRole::Caption)
                .tone(Tone::Muted),
        );
    }
    column
}

fn slider_value(value: f32, unit: Option<&str>) -> String {
    match unit {
        Some(unit) => format!("{value:.2} {unit}"),
        None => format!("{value:.2}"),
    }
}

/// A single-line text field with a label, helper, icons, selection and clear.
///
/// The surface is a `SurfaceSunken` control with a `Border` outline; an error
/// recolours the border and adds a `Danger` helper. The library draws the focus
/// ring because the surface is interactive, and `focused` also marks it
/// selected for hosts that render their own focus affordance.
///
/// # Examples
///
/// ```
/// use guimaker::prelude::*;
/// let field: Element<()> = text_field(Some("Name"), "Jane", "Jane", false, None, false, None, None, None, None, ());
/// assert!(!field.children.is_empty());
/// ```
#[allow(clippy::too_many_arguments)] // reason: a text field is one flat set of independent visual states
#[must_use]
pub fn text_field<Msg: Clone>(
    label: Option<&str>,
    placeholder: &str,
    value: &str,
    focused: bool,
    error: Option<&str>,
    disabled: bool,
    helper: Option<&str>,
    prefix: Option<Icon>,
    suffix: Option<Icon>,
    selection: Option<(usize, usize)>,
    message: Msg,
) -> Element<Msg> {
    let size = ControlSize::Md;
    let spec = field_spec(error.is_some());
    let mut surface = field_surface(spec, size, disabled, focused, message.clone());
    if let Some(icon) = prefix {
        surface = surface.child(icon_box(icon, size, spec));
    }
    surface = surface.child(field_value(
        value,
        placeholder,
        selection,
        focused,
        size,
        spec,
    ));
    if let Some(icon) = suffix {
        surface = surface.child(icon_box(icon, size, spec));
    }
    if !value.is_empty() && !disabled {
        surface = surface.child(icon_action(
            Icon::Close,
            "Clear",
            size,
            action_spec(),
            message,
        ));
    }
    labelled_field(label, surface, error, helper)
}

/// A numeric field with a unit, step buttons and a drag-to-scrub value area.
///
/// The value area emits `drag_message` for a scrub gesture, while the minus and
/// plus buttons both emit `step_message`; the host decides the direction from
/// the pointer position or the sign it keeps in its own state.
///
/// # Examples
///
/// ```
/// use guimaker::prelude::*;
/// let field: Element<()> = number_field("1.0", Some("s"), (), (), false, None, false, ());
/// assert!(!field.children.is_empty());
/// ```
#[allow(clippy::too_many_arguments)] // reason: the step and scrub actions are independently meaningful states
#[must_use]
pub fn number_field<Msg: Clone>(
    value: &str,
    unit: Option<&str>,
    step_message: Msg,
    drag_message: Msg,
    focused: bool,
    error: Option<&str>,
    disabled: bool,
    message: Msg,
) -> Element<Msg> {
    let size = ControlSize::Md;
    let spec = field_spec(error.is_some());
    let mut surface = field_surface(spec, size, disabled, focused, message);
    surface = surface.child(icon_action(
        Icon::Minus,
        "Decrease",
        size,
        action_spec(),
        step_message.clone(),
    ));
    let mut value_area = Element::row()
        .align(Align::Center)
        .justify(Justify::Center)
        .width(SizeSpec::Fill)
        .min_height(size.height())
        .pad_x(Spacing::S1)
        .cursor(Cursor::Pointer)
        .transition(Duration::Fast)
        .on_click(drag_message)
        .child(spec.apply_tone(text::<Msg>(value).text_role(size.label_role())));
    if let Some(unit) = unit {
        value_area = value_area.child(
            text::<Msg>(unit)
                .text_role(TypeRole::Caption)
                .tone(if disabled { Tone::Faint } else { Tone::Muted }),
        );
    }
    if disabled {
        value_area = value_area.disabled();
    }
    surface = surface.child(value_area);
    surface = surface.child(icon_action(
        Icon::Plus,
        "Increase",
        size,
        action_spec(),
        step_message,
    ));
    labelled_field(None, surface, error, None)
}

/// A multiline text area whose visible line count is set by `rows`.
///
/// # Examples
///
/// ```
/// use guimaker::prelude::*;
/// let field: Element<()> = textarea("", "Notes", 4, false, None, false, ());
/// assert!(!field.children.is_empty());
/// ```
#[allow(clippy::too_many_arguments)] // reason: the field's visual states are independent flags
#[must_use]
pub fn textarea<Msg: Clone>(
    value: &str,
    placeholder: &str,
    rows: u16,
    focused: bool,
    error: Option<&str>,
    disabled: bool,
    message: Msg,
) -> Element<Msg> {
    let size = ControlSize::Md;
    let spec = field_spec(error.is_some());
    let lines = rows.max(1);
    let mut surface = field_surface(spec, size, disabled, focused, message)
        .align(Align::Start)
        .justify(Justify::Start)
        .pad_y(Spacing::S2)
        .min_height(Spacing::S12);
    surface = if value.is_empty() {
        surface.child(
            text::<Msg>(placeholder)
                .text_role(size.label_role())
                .tone(Tone::Faint)
                .lines(lines),
        )
    } else {
        surface.child(
            spec.apply_tone(text::<Msg>(value).text_role(size.label_role()))
                .lines(lines),
        )
    };
    labelled_field(None, surface, error, None)
}

/// A labelled checkbox that reports its next state through one message.
///
/// The box uses the accent fill when checked and the disabled tone when the
/// whole row is disabled.
///
/// # Examples
///
/// ```
/// use guimaker::prelude::*;
/// let control: Element<()> = checkbox("Loop", true, false, ());
/// assert_eq!(control.children[0].style.bg, Some(ColorRole::AccentFill));
/// ```
#[must_use]
pub fn checkbox<Msg: Clone>(
    label: &str,
    checked: bool,
    disabled: bool,
    message: Msg,
) -> Element<Msg> {
    let size = ControlSize::Md;
    let spec = choice_spec();
    let box_spec = if checked {
        checked_box_spec()
    } else {
        unchecked_box_spec()
    };
    let mut box_element = box_spec.apply(
        Element::row()
            .align(Align::Center)
            .justify(Justify::Center)
            .width(SizeSpec::Token(size.icon()))
            .height(SizeSpec::Token(size.icon()))
            .radius(Radius::Xs)
            .transition(Duration::Fast),
    );
    if checked {
        box_element = box_element.child(icon_box(Icon::CheckMark, size, box_spec));
    }
    let row = spec
        .apply(
            Element::row()
                .gap(Spacing::S2)
                .align(Align::Center)
                .width(SizeSpec::Hug)
                .pad_y(Spacing::S1)
                .min_height(size.height())
                .radius(Radius::Xs)
                .cursor(Cursor::Pointer)
                .transition(Duration::Fast)
                .on_click(message),
        )
        .child(box_element)
        .child(label_text(label, size.label_role(), spec));
    if disabled {
        row.disabled()
    } else {
        row
    }
}

/// A vertical radio group where exactly one item is selected by index.
///
/// # Examples
///
/// ```
/// use guimaker::prelude::*;
/// let group: Element<u8> = radio_group([("One", 0u8), ("Two", 1u8)], 0, false);
/// assert_eq!(group.children.len(), 2);
/// ```
#[must_use]
pub fn radio_group<Msg: Clone>(
    items: impl IntoIterator<Item = (&'static str, Msg)>,
    selected: usize,
    disabled: bool,
) -> Element<Msg> {
    let size = ControlSize::Md;
    let spec = choice_spec();
    let mut column = Element::column().gap(Spacing::S1).width(SizeSpec::Fill);
    for (index, (label, message)) in items.into_iter().enumerate() {
        let is_selected = index == selected;
        let mut radio = spec
            .apply(
                Element::row()
                    .gap(Spacing::S2)
                    .align(Align::Center)
                    .pad_y(Spacing::S1)
                    .min_height(size.height())
                    .radius(Radius::Xs)
                    .cursor(Cursor::Pointer)
                    .transition(Duration::Fast)
                    .selected(is_selected)
                    .on_click(message),
            )
            .child(icon_box(
                if is_selected { Icon::Dot } else { Icon::Circle },
                size,
                if is_selected {
                    StateSpec::tone(Tone::Accent)
                } else {
                    StateSpec::tone(Tone::Muted)
                },
            ))
            .child(label_text(label, size.label_role(), spec));
        if disabled {
            radio = radio.disabled();
        }
        column = column.child(radio);
    }
    column
}

/// A labelled switch whose thumb slides between the off and on ends.
///
/// # Examples
///
/// ```
/// use guimaker::prelude::*;
/// let control: Element<()> = switch("Enabled", true, false, ());
/// assert!(control.style.selected);
/// ```
#[must_use]
pub fn switch<Msg: Clone>(label: &str, on: bool, disabled: bool, message: Msg) -> Element<Msg> {
    let size = ControlSize::Md;
    let spec = choice_spec();
    let track_spec = switch_track_spec();
    let thumb = Element::row()
        .width(SizeSpec::Token(size.icon()))
        .height(SizeSpec::Token(size.icon()))
        .radius(Radius::Full)
        .bg(ColorRole::SurfaceRaised)
        .border(ColorRole::Border);
    let track = track_spec
        .apply(
            Element::row()
                .align(Align::Center)
                .justify(if on { Justify::End } else { Justify::Start })
                .pad(Spacing::S1)
                .width(SizeSpec::Token(Spacing::S10))
                .height(SizeSpec::Token(Spacing::S6))
                .radius(Radius::Full)
                .transition(Duration::Fast)
                .selected(on),
        )
        .child(thumb);
    let row_spec = StateSpec {
        selected_bg: None,
        ..spec
    };
    let mut row = row_spec
        .apply(
            Element::row()
                .gap(Spacing::S2)
                .align(Align::Center)
                .width(SizeSpec::Hug)
                .pad_y(Spacing::S1)
                .min_height(size.height())
                .radius(Radius::Xs)
                .cursor(Cursor::Pointer)
                .transition(Duration::Fast)
                .selected(on)
                .on_click(message),
        )
        .child(track)
        .child(label_text(label, size.label_role(), row_spec));
    if disabled {
        row = row.disabled();
    }
    row
}

/// A horizontal slider with a value in `0.0..=1.0`, a label and a unit.
///
/// # Examples
///
/// ```
/// use guimaker::prelude::*;
/// let control: Element<()> = slider(0.4, "Opacity", None, false, ());
/// assert!(!control.children.is_empty());
/// ```
#[must_use]
pub fn slider<Msg: Clone>(
    value: f32,
    label: &str,
    unit: Option<&str>,
    disabled: bool,
    message: Msg,
) -> Element<Msg> {
    let value = value.clamp(0.0, 1.0);
    let size = ControlSize::Md;
    let track_spec = slider_track_spec();
    let fill_role = if disabled {
        ColorRole::Border
    } else {
        ColorRole::AccentFill
    };
    let thumb = Element::row()
        .width(SizeSpec::Token(size.icon()))
        .height(SizeSpec::Token(size.icon()))
        .radius(Radius::Full)
        .bg(ColorRole::SurfaceRaised)
        .border(ColorRole::Border);
    let fill = Element::row()
        .width(SizeSpec::Fraction(value))
        .height(SizeSpec::Fill)
        .align(Align::Center)
        .justify(Justify::End)
        .bg(fill_role)
        .radius(Radius::Full)
        .clip()
        .child(thumb);
    let mut track = track_spec
        .apply(
            Element::row()
                .align(Align::Center)
                .width(SizeSpec::Fill)
                .height(SizeSpec::Token(size.icon()))
                .min_height(Spacing::S6)
                .radius(Radius::Full)
                .clip()
                .cursor(Cursor::Pointer)
                .transition(Duration::Fast)
                .on_click(message),
        )
        .child(fill);
    if disabled {
        track = track.disabled();
    }
    let label_spec = StateSpec::tone(if disabled { Tone::Faint } else { Tone::Muted });
    let header = Element::row()
        .align(Align::Center)
        .justify(Justify::SpaceBetween)
        .width(SizeSpec::Fill)
        .child(
            label_text(label, size.label_role(), label_spec)
                .lines(1)
                .ellipsis(crate::text::Ellipsis::End),
        )
        .child(
            text::<Msg>(slider_value(value, unit))
                .text_role(TypeRole::Caption)
                .tone(if disabled { Tone::Faint } else { Tone::Muted }),
        );
    Element::column()
        .gap(Spacing::S1)
        .width(SizeSpec::Fill)
        .child(header)
        .child(track)
}

/// A horizontal range slider with independent start and end values.
///
/// # Examples
///
/// ```
/// use guimaker::prelude::*;
/// let control: Element<()> = range_slider(0.2, 0.8, "Range", false, ());
/// assert!(!control.children.is_empty());
/// ```
#[must_use]
pub fn range_slider<Msg: Clone>(
    start: f32,
    end: f32,
    label: &str,
    disabled: bool,
    message: Msg,
) -> Element<Msg> {
    let start = start.clamp(0.0, 1.0);
    let end = end.clamp(start, 1.0);
    let size = ControlSize::Md;
    let track_spec = slider_track_spec();
    let fill_role = if disabled {
        ColorRole::Border
    } else {
        ColorRole::AccentFill
    };
    let thumb = Element::row()
        .width(SizeSpec::Token(Spacing::S3))
        .height(SizeSpec::Token(Spacing::S3))
        .radius(Radius::Full)
        .bg(ColorRole::SurfaceRaised)
        .border(ColorRole::Border);
    let band = Element::row()
        .width(SizeSpec::Fraction(end - start))
        .height(SizeSpec::Fill)
        .align(Align::Center)
        .justify(Justify::End)
        .bg(fill_role)
        .radius(Radius::Full)
        .clip()
        .child(thumb);
    let mut track = track_spec
        .apply(
            Element::row()
                .align(Align::Center)
                .width(SizeSpec::Fill)
                .height(SizeSpec::Token(size.height()))
                .min_height(Spacing::S6)
                .radius(Radius::Full)
                .clip()
                .cursor(Cursor::Pointer)
                .transition(Duration::Fast)
                .selected(start > 0.0)
                .on_click(message),
        )
        .child(
            Element::row()
                .width(SizeSpec::Fraction(start))
                .height(SizeSpec::Fill),
        )
        .child(band);
    if disabled {
        track = track.disabled();
    }
    let label_spec = StateSpec::tone(if disabled { Tone::Faint } else { Tone::Muted });
    let header = Element::row()
        .align(Align::Center)
        .justify(Justify::SpaceBetween)
        .width(SizeSpec::Fill)
        .child(label_text(label, size.label_role(), label_spec))
        .child(
            text::<Msg>(format!("{start:.2} - {end:.2}"))
                .text_role(TypeRole::Caption)
                .tone(if disabled { Tone::Faint } else { Tone::Muted }),
        );
    Element::column()
        .gap(Spacing::S1)
        .width(SizeSpec::Fill)
        .child(header)
        .child(track)
}

/// A select control that shows its current value, or a placeholder when empty.
///
/// The control marks itself selected while `open`, so the chevron and the
/// selected tone communicate the expanded state.
///
/// # Examples
///
/// ```
/// use guimaker::prelude::*;
/// let control: Element<()> = select("Mode", "", "Choose a mode", false, false, ());
/// assert!(!control.children.is_empty());
/// ```
#[must_use]
pub fn select<Msg: Clone>(
    label: &str,
    value: &str,
    placeholder: &str,
    open: bool,
    disabled: bool,
    message: Msg,
) -> Element<Msg> {
    let size = ControlSize::Md;
    let mut spec = field_spec(false);
    spec.selected_bg = Some(ColorRole::AccentSubtle);
    spec.selected_tone = Some(Tone::Accent);
    let mut surface = field_surface(spec, size, disabled, open, message);
    surface = surface.child(if value.is_empty() {
        text::<Msg>(placeholder)
            .text_role(size.label_role())
            .tone(Tone::Faint)
    } else {
        spec.apply_tone(text::<Msg>(value).text_role(size.label_role()))
    });
    surface = surface.child(icon_box(
        if open {
            Icon::ChevronUp
        } else {
            Icon::ChevronDown
        },
        size,
        spec,
    ));
    labelled_field(Some(label), surface, None, None)
}

/// A combobox with a search row and an optional list of results.
///
/// The search row emits `message`; each result row emits its own message. The
/// list is only built while `open` is true.
///
/// # Examples
///
/// ```
/// use guimaker::prelude::*;
/// let control: Element<()> = combobox("Mode", "", [("One", ()), ("Two", ())], true, false, ());
/// assert!(!control.children.is_empty());
/// ```
#[must_use]
pub fn combobox<Msg: Clone>(
    label: &str,
    query: &str,
    items: impl IntoIterator<Item = (&'static str, Msg)>,
    open: bool,
    disabled: bool,
    message: Msg,
) -> Element<Msg> {
    let size = ControlSize::Md;
    let mut spec = field_spec(false);
    spec.selected_bg = Some(ColorRole::AccentSubtle);
    spec.selected_tone = Some(Tone::Accent);
    let mut search = field_surface(spec, size, disabled, open, message);
    search = search.child(icon_box(Icon::Search, size, spec));
    search = search.child(if query.is_empty() {
        text::<Msg>("Search")
            .text_role(size.label_role())
            .tone(Tone::Faint)
    } else {
        spec.apply_tone(text::<Msg>(query).text_role(size.label_role()))
    });
    search = search.child(icon_box(
        if open {
            Icon::ChevronUp
        } else {
            Icon::ChevronDown
        },
        size,
        spec,
    ));
    let mut column = Element::column()
        .gap(Spacing::S1)
        .width(SizeSpec::Fill)
        .child(search);
    if open {
        let option_spec = choice_spec();
        let mut list = Element::column().gap(Spacing::S1).width(SizeSpec::Fill);
        for (item, item_message) in items {
            let mut option = option_spec
                .apply(
                    Element::row()
                        .align(Align::Center)
                        .gap(Spacing::S2)
                        .pad_x(size.pad_x())
                        .pad_y(Spacing::S1)
                        .min_height(size.height())
                        .radius(Radius::Xs)
                        .cursor(Cursor::Pointer)
                        .transition(Duration::Fast)
                        .on_click(item_message),
                )
                .child(label_text(item, size.label_role(), option_spec));
            if disabled {
                option = option.disabled();
            }
            list = list.child(option);
        }
        column = column.child(list);
    }
    labelled_field(Some(label), column, None, None)
}

/// A colour swatch row that shows a theme role and a check mark when selected.
///
/// # Examples
///
/// ```
/// use guimaker::prelude::*;
/// let control: Element<()> = color_swatch("Accent", ColorRole::Accent, true, false, ());
/// assert!(control.style.selected);
/// ```
#[must_use]
pub fn color_swatch<Msg: Clone>(
    label: &str,
    color: ColorRole,
    selected: bool,
    disabled: bool,
    message: Msg,
) -> Element<Msg> {
    let size = ControlSize::Md;
    let spec = choice_spec();
    let mut row = spec.apply(
        Element::row()
            .align(Align::Center)
            .gap(Spacing::S2)
            .pad_x(Spacing::S2)
            .pad_y(Spacing::S1)
            .min_height(size.height())
            .radius(Radius::Xs)
            .cursor(Cursor::Pointer)
            .transition(Duration::Fast)
            .selected(selected)
            .on_click(message),
    );
    let swatch = Element::row()
        .width(SizeSpec::Token(size.icon()))
        .height(SizeSpec::Token(size.icon()))
        .radius(Radius::Xs)
        .bg(color)
        .border(ColorRole::Border);
    row = row
        .child(swatch)
        .child(label_text(label, size.label_role(), spec));
    if selected {
        row = row.child(icon_box(
            Icon::Check,
            size,
            StateSpec::tone(if disabled { Tone::Faint } else { Tone::Accent }),
        ));
    }
    if disabled {
        row.disabled()
    } else {
        row
    }
}

/// A path field with a folder prefix and a browse action.
///
/// The main surface emits `message` and the browse icon emits `browse`.
///
/// # Examples
///
/// ```
/// use guimaker::prelude::*;
/// let control: Element<()> = path_field("Path", "", "Choose a file", (), false, ());
/// assert!(!control.children.is_empty());
/// ```
#[must_use]
pub fn path_field<Msg: Clone>(
    label: &str,
    path: &str,
    placeholder: &str,
    browse: Msg,
    disabled: bool,
    message: Msg,
) -> Element<Msg> {
    let size = ControlSize::Md;
    let spec = field_spec(false);
    let mut surface = field_surface(spec, size, disabled, false, message);
    surface = surface.child(icon_box(Icon::Folder, size, spec));
    surface = surface.child(if path.is_empty() {
        text::<Msg>(placeholder)
            .text_role(size.label_role())
            .tone(Tone::Faint)
    } else {
        spec.apply_tone(text::<Msg>(path).text_role(size.label_role()))
    });
    let mut browse_button = icon_action(Icon::FolderOpen, "Browse", size, action_spec(), browse);
    if disabled {
        browse_button = browse_button.disabled();
    }
    surface = surface.child(browse_button);
    labelled_field(Some(label), surface, None, None)
}

/// A responsive form row with a label column, a control and an optional hint.
///
/// The row stacks below `Sm` and becomes a label/control row from `Sm` upwards.
///
/// # Examples
///
/// ```
/// use guimaker::prelude::*;
/// let row: Element<()> = form_row("Name", text::<()>("Jane"), Some("Your name"), true);
/// assert_eq!(row.kind, NodeKind::Row);
/// ```
#[must_use]
pub fn form_row<Msg: Clone>(
    label: &str,
    control: Element<Msg>,
    hint: Option<&str>,
    required: bool,
) -> Element<Msg> {
    let mut label_column = Element::column()
        .align(Align::Start)
        .gap(Spacing::S1)
        .width(crate::resp![
            Xs: SizeSpec::Fill,
            Sm: SizeSpec::Token(Spacing::S24)
        ]);
    let mut label_row = Element::row()
        .align(Align::Center)
        .gap(Spacing::S1)
        .child(label_text(
            label,
            TypeRole::Label,
            StateSpec::tone(Tone::Default),
        ));
    if required {
        label_row = label_row.child(
            text::<Msg>("*")
                .text_role(TypeRole::Label)
                .tone(Tone::Danger),
        );
    }
    label_column = label_column.child(label_row);
    if let Some(hint) = hint {
        label_column = label_column.child(
            text::<Msg>(hint)
                .text_role(TypeRole::Caption)
                .tone(Tone::Muted),
        );
    }
    let control_column = Element::column().width(SizeSpec::Fill).child(control);
    Element::row()
        .direction(crate::resp![Xs: Direction::Column, Sm: Direction::Row])
        .align(Align::Start)
        .justify(Justify::Start)
        .gap(crate::resp![Xs: Spacing::S2, Sm: Spacing::S4])
        .width(SizeSpec::Fill)
        .child(label_column)
        .child(control_column)
}

/// A mutable text-field model around [`TextEditor`].
///
/// The element stays pure: the model owns editing state, [`TextFieldModel::view`]
/// turns it into an `Element`, and the host forwards keys to the model.
///
/// # Examples
///
/// ```
/// use guimaker::prelude::*;
/// let model = TextFieldModel::new(guimaker::input::text_editor::TextEditor::new("hi"));
/// assert_eq!(model.editor.text(), "hi");
/// ```
#[must_use]
#[derive(Clone, Debug)]
pub struct TextFieldModel {
    /// The editing state: text, caret, selection and undo history.
    pub editor: TextEditor,
    /// The placeholder shown while the editor is empty.
    pub placeholder: String,
    /// The validation message shown under the field.
    pub error: Option<String>,
    /// Whether the field rejects input and draws with the disabled tone.
    pub disabled: bool,
}

impl TextFieldModel {
    /// Builds a model over an editor with no placeholder, error or disabled
    /// state.
    ///
    /// # Examples
    ///
    /// ```
    /// use guimaker::prelude::*;
    /// let model = TextFieldModel::new(guimaker::input::text_editor::TextEditor::new(""));
    /// assert_eq!(model.placeholder, "");
    /// ```
    pub fn new(editor: TextEditor) -> Self {
        Self {
            editor,
            placeholder: String::new(),
            error: None,
            disabled: false,
        }
    }

    /// Forwards one key to the editor and returns its outcome.
    ///
    /// # Examples
    ///
    /// ```
    /// use guimaker::prelude::*;
    /// let mut model = TextFieldModel::new(guimaker::input::text_editor::TextEditor::new("a"));
    /// let _ = model.handle_key(guimaker::input::text_editor::EditorKey::Home, guimaker::input::text_editor::Modifiers::none(), 0.0, &mut guimaker::input::text_editor::NullClipboard);
    /// ```
    #[must_use]
    pub fn handle_key(
        &mut self,
        key: EditorKey,
        modifiers: Modifiers,
        now_ms: f64,
        clipboard: &mut dyn Clipboard,
    ) -> EditorOutcome {
        self.editor.handle_key(key, modifiers, now_ms, clipboard)
    }

    /// Inserts each character through the editor, honouring its field kind.
    ///
    /// # Examples
    ///
    /// ```
    /// use guimaker::prelude::*;
    /// let mut model = TextFieldModel::new(guimaker::input::text_editor::TextEditor::new(""));
    /// model.insert_text("hi", 0.0);
    /// ```
    pub fn insert_text(&mut self, text: &str, now_ms: f64) {
        let mut clipboard = NullClipboard;
        for character in text.chars() {
            let _ = self.editor.handle_key(
                EditorKey::Char(character),
                Modifiers::none(),
                now_ms,
                &mut clipboard,
            );
        }
    }

    /// Copies the current selection to the clipboard.
    ///
    /// # Examples
    ///
    /// ```
    /// use guimaker::prelude::*;
    /// let mut model = TextFieldModel::new(guimaker::input::text_editor::TextEditor::new("hi"));
    /// let _ = model.copy(&mut guimaker::input::text_editor::NullClipboard);
    /// ```
    #[must_use]
    pub fn copy(&mut self, clipboard: &mut dyn Clipboard) -> EditorOutcome {
        self.editor.copy(clipboard)
    }

    /// Cuts the current selection to the clipboard.
    ///
    /// # Examples
    ///
    /// ```
    /// use guimaker::prelude::*;
    /// let mut model = TextFieldModel::new(guimaker::input::text_editor::TextEditor::new("hi"));
    /// let _ = model.cut(0.0, &mut guimaker::input::text_editor::NullClipboard);
    /// ```
    #[must_use]
    pub fn cut(&mut self, now_ms: f64, clipboard: &mut dyn Clipboard) -> EditorOutcome {
        self.editor.cut(now_ms, clipboard)
    }

    /// Pastes the clipboard into the editor, sanitised for its field kind.
    ///
    /// # Examples
    ///
    /// ```
    /// use guimaker::prelude::*;
    /// let mut model = TextFieldModel::new(guimaker::input::text_editor::TextEditor::new(""));
    /// let _ = model.paste(0.0, &mut guimaker::input::text_editor::NullClipboard);
    /// ```
    #[must_use]
    pub fn paste(&mut self, now_ms: f64, clipboard: &mut dyn Clipboard) -> EditorOutcome {
        self.editor.paste(now_ms, clipboard)
    }

    /// Renders the editor's text with a real caret and selection rects.
    ///
    /// The caret and selection come from the shaped glyph positions, so they
    /// land on the exact byte and never use a `|` character. The result is a
    /// field surface that emits `message(text)` when clicked.
    ///
    /// # Examples
    ///
    /// ```
    /// use guimaker::prelude::*;
    /// let mut model = TextFieldModel::new(guimaker::input::text_editor::TextEditor::new("hi"));
    /// let view: Element<()> = model.view(|_| ());
    /// ```
    #[must_use]
    pub fn view<Msg: Clone>(&self, message: impl Fn(String) -> Msg) -> Element<Msg> {
        self.view_with_focus(true, message)
    }

    /// Renders the field with an explicit focus state.
    ///
    /// A host that keeps the model visible before the user clicks it renders
    /// with `focused = false` (no caret, rest colours) and switches to `true`
    /// once its click message focused the field; the caret and selection then
    /// come from the model. [`Self::view`] is the always-focused shorthand.
    ///
    /// # Examples
    ///
    /// ```
    /// use guimaker::prelude::*;
    /// let mut model = TextFieldModel::new(guimaker::input::text_editor::TextEditor::new("hi"));
    /// let view: Element<()> = model.view_with_focus(false, |_| ());
    /// ```
    #[must_use]
    pub fn view_with_focus<Msg: Clone>(
        &self,
        focused: bool,
        message: impl Fn(String) -> Msg,
    ) -> Element<Msg> {
        let size = ControlSize::Md;
        let spec = field_spec(self.error.is_some());
        let emit = message(self.editor.text().to_string());
        let mut surface = field_surface(spec, size, self.disabled, focused, emit);
        // The caret and the selection are a focus affordance: an unfocused
        // field shows its text at rest (prompt 15 Phase 9D finding 4).
        let caret = focused.then_some(self.editor.caret());
        if self.editor.text().is_empty() {
            let mut label = text::<Msg>(self.placeholder.clone())
                .text_role(size.label_role())
                .tone(Tone::Faint);
            if let Some(caret) = caret {
                label = label.caret(caret);
            }
            surface = surface.child(label);
        } else {
            let mut value =
                spec.apply_tone(text::<Msg>(self.editor.text()).text_role(size.label_role()));
            if focused {
                if let Some((start, end)) = self.editor.selection() {
                    value = value.selection(start, end);
                }
            }
            if let Some(caret) = caret {
                value = value.caret(caret);
            }
            surface = surface.child(value);
        }
        labelled_field(None, surface, self.error.as_deref(), None)
    }
}

#[cfg(test)]
mod tests {
    use super::{
        checkbox, color_swatch, combobox, form_row, number_field, path_field, radio_group,
        range_slider, select, slider, switch, text_field, textarea, TextFieldModel,
    };
    use crate::input::text_editor::{Clipboard, EditorOutcome, TextEditor};
    use crate::prelude::*;

    fn count_actions(element: &Element<()>) -> usize {
        let mut count = usize::from(element.action.is_some());
        for child in &element.children {
            count += count_actions(child);
        }
        count
    }

    fn caret_offset(element: &Element<()>) -> Option<usize> {
        element
            .style
            .caret
            .or_else(|| element.children.iter().find_map(caret_offset))
    }

    fn count_disabled(element: &Element<()>) -> usize {
        let mut count = usize::from(element.style.disabled);
        for child in &element.children {
            count += count_disabled(child);
        }
        count
    }

    fn samples() -> Vec<Element<()>> {
        vec![
            text_field(
                Some("Name"),
                "Jane",
                "Jane",
                false,
                None,
                false,
                Some("Your name"),
                Some(Icon::User),
                None,
                None,
                (),
            ),
            number_field("1.0", Some("s"), (), (), false, None, false, ()),
            textarea("", "Notes", 4, false, None, false, ()),
            checkbox("Loop", true, false, ()),
            radio_group([("One", ()), ("Two", ())], 0, false),
            switch("Enabled", true, false, ()),
            slider(0.4, "Opacity", None, false, ()),
            range_slider(0.2, 0.8, "Range", false, ()),
            select("Mode", "", "Choose a mode", false, false, ()),
            combobox("Mode", "", [("One", ()), ("Two", ())], true, false, ()),
            color_swatch("Accent", ColorRole::Accent, true, false, ()),
            path_field("Path", "", "Choose a file", (), false, ()),
            form_row(
                "Name",
                text::<()>("Jane").on_click(()),
                Some("Your name"),
                true,
            ),
        ]
    }

    #[test]
    fn every_input_builds() {
        for control in samples() {
            assert!(!control.children.is_empty());
        }
    }

    #[test]
    fn every_interactive_input_carries_an_action() {
        for control in samples() {
            assert!(
                count_actions(&control) > 0,
                "{:?} has no action",
                control.kind
            );
        }
    }

    #[test]
    fn disabled_inputs_are_disabled() {
        let controls = vec![
            text_field(
                Some("Name"),
                "",
                "",
                false,
                None,
                true,
                None,
                None,
                None,
                None,
                (),
            ),
            number_field("1", None, (), (), false, None, true, ()),
            textarea("", "", 2, false, None, true, ()),
            checkbox("Loop", false, true, ()),
            radio_group([("One", ())], 0, true),
            switch("Enabled", false, true, ()),
            slider(0.0, "Opacity", None, true, ()),
            range_slider(0.0, 1.0, "Range", true, ()),
            select("Mode", "", "Choose", false, true, ()),
            combobox("Mode", "", [("One", ())], false, true, ()),
            color_swatch("Accent", ColorRole::Accent, false, true, ()),
            path_field("Path", "", "Choose", (), true, ()),
        ];
        for control in controls {
            assert!(
                count_disabled(&control) > 0,
                "{:?} is not disabled",
                control.kind
            );
        }
    }

    #[test]
    fn checkbox_and_switch_reflect_their_checked_state() {
        let checked = checkbox("Loop", true, false, ());
        let unchecked = checkbox("Loop", false, false, ());
        assert_eq!(checked.children[0].style.bg, Some(ColorRole::AccentFill));
        assert_eq!(unchecked.children[0].style.bg, Some(ColorRole::Surface));
        assert!(!checked.children[0].children.is_empty());
        let on = switch("Enabled", true, false, ());
        let off = switch("Enabled", false, false, ());
        assert!(on.style.selected);
        assert!(!off.style.selected);
    }

    #[test]
    fn a_non_empty_text_field_has_a_clear_action() {
        let with_value = text_field(
            None,
            "x",
            "x",
            false,
            None,
            false,
            None,
            None,
            None,
            None,
            (),
        );
        let without = text_field(
            None,
            "x",
            "",
            false,
            None,
            false,
            None,
            None,
            None,
            None,
            (),
        );
        assert!(count_actions(&with_value) > count_actions(&without));
    }

    #[test]
    fn form_row_stacks_below_sm_and_rows_from_sm() {
        let row: Element<()> = form_row("Name", text::<()>("Jane"), None, false);
        let breakpoints = Breakpoints::default();
        assert_eq!(
            *row.style.direction.resolve(360.0, &breakpoints),
            Direction::Column
        );
        assert_eq!(
            *row.style.direction.resolve(800.0, &breakpoints),
            Direction::Row
        );
    }

    struct TestClipboard(Option<String>);

    impl Clipboard for TestClipboard {
        fn read(&mut self) -> Option<String> {
            self.0.clone()
        }

        fn write(&mut self, text: &str) {
            self.0 = Some(text.to_string());
        }
    }

    #[test]
    fn text_field_model_delegates_typing_and_clipboard_to_text_editor() {
        let mut model = TextFieldModel::new(TextEditor::new("").with_single_line(true));
        model.insert_text("hello", 0.0);
        assert_eq!(model.editor.text(), "hello");
        model.editor.select_all();
        let mut clipboard = TestClipboard(None);
        assert_eq!(model.copy(&mut clipboard), EditorOutcome::Changed);
        assert_eq!(clipboard.0.as_deref(), Some("hello"));
        model.editor.set_caret(0);
        model.editor.select_all();
        assert_eq!(model.cut(0.0, &mut clipboard), EditorOutcome::Changed);
        assert_eq!(model.editor.text(), "");
        model.editor.set_caret(0);
        let mut pasted = TestClipboard(Some("typed".to_string()));
        assert_eq!(model.paste(0.0, &mut pasted), EditorOutcome::Changed);
        assert_eq!(model.editor.text(), "typed");
    }

    #[test]
    fn text_field_model_view_marks_the_caret() {
        let mut model = TextFieldModel::new(TextEditor::new("ab"));
        model.editor.set_caret(1);
        let view: Element<()> = model.view(|_| ());
        let rendered = view
            .children
            .iter()
            .flat_map(|child| &child.children)
            .filter_map(|child| child.text.as_ref())
            .map(TextSpec::text)
            .collect::<String>();
        assert_eq!(
            rendered, "ab",
            "the text must not be mutated with a | marker"
        );
        assert_eq!(caret_offset(&view), Some(1), "the caret is a byte offset");
    }

    /// The standard editable path used by apps and the bench: the model owns
    /// the text and the caret, an unfocused render has no caret, and committed
    /// text (`on_text` → `insert_text`) grows the document
    /// (prompt 15 Phase 9D finding 4).
    #[test]
    fn an_unfocused_field_model_has_no_caret_and_typing_grows_the_text() {
        let mut model = TextFieldModel::new(TextEditor::new("ab"));
        model.editor.set_caret(2);
        let unfocused: Element<()> = model.view_with_focus(false, |_| ());
        assert_eq!(
            caret_offset(&unfocused),
            None,
            "an unfocused field has no caret"
        );
        let focused: Element<()> = model.view_with_focus(true, |_| ());
        assert_eq!(caret_offset(&focused), Some(2));

        model.insert_text("cd", 0.0);
        assert_eq!(model.editor.text(), "abcd");
    }
}
