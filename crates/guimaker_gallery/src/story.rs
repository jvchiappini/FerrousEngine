//! The story registry: one entry per screen the gallery can render.
//!
//! A story is a pure function of `(theme, viewport, scale)`. It builds a
//! `guimaker::Ui`, lays the view out at the requested viewport and scale, sets
//! the injected clock to zero and returns the flat display list. Nothing here
//! touches a GPU, so a story is also the unit the goldens and the host
//! example share.

use guimaker::display_list::DisplayList;
use guimaker::geom::Size;
use guimaker::recipe::start_screen;
use guimaker::style::Tone;
use guimaker::theme::{ColorRole, Radius, Spacing, Theme, TypeRole};
use guimaker::view::{text, Element};
use guimaker::{Frame, Ui};
use guimaker_ferrous::FerrousTextSystem;

use crate::components::Component;

/// The deterministic instant the start story injects: 11:42.
///
/// A golden run at any wall-clock hour paints the same time text because the
/// story fixes the instant and the recipe never reads the system clock.
pub const START_TIME_MS: f64 = 42_120_000.0;

/// A screen the gallery knows how to paint.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Story {
    /// The Phase 3 start-screen recipe: header, recent projects and the
    /// new-project form.
    Start,
    /// A components/actions board: the primitive vocabulary a screen is built
    /// from, with every action, tone, surface and divider in one place.
    Components,
    /// One Phase 6 component, in every variant and state.
    Component(Component),
}

impl Story {
    /// The two hand-built board stories.
    pub const ALL: [Self; 2] = [Self::Start, Self::Components];

    /// Every story, in registry order: the boards, then every component.
    #[must_use]
    pub fn all() -> Vec<Self> {
        let mut stories = Vec::with_capacity(Self::ALL.len() + Component::ALL.len());
        stories.extend(Self::ALL);
        stories.extend(Component::ALL.into_iter().map(Self::Component));
        stories
    }

    /// The stable CLI name.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Start => "start",
            Self::Components => "components",
            Self::Component(component) => component.name(),
        }
    }

    /// Parses a CLI name.
    #[must_use]
    pub fn parse(name: &str) -> Option<Self> {
        match name {
            "start" => Some(Self::Start),
            "components" => Some(Self::Components),
            other => Component::parse(other).map(Self::Component),
        }
    }

    /// Paints the story and returns its display list.
    ///
    /// `viewport` is logical pixels and `scale` is the logical-to-physical
    /// factor the host will use; both are handed to [`Ui::set_viewport`] so
    /// responsive values and the display list agree with the eventual target.
    #[must_use]
    pub fn display_list(self, theme: &Theme, viewport: Size, scale: f32) -> DisplayList {
        paint(&self.view(), theme, viewport, scale)
    }

    /// The story's view with its message type erased.
    ///
    /// A story paints and lints the same way whatever its actions are, so the
    /// gallery exposes one `Element<()>` for tools that do not route input
    /// (the lint, the overlay and every headless command). Erasing an action
    /// changes no style, text or rectangle, so the display list stays
    /// byte-identical.
    #[must_use]
    pub fn view(self) -> Element<()> {
        match self {
            Self::Start => erase_actions(start_screen(START_TIME_MS)),
            Self::Components => erase_actions(components_screen()),
            Self::Component(component) => component.view(),
        }
    }
}

/// Rebuilds an element tree with no action messages.
fn erase_actions<Msg>(element: Element<Msg>) -> Element<()> {
    Element {
        kind: element.kind,
        style: element.style,
        text: element.text,
        children: element.children.into_iter().map(erase_actions).collect(),
        key: element.key,
        action: None,
        image: element.image,
        icon_path: element.icon_path,
        path_points: element.path_points,
    }
}

/// Lays out and paints one view at the requested viewport/scale.
fn paint<Msg: Clone>(
    view: &Element<Msg>,
    theme: &Theme,
    viewport: Size,
    scale: f32,
) -> DisplayList {
    let mut ui: Ui<Msg> = Ui::new(theme.clone());
    ui.set_viewport(viewport, scale);
    let _ = ui.set_time_ms(0.0);
    let mut text_system = FerrousTextSystem::new();
    let mut frame = Frame::new();
    ui.paint(view, &mut frame, &mut text_system);
    frame.display_list
}

/// A components/actions board with no engine dependency beyond the core view
/// vocabulary.
fn components_screen() -> Element<ComponentsMsg> {
    let content = Element::column()
        .align(guimaker::style::Align::Stretch)
        .gap(Spacing::S6)
        .child(header())
        .child(Element::divider())
        .child(actions())
        .child(Element::divider())
        .child(status_tones())
        .child(Element::divider())
        .child(surfaces());

    Element::column()
        .key("components")
        .align(guimaker::style::Align::Stretch)
        .width(guimaker::style::SizeSpec::Fill)
        .height(guimaker::style::SizeSpec::Fill)
        .bg(ColorRole::Bg)
        .pad(Spacing::S8)
        .scroll_axes(guimaker::style::ScrollAxes::Vertical)
        .child(content)
}

impl ComponentsMsg {
    /// The display label of an action, used by the demo host log.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Primary => "primary action",
            Self::Secondary => "secondary action",
            Self::Ghost => "ghost action",
            Self::Danger => "danger action",
            Self::Toggle => "toggle",
        }
    }
}

/// A component-board action.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ComponentsMsg {
    /// The filled primary action.
    Primary,
    /// The raised secondary action.
    Secondary,
    /// The borderless ghost action.
    Ghost,
    /// The destructive action.
    Danger,
    /// A toggle-style action.
    Toggle,
}

fn header() -> Element<ComponentsMsg> {
    Element::column()
        .key("components_header")
        .gap(Spacing::S1)
        .child(
            text::<ComponentsMsg>("Components")
                .text_role(TypeRole::Display)
                .tone(guimaker::style::Tone::Default),
        )
        .child(
            text::<ComponentsMsg>("The primitive vocabulary every screen composes")
                .text_role(TypeRole::Body)
                .tone(guimaker::style::Tone::Muted),
        )
}

fn actions() -> Element<ComponentsMsg> {
    Element::column()
        .key("actions")
        .gap(Spacing::S3)
        .child(
            text::<ComponentsMsg>("Actions")
                .text_role(TypeRole::Subheading)
                .tone(guimaker::style::Tone::Default),
        )
        .child(
            Element::row()
                .gap(Spacing::S3)
                .align(guimaker::style::Align::Center)
                .flow_wrap()
                .child(filled_action(
                    "Primary",
                    ColorRole::AccentFill,
                    ComponentsMsg::Primary,
                ))
                .child(
                    filled_action(
                        "Secondary",
                        ColorRole::SurfaceRaised,
                        ComponentsMsg::Secondary,
                    )
                    .border(ColorRole::Border),
                )
                .child(
                    filled_action("Ghost", ColorRole::Surface, ComponentsMsg::Ghost)
                        .border(ColorRole::Border),
                )
                .child(
                    filled_action("Delete", ColorRole::DangerSubtle, ComponentsMsg::Danger)
                        .tone(guimaker::style::Tone::Danger),
                ),
        )
        .child(
            Element::row()
                .gap(Spacing::S2)
                .align(guimaker::style::Align::Center)
                .child(chip("Toggle", ColorRole::AccentSubtle).on_click(ComponentsMsg::Toggle))
                .child(chip("Play", ColorRole::SuccessSubtle))
                .child(chip("Record", ColorRole::DangerSubtle)),
        )
}

fn filled_action(
    label: &'static str,
    fill: ColorRole,
    message: ComponentsMsg,
) -> Element<ComponentsMsg> {
    let action = text::<ComponentsMsg>(label)
        .key(label_key(label))
        .text_role(TypeRole::Label)
        .pad_x(Spacing::S5)
        .pad_y(Spacing::S3)
        .radius(Radius::Sm)
        .on_click(message);
    match fill {
        ColorRole::AccentFill => action.on_accent_fill(),
        other => action.bg(other).tone(Tone::Default),
    }
}

fn label_key(label: &'static str) -> &'static str {
    match label {
        "Primary" => "action_primary",
        "Secondary" => "action_secondary",
        "Ghost" => "action_ghost",
        _ => "action_danger",
    }
}

fn chip(label: &'static str, fill: ColorRole) -> Element<ComponentsMsg> {
    // A subtle chip uses the semantic role that the fill is made from; the
    // saturated role against its 14 % wash stays readable and carries meaning.
    let tone = match fill {
        ColorRole::AccentSubtle => Tone::Accent,
        ColorRole::SuccessSubtle => Tone::Success,
        ColorRole::DangerSubtle => Tone::Danger,
        ColorRole::WarningSubtle => Tone::Warning,
        ColorRole::InfoSubtle => Tone::Info,
        _ => Tone::Default,
    };
    text::<ComponentsMsg>(label)
        .text_role(TypeRole::Caption)
        .tone(tone)
        .pad_x(Spacing::S3)
        .pad_y(Spacing::S1)
        .min_height(Spacing::S6)
        .bg(fill)
        .radius(Radius::Full)
}

fn status_tones() -> Element<ComponentsMsg> {
    Element::column()
        .key("tones")
        .gap(Spacing::S3)
        .child(
            text::<ComponentsMsg>("Status tones")
                .text_role(TypeRole::Subheading)
                .tone(guimaker::style::Tone::Default),
        )
        .child(
            Element::row()
                .gap(Spacing::S4)
                .align(guimaker::style::Align::Center)
                .flow_wrap()
                .child(tone(
                    "Info",
                    guimaker::style::Tone::Info,
                    ColorRole::InfoSubtle,
                ))
                .child(tone(
                    "Success",
                    guimaker::style::Tone::Success,
                    ColorRole::SuccessSubtle,
                ))
                .child(tone(
                    "Warning",
                    guimaker::style::Tone::Warning,
                    ColorRole::WarningSubtle,
                ))
                .child(tone(
                    "Danger",
                    guimaker::style::Tone::Danger,
                    ColorRole::DangerSubtle,
                )),
        )
}

fn tone(
    label: &'static str,
    tone: guimaker::style::Tone,
    fill: ColorRole,
) -> Element<ComponentsMsg> {
    Element::row()
        .gap(Spacing::S2)
        .align(guimaker::style::Align::Center)
        .pad_x(Spacing::S3)
        .pad_y(Spacing::S2)
        .bg(fill)
        .radius(Radius::Sm)
        .child(
            text::<ComponentsMsg>(label)
                .text_role(TypeRole::Label)
                .tone(tone),
        )
}

fn surfaces() -> Element<ComponentsMsg> {
    Element::column()
        .key("surfaces")
        .gap(Spacing::S3)
        .child(
            text::<ComponentsMsg>("Surfaces")
                .text_role(TypeRole::Subheading)
                .tone(guimaker::style::Tone::Default),
        )
        .child(
            Element::row()
                .gap(Spacing::S3)
                .align(guimaker::style::Align::Stretch)
                .flow_wrap()
                .child(card(
                    "Bg",
                    "The window backdrop",
                    ColorRole::Bg,
                    ColorRole::Border,
                ))
                .child(card(
                    "Surface",
                    "A panel body",
                    ColorRole::Surface,
                    ColorRole::Border,
                ))
                .child(card(
                    "Raised",
                    "A card or popover",
                    ColorRole::SurfaceRaised,
                    ColorRole::BorderStrong,
                ))
                .child(card(
                    "Sunken",
                    "A well or track",
                    ColorRole::SurfaceSunken,
                    ColorRole::Border,
                )),
        )
}

fn card(
    title: &'static str,
    body: &'static str,
    fill: ColorRole,
    border: ColorRole,
) -> Element<ComponentsMsg> {
    Element::column()
        .gap(Spacing::S1)
        .width(guimaker::style::SizeSpec::Px(220.0))
        .pad(Spacing::S4)
        .bg(fill)
        .border(border)
        .radius(Radius::Md)
        .child(
            text::<ComponentsMsg>(title)
                .text_role(TypeRole::Label)
                .tone(guimaker::style::Tone::Default),
        )
        .child(
            text::<ComponentsMsg>(body)
                .text_role(TypeRole::Caption)
                .tone(guimaker::style::Tone::Muted),
        )
}

#[cfg(test)]
mod tests {
    use super::{Story, START_TIME_MS};
    use guimaker::display_list::DrawCommand;
    use guimaker::geom::Size;
    use guimaker::theme::Theme;

    #[test]
    fn every_story_paints_a_non_empty_list() {
        for story in Story::all() {
            let list = story.display_list(&Theme::dark(), Size::new(1280.0, 800.0), 1.5);
            assert!(
                !list.commands.is_empty(),
                "{} painted no commands",
                story.name()
            );
        }
    }

    #[test]
    fn the_start_story_injects_a_fixed_clock() {
        let list = Story::Start.display_list(&Theme::light(), Size::new(1280.0, 800.0), 1.5);
        let times = list
            .commands
            .iter()
            .filter_map(|command| match command {
                DrawCommand::Text { run, .. } => Some(run.text.as_ref()),
                _ => None,
            })
            .filter(|text| *text == "11:42")
            .count();
        assert_eq!(times, 5, "the five recent rows share the injected time");
        assert!((START_TIME_MS - 42_120_000.0).abs() < f64::EPSILON);
    }

    #[test]
    fn names_round_trip() {
        for story in Story::all() {
            assert_eq!(Story::parse(story.name()), Some(story));
        }
        assert_eq!(Story::parse("nope"), None);
    }
}
