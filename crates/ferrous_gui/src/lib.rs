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
//! | `widget` | `Widget` trait: collect / hit-test / mouse_input / keyboard_input / tooltip |
//! | `layout` | `Node`, `Style`, `Rect`, `Units`, `Alignment`, `DisplayMode`, `RenderCommand` |
//! | `constraint` | `SizeExpr`, `Constraint` — reactive layout expressions |
//! | `canvas` | Widget container with focus tracking |
//! | `ui` | High-level `Ui` wrapper around `Canvas` |
//! | `renderer` | `GuiRenderer`, `GuiBatch`, `GuiQuad`, `TextBatch` |
//! | `button` | Interactive button with optional centred label, tooltip, `on_click` |
//! | `slider` | Slider with configurable `min`/`max` range and `on_change` |
//! | `textinput` | Single-line text input with cursor and `on_change` |
//! | `label` | First-class static text label registrable in `Ui` |
//! | `checkbox` | Checkbox with label and `on_change` |
//! | `dropdown` | Drop-down / combo-box widget |
//! | `panel` | `PanelBuilder` for automatic column/row layout with shared handles |
//! | `color_picker` | HSV colour picker wheel |
//! | `builders` | Declarative `Row`, `Column`, `Text`, `UiButton` (layout-only) |
//! | `viewport_widget` | Embedded 3D viewport region |
//! | `container` | Grouping / panel widget with optional clip/scissor |

pub mod builders;
pub mod button;
pub mod canvas;
pub mod checkbox;
pub mod color_picker;
pub mod constraint;
pub mod container;
pub mod dropdown;
pub mod label;
pub mod layout;
pub mod panel;
pub mod panel_resize_handle;
pub mod renderer;
pub mod slider;
pub mod textinput;
pub mod ui;
pub mod viewport_widget;
pub mod widget;
#[cfg(feature = "assets")]
pub mod image;

pub use constraint::{Constraint, SizeExpr};
pub use layout::{Alignment, DisplayMode, Node, Rect, RenderCommand, Style, ToBatches, Units};
pub use panel::RowItem;
pub use renderer::{GuiBatch, GuiQuad, GuiRenderer, TextBatch};
// re-export widgets
pub use crate::color_picker::{ColorPicker, PickerShape};
pub use button::Button;
pub use button::Button as InteractiveButton;
pub use checkbox::Checkbox;
pub use container::Container;
pub use dropdown::Dropdown;
pub use label::Label;
pub use panel::{Panel, PanelBuilder, PanelDirection};
pub use panel_resize_handle::{PanelResizeHandle, ResizeAxis};
pub use slider::Slider;
pub use textinput::TextInput;
#[cfg(feature = "assets")]
pub use image::Image;

// GuiKey is a lightweight enum used throughout the GUI crate instead of
// depending directly on winit's key code type.
pub mod key;
pub use key::GuiKey;
pub use widget::Widget;
// core UI helpers
pub use canvas::Canvas;
pub use ui::Ui;
pub use viewport_widget::ViewportWidget;
// declarative builders
pub use builders::{Column, Row, Text, UiButton};
