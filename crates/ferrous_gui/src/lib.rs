//! `ferrous_gui` — Orquestador maestro del sistema de UI de Ferrous Engine.
//!
//! Este crate actúa como la fachada principal (Facade), coordinando los sub-sistemas
//! de widgets, layout, eventos y renderizado para proporcionar una experiencia de
//! desarrollo fluida y de alto rendimiento.
//!
//! # API rápida
//!
//! ```rust,ignore
//! use ferrous_gui::prelude::*;
//!
//! // El hit-testing y los eventos son automáticos: solo defines el callback.
//! ui.button("Guardar")
//!     .size(120.0, 36.0)
//!     .border_radius(8.0)
//!     .on_click(|ctx| ctx.app.save())
//!     .spawn(&mut ui);
//!
//! ui.label("Hola mundo")
//!     .at(20.0, 60.0)
//!     .font_size(18.0)
//!     .spawn(&mut ui);
//! ```

pub mod builder;
pub mod system;
pub mod toaster;


pub use ferrous_ui_render::{
    GuiBatch, GuiQuad, GuiRenderer, TextQuad, ToBatches,
    MAX_TEXTURE_SLOTS, TEXTURED_BIT, GRADIENT_BIT, GRADIENT_V_BIT, GRADIENT_RADIAL_BIT,
    GRADIENT_STRIP_BIT, BORDER_BIT, SHADOW_BIT,
};
pub use system::UiSystem;

// Re-exportaciones de ferrous_ui_core
pub use ferrous_ui_core::{
    theme,
    // Tipos de layout y estilo
    Alignment, DisplayMode, Overflow, Position, Rect, RectOffset, Style, Units,
    // Tipos de texto y alineación
    TextAlign, HAlign, VAlign,
    // Tipos de color y fondo
    Color, Background, GradientStop, GradientAngle,
    // Contextos del ciclo de vida
    BuildContext, DrawContext, EventContext, LayoutContext, UpdateContext,
    // Sistema de eventos
    EventResponse, GuiKey, MouseButton, UiEvent,
    // Árbol de UI
    DirtyFlags, NodeId, UiTree,
    // Trait y comando de render
    RenderCommand, Widget,
    // Widgets concretos
    Button, Slider, ColorPicker,
    // Tipos de picker
    PaletteCategory, PickerShape,
    // Sistema reactivo
    ViewportWidget, WidgetCategory, WidgetKind, WIDGET_REGISTRY,
    // Animaciones (NUEVO)
    Animated, Spring, Tween, Easing, Lerp,
};

// Motores
pub use ferrous_events::EventManager;
pub use ferrous_layout::LayoutEngine;

// Builders
pub use builder::{ButtonBuilder, LabelBuilder, PanelBuilder, WidgetBuilder};

// Macros
pub use ferrous_ui_core::ui;

/// Módulo prelude: importa todo lo necesario para crear UIs con la API fluent.
///
/// ```rust,ignore
/// use ferrous_gui::prelude::*;
/// ```
pub mod prelude {
    pub use crate::{
        // Builders
        builder::{ButtonBuilder, LabelBuilder, PanelBuilder, WidgetBuilder},
        // Tipos de layout y estilo
        Alignment,
        Background,
        Color,
        DisplayMode,
        Overflow,
        Position,
        Rect,
        RectOffset,
        Style,
        Units,
        // Contextos de eventos
        EventContext,
        EventManager,
        EventResponse,
        GuiKey,
        MouseButton,
        // Sistema de animaciones (NUEVO)
        Animated,
        Easing,
        Lerp,
        Spring,
        Tween,
        // Motores
        LayoutEngine,
        // Árbol
        NodeId,
        RenderCommand,
        Slider,
        UiEvent,
        UiSystem,
        Widget,
        // Shader flags (para widgets custom)
        BORDER_BIT,
        GRADIENT_BIT,
        SHADOW_BIT,
    };
    pub use ferrous_ui_core::{Button, Checkbox, Label, Panel, TextInput};
}
