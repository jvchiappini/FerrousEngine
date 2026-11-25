# Screen recipes

A recipe is one pure `Element<Msg>` builder for a whole screen, like
`start_screen`. It composes the components and names only theme tokens: no
colour, radius, spacing or pixel value of its own. Every recipe is a gallery
story, is lint-clean at `1280x800` and across the ten-width, four-scale sweep,
and has light and dark tolerance goldens.

Copy a builder into your app, replace `RecipeMsg` with your own message type,
and route the interactions you care about. The launcher is the existing
`start_screen`; the other seven live in `crates/guimaker/src/recipes.rs`.

| Recipe | Builder | Story | Golden stem | Use it when |
|---|---|---|---|---|
| Launcher | `start_screen(now_ms)` | `launcher` | `launcher` | The app opens on recent projects plus a new-project form. |
| New item | `new_item_dialog()` | `new-item-dialog` | `new-item-dialog` | A form belongs in a modal over the current screen. |
| Settings | `settings_screen()` | `settings` | `settings` | Preferences split into a sidebar and grouped forms. |
| File browser | `file_browser()` | `file-browser` | `file-browser` | The user picks a project or an asset from a grid. |
| Empty state | `empty_first_run()` | `empty-state` | `empty-state-screen` | A first run has nothing to show yet and one clear next step. |
| Editor shell | `editor_shell()` | `editor-shell` | `editor-shell` | A document app needs a toolbar, sidebar, canvas, inspector and status bar. |
| Command palette | `command_palette_screen()` | `command-palette` | `command-palette-screen` | Every command should be reachable from the keyboard. |
| Toasts | `toasts_after_action()` | `toasts` | `toasts` | An action finished and the result should not block the screen. |

The `empty-state` and `command-palette` names are also component story names.
The recipe goldens take a `-screen` suffix so the component goldens are never
rewritten; the recipe story names stay `empty-state` and `command-palette`.

Every screenshot uses `1280x800` at scale `1.5`, light and dark, under
`crates/guimaker_gallery/goldens/`.

## Launcher

Use it when the app opens on recent projects and a new-project form. It is the
same builder the Phase 3 start story renders: story `launcher`, goldens
`goldens/launcher-light-1280x800-1.5.png` and
`goldens/launcher-dark-1280x800-1.5.png`.

```rust
/// Builds the start screen at the injected clock instant, in milliseconds.
#[must_use]
pub fn start_screen(now_ms: f64) -> Element<StartMsg> {
    let time = clock_label(now_ms);
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
        recent = recent.child(recent_row(index, time.clone()));
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
```

Its private helpers (`content_column`, `link`, `recent_row`, `clock_label` and
`field`) are in `crates/guimaker/src/recipe.rs:207-305`; copy them with the
function.

## New item

Use it when a form belongs in a modal over the current screen. Story
`new-item-dialog`, goldens `goldens/new-item-dialog-light-1280x800-1.5.png`
and `goldens/new-item-dialog-dark-1280x800-1.5.png`.

```rust
/// A new-item dialog over a dimmed workspace.
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
```

## Settings

Use it when preferences split into a sidebar and grouped forms. Story
`settings`, goldens `goldens/settings-light-1280x800-1.5.png` and
`goldens/settings-dark-1280x800-1.5.png`.

```rust
/// A settings window with a sidebar and grouped forms.
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
        .grow(1.0)
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
        .grow(1.0)
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
```

## File browser

Use it when the user picks a project or an asset from a grid. Story
`file-browser`, goldens `goldens/file-browser-light-1280x800-1.5.png` and
`goldens/file-browser-dark-1280x800-1.5.png`.

```rust
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
                .grow(1.0)
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
        .grow(1.0)
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
```

## Empty state

Use it when a first run has nothing to show yet and one clear next step. Story
`empty-state`, goldens `goldens/empty-state-screen-light-1280x800-1.5.png` and
`goldens/empty-state-screen-dark-1280x800-1.5.png`.

```rust
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
        .grow(1.0)
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
```

## Editor shell

Use it when a document app needs a toolbar, sidebar, canvas, inspector and
status bar. Story `editor-shell`, goldens
`goldens/editor-shell-light-1280x800-1.5.png` and
`goldens/editor-shell-dark-1280x800-1.5.png`.

```rust
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
                .grow(1.0)
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
    .grow(1.0);

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
```

## Command palette

Use it when every command should be reachable from the keyboard. Story
`command-palette`, goldens
`goldens/command-palette-screen-light-1280x800-1.5.png` and
`goldens/command-palette-screen-dark-1280x800-1.5.png`.

```rust
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
```

## Toasts

Use it when an action finished and the result should not block the screen.
Story `toasts`, goldens `goldens/toasts-light-1280x800-1.5.png` and
`goldens/toasts-dark-1280x800-1.5.png`.

```rust
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
```

## Shared helpers

These private helpers are shared by two or more recipes; copy them with the
builders that use them.

```rust
/// The hidden recent-projects workspace behind a dialog or a palette.
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
                .grow(1.0)
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
```

## Regenerating a recipe golden

```text
cargo run -p guimaker_gallery -- golden <story> --size 1280x800 --scale 1.5 \
    --theme light --update --reason "why the screen changed"
```

The two collision names need `--out` because their default stem belongs to the
component golden:

```text
cargo run -p guimaker_gallery -- golden empty-state --size 1280x800 --scale 1.5 \
    --theme light --update --reason "why the screen changed" \
    --out crates/guimaker_gallery/goldens/empty-state-screen-light-1280x800-1.5.png
```
