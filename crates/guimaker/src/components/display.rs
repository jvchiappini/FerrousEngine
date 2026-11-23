//! Display components: text, badges, tags, keycaps, media and feedback.
//!
//! Every builder is a pure function of its data and returns an [`Element`]; the
//! paint layer resolves hover, press and selection from the keyed store. Sizes
//! come from [`ControlSize`] and [`TypeRole`], colours from [`Status`] and
//! [`ColorRole`], so a display never hard-codes a design value.
//!
//! ```
//! use guimaker::prelude::*;
//! let badge: Element<()> = badge("New", Status::Success);
//! assert_eq!(badge.style.bg, Some(Status::Success.subtle()));
//! ```

use std::sync::Arc;

use glam::Vec2;

use crate::components::{icon_box, label_text, ControlSize, StateSpec, Status};
use crate::icons::Icon;
use crate::style::{Align, Cursor, Justify, SizeSpec, Tone};
use crate::text::Ellipsis;
use crate::theme::{BorderWidth, ColorRole, ContentWidth, Duration, Radius, Spacing, TypeRole};
use crate::view::Element;

/// The quiet action spec every icon-only control in this module shares.
const fn inline_action_spec() -> StateSpec {
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

/// The icon that carries a status in a callout.
const fn callout_icon(status: Status) -> Icon {
    match status {
        Status::Info => Icon::Bell,
        Status::Success => Icon::Check,
        Status::Warning => Icon::Warning,
        Status::Danger => Icon::Error,
    }
}

/// A text run at a type role and tone.
///
/// ```
/// use guimaker::prelude::*;
/// let body: Element<()> = guimaker::components::text("Hello", TypeRole::Body, Tone::Default);
/// assert_eq!(body.style.text_role, TypeRole::Body);
/// ```
#[must_use]
pub fn text<Msg: Clone>(content: impl Into<Arc<str>>, role: TypeRole, tone: Tone) -> Element<Msg> {
    label_text::<Msg>(content, role, StateSpec::tone(tone))
}

/// A paragraph clamped to `max_lines` lines with an end ellipsis.
///
/// ```
/// use guimaker::prelude::*;
/// let para: Element<()> = guimaker::components::paragraph("Long text", TypeRole::Body, Tone::Muted, 2);
/// assert_eq!(para.style.max_lines, Some(2));
/// ```
#[must_use]
pub fn paragraph<Msg: Clone>(
    content: impl Into<Arc<str>>,
    role: TypeRole,
    tone: Tone,
    max_lines: u16,
) -> Element<Msg> {
    label_text::<Msg>(content, role, StateSpec::tone(tone))
        .lines(max_lines)
        .ellipsis(Ellipsis::End)
}

/// A heading at level `1`, `2` or `3`, with an optional overline above it.
///
/// Level 1 maps to [`TypeRole::Display`], 2 to [`TypeRole::Title`] and 3 to
/// [`TypeRole::Heading`]; other values clamp to the nearest level. An overline
/// is a tracked [`TypeRole::Overline`] line in the muted tone.
///
/// ```
/// use guimaker::prelude::*;
/// let h1: Element<()> = heading("Title", 1, Some("Section"));
/// assert_eq!(h1.children[0].style.text_role, TypeRole::Overline);
/// ```
#[must_use]
pub fn heading<Msg: Clone>(
    content: impl Into<Arc<str>>,
    level: u8,
    overline: Option<&str>,
) -> Element<Msg> {
    let role = match level.clamp(1, 3) {
        1 => TypeRole::Display,
        2 => TypeRole::Title,
        _ => TypeRole::Heading,
    };
    let heading = label_text::<Msg>(content, role, StateSpec::tone(Tone::Default));
    match overline {
        Some(overline) => Element::column()
            .gap(Spacing::S1)
            .child(label_text::<Msg>(
                overline,
                TypeRole::Overline,
                StateSpec::tone(Tone::Muted),
            ))
            .child(heading),
        None => heading,
    }
}

/// A pill badge in a semantic status wash.
///
/// The fill is [`Status::subtle`], the label [`Status::tone`] and the shape
/// [`Radius::Full`], so a badge reads as one meaning without a hand-picked
/// colour.
///
/// ```
/// use guimaker::prelude::*;
/// let tag: Element<()> = badge("New", Status::Success);
/// assert_eq!(tag.style.bg, Some(Status::Success.subtle()));
/// ```
#[must_use]
pub fn badge<Msg: Clone>(label: impl Into<String>, status: Status) -> Element<Msg> {
    let label: String = label.into();
    Element::row()
        .align(Align::Center)
        .justify(Justify::Center)
        .pad_x(Spacing::S3)
        .pad_y(Spacing::S1)
        .bg(status.subtle())
        .radius(Radius::Full)
        .child(label_text::<Msg>(
            label,
            TypeRole::Caption,
            StateSpec::tone(Tone::Default),
        ))
}

/// A pill tag that can carry an icon-only remove action.
///
/// When `removable` is true and `remove` is `Some`, the tag appends a
/// [`Icon::Close`] control with the `"Remove"` tooltip. That control gets hover,
/// press and focus from the library and animates its state colours.
///
/// ```
/// use guimaker::prelude::*;
/// let tag: Element<()> = tag("Draft", true, Some(()));
/// assert!(tag.children.len() > 1);
/// ```
#[must_use]
pub fn tag<Msg: Clone>(
    label: impl Into<String>,
    removable: bool,
    remove: Option<Msg>,
) -> Element<Msg> {
    let size = ControlSize::Sm;
    let label: String = label.into();
    let mut row = Element::row()
        .align(Align::Center)
        .justify(Justify::Center)
        .gap(Spacing::S1)
        .pad_x(Spacing::S2)
        .pad_y(Spacing::S1)
        .bg(ColorRole::SurfaceRaised)
        .border(ColorRole::Border)
        .radius(Radius::Full)
        .child(label_text::<Msg>(
            label,
            TypeRole::Caption,
            StateSpec::tone(Tone::Default),
        ));
    if let (true, Some(message)) = (removable, remove) {
        let spec = inline_action_spec();
        row = row.child(
            spec.apply(
                Element::row()
                    .align(Align::Center)
                    .justify(Justify::Center)
                    .width(SizeSpec::Token(size.icon()))
                    .height(SizeSpec::Token(size.icon()))
                    .radius(Radius::Full)
                    .cursor(Cursor::Pointer)
                    .tooltip("Remove")
                    .transition(Duration::Fast)
                    .on_click(message),
            )
            .child(icon_box(Icon::Close, size, spec)),
        );
    }
    row
}

/// A single key cap at [`TypeRole::Mono`].
///
/// A cap is a raised [`ColorRole::SurfaceRaised`] block with a
/// [`ColorRole::Border`] outline and the [`Radius::Xs`] corner, the same shape a
/// shortcut chip uses everywhere in the app.
///
/// ```
/// use guimaker::prelude::*;
/// let key: Element<()> = kbd("K");
/// assert_eq!(key.style.bg, Some(ColorRole::SurfaceRaised));
/// ```
#[must_use]
pub fn kbd<Msg: Clone>(keys: &str) -> Element<Msg> {
    Element::row()
        .align(Align::Center)
        .justify(Justify::Center)
        .min_width(Spacing::S6)
        .min_height(Spacing::S6)
        .pad_x(Spacing::S2)
        .pad_y(Spacing::S1)
        .bg(ColorRole::SurfaceRaised)
        .border(ColorRole::Border)
        .radius(Radius::Xs)
        .child(label_text::<Msg>(
            keys,
            TypeRole::Mono,
            StateSpec::tone(Tone::Default),
        ))
}

/// A row of key caps joined by a [`TypeRole::Caption`] `"+"` separator.
///
/// ```
/// use guimaker::prelude::*;
/// let combo: Element<()> = kbd_combo(["Ctrl", "K"]);
/// assert_eq!(combo.children.len(), 3);
/// ```
#[must_use]
pub fn kbd_combo<Msg: Clone>(keys: impl IntoIterator<Item = &'static str>) -> Element<Msg> {
    let mut row = Element::row()
        .align(Align::Center)
        .gap(Spacing::S1)
        .flow_wrap();
    let mut first = true;
    for key in keys {
        if first {
            first = false;
        } else {
            row = row.child(label_text::<Msg>(
                "+",
                TypeRole::Caption,
                StateSpec::tone(Tone::Faint),
            ));
        }
        row = row.child(kbd::<Msg>(key));
    }
    row
}

/// A circular avatar with optional initials, a status ring and an image.
///
/// The circle is [`Radius::Full`], its diameter and initials role come from
/// `size`, and a [`Status`] draws a [`BorderWidth::Regular`] ring in the status
/// role. An image fills the circle when `image` is `Some`.
///
/// ```
/// use guimaker::prelude::*;
/// let me: Element<()> = avatar("JD", Some(Status::Success), ControlSize::Md, None);
/// assert_eq!(*me.style.radius.resolve(0.0, &Breakpoints::default()), Radius::Full);
/// ```
#[must_use]
pub fn avatar<Msg: Clone>(
    initials: &str,
    status: Option<Status>,
    size: ControlSize,
    image: Option<u64>,
) -> Element<Msg> {
    let border = status.map_or(ColorRole::Border, Status::role);
    let width = if status.is_some() {
        BorderWidth::Regular
    } else {
        BorderWidth::Hairline
    };
    let mut circle = Element::row()
        .align(Align::Center)
        .justify(Justify::Center)
        .width(SizeSpec::Token(size.height()))
        .height(SizeSpec::Token(size.height()))
        .bg(ColorRole::SurfaceRaised)
        .border(border)
        .border_width(width)
        .radius(Radius::Full)
        .clip()
        .transition(Duration::Fast);
    if let Some(source) = image {
        circle = circle.child(
            Element::image(source)
                .width(SizeSpec::Fill)
                .height(SizeSpec::Fill)
                .radius(Radius::Full),
        );
    } else {
        circle = circle.child(label_text::<Msg>(
            initials,
            size.label_role(),
            StateSpec::tone(Tone::Default),
        ));
    }
    circle
}

/// A tight row of avatars, each ringed in the page background.
///
/// A negative gap is not a layout primitive, so the group uses the smallest
/// positive gap and a [`ColorRole::Bg`] ring on every avatar to keep the circles
/// visually separate.
///
/// ```
/// use guimaker::prelude::*;
/// let team: Element<()> = avatar_group(["AB", "CD"], ControlSize::Sm);
/// assert_eq!(team.children.len(), 2);
/// ```
#[must_use]
pub fn avatar_group<Msg: Clone>(
    initials: impl IntoIterator<Item = &'static str>,
    size: ControlSize,
) -> Element<Msg> {
    let mut row = Element::row()
        .align(Align::Center)
        .gap_px(-Spacing::S2.px());
    for name in initials {
        row = row.child(
            avatar::<Msg>(name, None, size, None)
                .border(ColorRole::Border)
                .border_width(BorderWidth::Regular),
        );
    }
    row
}

/// A media thumbnail with a fixed aspect ratio and an optional status ring.
///
/// A loaded thumbnail is an [`Element::image`] with the aspect ratio,
/// [`Radius::Md`] and clipping. While `loading` is true, or when there is no
/// source, it is a [`Element::stack`] that shows a [`skeleton`] instead.
///
/// ```
/// use guimaker::prelude::*;
/// let shot: Element<()> = thumbnail(Some(7), 16.0 / 9.0, false, None);
/// assert_eq!(shot.kind, NodeKind::Image);
/// ```
#[must_use]
pub fn thumbnail<Msg: Clone>(
    source: Option<u64>,
    aspect: f32,
    loading: bool,
    status: Option<Status>,
) -> Element<Msg> {
    let border = status.map_or(ColorRole::Border, Status::role);
    let width = if status.is_some() {
        BorderWidth::Regular
    } else {
        BorderWidth::Hairline
    };
    let frame: Element<Msg> = match (loading, source) {
        (false, Some(image)) => Element::image(image).width(SizeSpec::Fill),
        _ => Element::stack()
            .align(Align::Center)
            .justify(Justify::Center)
            .width(SizeSpec::Fill)
            .child(skeleton(Spacing::S8, Spacing::S8, 1).align_self(Align::Center)),
    };
    frame
        .aspect_ratio(aspect)
        .bg(ColorRole::SurfaceSunken)
        .border(border)
        .border_width(width)
        .radius(Radius::Md)
        .clip()
        .transition(Duration::Fast)
}

/// A progress bar in a semantic status role.
///
/// The track is [`ColorRole::SurfaceSunken`] and [`Radius::Full`]; the fill is
/// the status role and its width is a [`SizeSpec::Fraction`] of the clamped
/// value. An indeterminate bar shows a partial [`ColorRole::AccentFill`] bar of
/// a fixed fraction: the host drives its motion, because the library does not
/// animate progress on its own.
///
/// ```
/// use guimaker::prelude::*;
/// let bar: Element<()> = progress(0.4, Status::Info, false);
/// assert_eq!(bar.children.len(), 1);
/// ```
#[must_use]
pub fn progress<Msg: Clone>(value: f32, status: Status, indeterminate: bool) -> Element<Msg> {
    let value = value.clamp(0.0, 1.0);
    let (fraction, role) = if indeterminate {
        (0.3, ColorRole::AccentFill)
    } else {
        (value, status.role())
    };
    Element::row()
        .align(Align::Center)
        .width(SizeSpec::Fill)
        .height(SizeSpec::Token(Spacing::S2))
        .bg(ColorRole::SurfaceSunken)
        .radius(Radius::Full)
        .clip()
        .transition(Duration::Fast)
        .child(
            Element::row()
                .width(SizeSpec::Fraction(fraction))
                .height(SizeSpec::Fill)
                .bg(role)
                .radius(Radius::Full),
        )
}

/// A circular progress surface with a centred percentage caption.
///
/// The value is a real arc: the ring emits a polyline over the shaped sweep and
/// the adapter strokes it with round-capped segments, so the leading edge is
/// smooth at every size. The track uses [`ColorRole::SurfaceSunken`]; the
/// caption stays in [`Status::tone`].
///
/// ```
/// use guimaker::prelude::*;
/// let ring: Element<()> = progress_ring(0.42, ControlSize::Lg, Status::Success);
/// assert_eq!(ring.children.len(), 3);
/// ```
#[must_use]
pub fn progress_ring<Msg: Clone>(value: f32, size: ControlSize, status: Status) -> Element<Msg> {
    let value = value.clamp(0.0, 1.0);
    let percentage = format!("{:.0}%", value * 100.0);
    let diameter = size.height();
    let track = Element::row()
        .width(SizeSpec::Fill)
        .height(SizeSpec::Fill)
        .bg(ColorRole::SurfaceSunken)
        .radius(Radius::Full);
    let arc = Element::<Msg>::path(arc_points(value))
        .width(SizeSpec::Fill)
        .height(SizeSpec::Fill)
        .stroke_width(Spacing::S1)
        .tone(status.tone());
    let caption = Element::column()
        .width(SizeSpec::Fill)
        .height(SizeSpec::Fill)
        .align(Align::Center)
        .justify(Justify::Center)
        .child(
            label_text::<Msg>(
                percentage,
                TypeRole::Caption,
                StateSpec::tone(status.tone()),
            )
            .tabular(),
        );
    Element::stack()
        .key("progress_ring")
        .align(Align::Center)
        .justify(Justify::Center)
        .width(SizeSpec::Token(diameter))
        .height(SizeSpec::Token(diameter))
        .transition(Duration::Fast)
        .child(track)
        .child(arc)
        .child(caption)
}

/// Points for an arc sweep from twelve o'clock, clockwise, over `value`.
///
/// The points are normalised to `0.0..=1.0` inside the node rect, so a path
/// node sizes the arc to whatever box it is given. Zero draws nothing.
#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::cast_precision_loss
)] // reason: the sweep is clamped to 0..=1, so 128 segments and the index fit
fn arc_points(value: f32) -> Vec<Vec2> {
    let value = value.clamp(0.0, 1.0);
    if value <= 0.0 {
        return Vec::new();
    }
    let segments = (value * 128.0).ceil().max(2.0) as usize;
    let start = -std::f32::consts::FRAC_PI_2;
    let denominator = segments as f32;
    (0..=segments)
        .map(|step| {
            let angle = start + std::f32::consts::TAU * value * (step as f32 / denominator);
            Vec2::new(0.5 + 0.5 * angle.cos(), 0.5 + 0.5 * angle.sin())
        })
        .collect()
}

/// A spinner mark at a control size and tone.
///
/// The mark is [`Icon::Refresh`], drawn as a static icon: Phase 7 adds the
/// rotation on the injected clock. The builder already carries the tone the
/// animation will spin.
///
/// ```
/// use guimaker::prelude::*;
/// let spinner_icon: Element<()> = spinner(ControlSize::Md, Tone::Muted);
/// assert_eq!(spinner_icon.kind, NodeKind::Icon);
/// ```
#[must_use]
pub fn spinner<Msg: Clone>(size: ControlSize, tone: Tone) -> Element<Msg> {
    icon_box(Icon::Refresh, size, StateSpec::tone(tone)).transition(Duration::Fast)
}

/// Placeholder blocks for content that is still loading.
///
/// Each line is a [`ColorRole::SurfaceSunken`] block of `width` by `height`
/// with the [`Radius::Sm`] corner, stacked with a small gap. `lines` is clamped
/// to at least one block.
///
/// ```
/// use guimaker::prelude::*;
/// let blocks: Element<()> = skeleton(Spacing::S12, Spacing::S2, 3);
/// assert_eq!(blocks.children.len(), 3);
/// ```
#[must_use]
pub fn skeleton<Msg: Clone>(width: Spacing, height: Spacing, lines: u16) -> Element<Msg> {
    let mut column = Element::column().align(Align::Start).gap(Spacing::S2);
    for _ in 0..lines.max(1) {
        column = column.child(
            Element::row()
                .width(SizeSpec::Token(width))
                .height(SizeSpec::Token(height))
                .bg(ColorRole::SurfaceSunken)
                .radius(Radius::Sm),
        );
    }
    column
}

/// A centred empty state with an icon, a title, a body and actions.
///
/// The column is a centred prose measure:
/// [`ContentWidth::Prose`] with [`Align::Center`] on both axes, and the action
/// row wraps below the body.
///
/// ```
/// use guimaker::prelude::*;
/// let empty: Element<()> = empty_state(Icon::Search, "No results", "Try again", std::iter::empty());
/// assert_eq!(empty.style.content_width, Some(ContentWidth::Prose));
/// ```
#[must_use]
pub fn empty_state<Msg: Clone>(
    icon: Icon,
    title: &str,
    body: &str,
    actions: impl IntoIterator<Item = Element<Msg>>,
) -> Element<Msg> {
    let actions_row = Element::row()
        .align(Align::Center)
        .justify(Justify::Center)
        .gap(crate::resp![Xs: Spacing::S2, Sm: Spacing::S3])
        .flow_wrap()
        .children(actions);
    Element::column()
        .align(Align::Center)
        .justify(Justify::Center)
        .gap(crate::resp![Xs: Spacing::S2, Lg: Spacing::S3])
        .pad(crate::resp![Xs: Spacing::S4, Lg: Spacing::S6])
        .max_width(ContentWidth::Prose)
        .align_self(Align::Center)
        .child(
            icon_box(icon, ControlSize::Lg, StateSpec::tone(Tone::Muted))
                .width(SizeSpec::Token(Spacing::S8))
                .height(SizeSpec::Token(Spacing::S8)),
        )
        .child(label_text::<Msg>(
            title,
            TypeRole::Heading,
            StateSpec::tone(Tone::Default),
        ))
        .child(label_text::<Msg>(
            body,
            TypeRole::Body,
            StateSpec::tone(Tone::Muted),
        ))
        .child(actions_row)
}

/// A callout: a status wash with a leading rule, a status icon and actions.
///
/// The surface is [`Status::subtle`], the leading rule is a real per-edge
/// border on the left only ([`Status::role`]), the title reads in
/// [`Status::tone`] and the body stays in the default tone. The action row
/// wraps.
///
/// ```
/// use guimaker::prelude::*;
/// let note: Element<()> = callout(Status::Warning, "Heads up", "Body", std::iter::empty());
/// assert_eq!(note.style.bg, Some(Status::Warning.subtle()));
/// ```
#[must_use]
pub fn callout<Msg: Clone>(
    status: Status,
    title: &str,
    body: &str,
    actions: impl IntoIterator<Item = Element<Msg>>,
) -> Element<Msg> {
    let header = Element::row()
        .align(Align::Center)
        .gap(Spacing::S2)
        .child(icon_box(
            callout_icon(status),
            ControlSize::Md,
            StateSpec::tone(status.tone()),
        ))
        .child(label_text::<Msg>(
            title,
            TypeRole::Label,
            StateSpec::tone(Tone::Default),
        ));
    let actions_row = Element::row()
        .align(Align::Center)
        .gap(Spacing::S2)
        .flow_wrap()
        .children(actions);
    Element::row()
        .key("callout")
        .align(Align::Stretch)
        .gap(Spacing::S3)
        .pad(crate::resp![Xs: Spacing::S3, Lg: Spacing::S4])
        .bg(status.subtle())
        .border(status.role())
        .border_left(BorderWidth::Regular)
        .radius(Radius::Md)
        .clip()
        .child(
            Element::column()
                .width(SizeSpec::Fill)
                .max_width(ContentWidth::Md)
                .width(SizeSpec::Fill)
                .gap(Spacing::S1)
                .child(header)
                .child(label_text::<Msg>(
                    body,
                    TypeRole::Body,
                    StateSpec::tone(Tone::Default),
                ))
                .child(actions_row),
        )
}

#[cfg(test)]
mod tests {
    use super::{
        avatar, avatar_group, badge, callout, empty_state, heading, kbd, kbd_combo, paragraph,
        progress, progress_ring, skeleton, spinner, tag, text, thumbnail,
    };
    use crate::components::{ControlSize, Status};
    use crate::icons::Icon;
    use crate::style::{Align, SizeSpec, Tone};
    use crate::text::Ellipsis;
    use crate::theme::{Breakpoints, ColorRole, ContentWidth, Radius, Spacing, TypeRole};
    use crate::view::{Element, NodeKind};

    fn node_count<Msg>(element: &Element<Msg>) -> usize {
        let mut count = 1;
        for child in &element.children {
            count += node_count(child);
        }
        count
    }

    fn count_actions(element: &Element<()>) -> usize {
        let mut count = usize::from(element.action.is_some());
        for child in &element.children {
            count += count_actions(child);
        }
        count
    }

    fn has_tooltip(element: &Element<()>, needle: &str) -> bool {
        element
            .style
            .tooltip
            .as_ref()
            .is_some_and(|tooltip| &*tooltip.text == needle)
            || element
                .children
                .iter()
                .any(|child| has_tooltip(child, needle))
    }

    #[test]
    fn every_display_builder_builds() {
        let elements: Vec<Element<()>> = vec![
            text("Body", TypeRole::Body, Tone::Default),
            paragraph("Long body", TypeRole::Body, Tone::Muted, 2),
            heading("Title", 1, Some("Intro")),
            heading("Title", 3, None),
            badge("New", Status::Info),
            tag("Draft", true, Some(())),
            kbd("K"),
            kbd_combo(["Ctrl", "K"]),
            avatar("JD", Some(Status::Success), ControlSize::Md, None),
            avatar("JD", None, ControlSize::Lg, Some(7)),
            avatar_group(["AB", "CD"], ControlSize::Sm),
            thumbnail(Some(1), 16.0 / 9.0, false, None),
            thumbnail(None, 1.0, true, Some(Status::Warning)),
            progress(0.5, Status::Info, false),
            progress(0.0, Status::Info, true),
            progress_ring(0.42, ControlSize::Lg, Status::Success),
            spinner(ControlSize::Md, Tone::Muted),
            skeleton(Spacing::S12, Spacing::S2, 3),
            empty_state(
                Icon::Search,
                "No results",
                "Try another search",
                [crate::view::text::<()>("Reset")],
            ),
            callout(
                Status::Warning,
                "Heads up",
                "Body",
                [crate::view::text::<()>("Dismiss")],
            ),
        ];
        for element in elements {
            assert!(node_count(&element) >= 1);
        }
    }

    #[test]
    fn text_and_paragraph_apply_their_roles() {
        let body: Element<()> = text("Body", TypeRole::Body, Tone::Muted);
        assert_eq!(body.style.text_role, TypeRole::Body);
        assert_eq!(body.style.tone, Tone::Muted);
        let para: Element<()> = paragraph("Long body", TypeRole::Body, Tone::Default, 2);
        assert_eq!(para.style.max_lines, Some(2));
        assert_eq!(para.style.ellipsis, Ellipsis::End);
    }

    #[test]
    fn heading_maps_levels_and_an_overline() {
        assert_eq!(
            heading::<()>("A", 1, None).style.text_role,
            TypeRole::Display
        );
        assert_eq!(heading::<()>("A", 2, None).style.text_role, TypeRole::Title);
        assert_eq!(
            heading::<()>("A", 3, None).style.text_role,
            TypeRole::Heading
        );
        let with_overline: Element<()> = heading("A", 2, Some("Intro"));
        assert_eq!(with_overline.kind, NodeKind::Column);
        assert_eq!(with_overline.children.len(), 2);
        assert_eq!(
            with_overline.children[0].style.text_role,
            TypeRole::Overline
        );
    }

    #[test]
    fn badge_tag_and_kbd_use_theme_roles() {
        let breakpoints = Breakpoints::default();
        let badge: Element<()> = badge("New", Status::Success);
        assert_eq!(badge.style.bg, Some(Status::Success.subtle()));
        assert_eq!(*badge.style.radius.resolve(0.0, &breakpoints), Radius::Full);
        assert_eq!(badge.children[0].style.tone, Tone::Default);

        let tag: Element<()> = tag("Draft", false, None::<()>);
        assert_eq!(tag.style.bg, Some(ColorRole::SurfaceRaised));
        assert_eq!(tag.style.border, Some(ColorRole::Border));
        assert_eq!(*tag.style.radius.resolve(0.0, &breakpoints), Radius::Full);

        let kbd: Element<()> = kbd("K");
        assert_eq!(kbd.style.bg, Some(ColorRole::SurfaceRaised));
        assert_eq!(kbd.style.border, Some(ColorRole::Border));
        assert_eq!(*kbd.style.radius.resolve(0.0, &breakpoints), Radius::Xs);
        assert_eq!(kbd.children[0].style.text_role, TypeRole::Mono);
    }

    #[test]
    fn a_removable_tag_has_a_tooltipped_close_action() {
        let removable: Element<()> = tag("Draft", true, Some(()));
        assert!(count_actions(&removable) > 0);
        assert!(has_tooltip(&removable, "Remove"));
        let plain: Element<()> = tag("Draft", false, Some(()));
        assert_eq!(count_actions(&plain), 0);
    }

    #[test]
    fn a_kbd_combo_joins_caps_with_a_plus() {
        let combo: Element<()> = kbd_combo(["Ctrl", "K"]);
        assert_eq!(combo.children.len(), 3);
        let plus = combo.children[1].text.as_ref().expect("separator");
        assert_eq!(plus.text(), "+");
        assert_eq!(combo.children[1].style.text_role, TypeRole::Caption);
    }

    #[test]
    fn avatars_are_circles_and_groups_ring_them() {
        let breakpoints = Breakpoints::default();
        let person: Element<()> = avatar("JD", Some(Status::Success), ControlSize::Md, None);
        assert_eq!(
            *person.style.radius.resolve(0.0, &breakpoints),
            Radius::Full
        );
        assert_eq!(person.style.border, Some(ColorRole::Success));
        assert_eq!(
            person.children[0].style.text_role,
            ControlSize::Md.label_role()
        );
        let group: Element<()> = avatar_group(["AB", "CD"], ControlSize::Sm);
        assert_eq!(group.children.len(), 2);
        assert_eq!(group.children[0].style.border, Some(ColorRole::Border));
    }

    #[test]
    fn a_thumbnail_uses_its_image_or_a_skeleton() {
        let loaded: Element<()> = thumbnail(Some(9), 1.5, false, Some(Status::Danger));
        assert_eq!(loaded.style.border, Some(ColorRole::Danger));
        assert_eq!(loaded.kind, NodeKind::Image);
        let loading: Element<()> = thumbnail(None, 1.5, true, None);
        assert_eq!(loading.kind, NodeKind::Stack);
        assert_eq!(loading.children[0].kind, NodeKind::Column);
    }

    #[test]
    fn progress_clamps_its_fraction() {
        let breakpoints = Breakpoints::default();
        let fraction = |element: &Element<()>| match *element.children[0]
            .style
            .width
            .resolve(0.0, &breakpoints)
        {
            SizeSpec::Fraction(value) => value,
            _ => panic!("progress fill is not a fraction"),
        };
        let over: Element<()> = progress(1.5, Status::Success, false);
        let under: Element<()> = progress(-0.5, Status::Warning, false);
        assert!((fraction(&over) - 1.0).abs() < f32::EPSILON);
        assert!(fraction(&under).abs() < f32::EPSILON);
        let indeterminate: Element<()> = progress(0.8, Status::Info, true);
        assert!(fraction(&indeterminate) < 1.0);
    }

    #[test]
    fn a_callout_keeps_its_actions_and_status_wash() {
        let note: Element<()> = callout(
            Status::Info,
            "Title",
            "Body",
            [
                crate::view::text::<()>("One").on_click(()),
                crate::view::text::<()>("Two").on_click(()),
            ],
        );
        assert_eq!(note.style.bg, Some(Status::Info.subtle()));
        assert_eq!(count_actions(&note), 2);
    }

    #[test]
    fn an_empty_state_is_a_centred_prose_column() {
        let breakpoints = Breakpoints::default();
        let empty: Element<()> = empty_state(
            Icon::Search,
            "No results",
            "Try again",
            [crate::view::text::<()>("Reset")],
        );
        assert_eq!(empty.style.align_self, Some(Align::Center));
        assert_eq!(empty.style.content_width, Some(ContentWidth::Prose));
        assert_eq!(*empty.style.align.resolve(0.0, &breakpoints), Align::Center);
    }

    #[test]
    fn spinner_and_skeleton_are_placeholders() {
        let mark: Element<()> = spinner(ControlSize::Md, Tone::Muted);
        assert_eq!(mark.kind, NodeKind::Icon);
        assert!(mark.icon_path.is_some());
        let blocks: Element<()> = skeleton(Spacing::S12, Spacing::S2, 3);
        assert_eq!(blocks.children.len(), 3);
        assert!(blocks
            .children
            .iter()
            .all(|child| child.style.bg == Some(ColorRole::SurfaceSunken)));
    }
}
