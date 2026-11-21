//! The start-screen recipe: the phase 2 layout benchmark and a realistic view.
//!
//! This is a structural screen, not a component library: the phase 6 components
//! replace the hand-built rows. It exists so the 0.3 ms layout threshold has a
//! concrete, measured workload and so phase 6 has a target to match.

#![allow(clippy::too_many_lines)] // reason: the recipe is one declarative screen
use crate::style::{Align, SizeSpec, Tone};
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
        .gap(Spacing::S3)
        .align(Align::Center)
        .child(
            Element::column()
                .key("brand")
                .gap(Spacing::S1)
                .child(
                    text::<StartMsg>("Motioner")
                        .key("title")
                        .text_role(TypeRole::Title)
                        .tone(Tone::Default),
                )
                .child(
                    text::<StartMsg>("Code-driven motion graphics")
                        .text_role(TypeRole::Caption)
                        .tone(Tone::Muted),
                ),
        )
        .child(Element::spacer())
        .child(
            Element::row()
                .key("header_actions")
                .gap(Spacing::S2)
                .align(Align::Center)
                .child(link("Settings", StartMsg::Settings))
                .child(link("Docs", StartMsg::Docs)),
        );

    let mut recent = Element::column()
        .key("recent")
        .align(Align::Stretch)
        .gap(Spacing::S3)
        .width(SizeSpec::Fraction(0.55))
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
        .gap(Spacing::S4)
        .width(SizeSpec::Fraction(0.45))
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
                .lines(2),
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
                    .pad_x(Spacing::S5)
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
                    .pad_x(Spacing::S5)
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
        .pad(Spacing::S8)
        .gap(Spacing::S6)
        .child(header)
        .child(Element::divider())
        .child(
            Element::row()
                .key("body")
                .gap(Spacing::S8)
                .grow(1.0)
                .child(recent)
                .child(fields),
        )
        .child(
            Element::row()
                .key("footer")
                .gap(Spacing::S2)
                .align(Align::Center)
                .child(
                    text::<StartMsg>("FerrousEngine 0.1")
                        .text_role(TypeRole::Caption)
                        .tone(Tone::Faint),
                )
                .child(Element::spacer())
                .child(
                    text::<StartMsg>("Apache-2.0")
                        .text_role(TypeRole::Caption)
                        .tone(Tone::Faint),
                ),
        )
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
        .gap(Spacing::S3)
        .align(Align::Center)
        .pad(Spacing::S3)
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
                        .ellipsis(Ellipsis::End)
                        .lines(1),
                )
                .child(
                    text::<StartMsg>("Edited recently")
                        .text_role(TypeRole::Caption)
                        .tone(Tone::Muted),
                ),
        )
        .child(
            text::<StartMsg>("11:42")
                .text_role(TypeRole::Caption)
                .tone(Tone::Faint)
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
                        .tone(Tone::Faint),
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
        assert_eq!(
            tree.nodes.iter().filter(|node| node.interactive).count(),
            5 + 2 + 2
        );
    }
}
