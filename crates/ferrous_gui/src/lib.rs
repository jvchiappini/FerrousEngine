//! `ferrous_gui` — Orquestador maestro del sistema de UI de Ferrous Engine.
//!
//! Este crate actúa como la fachada principal (Facade), coordinando los sub-sistemas
//! de widgets, layout, eventos y renderizado para proporcionar una experiencia de
//! desarrollo fluida y de alto rendimiento ("Lag Cero").
//!
//! # API rápida (recomendada para GUIMaker)
//!
//! ```rust,ignore
//! use ferrous_gui::prelude::*;
//!
//! ui.button("OK")
//!     .at(100.0, 200.0)
//!     .size(120.0, 36.0)
//!     .on_click(|_ctx| println!("click!"))
//!     .spawn(&mut ui);
//! ```

pub mod builder;
pub mod system;

pub use ferrous_ui_render::{
    GuiBatch, GuiQuad, GuiRenderer, TextQuad, ToBatches, MAX_TEXTURE_SLOTS, TEXTURED_BIT,
    GRADIENT_BIT, GRADIENT_STRIP_BIT,
};
pub use system::UiSystem;

// Re-exportaciones útiles de otros crates para centralizar la API
pub use ferrous_ui_core::{
    theme, Alignment, BuildContext, Button, Color, ColorPicker, DirtyFlags, DisplayMode,
    DrawContext, EventContext, EventResponse, GuiKey, LayoutContext, MouseButton, NodeId, Overflow,
    PaletteCategory, PickerShape, Position, Rect, RectOffset, RenderCommand, Slider, Style,
    UiEvent, UiTree, Units, UpdateContext, ViewportWidget, Widget, WidgetCategory, WidgetKind,
    WIDGET_REGISTRY,
};
// Re-exportamos los sistemas ya integrados
pub use ferrous_events::EventManager;
pub use ferrous_layout::LayoutEngine;

// Builders
pub use builder::{ButtonBuilder, LabelBuilder, PanelBuilder, WidgetBuilder};

// Incluimos macros si están disponibles
pub use ferrous_ui_core::ui;

/// Módulo prelude: importa todo lo necesario para crear UIs con la API fluent.
///
/// ```rust,ignore
/// use ferrous_gui::prelude::*;
/// ```
pub mod prelude {
    pub use crate::{
        builder::{ButtonBuilder, LabelBuilder, PanelBuilder, WidgetBuilder},
        Alignment,
        // Widgets concretos
        Button,
        Color,
        ColorPicker,
        // Layout
        DisplayMode,
        EventContext,
        EventManager,
        EventResponse,
        GuiKey,
        // Motores
        LayoutEngine,
        MouseButton,
        // Tipos de layout y estilo
        NodeId,
        Overflow,
        Position,
        Rect,
        RectOffset,
        Slider,
        Style,
        UiEvent,
        UiSystem,
        Units,
        // Trait y contextos
        Widget,
    };
    // Re-exportamos Label y Panel directamente desde su crate origen
    pub use ferrous_ui_core::{Checkbox, Label, Panel, TextInput};
}
