//! `GUIMaker`: a UI library that is beautiful by default.
//!
//! `GUIMaker` turns structure into a screen. A view is built from tokens —
//! spacing, type roles, radii, elevation, colour roles, motion and density —
//! laid out by the library, routed by the library, and drawn by `FerrousEngine`.
//! The app writes what the screen *is*; it never computes where anything goes.
//!
//! The crate is deliberately headless: no shader, no rasteriser and no GPU
//! code. Its output is a list of engine-agnostic draw commands that
//! `guimaker_ferrous` maps onto the engine's batch.
//!
//! # The API in one screen
//!
//! ```
//! use guimaker::prelude::*;
//!
//! #[derive(Clone, Debug, PartialEq)]
//! enum Msg {
//!     NewProject,
//! }
//!
//! let view: Element<Msg> = Element::column()
//!     .key("start")
//!     .pad(Spacing::S6)
//!     .gap(Spacing::S4)
//!     .bg(ColorRole::Bg)
//!     .child(
//!         text("Motioner")
//!             .text_role(TypeRole::Title)
//!             .tone(Tone::Default),
//!     )
//!     .child(
//!         Element::row()
//!             .gap(Spacing::S2)
//!             .align(Align::Center)
//!             .child(
//!                 text("New project")
//!                     .pad_x(Spacing::S4)
//!                     .pad_y(Spacing::S2)
//!                     .bg(ColorRole::AccentFill)
//!                     .radius(Radius::Sm)
//!                     .on_click(Msg::NewProject),
//!             )
//!             .child(Element::spacer()),
//!     );
//!
//! let mut ui: Ui<Msg> = Ui::new(Theme::dark());
//! ui.set_viewport(Size::new(1280.0, 800.0), 1.0);
//! let mut text_system = MetricsTextSystem::new();
//! let mut frame = Frame::new();
//! ui.paint(&view, &mut frame, &mut text_system);
//!
//! // The start screen fills the viewport, and the title is a real text run.
//! assert_eq!(ui.layout().rect(NodeId(0)).width, 1280.0);
//! assert!(ui.inspect().contains("column start"));
//! assert!(!frame.display_list.commands.is_empty());
//! ```
//!
//! # Design values live in one module
//!
//! [`theme`] is the only module with literal design values. Styling APIs take
//! tokens (`pad(Spacing::S4)`, `radius(Radius::Md)`, `text_role(TypeRole::Body)`,
//! `Tone::Muted`); the pixel escape hatch is explicit ([`style::SizeSpec::Px`])
//! and reported by the design lint.
//!
//! # Determinism
//!
//! Layout and paint depend only on the view, the theme, the viewport size, the
//! scale factor, the keyed store and an injected clock. `Instant::now()` is
//! never read inside the library, so the same inputs give a byte-identical
//! display list and therefore a byte-identical screenshot.
//!
//! # What each frame does
//!
//! 1. **Build** the arena from the view (skipped when the view hash is
//!    unchanged).
//! 2. **Layout** on `taffy` 0.7.7, memoised by a content hash: unchanged
//!    subtrees are not re-synced and taffy's cache does the rest.
//! 3. **Route** this frame's input through the laid-out tree, updating the
//!    keyed store.
//! 4. **Paint** a flat display list, reusing each node's commands when its
//!    hash and interaction state are unchanged.
//! 5. **Damage**: the union of nodes whose rectangle or paint changed.
//! 6. **Explain**: [`Ui::inspect`], [`Ui::explain`] and [`Ui::debug`] expose
//!    the data prompt 19's panels read.

#![forbid(unsafe_code)]
#![warn(missing_docs)]

pub mod components;
pub mod debug;
pub mod display_list;
pub mod format;
pub mod frame;
pub mod geom;
pub mod hash;
pub mod icons;
pub mod input;
pub mod inspect;
pub mod layout;
pub mod lint;
pub mod motion;
pub mod overlay;
pub mod patterns;
pub mod recipe;
pub mod recipes;
pub mod spatial;
pub mod store;
pub mod style;
pub mod sweep;
pub mod text;
pub mod theme;
pub mod view;
pub mod visual;

/// The types an app needs to build a screen.
pub mod prelude {
    pub use crate::components::*;
    pub use crate::debug::{CacheState, FrameDebug, NodeDebug};
    pub use crate::display_list::{DisplayList, DrawCommand, ImageFit};
    pub use crate::frame::{Frame, FrameStats, PopupBatch, Ui};
    pub use crate::geom::{Rect, Size};
    pub use crate::icons::Icon;
    pub use crate::input::focus::FocusScope;
    pub use crate::input::shortcut::{Chord, Platform, Scope, ShortcutKey, ShortcutRegistry};
    pub use crate::input::ScrollKind;
    pub use crate::input::{InputEvent, Modifiers, PointerButton};
    pub use crate::layout::{AxisCause, Cause, LayoutStats};
    pub use crate::lint::{Finding, Rule, Severity};
    pub use crate::motion::{caret_visible, next_caret_wake_ms, EnterExit};
    pub use crate::recipe::{start_screen, StartMsg};
    pub use crate::recipes::{
        command_palette_screen, editor_shell, empty_first_run, file_browser, new_item_dialog,
        settings_screen, toasts_after_action, RecipeMsg,
    };
    pub use crate::resp;
    pub use crate::store::{NodeState, RepaintReason};
    pub use crate::style::{
        Align, Columns, Direction, Edges, Fluid, FluidText, Justify, Lines, Priority, Responsive,
        ScrollAxes, SizeSpec, Span, Split, Style, TextSpec, Tone,
    };
    pub use crate::text::{Ellipsis, MetricsTextSystem, TextShape, TextSystem};
    pub use crate::theme::{
        AccentState, BorderWidth, Breakpoint, Breakpoints, Color, ColorRole, ColorSeed,
        ContentWidth, Density, ElevationLevel, Radius, Spacing, Theme, ThemeMode, TypeRole,
    };
    pub use crate::view::{text, text_spans, Element, Key, NodeId, NodeKind, Tree};
}

pub use frame::{Frame, Ui};
pub use overlay::{HostCapabilities, OverlayHost, Placement, Side};
pub use view::{Element, NodeId};
