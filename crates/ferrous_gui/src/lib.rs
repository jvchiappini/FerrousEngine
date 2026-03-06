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

// Re-export or stub the key code type depending on the backend feature. When
// the winit backend is enabled we simply re‑export the real type so downstream
// code can work with it directly.  When the backend is disabled we need a
// placeholder so that the public API signatures remain the same; rather than
// an entirely empty enum we include the few variants that the GUI logic
// actually compares against (currently only `Backspace` used by the text
// input widget).  This keeps `cargo check --no-default-features` from
// failing while avoiding a hard dependency on winit.

#[cfg(feature = "winit-backend")]
pub use winit::keyboard::KeyCode;

#[cfg(not(feature = "winit-backend"))]
/// Minimal stub used when the winit backend is disabled. Variant list may
/// grow as the GUI code begins to depend on additional key codes, but for the
/// moment only backspace is required.
pub enum KeyCode {
	/// Represents the backspace key, used by text input widgets to delete
	/// characters.
	Backspace,
}
pub use widget::Widget;
// container/grouping widget
pub use container::Container;
// core UI helpers
pub use canvas::Canvas; // re-export for convenience
pub use ui::Ui;
pub use viewport_widget::ViewportWidget;
// declarative builders
pub use builders::{Column, Row, Text, UiButton};
