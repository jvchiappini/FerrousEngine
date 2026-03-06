//! `ferrous_gui` — retained-mode 2D GUI system for FerrousEngine.
//!
//! Provides a full widget toolkit without any egui dependency. All widgets are
//! rendered via `GuiRenderer` which emits textured quads consumed by the
//! renderer's `UiPass`.
//!
//! ## Module layout
//!
//! | Module | Responsibility |
//! |--------|----------------|
//! | `widget` | `Widget` trait: collect / hit-test / mouse_input / keyboard_input |
//! | `layout` | `Node`, `Style`, `Rect`, `Units`, `Alignment`, `DisplayMode` |
//! | `canvas` | Widget container with focus tracking |
//! | `ui` | High-level `Ui` wrapper around `Canvas` |
//! | `renderer` | `GuiRenderer`, `GuiBatch`, `GuiQuad`, `TextBatch` |
//! | `button`, `slider`, `textinput`, `color_picker` | Concrete interactive widgets |
//! | `builders` | Declarative `Row`, `Column`, `Text`, `UiButton` |
//! | `viewport_widget` | Embedded 3D viewport region |
//! | `container` | Grouping / panel widget |

pub mod builders;
pub mod button;
pub mod canvas;
pub mod color_picker;
pub mod container;
pub mod layout;
pub mod renderer;
pub mod slider;
pub mod textinput;
pub mod ui;
pub mod viewport_widget;
pub mod widget;

pub use layout::{Alignment, DisplayMode, Node, Rect, RenderCommand, Style, Units};
pub use renderer::{GuiBatch, GuiQuad, GuiRenderer, TextBatch};
// re-export new widgets
pub use crate::color_picker::{ColorPicker, PickerShape};
// UiButton is the declarative node-based button builder
pub use button::Button as InteractiveButton;
pub use slider::Slider;
pub use textinput::TextInput;

// GuiKey is a lightweight enum used throughout the GUI crate instead of
// depending directly on winit's key code type.  The variant set is kept
// minimal; additional entries can be added as widgets need them.  When the
// `winit-backend` feature is enabled we provide an `impl From<winit::keyboard::KeyCode>`
// so that callers can convert incoming events without pulling winit into the
// public API.

pub mod key;
pub use key::GuiKey;
pub use widget::Widget;
// container/grouping widget
pub use container::Container;
// core UI helpers
pub use canvas::Canvas; // re-export for convenience
pub use ui::Ui;
pub use viewport_widget::ViewportWidget;
// declarative builders
pub use builders::{Column, Row, Text, UiButton};
