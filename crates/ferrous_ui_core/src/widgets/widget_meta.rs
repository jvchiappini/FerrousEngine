// ── Widget Metadata ───────────────────────────────────────────────────────────
//
// Defines `WidgetKind`, `WidgetCategory`, and the `WIDGET_REGISTRY` constant,
// which are the single source of truth for all UI widgets that exist in the
// engine.  Tools like GUIMaker consume this instead of maintaining their own
// duplicate enums.

// ── Category ─────────────────────────────────────────────────────────────────

/// Top-level category a widget belongs to (used by editors / design tools).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum WidgetCategory {
    Basic,
    Input,
    Layout,
    Display,
    Navigation,
    Data,
    Feedback,
}

impl WidgetCategory {
    /// Human-readable name for display in editor panels.
    pub fn name(self) -> &'static str {
        match self {
            WidgetCategory::Basic => "Basic",
            WidgetCategory::Input => "Input",
            WidgetCategory::Layout => "Layout",
            WidgetCategory::Display => "Display",
            WidgetCategory::Navigation => "Navigation",
            WidgetCategory::Data => "Data",
            WidgetCategory::Feedback => "Feedback",
        }
    }

    /// Accent color for the category, as linear RGBA.
    pub fn color(self) -> [f32; 4] {
        match self {
            WidgetCategory::Basic => [0.18, 0.46, 0.71, 0.85],
            WidgetCategory::Input => [0.20, 0.63, 0.35, 0.85],
            WidgetCategory::Layout => [0.55, 0.30, 0.80, 0.85],
            WidgetCategory::Display => [0.85, 0.50, 0.10, 0.85],
            WidgetCategory::Navigation => [0.10, 0.65, 0.65, 0.85],
            WidgetCategory::Data => [0.75, 0.20, 0.50, 0.85],
            WidgetCategory::Feedback => [0.80, 0.70, 0.10, 0.85],
        }
    }
}

// ── WidgetKind ────────────────────────────────────────────────────────────────

/// Every widget kind available in the engine.
/// This is the authoritative list — editors must not define their own.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
pub enum WidgetKind {
    // Basic
    Label,
    Button,
    Panel,
    Separator,
    Spacer,
    Placeholder,
    // Input
    TextInput,
    Checkbox,
    Slider,
    NumberInput,
    ToggleSwitch,
    DropDown,
    ColorPicker,
    // Layout
    ScrollView,
    SplitPane,
    DockLayout,
    AspectRatio,
    // Display
    Image,
    Svg,
    ProgressBar,
    // Navigation / Containers
    Tabs,
    Accordion,
    TreeView,
    Modal,
    Tooltip,
    // Data
    DataTable,
    VirtualList,
    VirtualGrid,
    // Feedback
    Toast,
}

impl WidgetKind {
    /// Human-readable display name.
    pub fn display_name(self) -> &'static str {
        match self {
            WidgetKind::Label => "Label",
            WidgetKind::Button => "Button",
            WidgetKind::Panel => "Panel",
            WidgetKind::Separator => "Separator",
            WidgetKind::Spacer => "Spacer",
            WidgetKind::Placeholder => "Placeholder",
            WidgetKind::TextInput => "Text Input",
            WidgetKind::Checkbox => "Checkbox",
            WidgetKind::Slider => "Slider",
            WidgetKind::NumberInput => "Number Input",
            WidgetKind::ToggleSwitch => "Toggle Switch",
            WidgetKind::DropDown => "Drop Down",
            WidgetKind::ColorPicker => "Color Picker",
            WidgetKind::ScrollView => "Scroll View",
            WidgetKind::SplitPane => "Split Pane",
            WidgetKind::DockLayout => "Dock Layout",
            WidgetKind::AspectRatio => "Aspect Ratio",
            WidgetKind::Image => "Image",
            WidgetKind::Svg => "Svg",
            WidgetKind::ProgressBar => "Progress Bar",
            WidgetKind::Tabs => "Tabs",
            WidgetKind::Accordion => "Accordion",
            WidgetKind::TreeView => "Tree View",
            WidgetKind::Modal => "Modal",
            WidgetKind::Tooltip => "Tooltip",
            WidgetKind::DataTable => "Data Table",
            WidgetKind::VirtualList => "Virtual List",
            WidgetKind::VirtualGrid => "Virtual Grid",
            WidgetKind::Toast => "Toast",
        }
    }

    /// Category this widget belongs to.
    pub fn category(self) -> WidgetCategory {
        match self {
            WidgetKind::Label
            | WidgetKind::Button
            | WidgetKind::Panel
            | WidgetKind::Separator
            | WidgetKind::Spacer
            | WidgetKind::Placeholder => WidgetCategory::Basic,

            WidgetKind::TextInput
            | WidgetKind::Checkbox
            | WidgetKind::Slider
            | WidgetKind::NumberInput
            | WidgetKind::ToggleSwitch
            | WidgetKind::DropDown
            | WidgetKind::ColorPicker => WidgetCategory::Input,

            WidgetKind::ScrollView
            | WidgetKind::SplitPane
            | WidgetKind::DockLayout
            | WidgetKind::AspectRatio => WidgetCategory::Layout,

            WidgetKind::Image | WidgetKind::Svg | WidgetKind::ProgressBar => {
                WidgetCategory::Display
            }

            WidgetKind::Tabs
            | WidgetKind::Accordion
            | WidgetKind::TreeView
            | WidgetKind::Modal
            | WidgetKind::Tooltip => WidgetCategory::Navigation,

            WidgetKind::DataTable | WidgetKind::VirtualList | WidgetKind::VirtualGrid => {
                WidgetCategory::Data
            }

            WidgetKind::Toast => WidgetCategory::Feedback,
        }
    }

    /// Accent/tint color for this widget (inherits from its category).
    pub fn color(self) -> [f32; 4] {
        self.category().color()
    }

    /// Default size (width, height) in world/canvas units when dropped onto a
    /// design canvas.
    pub fn default_size(self) -> (f32, f32) {
        match self {
            WidgetKind::Separator => (200.0, 4.0),
            WidgetKind::Spacer => (80.0, 20.0),
            WidgetKind::Placeholder => (120.0, 60.0),
            WidgetKind::Panel => (200.0, 150.0),
            WidgetKind::ScrollView => (220.0, 180.0),
            WidgetKind::SplitPane => (300.0, 200.0),
            WidgetKind::DockLayout => (320.0, 220.0),
            WidgetKind::AspectRatio => (160.0, 90.0),
            WidgetKind::DataTable => (360.0, 200.0),
            WidgetKind::VirtualList => (200.0, 200.0),
            WidgetKind::VirtualGrid => (280.0, 200.0),
            WidgetKind::Tabs => (280.0, 160.0),
            WidgetKind::Accordion => (240.0, 120.0),
            WidgetKind::TreeView => (200.0, 180.0),
            WidgetKind::Modal => (320.0, 200.0),
            WidgetKind::Image => (120.0, 90.0),
            WidgetKind::Svg => (64.0, 64.0),
            WidgetKind::ProgressBar => (200.0, 20.0),
            WidgetKind::ColorPicker => (180.0, 220.0),
            WidgetKind::Slider => (160.0, 28.0),
            WidgetKind::ToggleSwitch => (80.0, 28.0),
            WidgetKind::DropDown => (160.0, 32.0),
            WidgetKind::Checkbox => (140.0, 28.0),
            WidgetKind::NumberInput => (120.0, 32.0),
            WidgetKind::TextInput => (160.0, 32.0),
            WidgetKind::Toast => (260.0, 48.0),
            WidgetKind::Tooltip => (140.0, 36.0),
            WidgetKind::Button => (100.0, 34.0),
            WidgetKind::Label => (100.0, 22.0),
        }
    }
}

// ── Registry ──────────────────────────────────────────────────────────────────

/// One palette category entry.
pub struct PaletteCategory {
    pub name: &'static str,
    pub widgets: &'static [WidgetKind],
}

/// The complete widget palette, ordered for display in design tools.
/// This is the single authoritative list — do not duplicate it in editor code.
pub const WIDGET_REGISTRY: &[PaletteCategory] = &[
    PaletteCategory {
        name: "Basic",
        widgets: &[
            WidgetKind::Label,
            WidgetKind::Button,
            WidgetKind::Panel,
            WidgetKind::Separator,
            WidgetKind::Spacer,
            WidgetKind::Placeholder,
        ],
    },
    PaletteCategory {
        name: "Input",
        widgets: &[
            WidgetKind::TextInput,
            WidgetKind::Checkbox,
            WidgetKind::Slider,
            WidgetKind::NumberInput,
            WidgetKind::ToggleSwitch,
            WidgetKind::DropDown,
            WidgetKind::ColorPicker,
        ],
    },
    PaletteCategory {
        name: "Layout",
        widgets: &[
            WidgetKind::ScrollView,
            WidgetKind::SplitPane,
            WidgetKind::DockLayout,
            WidgetKind::AspectRatio,
        ],
    },
    PaletteCategory {
        name: "Display",
        widgets: &[WidgetKind::Image, WidgetKind::Svg, WidgetKind::ProgressBar],
    },
    PaletteCategory {
        name: "Navigation",
        widgets: &[
            WidgetKind::Tabs,
            WidgetKind::Accordion,
            WidgetKind::TreeView,
            WidgetKind::Modal,
            WidgetKind::Tooltip,
        ],
    },
    PaletteCategory {
        name: "Data",
        widgets: &[
            WidgetKind::DataTable,
            WidgetKind::VirtualList,
            WidgetKind::VirtualGrid,
        ],
    },
    PaletteCategory {
        name: "Feedback",
        widgets: &[WidgetKind::Toast],
    },
];
