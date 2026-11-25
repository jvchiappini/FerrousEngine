//! The start-screen recipe: the layout benchmark, a realistic view and the
//! phase 3 responsive reference.
//!
//! The structure comes from the general component library: [`field`] is the
//! read-only form field, [`list_row`] is the recent-project card,
//! [`nav_link`] is the header action and [`content_column`] is the centring
//! slot. The recipe only decides what the screen *is*; it spells no coordinates
//! and no design values, so the phase 9 comparison against plain HTML + CSS can
//! count just the view.
//!
//! At `Lg` and up the body is two columns (recent projects and the new-project
//! form), each capped to a content column and centred; below `Lg` they stack and
//! the screen scrolls, because a 360 px window cannot show both without
//! scrolling. The title's type size is fluid between `Heading` and `Display`.
//!
//! The one deliberate unit test asserts the structure; `tests/responsive.rs`
//! sweeps it at ten widths and four scales.

#![allow(clippy::too_many_lines)] // reason: the recipe is one declarative screen
use crate::components::{
    brand, caption, content_column, field, list_row, nav_link, paragraph, section_heading,
};
use crate::resp;
use crate::style::{Align, Justify, Lines, Priority, SizeSpec, Split, Tone};
use crate::theme::{ColorRole, ContentWidth, Radius, Spacing, TypeRole};
use crate::view::{text, Element};

/// How many recent projects the sample screen shows.
///
/// It is demo data, not a design value: a real app feeds the rows from its own
/// recent-projects list.
const RECENT_COUNT: u64 = 5;

/// The recent-project indices, as a named range so the view spells no number.
const RECENT_INDICES: std::ops::Range<u64> = 0..RECENT_COUNT;

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

/// Builds the start screen at the injected clock instant, in milliseconds.
///
/// The recent-project times are formatted from `now_ms`; the library never
/// reads the wall clock, so a story or a golden injects a fixed instant and
/// stays identical at any hour.
#[must_use]
pub fn start_screen(now_ms: f64) -> Element<StartMsg> {
    let time = crate::format::clock_hh_mm(now_ms);
    let header = Element::row()
        .key("header")
        .gap(resp![Xs: Spacing::S2, Md: Spacing::S3])
        .align(Align::Center)
        .justify(Justify::SpaceBetween)
        .flow_wrap()
        .child(brand("Motioner", "Code-driven motion graphics"))
        .child(
            Element::row()
                .key("header_actions")
                .gap(Spacing::S1)
                .align(Align::Center)
                .child(nav_link("Settings", StartMsg::Settings))
                .child(nav_link("Docs", StartMsg::Docs)),
        );

    let mut recent = Element::column()
        .key("recent")
        .align(Align::Stretch)
        .gap(resp![Xs: Spacing::S2, Md: Spacing::S3])
        .width(SizeSpec::Fill)
        .max_width(ContentWidth::Lg)
        .child(section_heading::<StartMsg>("Recent projects"));
    for index in RECENT_INDICES {
        recent = recent.child(
            list_row(format!("Project {index}"), "Edited recently", time.clone())
                .key_id(index)
                .on_click(StartMsg::OpenRecent(index)),
        );
    }

    let mut fields = Element::column()
        .key("new_project")
        .align(Align::Stretch)
        .gap(resp![Xs: Spacing::S3, Md: Spacing::S4])
        .width(SizeSpec::Fill)
        .max_width(ContentWidth::Md)
        .child(section_heading::<StartMsg>("New project"))
        .child(
            paragraph::<StartMsg>(
                "Start with a prompt, an example, or an empty composition",
                TypeRole::Body,
                Tone::Muted,
                Lines::TWO,
            )
            .priority(Priority::SHRINK),
        );
    for (label, value) in [
        ("Name", "Untitled"),
        ("Size", "1920 x 1080"),
        ("Frame rate", "60 fps"),
        ("Engine", "FerrousEngine"),
    ] {
        fields = fields.child(field(label, value));
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
                    .pad_x(resp![Xs: Spacing::S3, Md: Spacing::S5])
                    .pad_y(Spacing::S3)
                    .on_accent_fill()
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
                .grow()
                .child(content_column(recent, SizeSpec::Fraction(Split::LEADING)))
                .child(content_column(fields, SizeSpec::Fraction(Split::TRAILING))),
        )
        .child(
            Element::row()
                .key("footer")
                .gap(Spacing::S2)
                .align(Align::Center)
                .child(caption::<StartMsg>("FerrousEngine 0.1").priority(Priority::SHRINK))
                .child(Element::spacer())
                .child(caption::<StartMsg>("Apache-2.0").priority(Priority::KEEP)),
        )
}

#[cfg(test)]
mod tests {
    use super::start_screen;
    use crate::view::Tree;

    #[test]
    fn the_recipe_has_a_real_structure() {
        let tree = Tree::build(&start_screen(0.0));
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
        let view = start_screen(0.0);
        let tree = Tree::build(&view);
        let body = tree.find_key("body").expect("body");
        assert!(tree.nodes[body.index()].style.is_responsive());
        let title = tree.find_key("title").expect("title");
        assert!(tree.nodes[title.index()].style.fluid_text.is_some());
        let start = tree.find_key("start").expect("start");
        assert!(tree.nodes[start.index()].style.is_responsive());
    }

    #[test]
    fn the_clock_comes_from_the_injected_instant() {
        assert_eq!(&*crate::format::clock_hh_mm(0.0), "00:00");

        let first = Tree::build(&start_screen(0.0));
        let second = Tree::build(&start_screen(9.0 * 3_600_000.0 + 5.0 * 60_000.0));
        assert!(has_text(&first, "00:00"));
        assert!(has_text(&second, "09:05"));
        assert!(!has_text(&second, "00:00"));
    }

    fn has_text(tree: &Tree<super::StartMsg>, needle: &str) -> bool {
        tree.nodes
            .iter()
            .any(|node| node.text.as_ref().is_some_and(|text| text.text() == needle))
    }
}
