//! Container components: cards, rows, tables, navigation and app frames.
//!
//! Every builder is a pure function of its data: it returns an [`Element`] and
//! names its states once, so the paint layer resolves hover, press, focus,
//! selection and disabled from the keyed store. A table is virtualised: a view
//! builds only the rows the window names.
//!
//! ```
//! use guimaker::prelude::*;
//! let view: Element<()> = card([text::<()>("Hello")], false, None);
//! assert_eq!(view.children.len(), 1);
//! ```

#![allow(clippy::too_many_lines)] // reason: one declarative builder per public container keeps the library flat
#![allow(clippy::too_many_arguments)] // reason: a container is one flat set of independent slots

use std::ops::Range;

use crate::components::{icon_box, label_text, ControlSize, StateSpec};
use crate::icons::Icon;
use crate::input::virtual_list::VirtualList;
use crate::resp;
use crate::style::{Align, ColumnTrack, Columns, Cursor, Direction, ScrollAxes, SizeSpec, Tone};
use crate::text::Ellipsis;
use crate::theme::{ColorRole, ContentWidth, Duration, ElevationLevel, Radius, Spacing, TypeRole};
use crate::view::{text, Element};

/// The uniform row height used by the virtualised table, in spacing tokens.
const TABLE_ROW_HEIGHT: f32 = Spacing::S6.px();

/// A card surface with an optional interactive state.
///
/// The card is a `Surface` with a `Border`, `Radius::Lg`, `S4` padding and
/// `E1` elevation. An interactive card lifts to `E2` and `SurfaceRaised` on
/// hover, takes the pressed and disabled roles, and carries its message.
///
/// ```
/// use guimaker::prelude::*;
/// let view: Element<()> = card([text::<()>("Hello")], true, Some(()));
/// assert!(view.action.is_some());
/// ```
#[must_use]
pub fn card<Msg: Clone>(
    children: impl IntoIterator<Item = Element<Msg>>,
    interactive: bool,
    message: Option<Msg>,
) -> Element<Msg> {
    let mut card = Element::column()
        .key("card")
        .align(Align::Stretch)
        .pad(Spacing::S4)
        .bg(ColorRole::Surface)
        .border(ColorRole::Border)
        .radius(Radius::Lg)
        .elevation(ElevationLevel::E1)
        .children(children);
    if interactive {
        card = card
            .cursor(Cursor::Pointer)
            .hover_bg(ColorRole::SurfaceRaised)
            .pressed_bg(ColorRole::SurfaceSunken)
            .selected_bg(ColorRole::AccentSubtle)
            .disabled_bg(ColorRole::Surface)
            .disabled_tone(Tone::Faint)
            .hover_elevation(ElevationLevel::E2)
            .transition(Duration::Fast)
            .interactive();
        if let Some(message) = message {
            card = card.on_click(message);
        }
    }
    card
}

/// A selectable list row with optional leading media and trailing metadata.
///
/// The row is a `Row` whose middle column holds a title and an optional
/// subtitle. A selected row uses `AccentSubtle` and the `Accent` tone; hover
/// uses `SurfaceRaised`. The row is radius `Sm` and animates with `Fast`.
///
/// ```
/// use guimaker::prelude::*;
/// let row: Element<()> = list_item(None, "File", Some("Edited"), None, false, ());
/// assert!(row.action.is_some());
/// ```
#[must_use]
pub fn list_item<Msg: Clone>(
    leading: Option<Element<Msg>>,
    title: &str,
    subtitle: Option<&str>,
    trailing: Option<Element<Msg>>,
    selected: bool,
    message: Msg,
) -> Element<Msg> {
    let size = ControlSize::Md;
    let spec = StateSpec {
        bg: None,
        hover_bg: Some(ColorRole::SurfaceRaised),
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
    };
    let mut row = spec.apply(
        Element::row()
            .align(Align::Center)
            .gap(Spacing::S2)
            .pad_x(Spacing::S3)
            .pad_y(Spacing::S1)
            .min_height(size.height())
            .radius(Radius::Sm)
            .cursor(Cursor::Pointer)
            .transition(Duration::Fast)
            .selected(selected)
            .on_click(message),
    );
    if let Some(leading) = leading {
        row = row.child(leading);
    }
    let mut column = Element::column()
        .align(Align::Stretch)
        .gap(Spacing::S1)
        .width(SizeSpec::Fill)
        .child(
            label_text(title, TypeRole::Label, spec)
                .priority(1)
                .lines(1)
                .ellipsis(Ellipsis::End),
        );
    if let Some(subtitle) = subtitle {
        column = column.child(
            label_text(subtitle, TypeRole::Caption, StateSpec::tone(Tone::Muted))
                .priority(1)
                .lines(1)
                .ellipsis(Ellipsis::End),
        );
    }
    row = row.child(column);
    if let Some(trailing) = trailing {
        row = row.child(trailing);
    }
    row
}

/// A vertical list with a `S1` gap and no surface of its own.
///
/// ```
/// use guimaker::prelude::*;
/// let view: Element<()> = list([text::<()>("One"), text::<()>("Two")]);
/// assert_eq!(view.children.len(), 2);
/// ```
#[must_use]
pub fn list<Msg: Clone>(items: impl IntoIterator<Item = Element<Msg>>) -> Element<Msg> {
    Element::column()
        .key("list")
        .align(Align::Stretch)
        .gap(Spacing::S1)
        .width(SizeSpec::Fill)
        .children(items)
}

/// How a table column takes its width.
///
/// All rows share one layout track per column, so a cell never sizes itself
/// from its own text.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub enum ColumnWidth {
    /// A fixed spacing-token width.
    Fixed(Spacing),
    /// A fraction of the table width.
    Fraction(f32),
    /// Fit the widest cell in the column.
    #[default]
    Fit,
}

/// One column of a [`TableModel`].
///
/// ```
/// use guimaker::prelude::*;
/// let column = TableColumn { title: "Name", width: ColumnWidth::Fixed(Spacing::S16), sortable: true };
/// assert!(column.sortable);
/// ```
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TableColumn {
    /// The header label.
    pub title: &'static str,
    /// How the column takes its width.
    pub width: ColumnWidth,
    /// Whether clicking the header sorts by this column.
    pub sortable: bool,
}

/// The data and virtual window of a table.
///
/// The model is a plain value the host owns: a view calls
/// [`TableModel::visible_range`] with the scroll offset and viewport height and
/// passes the result to [`table_view`], so only the visible rows are built.
///
/// ```
/// use guimaker::prelude::*;
/// let model = TableModel::new(Vec::new(), 1_000);
/// assert!(model.visible_range(0.0, 240.0).len() < 30);
/// ```
#[derive(Clone, Debug)]
pub struct TableModel {
    /// The column definitions.
    pub columns: Vec<TableColumn>,
    /// The number of rows in the model.
    pub rows: usize,
    /// The selected row, if any.
    pub selected: Option<usize>,
    /// The active sort: a column index and whether it is ascending.
    pub sort: Option<(usize, bool)>,
    /// The cell text, row-major. A missing cell falls back to the row number
    /// in column 0 and an empty string elsewhere.
    pub data: Vec<Vec<String>>,
    /// The uniform-row window arithmetic.
    visible: VirtualList,
}

impl TableModel {
    /// Builds a model over a column set and a row count.
    ///
    /// ```
    /// use guimaker::prelude::*;
    /// let model = TableModel::new([TableColumn { title: "Name", width: ColumnWidth::Fit, sortable: true }], 4);
    /// assert_eq!(model.rows, 4);
    /// ```
    #[must_use]
    pub fn new(columns: impl IntoIterator<Item = TableColumn>, rows: usize) -> Self {
        Self {
            columns: columns.into_iter().collect(),
            rows,
            selected: None,
            sort: None,
            data: Vec::new(),
            visible: VirtualList::new(rows, TABLE_ROW_HEIGHT),
        }
    }

    /// Attaches row-major cell text and sets the row count from `data`.
    ///
    /// ```
    /// use guimaker::prelude::*;
    /// let mut model = TableModel::new(Vec::new(), 0);
    /// model.with_data(vec![vec![String::from("Intro"), String::from("Scene")]]);
    /// assert_eq!(model.rows, 1);
    /// ```
    pub fn with_data(&mut self, data: Vec<Vec<String>>) {
        self.set_rows(data.len());
        self.data = data;
    }

    /// Replaces the row count and clears a selection that fell past the end.
    ///
    /// ```
    /// use guimaker::prelude::*;
    /// let mut model = TableModel::new(Vec::new(), 4);
    /// model.set_rows(8);
    /// assert_eq!(model.rows, 8);
    /// ```
    pub fn set_rows(&mut self, rows: usize) {
        self.rows = rows;
        self.visible.set_item_count(rows);
        if self.selected.is_some_and(|index| index >= rows) {
            self.selected = None;
        }
    }

    /// Sorts by `column`, toggling direction when it is already the sort key.
    ///
    /// ```
    /// use guimaker::prelude::*;
    /// let mut model = TableModel::new(Vec::new(), 4);
    /// model.sort(2);
    /// assert_eq!(model.sort, Some((2, true)));
    /// model.sort(2);
    /// assert_eq!(model.sort, Some((2, false)));
    /// ```
    pub fn sort(&mut self, column: usize) {
        self.sort = match self.sort {
            Some((current, ascending)) if current == column => Some((column, !ascending)),
            _ => Some((column, true)),
        };
    }

    /// Sorts by `column` in an explicit direction.
    ///
    /// ```
    /// use guimaker::prelude::*;
    /// let mut model = TableModel::new(Vec::new(), 4);
    /// model.sort_by(1, false);
    /// assert_eq!(model.sort, Some((1, false)));
    /// ```
    pub fn sort_by(&mut self, column: usize, ascending: bool) {
        self.sort = Some((column, ascending));
    }

    /// The overscanned rows to build for a scroll offset and viewport height.
    ///
    /// This is `VirtualList::window` exposed through the model: it is `O(1)`,
    /// allocation-free and clamps to the row count.
    ///
    /// ```
    /// use guimaker::prelude::*;
    /// let model = TableModel::new(Vec::new(), 1_000);
    /// assert!(model.visible_range(240.0, 240.0).start >= 3);
    /// ```
    #[must_use]
    pub fn visible_range(&self, offset: f32, viewport_height: f32) -> Range<usize> {
        self.visible.window(offset, viewport_height)
    }

    /// The selected row, if any.
    ///
    /// ```
    /// use guimaker::prelude::*;
    /// let mut model = TableModel::new(Vec::new(), 4);
    /// model.selected = Some(3);
    /// assert_eq!(model.selection(), Some(3));
    /// ```
    #[must_use]
    pub fn selection(&self) -> Option<usize> {
        self.selected
    }
}

/// Renders a table header and only the supplied visible rows.
///
/// This is the short name for [`table_view`]; it shares the model and range.
///
/// ```
/// use guimaker::prelude::*;
/// let mut model = TableModel::new([TableColumn { title: "Name", width: ColumnWidth::Fit, sortable: true }], 8);
/// let view: Element<()> = table(&mut model, 0..4, |_| (), |_| ());
/// assert_eq!(view.kind, NodeKind::Grid);
/// ```
#[must_use]
pub fn table<Msg: Clone>(
    model: &mut TableModel,
    visible: Range<usize>,
    header_message: impl Fn(usize) -> Msg,
    row_message: impl Fn(usize) -> Msg,
) -> Element<Msg> {
    table_view(model, visible, header_message, row_message)
}

/// Renders a table header and only the supplied visible rows.
///
/// Sortable headers emit `header_message(column)` and show a chevron; rows
/// alternate `Surface` and `SurfaceSunken`, a selected row uses `AccentSubtle`,
/// and every row emits `row_message(index)` with its own tab index.
///
/// ```
/// use guimaker::prelude::*;
/// let mut model = TableModel::new([TableColumn { title: "Name", width: ColumnWidth::Fit, sortable: true }], 20);
/// let view: Element<()> = table_view(&mut model, 0..5, |_| (), |_| ());
/// assert_eq!(view.kind, NodeKind::Grid);
/// ```
#[must_use]
pub fn table_view<Msg: Clone>(
    model: &mut TableModel,
    visible: Range<usize>,
    header_message: impl Fn(usize) -> Msg,
    row_message: impl Fn(usize) -> Msg,
) -> Element<Msg> {
    let size = ControlSize::Md;
    let row_count = model.rows;
    model.visible.set_item_count(row_count);
    let selected = model.selected;
    let sort = model.sort;
    let end = visible.end.min(row_count);
    let start = visible.start.min(end);
    // One grid, one track per column. Fixed, fraction and fit are resolved by
    // taffy for the whole table, so the header and every body cell share the
    // same x and the same width (the old per-row widths were independent).
    let tracks: Vec<ColumnTrack> = model
        .columns
        .iter()
        .map(|column| match column.width {
            ColumnWidth::Fixed(width) => ColumnTrack::Fixed(width),
            ColumnWidth::Fraction(fraction) => ColumnTrack::Fraction(fraction),
            ColumnWidth::Fit => ColumnTrack::Fit,
        })
        .collect();
    let columns = model.columns.len().max(1);
    let mut table = Element::grid(Columns::Tracks(tracks))
        .key("table")
        .align(Align::Stretch)
        .width(SizeSpec::Fill);
    for (column_index, column) in model.columns.iter().enumerate() {
        let sorted = sort.is_some_and(|(index, _)| index == column_index);
        let ascending = sort.is_some_and(|(index, ascending)| index == column_index && ascending);
        let spec = StateSpec {
            bg: Some(ColorRole::SurfaceRaised),
            hover_bg: Some(ColorRole::Surface),
            pressed_bg: Some(ColorRole::SurfaceSunken),
            selected_bg: None,
            disabled_bg: None,
            border: None,
            hover_border: None,
            tone: Tone::Muted,
            hover_tone: Some(Tone::Default),
            pressed_tone: Some(Tone::Default),
            selected_tone: Some(Tone::Accent),
            disabled_tone: Some(Tone::Faint),
            elevation_hover: None,
        };
        let mut cell = spec
            .apply(
                Element::row()
                    .key_id(u64::try_from(column_index).unwrap_or(u64::MAX))
                    .align(Align::Center)
                    .gap(Spacing::S1)
                    .pad_x(Spacing::S3)
                    .min_height(size.height())
                    .transition(Duration::Fast)
                    .selected(sorted),
            )
            .child(
                label_text(column.title, TypeRole::Label, spec)
                    .priority(1)
                    .lines(1)
                    .ellipsis(Ellipsis::End),
            );
        if column.sortable {
            cell = cell
                .cursor(Cursor::Pointer)
                .on_click(header_message(column_index));
        }
        if sorted {
            let icon = if ascending {
                Icon::ChevronUp
            } else {
                Icon::ChevronDown
            };
            cell = cell.child(icon_box(icon, size, spec));
        }
        table = table.child(cell);
    }
    for index in start..end {
        let background = if index % 2 == 0 {
            ColorRole::Surface
        } else {
            ColorRole::SurfaceSunken
        };
        let row_selected = selected == Some(index);
        let spec = StateSpec {
            bg: Some(if row_selected {
                ColorRole::AccentSubtle
            } else {
                background
            }),
            hover_bg: Some(ColorRole::SurfaceRaised),
            pressed_bg: Some(ColorRole::SurfaceSunken),
            selected_bg: Some(ColorRole::AccentSubtle),
            disabled_bg: Some(ColorRole::SurfaceSunken),
            border: None,
            hover_border: None,
            tone: Tone::Default,
            hover_tone: Some(Tone::Default),
            pressed_tone: Some(Tone::Default),
            selected_tone: Some(Tone::Default),
            disabled_tone: Some(Tone::Faint),
            elevation_hover: None,
        };
        for (column_index, _) in model.columns.iter().enumerate() {
            let row_number = index + 1;
            let label = model
                .data
                .get(index)
                .and_then(|cells| cells.get(column_index))
                .cloned()
                .unwrap_or_else(|| {
                    if column_index == 0 {
                        format!("{row_number}")
                    } else {
                        String::new()
                    }
                });
            let key = (columns as u64)
                .saturating_add(
                    u64::try_from(index)
                        .unwrap_or(u64::MAX)
                        .saturating_mul(columns as u64),
                )
                .saturating_add(column_index as u64);
            // Every cell in a body row uses the same type role; the header
            // uses Label. A per-column role made column 0 read differently.
            let cell = spec
                .apply(
                    Element::row()
                        .key_id(key)
                        .align(Align::Center)
                        .pad_x(Spacing::S3)
                        .min_height(size.height())
                        .transition(Duration::Fast)
                        .selected(row_selected)
                        .tab_index(i32::try_from(index).unwrap_or(i32::MAX))
                        .on_click(row_message(index)),
                )
                .child(
                    text::<Msg>(label)
                        .text_role(TypeRole::Body)
                        .priority(1)
                        .lines(1)
                        .ellipsis(Ellipsis::End),
                );
            table = table.child(cell);
        }
    }
    table
}

/// A row of tabs with a bottom accent bar on the selected tab.
///
/// Inactive tabs read in `Muted`, hover in `Default`, and the selected tab in
/// `Accent`; the selected tab also carries a token-sized accent bar.
///
/// ```
/// use guimaker::prelude::*;
/// let view: Element<u8> = tabs([("One", 0u8), ("Two", 1u8)], 0);
/// assert_eq!(view.children.len(), 2);
/// ```
#[must_use]
pub fn tabs<Msg: Clone>(
    items: impl IntoIterator<Item = (&'static str, Msg)>,
    selected: usize,
) -> Element<Msg> {
    let size = ControlSize::Md;
    let spec = StateSpec {
        bg: None,
        hover_bg: Some(ColorRole::SurfaceRaised),
        pressed_bg: Some(ColorRole::SurfaceSunken),
        selected_bg: None,
        disabled_bg: None,
        border: None,
        hover_border: None,
        tone: Tone::Muted,
        hover_tone: Some(Tone::Default),
        pressed_tone: Some(Tone::Default),
        selected_tone: Some(Tone::Accent),
        disabled_tone: Some(Tone::Faint),
        elevation_hover: None,
    };
    let mut row = Element::row()
        .key("tabs")
        .align(Align::End)
        .gap(Spacing::S1)
        .width(SizeSpec::Fill)
        .flow_wrap();
    for (index, (label, message)) in items.into_iter().enumerate() {
        let is_selected = index == selected;
        let mut tab = spec.apply(
            Element::column()
                .align(Align::Stretch)
                .cursor(Cursor::Pointer)
                .transition(Duration::Fast)
                .selected(is_selected)
                .on_click(message)
                .child(
                    label_text(label, size.label_role(), spec)
                        .pad_x(Spacing::S3)
                        .pad_y(Spacing::S1)
                        .min_height(size.height())
                        .radius(Radius::Sm)
                        .priority(1)
                        .lines(1)
                        .ellipsis(Ellipsis::End),
                ),
        );
        if is_selected {
            tab = tab.child(
                Element::row()
                    .width(SizeSpec::Fill)
                    .height(SizeSpec::Token(Spacing::S1))
                    .bg(ColorRole::Accent)
                    .radius(Radius::Full),
            );
        } else {
            tab = tab.child(
                Element::row()
                    .width(SizeSpec::Fill)
                    .height(SizeSpec::Token(Spacing::S1)),
            );
        }
        row = row.child(tab);
    }
    row
}

/// An accordion header with a rotating chevron and an optional open body.
///
/// ```
/// use guimaker::prelude::*;
/// let open: Element<()> = accordion("Details", text::<()>("Body"), true, ());
/// assert_eq!(open.children.len(), 2);
/// ```
#[must_use]
pub fn accordion<Msg: Clone>(
    title: &str,
    body: Element<Msg>,
    open: bool,
    toggle: Msg,
) -> Element<Msg> {
    let size = ControlSize::Md;
    let spec = StateSpec {
        bg: None,
        hover_bg: Some(ColorRole::SurfaceRaised),
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
    };
    let header = spec
        .apply(
            Element::row()
                .align(Align::Center)
                .gap(Spacing::S2)
                .pad_x(Spacing::S2)
                .pad_y(Spacing::S1)
                .min_height(size.height())
                .radius(Radius::Sm)
                .cursor(Cursor::Pointer)
                .transition(Duration::Fast)
                .selected(open)
                .on_click(toggle),
        )
        .child(icon_box(
            if open {
                Icon::ChevronDown
            } else {
                Icon::ChevronRight
            },
            size,
            spec,
        ))
        .child(
            label_text(title, size.label_role(), spec)
                .priority(1)
                .lines(1)
                .ellipsis(Ellipsis::End),
        );
    let mut section = Element::column()
        .key("accordion")
        .align(Align::Stretch)
        .gap(Spacing::S1)
        .width(SizeSpec::Fill)
        .child(header);
    if open {
        section = section.child(body);
    }
    section
}

/// A titled section: a faint overline, a heading and `S4`-spaced content.
///
/// The content column is capped at [`ContentWidth::Prose`], so prose keeps a
/// comfortable measure on a wide screen instead of running the full width.
///
/// ```
/// use guimaker::prelude::*;
/// let view: Element<()> = section("Settings", "Audio", [text::<()>("Volume")]);
/// assert_eq!(view.children.len(), 3);
/// ```
#[must_use]
pub fn section<Msg: Clone>(
    overline: &str,
    title: &str,
    children: impl IntoIterator<Item = Element<Msg>>,
) -> Element<Msg> {
    Element::column()
        .key("section")
        .align(Align::Stretch)
        .gap(Spacing::S2)
        .width(SizeSpec::Fill)
        .child(
            label_text(overline, TypeRole::Caption, StateSpec::tone(Tone::Faint))
                .priority(1)
                .lines(1)
                .ellipsis(Ellipsis::End),
        )
        .child(
            label_text(title, TypeRole::Heading, StateSpec::tone(Tone::Default))
                .priority(1)
                .lines(2),
        )
        .child(
            Element::column()
                .align(Align::Stretch)
                .gap(Spacing::S4)
                .width(SizeSpec::Fill)
                .max_width(ContentWidth::Prose)
                .children(children),
        )
}

/// A sidebar navigation row with an icon, a label and a left accent marker.
///
/// ```
/// use guimaker::prelude::*;
/// let item: Element<()> = nav_item(Icon::Home, "Home", true, ());
/// assert!(item.style.selected);
/// ```
#[must_use]
pub fn nav_item<Msg: Clone>(icon: Icon, label: &str, selected: bool, message: Msg) -> Element<Msg> {
    let size = ControlSize::Md;
    let spec = StateSpec {
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
    };
    let mut marker = Element::row()
        .width(SizeSpec::Token(Spacing::S1))
        .height(SizeSpec::Token(size.icon()))
        .radius(Radius::Full);
    if selected {
        marker = marker.bg(ColorRole::Accent);
    }
    spec.apply(
        Element::row()
            .align(Align::Center)
            .gap(Spacing::S2)
            .pad_x(Spacing::S2)
            .pad_y(Spacing::S1)
            .min_height(size.height())
            .width(SizeSpec::Fill)
            .radius(Radius::Sm)
            .cursor(Cursor::Pointer)
            .transition(Duration::Fast)
            .selected(selected)
            .on_click(message),
    )
    .child(marker)
    .child(icon_box(icon, size, spec))
    .child(
        label_text(label, size.label_role(), spec)
            .priority(1)
            .lines(1)
            .ellipsis(Ellipsis::End),
    )
}

/// A sidebar column with an optional footer.
///
/// The column is `Surface` with a `Border`, `S2` padding and an `S1` gap. It
/// does not hide itself at a breakpoint; a caller that wants a drawer below
/// `Md` adds [`Element::hidden_below`].
///
/// ```
/// use guimaker::prelude::*;
/// let view: Element<()> = sidebar([nav_item(Icon::Home, "Home", true, ())], Some(text::<()>("v1")));
/// assert_eq!(view.children.len(), 2);
/// ```
#[must_use]
pub fn sidebar<Msg: Clone>(
    items: impl IntoIterator<Item = Element<Msg>>,
    footer: Option<Element<Msg>>,
) -> Element<Msg> {
    let mut sidebar = Element::column()
        .key("sidebar")
        .align(Align::Stretch)
        .gap(Spacing::S1)
        .pad(Spacing::S2)
        .bg(ColorRole::Surface)
        .border(ColorRole::Border)
        .children(items);
    if let Some(footer) = footer {
        sidebar = sidebar.child(footer);
    }
    sidebar
}

/// A toolbar with a start group, a growing spacer and an end group.
///
/// The caller owns wrapping (`flow_wrap`) and the breakpoint at which the
/// toolbar hides; this builder sets the `SurfaceRaised` surface, the bottom
/// border, `S2` padding and gap, and the `S12` minimum height.
///
/// ```
/// use guimaker::prelude::*;
/// let view: Element<()> = toolbar([text::<()>("Start")], [text::<()>("End")]);
/// assert_eq!(view.children.len(), 3);
/// ```
#[must_use]
pub fn toolbar<Msg: Clone>(
    start: impl IntoIterator<Item = Element<Msg>>,
    end: impl IntoIterator<Item = Element<Msg>>,
) -> Element<Msg> {
    Element::row()
        .key("toolbar")
        .align(Align::Center)
        .gap(Spacing::S2)
        .pad(Spacing::S2)
        .min_height(Spacing::S12)
        .bg(ColorRole::SurfaceRaised)
        .border(ColorRole::Border)
        .child(
            Element::row()
                .align(Align::Center)
                .gap(Spacing::S2)
                .children(start),
        )
        .child(Element::spacer())
        .child(
            Element::row()
                .align(Align::Center)
                .gap(Spacing::S2)
                .children(end),
        )
}

/// A thin status row with a start group, a growing spacer and an end group.
///
/// Pass `Caption`/`Muted` children; the bar itself sets the `Surface` surface,
/// the top border, `S3` horizontal padding and the `S8` minimum height.
///
/// ```
/// use guimaker::prelude::*;
/// let view: Element<()> = status_bar([text::<()>("Ready")], [text::<()>("Ln 1")]);
/// assert_eq!(view.children.len(), 3);
/// ```
#[must_use]
pub fn status_bar<Msg: Clone>(
    start: impl IntoIterator<Item = Element<Msg>>,
    end: impl IntoIterator<Item = Element<Msg>>,
) -> Element<Msg> {
    Element::row()
        .key("status_bar")
        .align(Align::Center)
        .gap(Spacing::S2)
        .pad_x(Spacing::S3)
        .min_height(Spacing::S8)
        .bg(ColorRole::Surface)
        .border(ColorRole::Border)
        .text_role(TypeRole::Caption)
        .tone(Tone::Muted)
        .child(
            Element::row()
                .align(Align::Center)
                .gap(Spacing::S2)
                .children(start),
        )
        .child(Element::spacer())
        .child(
            Element::row()
                .align(Align::Center)
                .gap(Spacing::S2)
                .children(end),
        )
}

/// An application frame: title bar, body and status bar.
///
/// The title bar holds the app name in the `Label` role and the supplied
/// toolbar. The body is a row from `Md` upwards and a scrolling column below
/// it, so a 360 px window stacks sidebar, content and inspector instead of
/// overflowing. The content grows to fill the remaining space.
///
/// ```
/// use guimaker::prelude::*;
/// let shell: Element<()> = app_shell("App", text::<()>("bar"), text::<()>("side"), text::<()>("main"), None, text::<()>("ready"));
/// let tree = Tree::build(&shell);
/// let body = tree.find_key("app_body").expect("body");
/// assert!(tree.node(body).expect("node").style.is_responsive());
/// ```
#[must_use]
pub fn app_shell<Msg: Clone>(
    title: &str,
    toolbar: Element<Msg>,
    sidebar: Element<Msg>,
    content: Element<Msg>,
    inspector: Option<Element<Msg>>,
    status: Element<Msg>,
) -> Element<Msg> {
    let title_bar = Element::row()
        .key("app_title_bar")
        .align(Align::Center)
        .gap(Spacing::S2)
        .pad_x(Spacing::S3)
        .pad_y(Spacing::S2)
        .min_height(Spacing::S12)
        .bg(ColorRole::SurfaceRaised)
        .border(ColorRole::Border)
        .flow_wrap()
        .child(
            label_text(title, TypeRole::Label, StateSpec::tone(Tone::Default))
                .priority(9)
                .lines(1)
                .ellipsis(Ellipsis::End),
        )
        .child(toolbar.grow(1.0));
    let mut body = Element::row()
        .key("app_body")
        .align(Align::Stretch)
        .gap(Spacing::S2)
        .direction(resp![Xs: Direction::Column, Md: Direction::Row])
        .scroll_axes(resp![
            Xs: ScrollAxes::Vertical,
            Md: ScrollAxes::None
        ])
        .width(SizeSpec::Fill)
        .grow(1.0)
        .child(sidebar.width(resp![Xs: SizeSpec::Fill, Md: SizeSpec::Auto]))
        .child(content.grow(1.0).width(SizeSpec::Fill));
    if let Some(inspector) = inspector {
        body = body.child(inspector.width(resp![Xs: SizeSpec::Fill, Md: SizeSpec::Auto]));
    }
    Element::column()
        .key("app_shell")
        .align(Align::Stretch)
        .width(SizeSpec::Fill)
        .height(SizeSpec::Fill)
        .bg(ColorRole::Bg)
        .child(title_bar)
        .child(body)
        .child(status)
}

#[cfg(test)]
mod tests {
    use super::{
        accordion, app_shell, card, list, list_item, nav_item, section, sidebar, status_bar, table,
        table_view, tabs, toolbar, ColumnWidth, TableColumn, TableModel,
    };
    use crate::prelude::*;
    use crate::view::Tree;

    fn sample_columns() -> Vec<TableColumn> {
        vec![
            TableColumn {
                title: "Name",
                width: ColumnWidth::Fixed(Spacing::S16),
                sortable: true,
            },
            TableColumn {
                title: "Status",
                width: ColumnWidth::Fit,
                sortable: true,
            },
        ]
    }

    fn samples() -> Vec<Element<()>> {
        vec![
            card([text::<()>("Body")], true, Some(())),
            list_item(
                Some(Element::<()>::icon(Icon::File.path())),
                "File",
                Some("Edited"),
                Some(text::<()>("1h")),
                true,
                (),
            ),
            list([text::<()>("One"), text::<()>("Two")]),
            tabs([("One", ()), ("Two", ())], 0),
            accordion("Details", text::<()>("Body"), true, ()),
            section("Settings", "Audio", [text::<()>("Volume")]),
            nav_item(Icon::Home, "Home", true, ()),
            sidebar(
                [nav_item(Icon::Home, "Home", true, ())],
                Some(text::<()>("v1")),
            ),
            toolbar([text::<()>("Start")], [text::<()>("End")]),
            status_bar([text::<()>("Ready")], [text::<()>("Ln 1")]),
            app_shell(
                "App",
                text::<()>("bar"),
                text::<()>("side"),
                text::<()>("main"),
                Some(text::<()>("inspector")),
                text::<()>("ready"),
            ),
        ]
    }

    #[test]
    fn every_container_builds() {
        for container in samples() {
            assert!(!container.children.is_empty());
        }
    }

    #[test]
    fn interactive_rows_carry_actions() {
        let card = card([text::<()>("Body")], true, Some(()));
        assert!(card.action.is_some());
        let row = list_item(None, "File", None, None, false, ());
        assert!(row.action.is_some());
        let nav = nav_item(Icon::Home, "Home", false, ());
        assert!(nav.action.is_some());
        let tab_row = tabs([("One", ()), ("Two", ())], 0);
        assert!(tab_row.children.iter().all(|tab| tab.action.is_some()));

        let mut model = TableModel::new(sample_columns(), 40);
        let view: Element<()> = table_view(&mut model, 0..10, |_| (), |_| ());
        let tree = Tree::build(&view);
        assert!(
            tree.nodes
                .iter()
                .filter(|node| node.action.is_some())
                .count()
                > 1
        );
    }

    #[test]
    fn an_interactive_card_lifts_and_rests_flat() {
        let interactive = card::<()>([], true, Some(()));
        assert_eq!(
            interactive.style.visual.elevation_hover,
            Some(ElevationLevel::E2)
        );
        assert_eq!(
            interactive.style.visual.bg.hover,
            Some(ColorRole::SurfaceRaised)
        );
        assert_eq!(interactive.style.elevation, ElevationLevel::E1);
        let plain = card::<()>([], false, None);
        assert!(plain.action.is_none());
        assert!(plain.style.visual.elevation_hover.is_none());
    }

    #[test]
    fn a_selected_list_item_and_nav_item_take_the_selected_roles() {
        let row = list_item(None, "File", None, None, true, ());
        assert!(row.style.selected);
        assert_eq!(row.style.visual.bg.selected, Some(ColorRole::AccentSubtle));
        let nav = nav_item(Icon::Home, "Home", true, ());
        assert!(nav.style.selected);
        assert_eq!(nav.style.visual.bg.selected, Some(ColorRole::AccentSubtle));
        assert_eq!(row.style.visual.tone.selected, Some(Tone::Default));
        assert_eq!(nav.style.visual.tone.selected, Some(Tone::Default));
        assert_eq!(
            nav.children[0].style.bg,
            Some(ColorRole::Accent),
            "the left marker is visible when selected"
        );
        let plain = nav_item(Icon::Home, "Home", false, ());
        assert_eq!(plain.children[0].style.bg, None);
    }

    #[test]
    fn table_sort_toggles_and_reports_its_selection() {
        let mut model = TableModel::new(sample_columns(), 10);
        model.sort(1);
        assert_eq!(model.sort, Some((1, true)));
        model.sort(1);
        assert_eq!(model.sort, Some((1, false)));
        model.sort(0);
        assert_eq!(model.sort, Some((0, true)));
        model.sort_by(0, false);
        assert_eq!(model.sort, Some((0, false)));
        model.selected = Some(4);
        assert_eq!(model.selection(), Some(4));
    }

    #[test]
    fn table_set_rows_clamps_a_stale_selection() {
        let mut model = TableModel::new(sample_columns(), 10);
        model.selected = Some(9);
        model.set_rows(4);
        assert_eq!(model.rows, 4);
        assert_eq!(model.selection(), None);
        assert_eq!(model.visible_range(0.0, 240.0), 0..4);
    }

    #[test]
    fn a_thousand_row_table_builds_only_the_visible_window() {
        let mut model = TableModel::new(sample_columns(), 1_000);
        let window = model.visible_range(0.0, 240.0);
        assert!(window.len() < 30, "window is {window:?}");
        let view: Element<()> = table_view(&mut model, window.clone(), |_| (), |_| ());
        assert!(
            view.children.len() < 30,
            "built {} rows",
            view.children.len()
        );
        assert!(view.children.len() > 1);
        let alias: Element<()> = table(&mut model, window, |_| (), |_| ());
        assert_eq!(alias.kind, NodeKind::Grid);
    }

    #[test]
    fn the_app_shell_stacks_below_md_and_rows_from_md() {
        let shell: Element<()> = app_shell(
            "App",
            text::<()>("bar"),
            text::<()>("side"),
            text::<()>("main"),
            Some(text::<()>("inspector")),
            text::<()>("ready"),
        );
        let tree = Tree::build(&shell);
        let body = tree.find_key("app_body").expect("app body");
        let style = &tree.node(body).expect("body node").style;
        let breakpoints = Breakpoints::default();
        assert_eq!(
            *style.direction.resolve(360.0, &breakpoints),
            Direction::Column
        );
        assert_eq!(
            *style.direction.resolve(800.0, &breakpoints),
            Direction::Row
        );
        assert!(style.is_responsive());
    }

    #[test]
    fn a_closed_accordion_hides_its_body() {
        let closed: Element<()> = accordion("Details", text::<()>("Body"), false, ());
        assert_eq!(closed.children.len(), 1);
        let open: Element<()> = accordion("Details", text::<()>("Body"), true, ());
        assert_eq!(open.children.len(), 2);
    }

    #[test]
    fn a_section_caps_its_content_column() {
        let view: Element<()> = section("Settings", "Audio", [text::<()>("Volume")]);
        let content = &view.children[2];
        assert_eq!(content.style.content_width, Some(ContentWidth::Prose));
        assert_eq!(
            content.style.gap,
            crate::style::Responsive::single(Spacing::S4)
        );
    }
}
