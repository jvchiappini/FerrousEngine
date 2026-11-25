//! Adaptive pattern components: the phase 3 responsive recipes.
//!
//! Every desktop app repeats the same five layout switches. These builders
//! encode them so an app writes the structure once and gets the behaviour at
//! every container width:
//!
//! - [`collapsing_sidebar`]: full sidebar, then an icon rail, then a drawer
//!   that the app shows in an overlay (below `Sm`);
//! - [`overflowing_toolbar`]: items past the width's budget move into the
//!   caller's "More" element;
//! - [`split_panes`]: two panes side by side, stacked below `Md`;
//! - [`dialog_to_sheet`]: a centred dialog that becomes a full-screen sheet
//!   below `Sm`;
//! - [`responsive_table`]: a table that drops columns below the breakpoint
//!   they declare.
//!
//! They are structural, not skinned: each one returns a plain [`Element`] made
//! of tokens, so a story, a recipe or a test can wrap it and the design lint
//! sees through it. Icons are caller-supplied glyphs until phase 6 ships the
//! vector icon set.

// reason: the pattern builders are declarative screens with several token arguments each
#![allow(clippy::too_many_arguments)]
use crate::resp;
use crate::style::{Align, Direction, SizeSpec};
use crate::theme::{
    Breakpoint, ColorRole, ContentWidth, ElevationLevel, Radius, Spacing, TypeRole,
};
use crate::view::{text, Element};

/// A navigation row for [`collapsing_sidebar`] and [`sidebar_drawer`].
///
/// The icon and the label are one text run, so no breakpoint is involved: the
/// row ellipsises into the icon when the sidebar is a 48 px rail and shows the
/// label when the sidebar is full. `.priority(9)` keeps the row from ever
/// forcing the sidebar wider than its token width.
#[must_use]
pub fn nav_item<Msg: Clone>(icon: &'static str, label: &'static str, message: Msg) -> Element<Msg> {
    text::<Msg>(format!("{icon}  {label}"))
        .key(label)
        .text_role(TypeRole::Label)
        .tone(crate::style::Tone::Muted)
        .pad_x(Spacing::S2)
        .pad_y(Spacing::S2)
        .radius(Radius::Sm)
        .priority(9)
        .ellipsis(crate::text::Ellipsis::End)
        .lines(1)
        .on_click(message)
}

/// A sidebar that is full width, an icon rail, or absent.
///
/// - `Lg` and up: the full sidebar, with labels.
/// - `Sm` to `Md`: a 48 px icon rail (labels hidden, rows centred).
/// - below `Sm`: hidden; show [`sidebar_drawer`] in an overlay instead.
#[must_use]
pub fn collapsing_sidebar<Msg: Clone>(
    items: impl IntoIterator<Item = Element<Msg>>,
) -> Element<Msg> {
    let mut sidebar = Element::column()
        .key("sidebar")
        .align(Align::Stretch)
        .gap(Spacing::S1)
        .pad(resp![Xs: Spacing::S1, Lg: Spacing::S3])
        .width(resp![
            Xs: SizeSpec::Token(Spacing::S12),
            Lg: SizeSpec::Token(Spacing::S24)
        ])
        .bg(ColorRole::Surface)
        .border(ColorRole::Border)
        .radius(Radius::Md)
        .hidden_below(Breakpoint::Sm);
    for item in items {
        sidebar = sidebar.child(item);
    }
    sidebar
}

/// The drawer a narrow layout shows instead of the sidebar.
///
/// Call it from an overlay (`stack`) when the app's own state says the drawer
/// is open; the library does not own that state.
#[must_use]
pub fn sidebar_drawer<Msg: Clone>(items: impl IntoIterator<Item = Element<Msg>>) -> Element<Msg> {
    let mut drawer = Element::column()
        .key("sidebar_drawer")
        .align(Align::Stretch)
        .gap(Spacing::S2)
        .pad(Spacing::S4)
        .width(SizeSpec::Fraction(0.8))
        .height(SizeSpec::Fill)
        .max_width(ContentWidth::Sm)
        .bg(ColorRole::SurfaceRaised)
        .border(ColorRole::Border)
        .radius(Radius::Lg)
        .elevation(ElevationLevel::E3);
    for item in items {
        drawer = drawer.child(item);
    }
    drawer
}

/// A toolbar whose extra items become a "More" element on narrow containers.
///
/// `budgets` is how many actions stay visible at each class, `Xs` first; the
/// rest are hidden at that class and the caller's `more` element takes their
/// place until `more_from` (below it) and disappears from `more_from` upwards.
/// A toolbar that never overflows therefore passes `more_from` equal to the
/// class where every action fits.
#[must_use]
pub fn overflowing_toolbar<Msg: Clone>(
    actions: impl IntoIterator<Item = Element<Msg>>,
    more: Element<Msg>,
    budgets: [u8; Breakpoint::COUNT],
    more_from: Breakpoint,
) -> Element<Msg> {
    let mut toolbar = Element::row()
        .key("toolbar")
        .align(Align::Center)
        .gap(Spacing::S2)
        .flow_wrap();
    for (index, action) in actions.into_iter().enumerate() {
        let index = u8::try_from(index).unwrap_or(u8::MAX);
        let mut visible_from = Breakpoint::Xs;
        for breakpoint in Breakpoint::ALL {
            if budgets[breakpoint.index()] > index {
                visible_from = breakpoint;
            }
        }
        toolbar = toolbar.child(action.hidden_below(visible_from));
    }
    toolbar.child(more.hidden_above(more_from))
}

/// Two panes with a divider: side by side, stacked below `Md`.
#[must_use]
pub fn split_panes<Msg: Clone>(
    first: Element<Msg>,
    second: Element<Msg>,
    first_min: Spacing,
    second_min: Spacing,
) -> Element<Msg> {
    Element::row()
        .key("split")
        .align(Align::Stretch)
        .direction(resp![Xs: Direction::Column, Md: Direction::Row])
        .gap(Spacing::S2)
        .child(
            Element::column()
                .key("split_first")
                .grow()
                .min_width(resp![Xs: Spacing::S0, Md: first_min])
                .child(first),
        )
        .child(Element::divider())
        .child(
            Element::column()
                .key("split_second")
                .grow()
                .min_width(resp![Xs: Spacing::S0, Md: second_min])
                .child(second),
        )
}

/// A dialog that becomes a full-screen sheet below `Sm`.
///
/// Above `Sm` it is a centred card no wider than [`ContentWidth::Sm`]; below it
/// it fills the viewport with no radius, so a phone-sized window gets a sheet
/// instead of a floating card that cannot fit.
#[must_use]
pub fn dialog_to_sheet<Msg: Clone>(
    title: &'static str,
    body: Element<Msg>,
    actions: Element<Msg>,
) -> Element<Msg> {
    Element::center()
        .key("dialog")
        .pad(resp![Xs: Spacing::S0, Sm: Spacing::S6])
        .child(
            Element::column()
                .key("dialog_card")
                .gap(Spacing::S4)
                .pad(resp![Xs: Spacing::S4, Sm: Spacing::S6])
                .width(resp![Xs: SizeSpec::Fill, Sm: SizeSpec::Auto])
                .height(resp![Xs: SizeSpec::Fill, Sm: SizeSpec::Hug])
                .max_width(ContentWidth::Sm)
                .bg(ColorRole::SurfaceRaised)
                .border(ColorRole::Border)
                .radius(resp![Xs: Radius::None, Sm: Radius::Lg])
                .elevation(ElevationLevel::E3)
                .child(
                    text::<Msg>(title)
                        .key("dialog_title")
                        .text_role(TypeRole::Heading)
                        .priority(1),
                )
                .child(
                    Element::column()
                        .key("dialog_body")
                        .gap(Spacing::S2)
                        .child(body),
                )
                .child(actions),
        )
}

/// A column of a [`responsive_table`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TableColumn {
    /// The header label.
    pub title: &'static str,
    /// The container class from which the column is shown.
    pub visible_from: Breakpoint,
    /// An optional fixed column width as a spacing token.
    pub width: Option<Spacing>,
}

impl TableColumn {
    /// A column with a fixed token width.
    #[must_use]
    pub const fn new(title: &'static str, visible_from: Breakpoint, width: Spacing) -> Self {
        Self {
            title,
            visible_from,
            width: Some(width),
        }
    }

    /// A column that takes a fair share of the remaining width.
    #[must_use]
    pub const fn flex(title: &'static str, visible_from: Breakpoint) -> Self {
        Self {
            title,
            visible_from,
            width: None,
        }
    }
}

/// A table whose low-priority columns disappear below their breakpoint.
///
/// The first column is the identity column and is always shown. Every cell is
/// built from the matching [`TableColumn`], so a hidden column takes no width at
/// all: there is no empty gutter where it used to be.
#[must_use]
pub fn responsive_table<Msg: Clone>(
    columns: &[TableColumn],
    rows: impl IntoIterator<Item = Vec<&'static str>>,
) -> Element<Msg> {
    let mut table = Element::column()
        .key("table")
        .align(Align::Stretch)
        .gap(Spacing::S1)
        .width(SizeSpec::Fill);
    table = table.child(cells::<Msg>(
        columns.iter().map(|column| column.title),
        columns,
        true,
    ));
    table = table.child(Element::divider());
    for row in rows {
        table = table.child(
            Element::column()
                .gap(Spacing::S1)
                .child(cells::<Msg>(row, columns, false)),
        );
    }
    table
}

fn cells<Msg: Clone>(
    values: impl IntoIterator<Item = &'static str>,
    columns: &[TableColumn],
    header: bool,
) -> Element<Msg> {
    let mut row = Element::row()
        .align(Align::Center)
        .gap(Spacing::S3)
        .pad_y(Spacing::S1);
    for (index, value) in values.into_iter().enumerate() {
        let column = columns
            .get(index)
            .copied()
            .unwrap_or(TableColumn::flex("", Breakpoint::Xs));
        let mut cell = text::<Msg>(value)
            .priority(if index == 0 { 9 } else { 2 })
            .hidden_below(column.visible_from)
            .ellipsis(crate::text::Ellipsis::End)
            .lines(1);
        cell = match column.width {
            Some(width) => cell.width(SizeSpec::Token(width)),
            None => cell.width(SizeSpec::Fill),
        };
        cell = if header {
            cell.text_role(TypeRole::Label)
                .tone(crate::style::Tone::Muted)
        } else {
            cell.text_role(TypeRole::Body)
        };
        row = row.child(cell);
    }
    row
}

#[cfg(test)]
mod tests {
    use super::{
        collapsing_sidebar, dialog_to_sheet, nav_item, responsive_table, split_panes, TableColumn,
    };
    use crate::prelude::*;
    use crate::view::Tree;

    #[derive(Clone, Debug, PartialEq)]
    enum Msg {
        Go(u64),
    }

    fn sidebar() -> Element<Msg> {
        collapsing_sidebar([
            nav_item("[]", "Files", Msg::Go(1)),
            nav_item("()", "Search", Msg::Go(2)),
        ])
    }

    #[test]
    fn the_sidebar_hides_below_sm_and_grows_to_the_full_width() {
        let view = sidebar();
        let tree = Tree::build(&view);
        let sidebar = tree.find_key("sidebar").expect("sidebar");
        let style = &tree.nodes[sidebar.index()].style;
        assert_eq!(style.hidden_below, Some(Breakpoint::Sm));
        assert!(style.is_responsive());
        let files = tree.find_key("Files").expect("Files");
        assert_eq!(tree.nodes[files.index()].style.priority, 9);
        assert!(tree.nodes[files.index()].style.ellipsis != crate::text::Ellipsis::None);
    }

    #[test]
    fn the_dialog_card_becomes_a_sheet_below_sm() {
        let view: Element<Msg> = dialog_to_sheet("New", text("body"), Element::row());
        let tree = Tree::build(&view);
        let card = tree.find_key("dialog_card").expect("card");
        let style = &tree.nodes[card.index()].style;
        assert_eq!(style.hidden_below, None);
        assert_eq!(style.content_width, Some(ContentWidth::Sm));
        assert!(style.is_responsive());
    }

    #[test]
    fn a_table_column_hides_below_its_breakpoint() {
        let columns = [
            TableColumn::new("Name", Breakpoint::Xs, Spacing::S16),
            TableColumn::flex("Status", Breakpoint::Md),
            TableColumn::new("Time", Breakpoint::Lg, Spacing::S8),
        ];
        let view: Element<Msg> = responsive_table(&columns, [vec!["a", "b", "c"]]);
        let tree = Tree::build(&view);
        let hidden_at_lg = tree
            .nodes
            .iter()
            .filter(|node| node.style.hidden_below == Some(Breakpoint::Lg))
            .count();
        let hidden_at_md = tree
            .nodes
            .iter()
            .filter(|node| node.style.hidden_below == Some(Breakpoint::Md))
            .count();
        assert_eq!(hidden_at_lg, 2, "header and cell of the Time column");
        assert_eq!(hidden_at_md, 2, "header and cell of the Status column");
    }

    #[test]
    fn split_panes_switches_direction() {
        let view: Element<Msg> = split_panes(text("a"), text("b"), Spacing::S16, Spacing::S16);
        let tree = Tree::build(&view);
        let split = tree.find_key("split").expect("split");
        assert!(tree.nodes[split.index()].style.is_responsive());
    }
}
