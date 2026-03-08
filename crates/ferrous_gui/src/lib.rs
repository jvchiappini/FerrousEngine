//! `ferrous_gui` — Sistema de UI legado y utilidades de transición.
//!
//! Este crate está siendo migrado gradualmente a `ferrous_ui_core`.
//! Actualmente solo mantiene la lógica de Layout básica y los puentes hacia el renderer.

pub mod builders;
pub mod constraint;
pub mod key;
pub mod layout;
pub mod renderer;

pub use constraint::{Constraint, SizeExpr};
pub use layout::{Alignment, DisplayMode, Node, Rect, RenderCommand, Style, ToBatches, Units, LegacyNodeWidget};
pub use renderer::{GuiBatch, GuiQuad, GuiRenderer, TextQuad, MAX_TEXTURE_SLOTS, TEXTURED_BIT};
pub use key::GuiKey;
pub use builders::{Column, Row, Text, UiButton};
