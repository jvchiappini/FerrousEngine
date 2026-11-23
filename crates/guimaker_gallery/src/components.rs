//! One gallery story per component, showing every variant and state.
//!
//! A component story is a pure `Element<()>`; the gallery paints it in light
//! and dark at the requested size and scale, exactly like the start recipe.

use guimaker::components::text as display_text;
use guimaker::icons::Icon;
use guimaker::prelude::*;
use guimaker::style::ScrollAxes;

/// A component the gallery can show on its own.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Component {
    /// Button variants, sizes, icons, loading and states.
    Button,
    /// Icon-only buttons with their mandatory tooltips.
    IconButton,
    /// Toggle buttons in both states.
    ToggleButton,
    /// Segmented control.
    ButtonGroup,
    /// Split button.
    SplitButton,
    /// Text field states.
    TextField,
    /// Number field with unit.
    NumberField,
    /// Textarea.
    Textarea,
    /// Checkbox.
    Checkbox,
    /// Radio group.
    RadioGroup,
    /// Switch.
    Switch,
    /// Slider.
    Slider,
    /// Range slider.
    RangeSlider,
    /// Select.
    Select,
    /// Combobox with search.
    Combobox,
    /// Colour swatches.
    ColorSwatch,
    /// Path field with browse.
    PathField,
    /// Form row.
    FormRow,
    /// Text and paragraph.
    Text,
    /// Headings.
    Heading,
    /// Badges.
    Badge,
    /// Tags.
    Tag,
    /// Keyboard keys.
    Kbd,
    /// Avatar and avatar group.
    Avatar,
    /// Thumbnail.
    Thumbnail,
    /// Progress bar.
    Progress,
    /// Progress ring.
    ProgressRing,
    /// Spinner.
    Spinner,
    /// Skeleton.
    Skeleton,
    /// Empty state.
    EmptyState,
    /// Callout.
    Callout,
    /// Card.
    Card,
    /// List.
    List,
    /// List item.
    ListItem,
    /// Table.
    Table,
    /// Tabs.
    Tabs,
    /// Accordion.
    Accordion,
    /// Section.
    Section,
    /// Sidebar and nav items.
    Sidebar,
    /// Toolbar.
    Toolbar,
    /// Status bar.
    StatusBar,
    /// Application shell.
    AppShell,
    /// Layer stack.
    LayerStack,
    /// Tooltip.
    Tooltip,
    /// Popover.
    Popover,
    /// Menu and context menu.
    Menu,
    /// Dialog.
    Dialog,
    /// Command palette.
    CommandPalette,
    /// Toast stack.
    Toast,
    /// The full icon set.
    Icons,
}

impl Component {
    /// Every component story.
    pub const ALL: [Self; 50] = [
        Self::Button,
        Self::IconButton,
        Self::ToggleButton,
        Self::ButtonGroup,
        Self::SplitButton,
        Self::TextField,
        Self::NumberField,
        Self::Textarea,
        Self::Checkbox,
        Self::RadioGroup,
        Self::Switch,
        Self::Slider,
        Self::RangeSlider,
        Self::Select,
        Self::Combobox,
        Self::ColorSwatch,
        Self::PathField,
        Self::FormRow,
        Self::Text,
        Self::Heading,
        Self::Badge,
        Self::Tag,
        Self::Kbd,
        Self::Avatar,
        Self::Thumbnail,
        Self::Progress,
        Self::ProgressRing,
        Self::Spinner,
        Self::Skeleton,
        Self::EmptyState,
        Self::Callout,
        Self::Card,
        Self::List,
        Self::ListItem,
        Self::Table,
        Self::Tabs,
        Self::Accordion,
        Self::Section,
        Self::Sidebar,
        Self::Toolbar,
        Self::StatusBar,
        Self::AppShell,
        Self::LayerStack,
        Self::Tooltip,
        Self::Popover,
        Self::Menu,
        Self::Dialog,
        Self::CommandPalette,
        Self::Toast,
        Self::Icons,
    ];

    /// The stable CLI name.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Button => "button",
            Self::IconButton => "icon-button",
            Self::ToggleButton => "toggle-button",
            Self::ButtonGroup => "button-group",
            Self::SplitButton => "split-button",
            Self::TextField => "text-field",
            Self::NumberField => "number-field",
            Self::Textarea => "textarea",
            Self::Checkbox => "checkbox",
            Self::RadioGroup => "radio-group",
            Self::Switch => "switch",
            Self::Slider => "slider",
            Self::RangeSlider => "range-slider",
            Self::Select => "select",
            Self::Combobox => "combobox",
            Self::ColorSwatch => "color-swatch",
            Self::PathField => "path-field",
            Self::FormRow => "form-row",
            Self::Text => "text",
            Self::Heading => "heading",
            Self::Badge => "badge",
            Self::Tag => "tag",
            Self::Kbd => "kbd",
            Self::Avatar => "avatar",
            Self::Thumbnail => "thumbnail",
            Self::Progress => "progress",
            Self::ProgressRing => "progress-ring",
            Self::Spinner => "spinner",
            Self::Skeleton => "skeleton",
            Self::EmptyState => "empty-state",
            Self::Callout => "callout",
            Self::Card => "card",
            Self::List => "list",
            Self::ListItem => "list-item",
            Self::Table => "table",
            Self::Tabs => "tabs",
            Self::Accordion => "accordion",
            Self::Section => "section",
            Self::Sidebar => "sidebar",
            Self::Toolbar => "toolbar",
            Self::StatusBar => "status-bar",
            Self::AppShell => "app-shell",
            Self::LayerStack => "layer-stack",
            Self::Tooltip => "tooltip",
            Self::Popover => "popover",
            Self::Menu => "menu",
            Self::Dialog => "dialog",
            Self::CommandPalette => "command-palette",
            Self::Toast => "toast",
            Self::Icons => "icons",
        }
    }

    /// Parses a CLI name.
    #[must_use]
    pub fn parse(name: &str) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|component| component.name() == name)
    }

    /// Builds the story view.
    #[must_use]
    pub fn view(self) -> Element<()> {
        match self {
            Self::Button => button_story(),
            Self::IconButton => icon_button_story(),
            Self::ToggleButton => toggle_button_story(),
            Self::ButtonGroup => button_group_story(),
            Self::SplitButton => split_button_story(),
            Self::TextField => text_field_story(),
            Self::NumberField => number_field_story(),
            Self::Textarea => textarea_story(),
            Self::Checkbox => checkbox_story(),
            Self::RadioGroup => radio_group_story(),
            Self::Switch => switch_story(),
            Self::Slider => slider_story(),
            Self::RangeSlider => range_slider_story(),
            Self::Select => select_story(),
            Self::Combobox => combobox_story(),
            Self::ColorSwatch => color_swatch_story(),
            Self::PathField => path_field_story(),
            Self::FormRow => form_row_story(),
            Self::Text => text_story(),
            Self::Heading => heading_story(),
            Self::Badge => badge_story(),
            Self::Tag => tag_story(),
            Self::Kbd => kbd_story(),
            Self::Avatar => avatar_story(),
            Self::Thumbnail => thumbnail_story(),
            Self::Progress => progress_story(),
            Self::ProgressRing => progress_ring_story(),
            Self::Spinner => spinner_story(),
            Self::Skeleton => skeleton_story(),
            Self::EmptyState => empty_state_story(),
            Self::Callout => callout_story(),
            Self::Card => card_story(),
            Self::List => list_story(),
            Self::ListItem => list_item_story(),
            Self::Table => table_story(),
            Self::Tabs => tabs_story(),
            Self::Accordion => accordion_story(),
            Self::Section => section_story(),
            Self::Sidebar => sidebar_story(),
            Self::Toolbar => toolbar_story(),
            Self::StatusBar => status_bar_story(),
            Self::AppShell => app_shell_story(),
            Self::LayerStack => layer_stack_story(),
            Self::Tooltip => tooltip_story(),
            Self::Popover => popover_story(),
            Self::Menu => menu_story(),
            Self::Dialog => dialog_story(),
            Self::CommandPalette => command_palette_story(),
            Self::Toast => toast_story(),
            Self::Icons => icons_story(),
        }
    }
}

fn board(title: &str, body: Element<()>) -> Element<()> {
    Element::column()
        .key("component_story")
        .width(SizeSpec::Fill)
        .height(SizeSpec::Fill)
        .bg(ColorRole::Bg)
        .pad(Spacing::S8)
        .gap(Spacing::S6)
        .scroll_axes(ScrollAxes::Vertical)
        .child(display_text::<()>(title, TypeRole::Display, Tone::Default))
        .child(body)
}

fn row(children: Vec<Element<()>>) -> Element<()> {
    Element::row()
        .gap(Spacing::S3)
        .align(Align::Center)
        .flow_wrap()
        .children(children)
}

fn column(children: Vec<Element<()>>) -> Element<()> {
    Element::column()
        .width(SizeSpec::Fill)
        .gap(Spacing::S4)
        .align(Align::Stretch)
        .children(children)
}

fn label(text: &str) -> Element<()> {
    display_text::<()>(text, TypeRole::Caption, Tone::Muted)
}

fn labelled(caption: &str, control: Element<()>) -> Element<()> {
    Element::column()
        .gap(Spacing::S1)
        .align(Align::Center)
        .child(control)
        .child(label(caption))
}

fn button_story() -> Element<()> {
    board(
        "Button",
        column(vec![
            label("Variants"),
            row(vec![
                button("Primary", ButtonVariant::Primary, ()),
                button("Secondary", ButtonVariant::Secondary, ()),
                button("Ghost", ButtonVariant::Ghost, ()),
                button("Delete", ButtonVariant::Danger, ()),
                button("Link", ButtonVariant::Link, ()),
            ]),
            label("Sizes"),
            row(vec![
                labelled(
                    "Small",
                    button_with(
                        "Small",
                        ButtonVariant::Primary,
                        None,
                        ControlSize::Sm,
                        false,
                        (),
                    ),
                ),
                labelled(
                    "Medium",
                    button_with(
                        "Medium",
                        ButtonVariant::Primary,
                        None,
                        ControlSize::Md,
                        false,
                        (),
                    ),
                ),
                labelled(
                    "Large",
                    button_with(
                        "Large",
                        ButtonVariant::Primary,
                        None,
                        ControlSize::Lg,
                        false,
                        (),
                    ),
                ),
            ]),
            label("Icons and loading"),
            row(vec![
                button_with(
                    "Save",
                    ButtonVariant::Primary,
                    Some(Icon::Check),
                    ControlSize::Md,
                    false,
                    (),
                ),
                button_with(
                    "Export",
                    ButtonVariant::Secondary,
                    Some(Icon::Download),
                    ControlSize::Md,
                    false,
                    (),
                ),
                button_with(
                    "Saving",
                    ButtonVariant::Primary,
                    None,
                    ControlSize::Md,
                    true,
                    (),
                ),
            ]),
            label("States"),
            row(vec![
                button("Disabled", ButtonVariant::Primary, ()).disabled(),
                button("Disabled ghost", ButtonVariant::Ghost, ()).disabled(),
            ]),
        ]),
    )
}

fn icon_button_story() -> Element<()> {
    let icons = [
        (Icon::Close, "Close"),
        (Icon::Search, "Search"),
        (Icon::Settings, "Settings"),
        (Icon::Plus, "Add"),
        (Icon::Trash, "Delete"),
        (Icon::More, "More"),
    ];
    let mut cells = Vec::new();
    for (icon, name) in icons {
        cells.push(labelled(
            name,
            icon_button(icon, name, ButtonVariant::Ghost, ControlSize::Md, ()),
        ));
    }
    board(
        "Icon button",
        column(vec![
            label("Every icon-only control carries a mandatory tooltip"),
            row(cells),
            label("Disabled"),
            row(vec![icon_button(
                Icon::Close,
                "Close",
                ButtonVariant::Ghost,
                ControlSize::Md,
                (),
            )
            .disabled()]),
        ]),
    )
}

fn toggle_button_story() -> Element<()> {
    board(
        "Toggle button",
        column(vec![
            label("Rest and selected"),
            row(vec![
                labelled("Off", toggle_button("Bold", false, ())),
                labelled("On", toggle_button("Bold", true, ())),
            ]),
            label("Disabled"),
            row(vec![toggle_button("Bold", false, ()).disabled()]),
        ]),
    )
}

fn button_group_story() -> Element<()> {
    board(
        "Button group",
        column(vec![
            label("Segmented control"),
            button_group([("List", ()), ("Grid", ()), ("Gallery", ())], 0),
            label("Second segment selected"),
            button_group([("List", ()), ("Grid", ()), ("Gallery", ())], 1),
        ]),
    )
}

fn split_button_story() -> Element<()> {
    board(
        "Split button",
        column(vec![
            label("Primary action and menu"),
            row(vec![split_button("Render", (), ())]),
        ]),
    )
}

fn text_field_story() -> Element<()> {
    board(
        "Text field",
        column(vec![
            text_field(
                Some("Project name"),
                "Untitled",
                "Intro.motioner",
                false,
                None,
                false,
                Some("Shown on the start screen"),
                Some(Icon::File),
                None,
                None,
                (),
            ),
            text_field(
                Some("Focused"),
                "Type a name",
                "Focused field",
                true,
                None,
                false,
                None,
                None,
                Some(Icon::Check),
                Some((0, 7)),
                (),
            ),
            text_field(
                Some("Error"),
                "Frame rate",
                "59 fps",
                false,
                Some("Use 24, 30, 60 or 120"),
                false,
                None,
                None,
                None,
                None,
                (),
            ),
            text_field(
                Some("Disabled"),
                "Engine",
                "FerrousEngine",
                false,
                None,
                true,
                None,
                None,
                None,
                None,
                (),
            ),
        ]),
    )
}

fn number_field_story() -> Element<()> {
    board(
        "Number field",
        column(vec![
            number_field("1920", Some("px"), (), (), false, None, false, ()),
            number_field("60", Some("fps"), (), (), true, None, false, ()),
            number_field(
                "0",
                Some("%"),
                (),
                (),
                false,
                Some("Must be positive"),
                false,
                (),
            ),
            number_field("1080", Some("px"), (), (), false, None, true, ()),
        ]),
    )
}

fn textarea_story() -> Element<()> {
    board(
        "Textarea",
        column(vec![
            textarea(
                "A short description of the composition.",
                "Describe the motion…",
                4,
                false,
                None,
                false,
                (),
            ),
            textarea("Focused", "Type here", 3, true, None, false, ()),
            textarea(
                "Bad",
                "Too long",
                3,
                false,
                Some("Limit the text to 200 characters"),
                false,
                (),
            ),
        ]),
    )
}

fn checkbox_story() -> Element<()> {
    board(
        "Checkbox",
        column(vec![
            checkbox("Snap to grid", true, false, ()),
            checkbox("Show guides", false, false, ()),
            checkbox("Locked", true, true, ()),
        ]),
    )
}

fn radio_group_story() -> Element<()> {
    board(
        "Radio group",
        column(vec![
            radio_group([("24 fps", ()), ("30 fps", ()), ("60 fps", ())], 1, false),
            radio_group([("Fast", ()), ("Balanced", ()), ("Quality", ())], 0, true),
        ]),
    )
}

fn switch_story() -> Element<()> {
    board(
        "Switch",
        row(vec![
            labelled("On", switch("Auto-save", true, false, ())),
            labelled("Off", switch("Auto-save", false, false, ())),
            labelled("Disabled", switch("Auto-save", true, true, ())),
        ]),
    )
}

fn slider_story() -> Element<()> {
    board(
        "Slider",
        column(vec![
            slider(0.25, "Opacity", Some("%"), false, ()),
            slider(0.6, "Scale", Some("%"), false, ()),
            slider(1.0, "Disabled", None, true, ()),
        ]),
    )
}

fn range_slider_story() -> Element<()> {
    board(
        "Range slider",
        column(vec![
            range_slider(0.2, 0.7, "Visible range", false, ()),
            range_slider(0.4, 0.8, "Disabled", true, ()),
        ]),
    )
}

fn select_story() -> Element<()> {
    board(
        "Select",
        column(vec![
            select("Group", "Shapes", "Choose a group", false, false, ()),
            select("Group", "Motion", "Choose a group", true, false, ()),
            select("Group", "", "Choose a group", false, true, ()),
        ]),
    )
}

fn combobox_story() -> Element<()> {
    board(
        "Combobox",
        column(vec![combobox(
            "Effect",
            "blur",
            [
                ("Blur", ()),
                ("Shadow", ()),
                ("Glow", ()),
                ("Motion blur", ()),
            ],
            true,
            false,
            (),
        )]),
    )
}

fn color_swatch_story() -> Element<()> {
    board(
        "Colour swatch",
        row(vec![
            labelled(
                "Accent",
                color_swatch("Accent", ColorRole::Accent, true, false, ()),
            ),
            labelled(
                "Success",
                color_swatch("Success", ColorRole::Success, false, false, ()),
            ),
            labelled(
                "Warning",
                color_swatch("Warning", ColorRole::Warning, false, false, ()),
            ),
            labelled(
                "Danger",
                color_swatch("Danger", ColorRole::Danger, false, false, ()),
            ),
            labelled(
                "Disabled",
                color_swatch("Accent", ColorRole::Accent, false, true, ()),
            ),
        ]),
    )
}

fn path_field_story() -> Element<()> {
    board(
        "Path field",
        column(vec![
            path_field("Output", "out/render/", "Choose a folder", (), false, ()),
            path_field("Font", "", "Choose a font file", (), false, ()),
            path_field("Disabled", "out/", "Choose a folder", (), true, ()),
        ]),
    )
}

fn form_row_story() -> Element<()> {
    board(
        "Form row",
        column(vec![
            form_row(
                "Name",
                text_field(
                    None,
                    "Untitled",
                    "Intro",
                    false,
                    None,
                    false,
                    None,
                    None,
                    None,
                    None,
                    (),
                ),
                Some("The project name"),
                true,
            ),
            form_row(
                "Frame rate",
                select("Rate", "60 fps", "Choose", false, false, ()),
                None,
                false,
            ),
            form_row(
                "Notes",
                textarea("", "Optional", 2, false, None, false, ()),
                Some("Shown in the project browser"),
                false,
            ),
        ]),
    )
}

fn text_story() -> Element<()> {
    board(
        "Text",
        column(vec![
            display_text::<()>(
                "The quick brown fox jumps over the lazy dog.",
                TypeRole::Body,
                Tone::Default,
            ),
            display_text::<()>(
                "Secondary text explains the primary line without competing.",
                TypeRole::Body,
                Tone::Muted,
            ),
            display_text::<()>("Tertiary hints and timestamps.", TypeRole::Caption, Tone::Faint),
            paragraph::<()>(
                "A paragraph wraps at the container width and clamps to two lines with an end ellipsis when the text is longer than the available space allows.",
                TypeRole::Body,
                Tone::Muted,
                2,
            ),
        ]),
    )
}

fn heading_story() -> Element<()> {
    board(
        "Heading",
        column(vec![
            heading::<()>("Display heading", 1, None),
            heading::<()>("Title heading", 2, Some("SECTION")),
            heading::<()>("Heading level three", 3, None),
        ]),
    )
}

fn badge_story() -> Element<()> {
    board(
        "Badge",
        row(vec![
            badge::<()>("Info", Status::Info),
            badge::<()>("Success", Status::Success),
            badge::<()>("Warning", Status::Warning),
            badge::<()>("Danger", Status::Danger),
            badge::<()>("12", Status::Info),
        ]),
    )
}

fn tag_story() -> Element<()> {
    board(
        "Tag",
        row(vec![
            tag::<()>("Motion", false, None),
            labelled("Removable", tag("Blur", true, Some(()))),
            labelled("Disabled", tag::<()>("Locked", true, None).disabled()),
        ]),
    )
}

fn kbd_story() -> Element<()> {
    board(
        "Keyboard key",
        column(vec![
            labelled("Key", kbd::<()>("K")),
            labelled("Combo", kbd_combo(["Ctrl", "Shift", "K"])),
        ]),
    )
}

fn avatar_story() -> Element<()> {
    board(
        "Avatar",
        column(vec![
            row(vec![
                labelled(
                    "Small",
                    avatar::<()>("AL", Some(Status::Info), ControlSize::Sm, None),
                ),
                labelled(
                    "Medium",
                    avatar::<()>("AL", Some(Status::Success), ControlSize::Md, None),
                ),
                labelled(
                    "Large",
                    avatar::<()>("AL", Some(Status::Warning), ControlSize::Lg, None),
                ),
            ]),
            label("Group"),
            avatar_group(["AL", "MS", "JP", "RK"], ControlSize::Md),
        ]),
    )
}

fn thumbnail_story() -> Element<()> {
    board(
        "Thumbnail",
        row(vec![
            labelled("Loaded", thumbnail::<()>(Some(1), 16.0 / 9.0, false, None)),
            labelled("Loading", thumbnail::<()>(None, 16.0 / 9.0, true, None)),
            labelled(
                "Accent",
                thumbnail::<()>(Some(2), 1.0, false, Some(Status::Success)),
            ),
        ]),
    )
}

fn progress_story() -> Element<()> {
    board(
        "Progress",
        column(vec![
            progress::<()>(0.25, Status::Info, false),
            progress::<()>(0.6, Status::Success, false),
            progress::<()>(0.85, Status::Warning, false),
            progress::<()>(1.0, Status::Danger, false),
            progress::<()>(0.4, Status::Info, true),
        ]),
    )
}

fn progress_ring_story() -> Element<()> {
    board(
        "Progress ring",
        row(vec![
            labelled(
                "25%",
                progress_ring::<()>(0.25, ControlSize::Md, Status::Info),
            ),
            labelled(
                "60%",
                progress_ring::<()>(0.6, ControlSize::Lg, Status::Success),
            ),
            labelled(
                "90%",
                progress_ring::<()>(0.9, ControlSize::Md, Status::Warning),
            ),
        ]),
    )
}

fn spinner_story() -> Element<()> {
    board(
        "Spinner",
        row(vec![
            labelled("Small", spinner::<()>(ControlSize::Sm, Tone::Muted)),
            labelled("Medium", spinner::<()>(ControlSize::Md, Tone::Accent)),
            labelled("Large", spinner::<()>(ControlSize::Lg, Tone::Default)),
        ]),
    )
}

fn skeleton_story() -> Element<()> {
    board(
        "Skeleton",
        column(vec![
            skeleton::<()>(Spacing::S24, Spacing::S3, 3),
            skeleton::<()>(Spacing::S16, Spacing::S2, 1),
        ]),
    )
}

fn empty_state_story() -> Element<()> {
    board(
        "Empty state",
        empty_state(
            Icon::FolderOpen,
            "No projects yet",
            "Create a project or open one from disk to start animating.",
            [
                button("New project", ButtonVariant::Primary, ()),
                button("Open", ButtonVariant::Secondary, ()),
            ],
        ),
    )
}

fn callout_story() -> Element<()> {
    board(
        "Callout",
        column(vec![
            callout(
                Status::Info,
                "Rendering",
                "The composition is exporting in the background.",
                [button("Details", ButtonVariant::Ghost, ())],
            ),
            callout(
                Status::Success,
                "Saved",
                "All changes are stored and the project is up to date.",
                Vec::<Element<()>>::new(),
            ),
            callout(
                Status::Warning,
                "Missing font",
                "Inter Display was replaced with Inter.",
                [button("Relink", ButtonVariant::Secondary, ())],
            ),
            callout(
                Status::Danger,
                "Render failed",
                "The output folder is not writable.",
                [button("Retry", ButtonVariant::Danger, ())],
            ),
        ]),
    )
}

fn card_story() -> Element<()> {
    board(
        "Card",
        row(vec![
            labelled(
                "Static",
                card(
                    [
                        display_text::<()>("Recent project", TypeRole::Label, Tone::Default),
                        display_text::<()>("Edited 2 minutes ago", TypeRole::Caption, Tone::Muted),
                    ],
                    false,
                    None,
                ),
            ),
            labelled(
                "Interactive",
                card(
                    [
                        display_text::<()>("Open project", TypeRole::Label, Tone::Default),
                        display_text::<()>("Hover to lift", TypeRole::Caption, Tone::Muted),
                    ],
                    true,
                    Some(()),
                ),
            ),
        ]),
    )
}

fn list_story() -> Element<()> {
    board(
        "List",
        list([
            list_item::<()>(
                Some(avatar("AL", Some(Status::Info), ControlSize::Sm, None)),
                "Intro",
                Some("Edited recently"),
                Some(kbd::<()>("Enter")),
                false,
                (),
            ),
            list_item::<()>(
                Some(avatar("MS", None, ControlSize::Sm, None)),
                "Mascot",
                Some("Edited yesterday"),
                None,
                true,
                (),
            ),
            list_item::<()>(None, "Untitled", None, None, false, ()),
        ]),
    )
}

fn list_item_story() -> Element<()> {
    board(
        "List item",
        list([
            list_item::<()>(
                Some(avatar("AL", Some(Status::Success), ControlSize::Sm, None)),
                "Intro",
                Some("Selected row"),
                Some(badge::<()>("Live", Status::Success)),
                true,
                (),
            ),
            list_item::<()>(
                Some(avatar("MS", None, ControlSize::Sm, None)),
                "Mascot",
                Some("Rest row"),
                None,
                false,
                (),
            ),
        ]),
    )
}

fn table_story() -> Element<()> {
    let columns = [
        TableColumn {
            title: "Name",
            width: ColumnWidth::Fixed(Spacing::S24),
            sortable: true,
        },
        TableColumn {
            title: "Kind",
            width: ColumnWidth::Fixed(Spacing::S24),
            sortable: true,
        },
        TableColumn {
            title: "Duration",
            width: ColumnWidth::Fraction(1.0),
            sortable: true,
        },
    ];
    let mut model = TableModel::new(columns, 0);
    model.with_data(
        (0..40)
            .map(|index| {
                vec![
                    format!("Layer {}", index + 1),
                    if index % 3 == 0 { "Shape" } else { "Group" }.to_string(),
                    format!("{}s", (index + 1) * 2 % 30),
                ]
            })
            .collect(),
    );
    model.sort_by(0, true);
    model.selected = Some(2);
    board("Table", table(&mut model, 0..8, |_| (), |_| ()))
}

fn tabs_story() -> Element<()> {
    board(
        "Tabs",
        column(vec![
            tabs([("Design", ()), ("Animate", ()), ("Export", ())], 0),
            tabs([("Design", ()), ("Animate", ()), ("Export", ())], 1),
        ]),
    )
}

fn accordion_story() -> Element<()> {
    board(
        "Accordion",
        column(vec![
            accordion(
                "Composition",
                display_text::<()>(
                    "Width, height, frame rate and duration.",
                    TypeRole::Body,
                    Tone::Muted,
                ),
                true,
                (),
            ),
            accordion(
                "Colour",
                display_text::<()>("Background and guides.", TypeRole::Body, Tone::Muted),
                false,
                (),
            ),
        ]),
    )
}

fn section_story() -> Element<()> {
    board(
        "Section",
        section(
            "PROJECT",
            "Composition",
            [
                form_row(
                    "Name",
                    text_field(
                        None,
                        "Untitled",
                        "Intro",
                        false,
                        None,
                        false,
                        None,
                        None,
                        None,
                        None,
                        (),
                    ),
                    None,
                    true,
                ),
                form_row(
                    "Size",
                    text_field(
                        None,
                        "1920 x 1080",
                        "1920 x 1080",
                        false,
                        None,
                        false,
                        None,
                        None,
                        None,
                        None,
                        (),
                    ),
                    None,
                    false,
                ),
            ],
        ),
    )
}

fn sidebar_story() -> Element<()> {
    board(
        "Sidebar",
        sidebar(
            [
                nav_item(Icon::Home, "Home", true, ()),
                nav_item(Icon::List, "Layers", false, ()),
                nav_item(Icon::Image, "Assets", false, ()),
                nav_item(Icon::Settings, "Settings", false, ()),
            ],
            Some(button("Collapse", ButtonVariant::Ghost, ())),
        ),
    )
}

fn toolbar_story() -> Element<()> {
    board(
        "Toolbar",
        toolbar(
            [
                icon_button(
                    Icon::Undo,
                    "Undo",
                    ButtonVariant::Ghost,
                    ControlSize::Sm,
                    (),
                ),
                icon_button(
                    Icon::Redo,
                    "Redo",
                    ButtonVariant::Ghost,
                    ControlSize::Sm,
                    (),
                ),
                Element::divider(),
                button("Render", ButtonVariant::Primary, ()),
            ],
            [
                icon_button(
                    Icon::Search,
                    "Search",
                    ButtonVariant::Ghost,
                    ControlSize::Sm,
                    (),
                ),
                icon_button(
                    Icon::Settings,
                    "Settings",
                    ButtonVariant::Ghost,
                    ControlSize::Sm,
                    (),
                ),
            ],
        ),
    )
}

fn status_bar_story() -> Element<()> {
    board(
        "Status bar",
        status_bar(
            [
                display_text::<()>("Ready", TypeRole::Caption, Tone::Muted),
                display_text::<()>("60 fps", TypeRole::Caption, Tone::Faint),
            ],
            [display_text::<()>(
                "FerrousEngine 0.1",
                TypeRole::Caption,
                Tone::Faint,
            )],
        ),
    )
}

fn app_shell_story() -> Element<()> {
    let toolbar = toolbar(
        [
            button("New", ButtonVariant::Secondary, ()),
            button("Open", ButtonVariant::Ghost, ()),
        ],
        [icon_button(
            Icon::Settings,
            "Settings",
            ButtonVariant::Ghost,
            ControlSize::Sm,
            (),
        )],
    );
    let sidebar = sidebar(
        [
            nav_item(Icon::Home, "Home", true, ()),
            nav_item(Icon::List, "Layers", false, ()),
            nav_item(Icon::Image, "Assets", false, ()),
        ],
        None,
    );
    let content = card(
        [
            display_text::<()>("Canvas", TypeRole::Heading, Tone::Default),
            display_text::<()>("The composition renders here.", TypeRole::Body, Tone::Muted),
        ],
        false,
        None,
    )
    .grow(1.0);
    let inspector = card(
        [
            display_text::<()>("Inspector", TypeRole::Label, Tone::Default),
            form_row(
                "Name",
                text_field(
                    None,
                    "Untitled",
                    "Intro",
                    false,
                    None,
                    false,
                    None,
                    None,
                    None,
                    None,
                    (),
                ),
                None,
                false,
            ),
        ],
        false,
        None,
    )
    .width(SizeSpec::Token(Spacing::S24));
    let status = status_bar(
        [display_text::<()>("Ready", TypeRole::Caption, Tone::Muted)],
        [display_text::<()>(
            "1920 x 1080",
            TypeRole::Caption,
            Tone::Faint,
        )],
    );
    board(
        "Application shell",
        app_shell(
            "Motioner",
            toolbar,
            sidebar,
            content,
            Some(inspector),
            status,
        ),
    )
}

fn menu_items() -> Vec<MenuItem<()>> {
    vec![
        MenuItem::item("New project", ()).with_shortcut("Ctrl+N"),
        MenuItem::item("Open…", ()).with_shortcut("Ctrl+O"),
        MenuItem::separator(),
        MenuItem::item("Export", ()).with_submenu(vec![
            MenuItem::item("PNG", ()).with_shortcut("Ctrl+E"),
            MenuItem::item("Video", ()),
        ]),
        MenuItem::checkable("Snapping", true, ()),
        MenuItem::checkable("Guides", false, ()),
        MenuItem::separator(),
        MenuItem::item("Settings", ()).with_shortcut("Ctrl+,"),
    ]
}

fn layer_stack_story() -> Element<()> {
    layer_stack(
        card(
            [
                display_text::<()>("Base layer", TypeRole::Label, Tone::Default),
                display_text::<()>("The overlay sits above it.", TypeRole::Caption, Tone::Muted),
            ],
            false,
            None,
        ),
        [menu(&menu_items(), Some(3))],
    )
}

fn tooltip_story() -> Element<()> {
    board(
        "Tooltip",
        tooltip(
            button("Hover target", ButtonVariant::Secondary, ()),
            "Saves the current project",
            true,
            Placement::Bottom,
        ),
    )
}

fn popover_story() -> Element<()> {
    board(
        "Popover",
        popover(
            button("Open popover", ButtonVariant::Secondary, ()),
            column(vec![
                display_text::<()>("Quick settings", TypeRole::Label, Tone::Default),
                display_text::<()>(
                    "Snap to grid and show guides.",
                    TypeRole::Caption,
                    Tone::Muted,
                ),
            ]),
            true,
            Placement::Bottom,
        ),
    )
}

fn menu_story() -> Element<()> {
    board(
        "Menu",
        column(vec![
            labelled("Menu", menu(&menu_items(), Some(3))),
            labelled("Context menu", context_menu(&menu_items(), None)),
        ]),
    )
}

fn dialog_story() -> Element<()> {
    // The panels are deliberately plain: any text below the card lands in the
    // prefix target, and the sparse nine-tap blur echoes sharp glyphs across
    // the card interior. Coloured panels keep the backdrop a pure colour wash.
    let panels = [
        ColorRole::SurfaceRaised,
        ColorRole::SurfaceSunken,
        ColorRole::AccentSubtle,
        ColorRole::SuccessSubtle,
        ColorRole::WarningSubtle,
        ColorRole::InfoSubtle,
    ];
    let mut cards = Vec::with_capacity(panels.len());
    for role in panels {
        cards.push(
            Element::column()
                .grow(1.0)
                .height(SizeSpec::Fill)
                .align(Align::Stretch)
                .pad(Spacing::S4)
                .bg(role)
                .radius(Radius::Md),
        );
    }
    let mut rows = Vec::new();
    for chunk in cards.chunks(3) {
        rows.push(
            Element::row()
                .width(SizeSpec::Fill)
                .height(SizeSpec::Fill)
                .align(Align::Stretch)
                .gap(Spacing::S4)
                .children(chunk.iter().cloned()),
        );
    }
    let background = Element::column()
        .key("dialog_background")
        .width(SizeSpec::Fill)
        .height(SizeSpec::Fill)
        .gap(Spacing::S4)
        .pad(Spacing::S6)
        .children(rows);
    Element::stack()
        .key("dialog_story")
        .width(SizeSpec::Fill)
        .height(SizeSpec::Fill)
        .child(background)
        .child(dialog(
            "Rename project",
            text_field(
                Some("Name"),
                "Untitled",
                "Intro",
                true,
                None,
                false,
                Some("A short, memorable name"),
                None,
                None,
                None,
                (),
            ),
            [
                button("Cancel", ButtonVariant::Secondary, ()),
                button("Rename", ButtonVariant::Primary, ()),
            ],
            DialogSize::Sm,
        ))
}

fn command_palette_story() -> Element<()> {
    board(
        "Command palette",
        command_palette(
            "pro",
            [
                ("New project", "Ctrl+N", ()),
                ("Open project", "Ctrl+O", ()),
                ("Project settings", "Ctrl+,", ()),
                ("Render preview", "Ctrl+E", ()),
                ("Export video", "Ctrl+Shift+E", ()),
            ],
            1,
        ),
    )
}

fn toast_story() -> Element<()> {
    board(
        "Toast",
        toast_stack(
            [
                Toast::new("Saved", Status::Success).with_body("All changes stored"),
                Toast::new("Render finished", Status::Info),
                Toast::new("Missing font", Status::Warning),
            ],
            |_| (),
        ),
    )
}

fn icons_story() -> Element<()> {
    let mut cells = Vec::with_capacity(Icon::ALL.len());
    for icon in Icon::ALL {
        cells.push(
            Element::column()
                .gap(Spacing::S1)
                .align(Align::Center)
                .tone(Tone::Muted)
                .width(SizeSpec::Token(Spacing::S16))
                .child(
                    Element::<()>::icon(icon.path())
                        .width(SizeSpec::Token(Spacing::S5))
                        .height(SizeSpec::Token(Spacing::S5))
                        .align_self(Align::Center),
                )
                .child(
                    display_text::<()>(icon.label(), TypeRole::Caption, Tone::Muted)
                        .lines(1)
                        .ellipsis(Ellipsis::End),
                ),
        );
    }
    board(
        "Icons",
        Element::row()
            .flow_wrap()
            .gap(Spacing::S4)
            .align(Align::Start)
            .children(cells),
    )
}

#[cfg(test)]
mod tests {
    use super::Component;
    use guimaker::view::Tree;

    #[test]
    fn every_component_builds_and_has_a_unique_name() {
        let mut names = Vec::new();
        for component in Component::ALL {
            let tree = Tree::build(&component.view());
            assert!(tree.len() > 1, "{} built only the root", component.name());
            names.push(component.name());
            assert_eq!(Component::parse(component.name()), Some(component));
        }
        names.sort_unstable();
        names.dedup();
        assert_eq!(names.len(), Component::ALL.len());
    }

    #[test]
    fn the_icon_story_shows_every_icon() {
        let tree = Tree::build(&Component::Icons.view());
        assert!(tree.len() > guimaker::icons::Icon::ALL.len());
    }

    #[test]
    fn the_dialog_card_requests_a_real_backdrop_blur() {
        let tree = Tree::build(&Component::Dialog.view());
        assert!(
            tree.nodes
                .iter()
                .any(|node| node.style.backdrop_blur.is_some()),
            "the dialog card has no backdrop blur"
        );
    }
}
