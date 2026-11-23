//! The shared vector icon set, designed on a 16-unit grid.
//!
//! Icons are filled SVG paths scaled into the element box, so they stay crisp at
//! any DPI. The set is engine-agnostic: the backend turns each path into
//! `DrawCommand::Icon`. Motioner imports this enum instead of keeping a copy.
//!
//! ```
//! use guimaker::icons::Icon;
//! assert!(Icon::Search.path().starts_with('M'));
//! assert_eq!(Icon::ALL.len(), 79);
//! ```

/// Every icon a desktop app needs.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Icon {
    /// Play.
    Play,
    /// Pause.
    Pause,
    /// Step back.
    StepBack,
    /// Step forward.
    StepForward,
    /// Jump to start.
    ToStart,
    /// Jump to end.
    ToEnd,
    /// Loop.
    Loop,
    /// Expand.
    ChevronDown,
    /// Collapse.
    ChevronUp,
    /// Next.
    ChevronRight,
    /// Previous.
    ChevronLeft,
    /// Back.
    ArrowLeft,
    /// Forward.
    ArrowRight,
    /// Up.
    ArrowUp,
    /// Down.
    ArrowDown,
    /// Refresh.
    Refresh,
    /// Undo.
    Undo,
    /// Redo.
    Redo,
    /// Visible.
    Eye,
    /// Hidden.
    EyeOff,
    /// Locked.
    Lock,
    /// Unlocked.
    Unlock,
    /// Text.
    Text,
    /// Ellipse.
    Circle,
    /// Rectangle.
    Rect,
    /// Vector path.
    Path,
    /// Group.
    Group,
    /// Keyframed.
    Stopwatch,
    /// Snapping.
    Snap,
    /// Mask.
    Mask,
    /// Image.
    Image,
    /// Audio.
    Audio,
    /// Video.
    Video,
    /// Video clip.
    Film,
    /// Music.
    Music,
    /// Font.
    Font,
    /// List.
    List,
    /// Grid.
    Grid,
    /// Remove.
    Clean,
    /// Delete.
    Trash,
    /// Muted.
    Mute,
    /// Solo.
    Solo,
    /// Filter.
    Filter,
    /// Shadow.
    Shadow,
    /// Blur.
    Blur,
    /// Glow.
    Glow,
    /// Check.
    CheckMark,
    /// Selected.
    Dot,
    /// Warning.
    Warning,
    /// Error.
    Error,
    /// Done.
    Check,
    /// Time.
    Clock,
    /// Star.
    Star,
    /// More.
    More,
    /// Close.
    Close,
    /// Add.
    Plus,
    /// Remove.
    Minus,
    /// Search.
    Search,
    /// Settings.
    Settings,
    /// File.
    File,
    /// Folder.
    Folder,
    /// Open folder.
    FolderOpen,
    /// Home.
    Home,
    /// Download.
    Download,
    /// Upload.
    Upload,
    /// Edit.
    Edit,
    /// Copy.
    Copy,
    /// Link.
    Link,
    /// Date.
    Calendar,
    /// Mail.
    Mail,
    /// User.
    User,
    /// Notifications.
    Bell,
    /// Pin.
    Pin,
    /// Open externally.
    External,
    /// Command.
    Command,
    /// Save.
    Save,
    /// Zoom in.
    ZoomIn,
    /// Zoom out.
    ZoomOut,
    /// Safe area.
    SafeArea,
}

impl Icon {
    /// Every icon, in registry order.
    pub const ALL: [Self; 79] = [
        Self::Play,
        Self::Pause,
        Self::StepBack,
        Self::StepForward,
        Self::ToStart,
        Self::ToEnd,
        Self::Loop,
        Self::ChevronDown,
        Self::ChevronUp,
        Self::ChevronRight,
        Self::ChevronLeft,
        Self::ArrowLeft,
        Self::ArrowRight,
        Self::ArrowUp,
        Self::ArrowDown,
        Self::Refresh,
        Self::Undo,
        Self::Redo,
        Self::Eye,
        Self::EyeOff,
        Self::Lock,
        Self::Unlock,
        Self::Text,
        Self::Circle,
        Self::Rect,
        Self::Path,
        Self::Group,
        Self::Stopwatch,
        Self::Snap,
        Self::Mask,
        Self::Image,
        Self::Audio,
        Self::Video,
        Self::Film,
        Self::Music,
        Self::Font,
        Self::List,
        Self::Grid,
        Self::Clean,
        Self::Trash,
        Self::Mute,
        Self::Solo,
        Self::Filter,
        Self::Shadow,
        Self::Blur,
        Self::Glow,
        Self::CheckMark,
        Self::Dot,
        Self::Warning,
        Self::Error,
        Self::Check,
        Self::Clock,
        Self::Star,
        Self::More,
        Self::Close,
        Self::Plus,
        Self::Minus,
        Self::Search,
        Self::Settings,
        Self::File,
        Self::Folder,
        Self::FolderOpen,
        Self::Home,
        Self::Download,
        Self::Upload,
        Self::Edit,
        Self::Copy,
        Self::Link,
        Self::Calendar,
        Self::Mail,
        Self::User,
        Self::Bell,
        Self::Pin,
        Self::External,
        Self::Command,
        Self::Save,
        Self::ZoomIn,
        Self::ZoomOut,
        Self::SafeArea,
    ];

    /// The stable CLI/debug name.
    #[allow(clippy::match_same_arms)] // reason: names are unique in practice; the allow keeps the table flat
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Play => "Play",
            Self::Pause => "Pause",
            Self::StepBack => "StepBack",
            Self::StepForward => "StepForward",
            Self::ToStart => "ToStart",
            Self::ToEnd => "ToEnd",
            Self::Loop => "Loop",
            Self::ChevronDown => "ChevronDown",
            Self::ChevronUp => "ChevronUp",
            Self::ChevronRight => "ChevronRight",
            Self::ChevronLeft => "ChevronLeft",
            Self::ArrowLeft => "ArrowLeft",
            Self::ArrowRight => "ArrowRight",
            Self::ArrowUp => "ArrowUp",
            Self::ArrowDown => "ArrowDown",
            Self::Refresh => "Refresh",
            Self::Undo => "Undo",
            Self::Redo => "Redo",
            Self::Eye => "Eye",
            Self::EyeOff => "EyeOff",
            Self::Lock => "Lock",
            Self::Unlock => "Unlock",
            Self::Text => "Text",
            Self::Circle => "Circle",
            Self::Rect => "Rect",
            Self::Path => "Path",
            Self::Group => "Group",
            Self::Stopwatch => "Stopwatch",
            Self::Snap => "Snap",
            Self::Mask => "Mask",
            Self::Image => "Image",
            Self::Audio => "Audio",
            Self::Video => "Video",
            Self::Film => "Film",
            Self::Music => "Music",
            Self::Font => "Font",
            Self::List => "List",
            Self::Grid => "Grid",
            Self::Clean => "Clean",
            Self::Trash => "Trash",
            Self::Mute => "Mute",
            Self::Solo => "Solo",
            Self::Filter => "Filter",
            Self::Shadow => "Shadow",
            Self::Blur => "Blur",
            Self::Glow => "Glow",
            Self::CheckMark => "CheckMark",
            Self::Dot => "Dot",
            Self::Warning => "Warning",
            Self::Error => "Error",
            Self::Check => "Check",
            Self::Clock => "Clock",
            Self::Star => "Star",
            Self::More => "More",
            Self::Close => "Close",
            Self::Plus => "Plus",
            Self::Minus => "Minus",
            Self::Search => "Search",
            Self::Settings => "Settings",
            Self::File => "File",
            Self::Folder => "Folder",
            Self::FolderOpen => "FolderOpen",
            Self::Home => "Home",
            Self::Download => "Download",
            Self::Upload => "Upload",
            Self::Edit => "Edit",
            Self::Copy => "Copy",
            Self::Link => "Link",
            Self::Calendar => "Calendar",
            Self::Mail => "Mail",
            Self::User => "User",
            Self::Bell => "Bell",
            Self::Pin => "Pin",
            Self::External => "External",
            Self::Command => "Command",
            Self::Save => "Save",
            Self::ZoomIn => "ZoomIn",
            Self::ZoomOut => "ZoomOut",
            Self::SafeArea => "SafeArea",
        }
    }

    /// A human label, also the default tooltip for an icon-only control.
    #[allow(clippy::match_same_arms)] // reason: two icons can share a human label
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Play => "Play",
            Self::Pause => "Pause",
            Self::StepBack => "Step back",
            Self::StepForward => "Step forward",
            Self::ToStart => "Jump to start",
            Self::ToEnd => "Jump to end",
            Self::Loop => "Loop",
            Self::ChevronDown => "Expand",
            Self::ChevronUp => "Collapse",
            Self::ChevronRight => "Next",
            Self::ChevronLeft => "Previous",
            Self::ArrowLeft => "Back",
            Self::ArrowRight => "Forward",
            Self::ArrowUp => "Up",
            Self::ArrowDown => "Down",
            Self::Refresh => "Refresh",
            Self::Undo => "Undo",
            Self::Redo => "Redo",
            Self::Eye => "Visible",
            Self::EyeOff => "Hidden",
            Self::Lock => "Locked",
            Self::Unlock => "Unlocked",
            Self::Text => "Text",
            Self::Circle => "Ellipse",
            Self::Rect => "Rectangle",
            Self::Path => "Vector path",
            Self::Group => "Group",
            Self::Stopwatch => "Keyframed",
            Self::Snap => "Snapping",
            Self::Mask => "Mask",
            Self::Image => "Image",
            Self::Audio => "Audio",
            Self::Video => "Video",
            Self::Film => "Video clip",
            Self::Music => "Music",
            Self::Font => "Font",
            Self::List => "List",
            Self::Grid => "Grid",
            Self::Clean => "Clear",
            Self::Trash => "Delete",
            Self::Mute => "Muted",
            Self::Solo => "Solo",
            Self::Filter => "Filter",
            Self::Shadow => "Shadow",
            Self::Blur => "Blur",
            Self::Glow => "Glow",
            Self::CheckMark => "Check",
            Self::Dot => "Selected",
            Self::Warning => "Warning",
            Self::Error => "Error",
            Self::Check => "Done",
            Self::Clock => "Time",
            Self::Star => "Star",
            Self::More => "More",
            Self::Close => "Close",
            Self::Plus => "Add",
            Self::Minus => "Remove",
            Self::Search => "Search",
            Self::Settings => "Settings",
            Self::File => "File",
            Self::Folder => "Folder",
            Self::FolderOpen => "Open folder",
            Self::Home => "Home",
            Self::Download => "Download",
            Self::Upload => "Upload",
            Self::Edit => "Edit",
            Self::Copy => "Copy",
            Self::Link => "Link",
            Self::Calendar => "Date",
            Self::Mail => "Mail",
            Self::User => "User",
            Self::Bell => "Notifications",
            Self::Pin => "Pin",
            Self::External => "Open externally",
            Self::Command => "Command",
            Self::Save => "Save",
            Self::ZoomIn => "Zoom in",
            Self::ZoomOut => "Zoom out",
            Self::SafeArea => "Safe area",
        }
    }

    /// The filled SVG path data on the 16-unit grid.
    #[allow(clippy::match_same_arms)] // reason: a few icons intentionally share one glyph
    #[must_use]
    pub const fn path(self) -> &'static str {
        match self {
            Self::Play => "M4.5 2.5L13 8L4.5 13.5z",
            Self::Pause => "M4 3h3v10h-3zM9 3h3v10h-3z",
            Self::StepBack => "M10.5 3.5L10.5 12.5L4 8zM11.5 3.5h1.75v9h-1.75z",
            Self::StepForward => "M5.5 3.5L12 8L5.5 12.5zM2.75 3.5h1.75v9h-1.75z",
            Self::ToStart => "M2.5 3.5h1.75v9h-1.75zM9 3.5L9 12.5L4.5 8zM13.5 3.5L13.5 12.5L9 8z",
            Self::ToEnd => "M2.5 3.5L7 8L2.5 12.5zM7 3.5L11.5 8L7 12.5zM11.75 3.5h1.75v9h-1.75z",
            Self::Loop => "M2.4 8a5.6 5.6 0 1 0 11.2 0a5.6 5.6 0 1 0 -11.2 0zM3.9 8a4.1 4.1 0 1 1 8.2 0a4.1 4.1 0 1 1 -8.2 0zM9.5 1.6L13 4.6L9.5 7.6zM6.5 8.4L3 11.4L6.5 14.4z",
            Self::ChevronDown => "M4 6L12 6L8 10.5z",
            Self::ChevronUp => "M4 10L12 10L8 5.5z",
            Self::ChevronRight => "M6 4L10.5 8L6 12z",
            Self::ChevronLeft => "M10 4L5.5 8L10 12z",
            Self::ArrowLeft => "M6 7h7.5v2h-7.5zM6 3.5L1.5 8L6 12.5z",
            Self::ArrowRight => "M2.5 7h7.5v2h-7.5zM10 3.5L14.5 8L10 12.5z",
            Self::ArrowUp => "M7 6.5h2v7.5h-2zM3.5 6L8 1.5L12.5 6z",
            Self::ArrowDown => "M7 2h2v7.5h-2zM3.5 10L8 14.5L12.5 10z",
            Self::Refresh => "M2.2 8a5.8 5.8 0 1 0 11.6 0a5.8 5.8 0 1 0 -11.6 0zM3.8 8a4.2 4.2 0 1 1 8.4 0a4.2 4.2 0 1 1 -8.4 0zM2.2 5.2L6.4 4.8L3.9 8.6z",
            Self::Undo => "M4 9.8L11 9.8L11 8.2L4 8.2zM11.8 9L11.8 4.5L10.2 4.5L10.2 9zM7.5 5.5L4 9L7.5 12.5z",
            Self::Redo => "M5 9.8L12 9.8L12 8.2L5 8.2zM5.8 9L5.8 4.5L4.2 4.5L4.2 9zM8.5 5.5L12 9L8.5 12.5z",
            Self::Eye => "M1 8Q8 3.4 15 8Q8 12.6 1 8zM6.1 8a1.9 1.9 0 1 0 3.8 0a1.9 1.9 0 1 0 -3.8 0z",
            Self::EyeOff => "M1 8Q8 3.4 15 8Q8 12.6 1 8zM6.1 8a1.9 1.9 0 1 0 3.8 0a1.9 1.9 0 1 0 -3.8 0zM3.0657 14.0657L14.0657 3.0657L12.9343 1.9343L1.9343 12.9343z",
            Self::Lock => "M3 7h10v7h-10zM5 2.5h1.8v5h-1.8zM9.2 2.5h1.8v5h-1.8zM5 2.5h6v1.6h-6z",
            Self::Unlock => "M3 7h10v7h-10zM5 2.5h1.8v5h-1.8zM9.2 2.5h1.8v3h-1.8zM5 2.5h6v1.6h-6z",
            Self::Text => "M3 3h10v2h-10zM7 3h2v10h-2z",
            Self::Circle => "M2 8a6 6 0 1 0 12 0a6 6 0 1 0 -12 0zM3.6 8a4.4 4.4 0 1 1 8.8 0a4.4 4.4 0 1 1 -8.8 0z",
            Self::Rect => "M3.7 2.5h8.6a1.2 1.2 0 0 1 1.2 1.2v8.6a1.2 1.2 0 0 1 -1.2 1.2h-8.6a1.2 1.2 0 0 1 -1.2 -1.2v-8.6a1.2 1.2 0 0 1 1.2 -1.2zM4.2 4.2v7.6h7.6v-7.6z",
            Self::Path => "M1.5 12.5a1.5 1.5 0 1 0 3 0a1.5 1.5 0 1 0 -3 0zM11.5 9.5a1.5 1.5 0 1 0 3 0a1.5 1.5 0 1 0 -3 0zM5.5 4a1.5 1.5 0 1 0 3 0a1.5 1.5 0 1 0 -3 0zM3.5881 12.7768L7.5881 4.2768L6.4119 3.7232L2.4119 12.2232zM6.5608 4.4792L12.5608 9.9792L13.4392 9.0208L7.4392 3.5208z",
            Self::Group => "M2 3h5.5v3h-5.5zM3.2 5h9.6a1.2 1.2 0 0 1 1.2 1.2v6.1a1.2 1.2 0 0 1 -1.2 1.2h-9.6a1.2 1.2 0 0 1 -1.2 -1.2v-6.1a1.2 1.2 0 0 1 1.2 -1.2zM3.6 6.6v5.3h8.8v-5.3z",
            Self::Stopwatch => "M2.5 9a5.5 5.5 0 1 0 11 0a5.5 5.5 0 1 0 -11 0zM4 9a4 4 0 1 1 8 0a4 4 0 1 1 -8 0zM6.8 1.2h2.4v2.2h-2.4zM8.6 8.5L8.6 5.2L7.4 5.2L7.4 8.5z",
            Self::Snap => "M7.6 3h0.8a4.6 4.6 0 0 1 4.6 4.6v-0.2a4.6 4.6 0 0 1 -4.6 4.6h-0.8a4.6 4.6 0 0 1 -4.6 -4.6v0.2a4.6 4.6 0 0 1 4.6 -4.6zM4.8 4.8v5.4h6.4v-5.4zM3.5 10h2.4v4h-2.4zM10.1 10h2.4v4h-2.4z",
            Self::Mask => "M3.9 2.5h8.2a1.4 1.4 0 0 1 1.4 1.4v8.2a1.4 1.4 0 0 1 -1.4 1.4h-8.2a1.4 1.4 0 0 1 -1.4 -1.4v-8.2a1.4 1.4 0 0 1 1.4 -1.4zM4.2 4.2v7.6h7.6v-7.6zM7.7 10.5a2.8 2.8 0 1 0 5.6 0a2.8 2.8 0 1 0 -5.6 0zM8.9 10.5a1.6 1.6 0 1 1 3.2 0a1.6 1.6 0 1 1 -3.2 0z",
            Self::Image => "M3.4 2.5h9.2a1.4 1.4 0 0 1 1.4 1.4v8.2a1.4 1.4 0 0 1 -1.4 1.4h-9.2a1.4 1.4 0 0 1 -1.4 -1.4v-8.2a1.4 1.4 0 0 1 1.4 -1.4zM3.6 4.1v7.8h8.8v-7.8zM3.6 11.9L6.6 7.4L9.1 10.9L11.1 8.9L12.4 11.9zM9.3 5.6a1.2 1.2 0 1 0 2.4 0a1.2 1.2 0 1 0 -2.4 0z",
            Self::Audio => "M2.25 6h1.5v4h-1.5zM4.75 4h1.5v8h-1.5zM7.25 5h1.5v6h-1.5zM9.75 3h1.5v10h-1.5zM12.25 5.5h1.5v5h-1.5z",
            Self::Video => "M2.7 3h10.6a1.2 1.2 0 0 1 1.2 1.2v7.6a1.2 1.2 0 0 1 -1.2 1.2h-10.6a1.2 1.2 0 0 1 -1.2 -1.2v-7.6a1.2 1.2 0 0 1 1.2 -1.2zM3 4.5v7h10v-7zM3 4.5h3v2h-3zM3 9.5h3v2h-3zM10 4.5h3v2h-3zM10 9.5h3v2h-3zM6.5 6L9.8 8L6.5 10z",
            Self::Film => "M2.7 3h10.6a1.2 1.2 0 0 1 1.2 1.2v7.6a1.2 1.2 0 0 1 -1.2 1.2h-10.6a1.2 1.2 0 0 1 -1.2 -1.2v-7.6a1.2 1.2 0 0 1 1.2 -1.2zM3 4.5v7h10v-7zM3 4.5h3v2h-3zM3 9.5h3v2h-3zM10 4.5h3v2h-3zM10 9.5h3v2h-3zM6.5 6L9.8 8L6.5 10z",
            Self::Music => "M5.9 4L5.9 12L7.1 12L7.1 4zM6.6579 4.5789L12.1579 3.0789L11.8421 1.9211L6.3421 3.4211zM11.4 2.5L11.4 10.5L12.6 10.5L12.6 2.5zM2.7 12.2a2.1 2.1 0 1 0 4.2 0a2.1 2.1 0 1 0 -4.2 0zM8.2 10.7a2.1 2.1 0 1 0 4.2 0a2.1 2.1 0 1 0 -4.2 0z",
            Self::Font => "M4.2295 13.3283L8.7295 3.3283L7.2705 2.6717L2.7705 12.6717zM7.2705 3.3283L11.7705 13.3283L13.2295 12.6717L8.7295 2.6717zM5.2 9.9L10.8 9.9L10.8 8.5L5.2 8.5z",
            Self::List => "M2.1 3.6a1.1 1.1 0 1 0 2.2 0a1.1 1.1 0 1 0 -2.2 0zM2.1 8a1.1 1.1 0 1 0 2.2 0a1.1 1.1 0 1 0 -2.2 0zM2.1 12.4a1.1 1.1 0 1 0 2.2 0a1.1 1.1 0 1 0 -2.2 0zM6 3h7.5v1.3h-7.5zM6 7.4h7.5v1.3h-7.5zM6 11.8h7.5v1.3h-7.5z",
            Self::Grid => "M3.2 3h3a0.7 0.7 0 0 1 0.7 0.7v3a0.7 0.7 0 0 1 -0.7 0.7h-3a0.7 0.7 0 0 1 -0.7 -0.7v-3a0.7 0.7 0 0 1 0.7 -0.7zM9.8 3h3a0.7 0.7 0 0 1 0.7 0.7v3a0.7 0.7 0 0 1 -0.7 0.7h-3a0.7 0.7 0 0 1 -0.7 -0.7v-3a0.7 0.7 0 0 1 0.7 -0.7zM3.2 8.6h3a0.7 0.7 0 0 1 0.7 0.7v3a0.7 0.7 0 0 1 -0.7 0.7h-3a0.7 0.7 0 0 1 -0.7 -0.7v-3a0.7 0.7 0 0 1 0.7 -0.7zM9.8 8.6h3a0.7 0.7 0 0 1 0.7 0.7v3a0.7 0.7 0 0 1 -0.7 0.7h-3a0.7 0.7 0 0 1 -0.7 -0.7v-3a0.7 0.7 0 0 1 0.7 -0.7z",
            Self::Clean => "M4 3h8v1.7h-8zM6 1.4h4v1.7h-4zM5.8 5h4.4a0.8 0.8 0 0 1 0.8 0.8v7.4a0.8 0.8 0 0 1 -0.8 0.8h-4.4a0.8 0.8 0 0 1 -0.8 -0.8v-7.4a0.8 0.8 0 0 1 0.8 -0.8zM6.4 6.6v6h3.2v-6zM8 7L8 11.5L9 11.5L9 7z",
            Self::Trash => "M4 3h8v1.7h-8zM6 1.4h4v1.7h-4zM5.8 5h4.4a0.8 0.8 0 0 1 0.8 0.8v7.4a0.8 0.8 0 0 1 -0.8 0.8h-4.4a0.8 0.8 0 0 1 -0.8 -0.8v-7.4a0.8 0.8 0 0 1 0.8 -0.8zM6.4 6.6v6h3.2v-6zM8 7L8 11.5L9 11.5L9 7z",
            Self::Mute => "M2.5 6L5.5 6L8 3L8 13L5.5 10L2.5 10zM9.9869 5.8991L13.4869 10.3991L14.5131 9.6009L11.0131 5.1009zM13.4869 5.1009L9.9869 9.6009L11.0131 10.3991L14.5131 5.8991z",
            Self::Solo => "M2.8 8a5.2 5.2 0 1 0 10.4 0a5.2 5.2 0 1 0 -10.4 0zM4.3 8a3.7 3.7 0 1 1 7.4 0a3.7 3.7 0 1 1 -7.4 0zM2 8h2.4v4h-2.4zM11.6 8h2.4v4h-2.4z",
            Self::Filter => "M2.5 3L13.5 3L9.5 8L9.5 12.5L6.5 14L6.5 8z",
            Self::Shadow => "M3.5 2.5h6.5a1 1 0 0 1 1 1v6.5a1 1 0 0 1 -1 1h-6.5a1 1 0 0 1 -1 -1v-6.5a1 1 0 0 1 1 -1zM6.5 5.5h6.5a1 1 0 0 1 1 1v6.5a1 1 0 0 1 -1 1h-6.5a1 1 0 0 1 -1 -1v-6.5a1 1 0 0 1 1 -1zM6.9 6.9v5.7h5.7v-5.7z",
            Self::Blur => "M2 8a6 6 0 1 0 12 0a6 6 0 1 0 -12 0zM3.1 8a4.9 4.9 0 1 1 9.8 0a4.9 4.9 0 1 1 -9.8 0zM4.3 8a3.7 3.7 0 1 0 7.4 0a3.7 3.7 0 1 0 -7.4 0zM5.4 8a2.6 2.6 0 1 1 5.2 0a2.6 2.6 0 1 1 -5.2 0zM6.9 8a1.1 1.1 0 1 0 2.2 0a1.1 1.1 0 1 0 -2.2 0z",
            Self::Glow => "M5.4 8a2.6 2.6 0 1 0 5.2 0a2.6 2.6 0 1 0 -5.2 0zM7.25 1h1.5v2.5h-1.5zM7.25 12.5h1.5v2.5h-1.5zM1 7.25h2.5v1.5h-2.5zM12.5 7.25h2.5v1.5h-2.5z",
            Self::CheckMark => "M2.7787 9.6183L5.7787 13.1183L7.2213 11.8817L4.2213 8.3817zM7.2701 13.0562L13.7701 4.0562L12.2299 2.9438L5.7299 11.9438z",
            Self::Dot => "M5 8a3 3 0 1 0 6 0a3 3 0 1 0 -6 0z",
            Self::Warning => "M8 1.5L15 14L1 14zM4.6 11.2v2.3h6.8v-2.3zM5.9 10.4L10.1 10.4L8 4.2zM7.25 6.2h1.5v3.2h-1.5zM7 11a1 1 0 1 0 2 0a1 1 0 1 0 -2 0z",
            Self::Error => "M1 8a7 7 0 1 0 14 0a7 7 0 1 0 -14 0zM3 8a5 5 0 1 1 10 0a5 5 0 1 1 -10 0zM7.25 4h1.5v5h-1.5zM6.95 11.5a1.05 1.05 0 1 0 2.1 0a1.05 1.05 0 1 0 -2.1 0z",
            Self::Check => "M1 8a7 7 0 1 0 14 0a7 7 0 1 0 -14 0zM3 8a5 5 0 1 1 10 0a5 5 0 1 1 -10 0zM4.2 8.2L5.6 6.8L7.4 8.6L6 10zM6 10L5.2 9.2L10.6 3.8L11.8 5z",
            Self::Clock => "M1.6 8a6.4 6.4 0 1 0 12.8 0a6.4 6.4 0 1 0 -12.8 0zM3.1 8a4.9 4.9 0 1 1 9.8 0a4.9 4.9 0 1 1 -9.8 0zM8.65 8L8.65 4.2L7.35 4.2L7.35 8zM7.7945 8.6166L10.7945 9.6166L11.2055 8.3834L8.2055 7.3834z",
            Self::Star => "M8 1.8L9.7634 5.9729L14.277 6.3605L10.8532 9.3271L11.8794 13.7395L8 11.4L4.1206 13.7395L5.1468 9.3271L1.723 6.3605L6.2366 5.9729z",
            Self::More => "M2.1 8a1.4 1.4 0 1 0 2.8 0a1.4 1.4 0 1 0 -2.8 0zM6.6 8a1.4 1.4 0 1 0 2.8 0a1.4 1.4 0 1 0 -2.8 0zM11.1 8a1.4 1.4 0 1 0 2.8 0a1.4 1.4 0 1 0 -2.8 0z",
            Self::Close => "M2.8636 4.1364L11.8636 13.1364L13.1364 11.8636L4.1364 2.8636zM11.8636 2.8636L2.8636 11.8636L4.1364 13.1364L13.1364 4.1364z",
            Self::Plus => "M7 3h2v10h-2zM3 7h10v2h-10z",
            Self::Minus => "M3 7h10v2h-10z",
            Self::Search => "M2.4 7a4.6 4.6 0 1 0 9.2 0a4.6 4.6 0 1 0 -9.2 0zM4 7a3 3 0 1 1 6 0a3 3 0 1 1 -6 0zM9.7636 11.0364L13.3636 14.6364L14.6364 13.3636L11.0364 9.7636z",
            Self::Settings => "M2.4 8a5.6 5.6 0 1 0 11.2 0a5.6 5.6 0 1 0 -11.2 0zM4.2 8a3.8 3.8 0 1 1 7.6 0a3.8 3.8 0 1 1 -7.6 0zM5.8 8a2.2 2.2 0 1 0 4.4 0a2.2 2.2 0 1 0 -4.4 0zM7 8a1 1 0 1 1 2 0a1 1 0 1 1 -2 0zM7 0.8h2v2.6h-2zM7 12.6h2v2.6h-2zM0.8 7h2.6v2h-2.6zM12.6 7h2.6v2h-2.6zM1.5636 2.8364L3.7636 5.0364L5.0364 3.7636L2.8364 1.5636zM13.1636 1.5636L10.9636 3.7636L12.2364 5.0364L14.4364 2.8364zM2.8364 14.4364L5.0364 12.2364L3.7636 10.9636L1.5636 13.1636zM14.4364 13.1636L12.2364 10.9636L10.9636 12.2364L13.1636 14.4364z",
            Self::File => "M3 1.5L9.5 1.5L13 5L13 14.5L3 14.5zM9.5 1.5L9.5 5L13 5zM5 8.55L11 8.55L11 7.45L5 7.45zM5 11.35L11 11.35L11 10.25L5 10.25z",
            Self::Folder => "M1.5 4L6.5 4L8 5.8L14.5 5.8L14.5 13L1.5 13zM3 7.2L13 7.2L13 11.5L3 11.5z",
            Self::FolderOpen => "M1.5 4L6.5 4L8 5.8L14.5 5.8L14.5 8L1.5 8zM1.5 8L14.5 8L12.8 13L0.5 13z",
            Self::Home => "M8 1.5L15 8L13 8L13 14.5L3 14.5L3 8L1 8zM6.4 9.6v4.9h3.2v-4.9z",
            Self::Download => "M7.1 2.5L7.1 9.5L8.9 9.5L8.9 2.5zM4.5 7L8 10.5L11.5 7zM2.5 13.7L13.5 13.7L13.5 11.9L2.5 11.9z",
            Self::Upload => "M8.9 13.5L8.9 6.5L7.1 6.5L7.1 13.5zM4.5 8.5L8 5L11.5 8.5zM2.5 3.4L13.5 3.4L13.5 1.6L2.5 1.6z",
            Self::Edit => "M10.5 2.2L13.8 5.5L6 13.3L2 14L2.7 10zM9.6 3.1L12.9 6.4L11.6 7.7L8.3 4.4z",
            Self::Copy => "M6.5 2.5h6a1 1 0 0 1 1 1v7a1 1 0 0 1 -1 1h-6a1 1 0 0 1 -1 -1v-7a1 1 0 0 1 1 -1zM7 4v6h5v-6zM3.5 5.5h6a1 1 0 0 1 1 1v7a1 1 0 0 1 -1 1h-6a1 1 0 0 1 -1 -1v-7a1 1 0 0 1 1 -1zM4 7v6h5v-6z",
            Self::Link => "M2.1 10.5a3.4 3.4 0 1 0 6.8 0a3.4 3.4 0 1 0 -6.8 0zM3.7 10.5a1.8 1.8 0 1 1 3.6 0a1.8 1.8 0 1 1 -3.6 0zM7.1 5.5a3.4 3.4 0 1 0 6.8 0a3.4 3.4 0 1 0 -6.8 0zM8.7 5.5a1.8 1.8 0 1 1 3.6 0a1.8 1.8 0 1 1 -3.6 0zM5.5151 9.9353L11.1151 7.5353L10.4849 6.0647L4.8849 8.4647z",
            Self::Calendar => "M3 2.5h10a1 1 0 0 1 1 1v9.5a1 1 0 0 1 -1 1h-10a1 1 0 0 1 -1 -1v-9.5a1 1 0 0 1 1 -1zM3.5 4v8.5h9v-8.5zM2 4.2h12v1.5h-12zM5 1h1.5v3h-1.5zM9.5 1h1.5v3h-1.5zM4.5 7.5h2v2h-2zM8 7.5h2v2h-2zM4.5 10.5h2v2h-2z",
            Self::Mail => "M2.5 3h11a1 1 0 0 1 1 1v8a1 1 0 0 1 -1 1h-11a1 1 0 0 1 -1 -1v-8a1 1 0 0 1 1 -1zM2.8 4.2v7.6h10.4v-7.6zM2.2 4.2L8 8.5L13.8 4.2z",
            Self::User => "M4.8 5a3.2 3.2 0 1 0 6.4 0a3.2 3.2 0 1 0 -6.4 0zM2.5 14.5c0-3.2 2.6-5 5.5-5s5.5 1.8 5.5 5z",
            Self::Bell => "M8 1.5L12 5.2L12.5 10L3.5 10L4 5.2zM6.5 12.4a1.5 1.5 0 1 0 3 0a1.5 1.5 0 1 0 -3 0zM7 9.2h2v2.2h-2z",
            Self::Pin => "M8 14.5L4.5 8.5L11.5 8.5zM4.8 5.2a3.2 3.2 0 1 0 6.4 0a3.2 3.2 0 1 0 -6.4 0z",
            Self::External => "M3.5 5h6.5a1 1 0 0 1 1 1v6.5a1 1 0 0 1 -1 1h-6.5a1 1 0 0 1 -1 -1v-6.5a1 1 0 0 1 1 -1zM4 6.5v5.5h5.5v-5.5zM9.1364 9.1364L14.1364 4.1364L12.8636 2.8636L7.8636 7.8636zM9 2.5L13.5 2.5L13.5 7z",
            Self::Command => "M2.2 4.2a2 2 0 1 0 4 0a2 2 0 1 0 -4 0zM3.4 4.2a0.8 0.8 0 1 1 1.6 0a0.8 0.8 0 1 1 -1.6 0zM9.8 4.2a2 2 0 1 0 4 0a2 2 0 1 0 -4 0zM11 4.2a0.8 0.8 0 1 1 1.6 0a0.8 0.8 0 1 1 -1.6 0zM2.2 11.8a2 2 0 1 0 4 0a2 2 0 1 0 -4 0zM3.4 11.8a0.8 0.8 0 1 1 1.6 0a0.8 0.8 0 1 1 -1.6 0zM9.8 11.8a2 2 0 1 0 4 0a2 2 0 1 0 -4 0zM11 11.8a0.8 0.8 0 1 1 1.6 0a0.8 0.8 0 1 1 -1.6 0zM6.2 6.2h3.6v3.6h-3.6z",
            Self::Save => "M3 2h10a1 1 0 0 1 1 1v10a1 1 0 0 1 -1 1h-10a1 1 0 0 1 -1 -1v-10a1 1 0 0 1 1 -1zM3.6 3.6v8.8h8.8v-8.8zM5 3.5h6v4h-6zM5 9h6v4h-6z",
            Self::ZoomIn => "M2.4 7a4.6 4.6 0 1 0 9.2 0a4.6 4.6 0 1 0 -9.2 0zM4 7a3 3 0 1 1 6 0a3 3 0 1 1 -6 0zM9.7636 11.0364L13.3636 14.6364L14.6364 13.3636L11.0364 9.7636zM3.8 6.3h6.4v1.4h-6.4zM6.3 3.8h1.4v6.4h-1.4z",
            Self::ZoomOut => "M2.4 7a4.6 4.6 0 1 0 9.2 0a4.6 4.6 0 1 0 -9.2 0zM4 7a3 3 0 1 1 6 0a3 3 0 1 1 -6 0zM9.7636 11.0364L13.3636 14.6364L14.6364 13.3636L11.0364 9.7636zM3.8 6.3h6.4v1.4h-6.4z",
            Self::SafeArea => "M2.7 3h10.6a1.2 1.2 0 0 1 1.2 1.2v7.6a1.2 1.2 0 0 1 -1.2 1.2h-10.6a1.2 1.2 0 0 1 -1.2 -1.2v-7.6a1.2 1.2 0 0 1 1.2 -1.2zM3 4.4v7.2h10v-7.2zM4.8 5.5h6.4a0.8 0.8 0 0 1 0.8 0.8v3.4a0.8 0.8 0 0 1 -0.8 0.8h-6.4a0.8 0.8 0 0 1 -0.8 -0.8v-3.4a0.8 0.8 0 0 1 0.8 -0.8zM5.2 6.6v2.8h5.6v-2.8z",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::Icon;

    #[test]
    fn every_icon_has_a_filled_path() {
        assert_eq!(Icon::ALL.len(), 79);
        for icon in Icon::ALL {
            let path = icon.path();
            assert!(path.starts_with('M'), "{icon:?} has no path");
            assert!(
                path.ends_with('z') || path.ends_with('Z'),
                "{icon:?} path is not closed"
            );
            assert!(!icon.name().is_empty());
            assert!(!icon.label().is_empty());
        }
    }

    #[test]
    fn names_are_unique() {
        let mut names: Vec<&str> = Icon::ALL.iter().map(|icon| icon.name()).collect();
        names.sort_unstable();
        names.dedup();
        assert_eq!(names.len(), Icon::ALL.len());
    }
}
