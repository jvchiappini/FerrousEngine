//! `ferrous_gui` â€” Orquestador maestro del sistema de UI de Ferrous Engine.
//!
//! Este crate actÃºa como la fachada principal (Facade), coordinando los sub-sistemas
//! de widgets, layout, eventos y renderizado para proporcionar una experiencia de
//! desarrollo fluida y de alto rendimiento.
//!
//! # API rÃ¡pida
//!
//! ```rust,ignore
//! use ferrous_gui::prelude::*;
//!
//! // El hit-testing y los eventos son automÃ¡ticos: solo defines el callback.
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
    GuiBatch, GuiQuad, GuiRenderer, TextQuad, ToBatches, BORDER_BIT, GRADIENT_BIT,
    GRADIENT_RADIAL_BIT, GRADIENT_STRIP_BIT, GRADIENT_V_BIT, MAX_TEXTURE_SLOTS, SHADOW_BIT,
    TEXTURED_BIT,
};
pub use system::UiSystem;

// Re-exportaciones de ferrous_ui_core
pub use ferrous_ui_core::{
    theme,
    // Tipos de layout y estilo
    Alignment,
    // Animaciones (NUEVO)
    Animated,
    Background,
    // Contextos del ciclo de vida
    BuildContext,
    // Widgets concretos
    Button,
    // Tipos de color y fondo
    Color,
    ColorPicker,
    // Ãrbol de UI
    DirtyFlags,
    DisplayMode,
    DrawContext,
    Easing,
    EventContext,
    // Sistema de eventos
    EventResponse,
    GradientAngle,
    GradientStop,
    GuiKey,
    HAlign,
    LayoutContext,
    Lerp,
    MouseButton,
    NodeId,
    Overflow,
    // Tipos de picker
    PaletteCategory,
    PickerShape,
    Position,
    Rect,
    RectOffset,
    // Trait y comando de render
    RenderCommand,
    Slider,
    Spring,
    Style,
    // Tipos de texto y alineaciÃ³n
    TextAlign,
    Tween,
    UiEvent,
    UiTree,
    Units,
    UpdateContext,
    VAlign,
    // Sistema reactivo
    ViewportWidget,
    Widget,
    WidgetCategory,
    WidgetKind,
    WIDGET_REGISTRY,
};

// Motores
pub use ferrous_layout::LayoutEngine;

// Builders
pub use builder::{ButtonBuilder, LabelBuilder, PanelBuilder, WidgetBuilder};

// Macros
pub use ferrous_ui_core::ui;

/// MÃ³dulo prelude: importa todo lo necesario para crear UIs con la API fluent.
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
        // Sistema de animaciones (NUEVO)
        Animated,
        Background,
        Color,
        DisplayMode,
        Easing,
        // Contextos de eventos
        EventContext,
        EventResponse,
        GuiKey,
        // Motores
        LayoutEngine,
        Lerp,
        MouseButton,
        // Ãrbol
        NodeId,
        Overflow,
        Position,
        Rect,
        RectOffset,
        RenderCommand,
        Slider,
        Spring,
        Style,
        Tween,
        UiEvent,
        UiSystem,
        Units,
        Widget,
        // Shader flags (para widgets custom)
        BORDER_BIT,
        GRADIENT_BIT,
        SHADOW_BIT,
    };
    pub use ferrous_ui_core::{Button, Checkbox, Label, Panel, TextInput};
}
