//! The phase 9 screen recipes: eight realistic screens built only from the
//! component vocabulary and the theme tokens.
//!
//! A recipe is a pure `Element<Msg>` builder, exactly like
//! [`start_screen`](crate::recipe::start_screen). It names no pixel value, no
//! colour and no radius of its own; it composes the components in
//! [`crate::components`] and the tokens in [`crate::theme`]. The gallery renders
//! each one as a story, the design lint checks it at the desktop size and across
//! the responsive sweep, and `guimaker/docs/recipes.md` quotes the builders so
//! an app can copy a whole screen.
//!
//! The launcher is the existing [`start_screen`](crate::recipe::start_screen):
//! it already is the recent-projects plus new-project screen. The seven
//! builders here are the new screens.

#![allow(clippy::too_many_lines)]
// reason: each recipe is one declarative screen and reads top to bottom

use crate::components::{
    app_shell, button, button_group, card, checkbox, command_palette, dialog, empty_state,
    form_row, icon_button, nav_item, progress, section, select, sidebar, status_bar, switch,
    text_field, textarea, toast_stack, toolbar, ButtonVariant, ControlSize, DialogSize, Status,
    Toast,
};
use crate::icons::Icon;
use crate::resp;
use crate::style::{Align, Columns, Direction, Justify, ScrollAxes, SizeSpec, Tone};
use crate::text::Ellipsis;
use crate::theme::{ColorRole, Radius, Spacing, TypeRole};
use crate::view::{text, Element};

/// One action a recipe can send.
///
/// A recipe is a view builder, not a state machine: the app keeps its own state
/// and receives these messages from the screen's shared interactions.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RecipeMsg {
    /// Create a project or item.
    Create,
    /// Cancel the dialog or the form.
    Cancel,
    /// Save the current settings.
    Apply,
    /// Open the file browser.
    Browse,
    /// Open a recent project.
    Open(u64),
    /// Open one file in the browser.
    OpenFile(u64),
    /// Switch a navigation tab or a select value.
    Select(u64),
    /// Run a toolbar or palette command.
    Run(u64),
    /// Open the settings screen.
    Settings,
    /// Open the documentation.
    Docs,
    /// Dismiss one toast.
    Dismiss(usize),
    /// Start a new project.
    NewProject,
}

/// A new-item dialog over a dimmed workspace.
///
/// The hidden base is a recent-projects grid, so the scrim and the card's
/// backdrop blur integrate a real screen instead of a flat colour.
#[must_use]
pub fn new_item_dialog() -> Element<RecipeMsg> {
    let form = Element::column()
        .align(Align::Stretch)
        .gap(Spacing::S4)
        .child(form_row(
            "Name",
            text_field(
                None,
                "Untitled",
                "Untitled",
                true,
                None,
                false,
                Some("A short, memorable name"),
                Some(Icon::File),
                None,
                None,
                RecipeMsg::Create,
            ),
            Some("Shown on the start screen"),
            true,
        ))
        .child(select(
            "Kind",
            "Composition",
            "Choose a kind",
            false,
            false,
            RecipeMsg::Select(0),
        ))
        .child(select(
            "Frame rate",
            "60 fps",
            "Choose a frame rate",
            false,
            false,
            RecipeMsg::Select(1),
        ))
        .child(form_row(
            "Notes",
            textarea("", "Optional", 2, false, None, false, RecipeMsg::Create),
            None,
            false,
        ))
        .child(checkbox(
            "Start from a template",
            false,
            false,
            RecipeMsg::Create,
        ));

    Element::stack()
        .key("new_item_dialog")
        .width(SizeSpec::Fill)
        .height(SizeSpec::Fill)
        .child(dimmed_workspace("new_item_backdrop"))
        .child(dialog(
            "New item",
            form,
            [
                button("Cancel", ButtonVariant::Secondary, RecipeMsg::Cancel),
                button("Create", ButtonVariant::Primary, RecipeMsg::Create),
            ],
            DialogSize::Md,
        ))
}

/// A settings window with a sidebar and grouped forms.
///
/// The body is a row from `Md` upwards and a stacked column below it; the
/// content is the only scroller, so the sidebar stays visible while the form
/// scrolls.
#[must_use]
pub fn settings_screen() -> Element<RecipeMsg> {
    let nav = sidebar(
        [
            nav_item(Icon::Settings, "General", true, RecipeMsg::Select(0)),
            nav_item(Icon::Image, "Appearance", false, RecipeMsg::Select(1)),
            nav_item(Icon::File, "Projects", false, RecipeMsg::Select(2)),
            nav_item(Icon::Download, "Export", false, RecipeMsg::Select(3)),
            nav_item(Icon::Bell, "Notifications", false, RecipeMsg::Select(4)),
        ],
        Some(caption("Version 0.1")),
    )
    .width(resp![Xs: SizeSpec::Fill, Md: SizeSpec::Auto]);

    let content = Element::column()
        .key("settings_content")
        .align(Align::Stretch)
        .width(SizeSpec::Fill)
        .grow()
        .gap(Spacing::S6)
        .scroll_axes(ScrollAxes::Vertical)
        .child(section(
            "GENERAL",
            "Workspace",
            [
                form_row(
                    "Name",
                    text_field(
                        None,
                        "Motioner",
                        "Motioner",
                        false,
                        None,
                        false,
                        None,
                        None,
                        None,
                        None,
                        RecipeMsg::Apply,
                    ),
                    Some("The name shown in the title bar"),
                    true,
                ),
                form_row(
                    "Language",
                    text_field(
                        None,
                        "English",
                        "English",
                        false,
                        None,
                        false,
                        None,
                        None,
                        None,
                        None,
                        RecipeMsg::Apply,
                    ),
                    None,
                    false,
                ),
            ],
        ))
        .child(section(
            "GENERAL",
            "Startup",
            [
                switch(
                    "Restore the last project",
                    true,
                    false,
                    RecipeMsg::Select(5),
                ),
                switch("Check for updates", true, false, RecipeMsg::Select(6)),
                checkbox(
                    "Open the gallery at launch",
                    false,
                    false,
                    RecipeMsg::Select(7),
                ),
            ],
        ))
        .child(section(
            "APPEARANCE",
            "Theme",
            [
                select(
                    "Mode",
                    "Dark",
                    "Choose a mode",
                    false,
                    false,
                    RecipeMsg::Select(8),
                ),
                select(
                    "Density",
                    "Comfortable",
                    "Choose a density",
                    false,
                    false,
                    RecipeMsg::Select(9),
                ),
                checkbox("Reduce motion", false, false, RecipeMsg::Select(10)),
            ],
        ));

    let body = Element::row()
        .key("settings_body")
        .align(Align::Stretch)
        .gap(Spacing::S4)
        .direction(resp![Xs: Direction::Column, Md: Direction::Row])
        .scroll_axes(resp![
            Xs: ScrollAxes::Vertical,
            Md: ScrollAxes::None
        ])
        .width(SizeSpec::Fill)
        .grow()
        .child(nav)
        .child(content);

    Element::column()
        .key("settings")
        .align(Align::Stretch)
        .width(SizeSpec::Fill)
        .height(SizeSpec::Fill)
        .bg(ColorRole::Bg)
        .child(header_row(
            "Settings",
            "Preferences for this workspace",
            [
                button("Cancel", ButtonVariant::Ghost, RecipeMsg::Cancel),
                button("Apply", ButtonVariant::Primary, RecipeMsg::Apply),
            ],
        ))
        .child(body)
}

/// A file browser with a header, a view switch and a grid of thumbnails.
#[must_use]
pub fn file_browser() -> Element<RecipeMsg> {
    let header = Element::row()
        .key("file_browser_header")
        .align(Align::Center)
        .gap(Spacing::S3)
        .pad_x(resp![Xs: Spacing::S4, Md: Spacing::S5])
        .pad_y(Spacing::S3)
        .bg(ColorRole::SurfaceRaised)
        .flow_wrap()
        .child(
            Element::column()
                .gap(Spacing::S1)
                .grow()
                .child(
                    text::<RecipeMsg>("Files")
                        .text_role(TypeRole::Title)
                        .tone(Tone::Default)
                        .priority(9)
                        .lines(1)
                        .ellipsis(Ellipsis::End),
                )
                .child(caption("Projects / Motioner")),
        )
        .child(button_group(
            [
                ("Grid", RecipeMsg::Select(0)),
                ("List", RecipeMsg::Select(1)),
            ],
            0,
        ))
        .child(button(
            "New folder",
            ButtonVariant::Primary,
            RecipeMsg::Create,
        ));

    let mut grid = Element::grid(resp![
        Xs: Columns::Fixed(1),
        Sm: Columns::Fixed(2),
        Lg: Columns::Fixed(3),
    ])
    .width(SizeSpec::Fill)
    .gap(Spacing::S4);
    for index in 0..9u64 {
        grid = grid.child(file_tile(index));
    }

    let body = Element::column()
        .key("file_browser_body")
        .align(Align::Stretch)
        .width(SizeSpec::Fill)
        .grow()
        .pad(resp![Xs: Spacing::S4, Md: Spacing::S5])
        .gap(Spacing::S4)
        .scroll_axes(ScrollAxes::Vertical)
        .child(grid);

    Element::column()
        .key("file_browser")
        .align(Align::Stretch)
        .width(SizeSpec::Fill)
        .height(SizeSpec::Fill)
        .bg(ColorRole::Bg)
        .child(header)
        .child(body)
}

/// The first-run empty state: a brand bar and one primary action.
#[must_use]
pub fn empty_first_run() -> Element<RecipeMsg> {
    let bar = toolbar(
        [text::<RecipeMsg>("Motioner")
            .text_role(TypeRole::Label)
            .tone(Tone::Default)
            .lines(1)
            .ellipsis(Ellipsis::End)],
        [button("Docs", ButtonVariant::Ghost, RecipeMsg::Docs)],
    );

    let body = Element::column()
        .key("empty_body")
        .align(Align::Center)
        .justify(Justify::Center)
        .width(SizeSpec::Fill)
        .grow()
        .pad(resp![Xs: Spacing::S4, Md: Spacing::S6])
        .child(empty_state(
            Icon::FolderOpen,
            "No projects yet",
            "Create your first composition or open a project from disk to start animating.",
            [
                button("New project", ButtonVariant::Primary, RecipeMsg::NewProject),
                button("Open project", ButtonVariant::Secondary, RecipeMsg::Browse),
            ],
        ));

    Element::column()
        .key("empty_first_run")
        .align(Align::Stretch)
        .width(SizeSpec::Fill)
        .height(SizeSpec::Fill)
        .bg(ColorRole::Bg)
        .child(bar)
        .child(body)
}

/// An editor shell: toolbar, sidebar, canvas, inspector and status bar.
#[must_use]
pub fn editor_shell() -> Element<RecipeMsg> {
    let bar = toolbar(
        [
            icon_button(
                Icon::Undo,
                "Undo",
                ButtonVariant::Ghost,
                ControlSize::Sm,
                RecipeMsg::Apply,
            ),
            icon_button(
                Icon::Redo,
                "Redo",
                ButtonVariant::Ghost,
                ControlSize::Sm,
                RecipeMsg::Apply,
            ),
            Element::divider(),
            icon_button(
                Icon::Play,
                "Play",
                ButtonVariant::Ghost,
                ControlSize::Sm,
                RecipeMsg::Run(0),
            ),
            button("Render", ButtonVariant::Primary, RecipeMsg::Run(1)),
        ],
        [
            icon_button(
                Icon::Search,
                "Search",
                ButtonVariant::Ghost,
                ControlSize::Sm,
                RecipeMsg::Select(0),
            ),
            icon_button(
                Icon::Settings,
                "Settings",
                ButtonVariant::Ghost,
                ControlSize::Sm,
                RecipeMsg::Settings,
            ),
        ],
    );

    let side = sidebar(
        [
            nav_item(Icon::Home, "Home", true, RecipeMsg::Select(0)),
            nav_item(Icon::List, "Layers", false, RecipeMsg::Select(1)),
            nav_item(Icon::Image, "Assets", false, RecipeMsg::Select(2)),
            nav_item(Icon::Filter, "Effects", false, RecipeMsg::Select(3)),
        ],
        Some(icon_button(
            Icon::Plus,
            "New layer",
            ButtonVariant::Ghost,
            ControlSize::Sm,
            RecipeMsg::Create,
        )),
    );

    let canvas = card(
        [
            text::<RecipeMsg>("Composition")
                .text_role(TypeRole::Heading)
                .tone(Tone::Default)
                .priority(1)
                .lines(1)
                .ellipsis(Ellipsis::End),
            Element::row()
                .key("editor_canvas")
                .width(SizeSpec::Fill)
                .grow()
                .bg(ColorRole::SurfaceSunken)
                .border(ColorRole::Border)
                .radius(Radius::Md)
                .clip()
                .child(
                    Element::image(3)
                        .width(SizeSpec::Fill)
                        .height(SizeSpec::Fill),
                ),
            caption("1920 x 1080 · 60 fps"),
        ],
        false,
        None,
    )
    .grow();

    let inspector = Element::column()
        .key("inspector")
        .align(Align::Stretch)
        .gap(Spacing::S3)
        .pad(Spacing::S3)
        .bg(ColorRole::Surface)
        .border(ColorRole::Border)
        .radius(Radius::Md)
        .child(
            text::<RecipeMsg>("Inspector")
                .text_role(TypeRole::Subheading)
                .tone(Tone::Default)
                .lines(1)
                .ellipsis(Ellipsis::End),
        )
        .child(select(
            "Blend",
            "Normal",
            "Choose a blend",
            false,
            false,
            RecipeMsg::Select(0),
        ))
        .child(switch("Visible", true, false, RecipeMsg::Select(1)))
        .child(checkbox("Locked", false, false, RecipeMsg::Select(2)));

    let status = status_bar(
        [caption("Ready"), caption("Layer 3 of 12")],
        [caption("60 fps")],
    );

    app_shell("Motioner", bar, side, canvas, Some(inspector), status)
}

/// A command palette over a dimmed workspace.
#[must_use]
pub fn command_palette_screen() -> Element<RecipeMsg> {
    let palette = command_palette(
        "pro",
        [
            ("New project", "Ctrl+N", RecipeMsg::NewProject),
            ("Open project", "Ctrl+O", RecipeMsg::Browse),
            ("Project settings", "Ctrl+,", RecipeMsg::Settings),
            ("Render preview", "Ctrl+E", RecipeMsg::Run(0)),
            ("Export video", "Ctrl+Shift+E", RecipeMsg::Run(1)),
            ("Toggle inspector", "Ctrl+I", RecipeMsg::Select(0)),
        ],
        1,
    );

    let centred = Element::column()
        .key("command_overlay")
        .width(SizeSpec::Fill)
        .height(SizeSpec::Fill)
        .align(Align::Stretch)
        .justify(Justify::Start)
        .pad(resp![Xs: Spacing::S4, Md: Spacing::S8])
        .child(palette);

    Element::stack()
        .key("command_palette_screen")
        .width(SizeSpec::Fill)
        .height(SizeSpec::Fill)
        .child(dimmed_workspace("command_palette_backdrop"))
        .child(scrim())
        .child(centred)
}

/// Toasts after an export, over the screen that ran the action.
#[must_use]
pub fn toasts_after_action() -> Element<RecipeMsg> {
    let base = Element::column()
        .key("toast_backdrop")
        .align(Align::Stretch)
        .width(SizeSpec::Fill)
        .height(SizeSpec::Fill)
        .bg(ColorRole::Bg)
        .pad(resp![Xs: Spacing::S4, Md: Spacing::S6])
        .gap(Spacing::S4)
        .scroll_axes(ScrollAxes::Vertical)
        .child(
            text::<RecipeMsg>("Export")
                .text_role(TypeRole::Title)
                .tone(Tone::Default)
                .priority(9)
                .lines(1)
                .ellipsis(Ellipsis::End),
        )
        .child(card(
            [
                text::<RecipeMsg>("Rendering")
                    .text_role(TypeRole::Heading)
                    .tone(Tone::Default)
                    .lines(1)
                    .ellipsis(Ellipsis::End),
                text::<RecipeMsg>("Three clips exported to out/render/")
                    .text_role(TypeRole::Body)
                    .tone(Tone::Muted)
                    .lines(2)
                    .ellipsis(Ellipsis::End),
                progress(1.0, Status::Success, false),
            ],
            false,
            None,
        ));

    let overlay = Element::row()
        .key("toast_overlay")
        .width(SizeSpec::Fill)
        .height(SizeSpec::Fill)
        .align(Align::End)
        .justify(Justify::End)
        .pad(resp![Xs: Spacing::S4, Md: Spacing::S6])
        .child(toast_stack(
            [
                Toast::new("Render finished", Status::Success).with_body("Three clips exported"),
                Toast::new("Saved", Status::Info).with_body("Project up to date"),
            ],
            RecipeMsg::Dismiss,
        ));

    Element::stack()
        .key("toasts")
        .width(SizeSpec::Fill)
        .height(SizeSpec::Fill)
        .child(base)
        .child(overlay)
}

/// The hidden recent-projects workspace behind a dialog or a palette.
///
/// Shared by [`new_item_dialog`] and [`command_palette_screen`]: both dim a
/// real screen so the scrim and the card's blur have content to integrate.
fn dimmed_workspace(key: &'static str) -> Element<RecipeMsg> {
    let mut grid = Element::grid(resp![
        Xs: Columns::Fixed(1),
        Md: Columns::Fixed(3),
    ])
    .width(SizeSpec::Fill)
    .gap(Spacing::S4);
    for (title, detail) in [
        ("Intro", "A short motion study"),
        ("Logo sting", "Brand animation"),
        ("Dashboard", "Data in motion"),
        ("Social cut", "Nine by sixteen"),
        ("Lower third", "Name and role"),
        ("Title card", "Opening sequence"),
    ] {
        grid = grid.child(card(
            [
                text::<RecipeMsg>(title)
                    .text_role(TypeRole::Label)
                    .tone(Tone::Default)
                    .lines(1)
                    .ellipsis(Ellipsis::End),
                text::<RecipeMsg>(detail)
                    .text_role(TypeRole::Caption)
                    .tone(Tone::Muted)
                    .lines(2)
                    .ellipsis(Ellipsis::End),
            ],
            false,
            None,
        ));
    }

    Element::column()
        .key(key)
        .align(Align::Stretch)
        .width(SizeSpec::Fill)
        .height(SizeSpec::Fill)
        .bg(ColorRole::Bg)
        .pad(resp![Xs: Spacing::S4, Md: Spacing::S6, Lg: Spacing::S8])
        .gap(resp![Xs: Spacing::S3, Md: Spacing::S4])
        .scroll_axes(ScrollAxes::Vertical)
        .child(
            text::<RecipeMsg>("Recent projects")
                .text_role(TypeRole::Heading)
                .tone(Tone::Default)
                .priority(9)
                .lines(1)
                .ellipsis(Ellipsis::End),
        )
        .child(grid)
}

/// One file tile: a fixed-height preview, a name and an edited caption.
///
/// The preview is a token-height image surface rather than the
/// [`thumbnail`](crate::components::thumbnail) component: a grid row sizes to
/// its tallest tile, and an aspect-ratio child measured at one width and laid
/// out at another rounds the row by a pixel. A fixed token height keeps every
/// row on the grid.
fn file_tile(index: u64) -> Element<RecipeMsg> {
    card(
        [
            Element::row()
                .key("file_preview")
                .width(SizeSpec::Fill)
                .height(SizeSpec::Token(Spacing::S24))
                .bg(ColorRole::SurfaceSunken)
                .border(ColorRole::Border)
                .radius(Radius::Md)
                .clip()
                .child(
                    Element::image(index)
                        .width(SizeSpec::Fill)
                        .height(SizeSpec::Fill),
                ),
            text::<RecipeMsg>(format!("Scene {index}"))
                .text_role(TypeRole::Label)
                .tone(Tone::Default)
                .priority(1)
                .lines(1)
                .ellipsis(Ellipsis::End),
            caption("Edited two days ago"),
        ],
        true,
        Some(RecipeMsg::OpenFile(index)),
    )
}

/// A page header: a title, a subtitle and a trailing action row.
fn header_row<const N: usize>(
    title: &'static str,
    subtitle: &'static str,
    actions: [Element<RecipeMsg>; N],
) -> Element<RecipeMsg> {
    Element::row()
        .key("page_header")
        .align(Align::Center)
        .gap(Spacing::S3)
        .pad_x(resp![Xs: Spacing::S4, Md: Spacing::S6])
        .pad_y(Spacing::S3)
        .bg(ColorRole::SurfaceRaised)
        .flow_wrap()
        .child(
            Element::column()
                .gap(Spacing::S1)
                .grow()
                .child(
                    text::<RecipeMsg>(title)
                        .text_role(TypeRole::Title)
                        .tone(Tone::Default)
                        .priority(9)
                        .lines(1)
                        .ellipsis(Ellipsis::End),
                )
                .child(caption(subtitle)),
        )
        .children(actions)
}

/// A translucent full-viewport scrim for a non-modal overlay.
fn scrim() -> Element<RecipeMsg> {
    Element::row()
        .key("scrim")
        .width(SizeSpec::Fill)
        .height(SizeSpec::Fill)
        .bg(ColorRole::Overlay)
}

/// A one-line muted caption with an end ellipsis.
fn caption(content: &'static str) -> Element<RecipeMsg> {
    text::<RecipeMsg>(content)
        .text_role(TypeRole::Caption)
        .tone(Tone::Muted)
        .priority(1)
        .lines(1)
        .ellipsis(Ellipsis::End)
}

#[cfg(test)]
mod tests {
    use super::{
        caption, command_palette_screen, dimmed_workspace, editor_shell, empty_first_run,
        file_browser, header_row, new_item_dialog, settings_screen, toasts_after_action, RecipeMsg,
    };
    use crate::view::Tree;

    fn tree(element: &crate::view::Element<RecipeMsg>) -> Tree<RecipeMsg> {
        Tree::build(element)
    }

    #[test]
    fn every_recipe_paints_a_real_structure() {
        let launcher = Tree::build(&crate::recipe::start_screen(0.0));
        assert!(
            launcher.len() > 10,
            "launcher built only {} nodes",
            launcher.len()
        );

        let screens: [(&str, crate::view::Element<RecipeMsg>); 7] = [
            ("new-item-dialog", new_item_dialog()),
            ("settings", settings_screen()),
            ("file-browser", file_browser()),
            ("empty-state", empty_first_run()),
            ("editor-shell", editor_shell()),
            ("command-palette", command_palette_screen()),
            ("toasts", toasts_after_action()),
        ];
        for (name, screen) in &screens {
            let built = tree(screen);
            assert!(built.len() > 10, "{name} built only {} nodes", built.len());
        }
    }

    #[test]
    fn the_shared_helpers_are_real_nodes() {
        assert!(!tree(&dimmed_workspace("backdrop")).is_empty());
        assert!(!tree(&caption("Caption")).is_empty());
        assert!(!tree(&header_row("Title", "Subtitle", [caption("Action")],)).is_empty());
    }
}
