//! `ferrous_gui` — Orquestador maestro del sistema de UI de Ferrous Engine.
//!
//! Este crate actúa como la fachada principal (Facade), coordinando los sub-sistemas
//! de widgets, layout, eventos y renderizado para proporcionar una experiencia de
//! desarrollo fluida y de alto rendimiento ("Lag Cero").

pub mod system;

pub use system::UiSystem;
pub use ferrous_ui_render::{GuiBatch, GuiQuad, GuiRenderer, TextQuad, MAX_TEXTURE_SLOTS, TEXTURED_BIT, ToBatches};

// Re-exportaciones útiles de otros crates para centralizar la API
pub use ferrous_ui_core::{
    Widget, NodeId, theme, Rect, Color, Style, RenderCommand, 
    Position, Overflow, RectOffset, DirtyFlags, BuildContext,
    UpdateContext, LayoutContext, DrawContext, EventContext,
    EventResponse, UiTree, ViewportWidget, GuiKey,
    Button, Slider, ColorPicker, PickerShape, UiEvent, MouseButton, Units, DisplayMode, Alignment
};
// Re-exportamos los sistemas ya integrados
pub use ferrous_layout::LayoutEngine;
pub use ferrous_events::EventManager;

// Incluimos macros si están disponibles
pub use ferrous_ui_core::ui;
