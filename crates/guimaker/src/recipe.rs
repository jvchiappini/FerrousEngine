//! The start-screen recipe: the layout benchmark, a realistic view and the
//! phase 3 responsive reference.
//!
//! This is a structural screen, not a component library: the phase 6 components
//! replace the hand-built rows. It exists so the 0.3 ms layout threshold has a
//! concrete, measured workload, so phase 6 has a target to match, and so the
//! responsive sweep has a real screen to guard.
//!
//! At `Lg` and up the body is two columns (recent projects and the new-project
//! form), each capped to a content column and centred; below `Lg` they stack and
//! the screen scrolls, because a 360 px window cannot show both without
//! scrolling. The title's type size is fluid between `Heading` and `Display`.
//!
//! The one deliberate unit test asserts the structure; `tests/responsive.rs`
//! sweeps it at ten widths and four scales.

#![allow(clippy::too_many_lines)] // reason: the recipe is one declarative screen
use crate::resp;
use crate::style::{Align, FluidText, Justify, SizeSpec, Tone};
use crate::text::Ellipsis;
use crate::theme::{ColorRole, ContentWidth, Radius, Spacing, TypeRole};
use crate::view::{text, Element};

/// A start-screen action.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum StartMsg {
    /// Create a project from the form.
    NewProject,
    /// Open one of the recent projects.
    OpenRecent(u64),
    /// Open the settings panel.
    Settings,
    /// Open the documentation.
    Docs,
    /// Browse for a file.
    Browse,
}

/// Builds the start screen.
#[must_use]
pub fn start_screen() -> Element<StartMsg> {
    let header = Element::row()
        .key("header")
        .gap(resp![Xs: Spacing::S2, Md: Spacing::S3])
        .align(Align::Center)
        .justify(Justify::SpaceBetween)
        .flow_wrap()
        .child(
            Element::column()
                .key("brand")
                .gap(Spacing::S1)
                .child(
                    text::<StartMsg>("Motioner")
                        .key("title")
                        .text_role(TypeRole::Display)
                        .fluid_text(FluidText::new(TypeRole::Heading, 0.012, TypeRole::Display))
                        .tone(Tone::Default)
                        .priority(9),
                )
                .child(
                    text::<StartMsg>("Code-driven motion graphics")
                        .text_role(TypeRole::Caption)
                        .tone(Tone::Muted)
                        .priority(1)
                        .lines(1)
                        .ellipsis(Ellipsis::End),
                ),
        )
        .child(
            Element::row()
                .key("header_actions")
                .gap(Spacing::S1)
                .align(Align::Center)
                .child(link("Settings", StartMsg::Settings))
                .child(link("Docs", StartMsg::Docs)),
        );

    let mut recent = Element::column()
        .key("recent")
        .align(Align::Stretch)
        .gap(resp![Xs: Spacing::S2, Md: Spacing::S3])
        .width(SizeSpec::Fill)
        .max_width(ContentWidth::Lg)
        .child(
            text::<StartMsg>("Recent projects")
                .text_role(TypeRole::Heading)
                .tone(Tone::Default),
        );
    for index in 0..5u64 {
        recent = recent.child(recent_row(index));
    }

    let mut fields = Element::column()
        .key("new_project")
        .align(Align::Stretch)
        .gap(resp![Xs: Spacing::S3, Md: Spacing::S4])
        .width(SizeSpec::Fill)
        .max_width(ContentWidth::Md)
        .child(
            text::<StartMsg>("New project")
                .text_role(TypeRole::Heading)
                .tone(Tone::Default),
        )
        .child(
            text::<StartMsg>("Start with a prompt, an example, or an empty composition")
                .text_role(TypeRole::Body)
                .tone(Tone::Muted)
                .lines(2)
                .priority(1),
        );
    for (key, label, placeholder) in [
        ("field_name", "Name", "Untitled"),
        ("field_size", "Size", "1920 x 1080"),
        ("field_rate", "Frame rate", "60 fps"),
        ("field_engine", "Engine", "FerrousEngine"),
    ] {
        fields = fields.child(field(key, label, placeholder));
    }
    fields = fields.child(
        Element::row()
            .key("new_project_actions")
            .gap(Spacing::S2)
            .align(Align::Center)
            .child(
                text::<StartMsg>("Create")
                    .key("create")
                    .text_role(TypeRole::Label)
                    .tone(Tone::Default)
                    .pad_x(resp![Xs: Spacing::S3, Md: Spacing::S5])
                    .pad_y(Spacing::S3)
                    .bg(ColorRole::AccentFill)
                    .radius(Radius::Sm)
                    .on_click(StartMsg::NewProject),
            )
            .child(
                text::<StartMsg>("Browse…")
                    .key("browse")
                    .text_role(TypeRole::Label)
                    .tone(Tone::Muted)
                    .pad_x(resp![Xs: Spacing::S3, Md: Spacing::S5])
                    .pad_y(Spacing::S3)
                    .bg(ColorRole::SurfaceRaised)
                    .border(ColorRole::Border)
                    .radius(Radius::Sm)
                    .on_click(StartMsg::Browse),
            ),
    );

    Element::column()
        .key("start")
        .align(Align::Stretch)
        .width(SizeSpec::Fill)
        .height(SizeSpec::Fill)
        .bg(ColorRole::Bg)
        .pad(resp![Xs: Spacing::S4, Md: Spacing::S6, Lg: Spacing::S8])
        .gap(resp![Xs: Spacing::S4, Lg: Spacing::S6])
        // Below Lg the two columns stack and their sum is taller than a short
        // window, so the screen scrolls; at Lg and up it fits and does not clip.
        .scroll_axes(
            resp![Xs: crate::style::ScrollAxes::Vertical, Lg: crate::style::ScrollAxes::None],
        )
        .child(header)
        .child(Element::divider())
        .child(
            Element::row()
                .key("body")
                .align(Align::Stretch)
                .gap(resp![Xs: Spacing::S4, Lg: Spacing::S8])
                .direction(resp![
                    Xs: crate::style::Direction::Column,
                    Lg: crate::style::Direction::Row
                ])
                .grow(1.0)
                .child(content_column(recent, SizeSpec::Fraction(0.55)))
                .child(content_column(fields, SizeSpec::Fraction(0.45))),
        )
        .child(
            Element::row()
                .key("footer")
                .gap(Spacing::S2)
                .align(Align::Center)
                .child(
                    text::<StartMsg>("FerrousEngine 0.1")
                        .text_role(TypeRole::Caption)
                        .tone(Tone::Faint)
                        .priority(1),
                )
                .child(Element::spacer())
                .child(
                    text::<StartMsg>("Apache-2.0")
                        .text_role(TypeRole::Caption)
                        .tone(Tone::Faint)
                        .priority(9),
                ),
        )
}

/// A full-width centring slot whose child is capped by its own `max_width`.
///
/// Stacked, the slot is full width and the content column centres inside it;
/// side by side, the slot is a fraction of the row. Above the cap the column
/// stops growing, which is what keeps prose and forms readable at 3840 px.
fn content_column(content: Element<StartMsg>, width: SizeSpec) -> Element<StartMsg> {
    Element::row()
        .justify(Justify::Center)
        .align(Align::Start)
        .width(resp![Xs: SizeSpec::Fill, Lg: width])
        .child(content)
}

fn link(label: &'static str, message: StartMsg) -> Element<StartMsg> {
    text::<StartMsg>(label)
        .text_role(TypeRole::Label)
        .tone(Tone::Muted)
        .pad_x(Spacing::S3)
        .pad_y(Spacing::S2)
        .radius(Radius::Sm)
        .on_click(message)
}

fn recent_row(index: u64) -> Element<StartMsg> {
    Element::row()
        .key_id(index)
        .gap(resp![Xs: Spacing::S2, Md: Spacing::S3])
        .align(Align::Center)
        .pad(resp![Xs: Spacing::S2, Md: Spacing::S3])
        .bg(ColorRole::Surface)
        .border(ColorRole::Border)
        .radius(Radius::Md)
        .child(
            Element::column()
                .gap(Spacing::S1)
                .width(SizeSpec::Fill)
                .child(
                    text::<StartMsg>(format!("Project {index}"))
                        .text_role(TypeRole::Label)
                        .tone(Tone::Default)
                        .priority(1)
                        .ellipsis(Ellipsis::End)
                        .lines(1),
                )
                .child(
                    text::<StartMsg>("Edited recently")
                        .text_role(TypeRole::Caption)
                        .tone(Tone::Muted)
                        .ellipsis(Ellipsis::End)
                        .lines(1),
                ),
        )
        .child(
            text::<StartMsg>("11:42")
                .text_role(TypeRole::Caption)
                .tone(Tone::Faint)
                .priority(9)
                .tabular(),
        )
        .on_click(StartMsg::OpenRecent(index))
}

fn field(key: &'static str, label: &'static str, placeholder: &'static str) -> Element<StartMsg> {
    Element::column()
        .key(key)
        .gap(Spacing::S1)
        .child(
            text::<StartMsg>(label)
                .text_role(TypeRole::Caption)
                .tone(Tone::Muted),
        )
        .child(
            Element::row()
                .pad(Spacing::S3)
                .min_height(Spacing::S10)
                .width(SizeSpec::Fill)
                .bg(ColorRole::SurfaceSunken)
                .border(ColorRole::Border)
                .radius(Radius::Sm)
                .child(
                    text::<StartMsg>(placeholder)
                        .text_role(TypeRole::Body)
                        .tone(Tone::Faint)
                        .priority(1)
                        .ellipsis(Ellipsis::End)
                        .lines(1),
                ),
        )
}

#[cfg(test)]
mod tests {
    use super::start_screen;
    use crate::view::Tree;

    #[test]
    fn the_recipe_has_a_real_structure() {
        let tree = Tree::build(&start_screen());
        assert!(tree.len() > 40, "the recipe is {} nodes", tree.len());
        assert!(tree.find_key("start").is_some());
        assert!(tree.find_key("recent").is_some());
        assert!(tree.find_key("new_project").is_some());
        assert!(tree.find_key("create").is_some());
        // 5 actions + 2 nav items + 2 window buttons + the scroll container,
        // which is a hit target so the wheel can reach it.
        assert_eq!(
            tree.nodes.iter().filter(|node| node.interactive).count(),
            5 + 2 + 2 + 1
        );
    }

    #[test]
    fn the_recipe_is_responsive() {
        let view = start_screen();
        let tree = Tree::build(&view);
        let body = tree.find_key("body").expect("body");
        assert!(tree.nodes[body.index()].style.is_responsive());
        let title = tree.find_key("title").expect("title");
        assert!(tree.nodes[title.index()].style.fluid_text.is_some());
        let start = tree.find_key("start").expect("start");
        assert!(tree.nodes[start.index()].style.is_responsive());
    }
}
