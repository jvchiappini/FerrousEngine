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
pub use widget::Widget;
// container/grouping widget
pub use container::Container;
// core UI helpers
pub use canvas::Canvas; // re-export for convenience
pub use ui::Ui;
pub use viewport_widget::ViewportWidget;
// declarative builders
pub use builders::{Column, Row, Text, UiButton};
