//! Módulo de builders fluent para crear widgets en una sola cadena de métodos.
//!
//! Cada widget tiene su propio archivo con su builder:
//!
//! | Archivo        | Builder          | Widget destino           |
//! |----------------|------------------|--------------------------|
//! | `button.rs`    | `ButtonBuilder`  | `Button` con eventos     |
//! | `label.rs`     | `LabelBuilder`   | `Label` de texto         |
//! | `panel.rs`     | `PanelBuilder`   | `Panel` contenedor flex  |
//! | `widget.rs`    | `WidgetBuilder`  | Cualquier widget custom  |
//!
//! La lógica compartida (posición, tamaño, jerarquía) vive en `base.rs`.
//!
//! # Añadir un nuevo builder
//!
//! 1. Crea `builder/mi_widget.rs` con `pub struct MiWidgetBuilder<App>`.
//! 2. Añade `pub(super) base: BuilderBase` al struct.
//! 3. Invoca `impl_builder_base!(MiWidgetBuilder<App>)`.
//! 4. Implementa los métodos propios del widget + `spawn()`.
//! 5. Agrega `mod mi_widget; pub use mi_widget::MiWidgetBuilder;` aquí.
//! 6. Expón el factory method en `UiSystem` en `system.rs`.

mod base;
mod button;
mod label;
mod panel;
mod widget;

pub use button::ButtonBuilder;
pub use label::LabelBuilder;
pub use panel::PanelBuilder;
pub use widget::WidgetBuilder;
