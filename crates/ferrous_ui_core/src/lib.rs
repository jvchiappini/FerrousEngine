//! `ferrous_ui_core` — Data and logic core of the FerrousEngine UI system.
//!
//! This crate defines the fundamental structures for the "Retained Mode" UI system.
//! Unlike immediate mode, widgets here persist in a memory tree (`UiTree`),
//! allowing massive optimizations like deferred layout calculation and drawing command
//! caching ("Zero Lag").

pub mod animation;
pub mod background;
pub mod context;
pub mod controller;
pub mod events;
pub mod fui_loader;
pub mod primitives;
pub mod reactive;
pub mod reflect;
pub mod render_collector;
pub mod render_command;
pub mod style_builder;
pub mod text_field_state;
pub mod theme;
pub mod ui_tree;
pub mod widget_trait;
pub mod widgets;

// Re-export common types
pub use animation::{Animated, Easing, Lerp, Spring, Tween};
pub use background::{Background, GradientAngle, GradientStop, UvMode};
pub use controller::FerrousController;
pub use events::*;
pub use ferrous_ui_macros::{ui, FerrousWidget};
pub use fui_loader::FuiLoader;
pub use glam::Vec2;
pub use primitives::*;
pub use reactive::*;
pub use reflect::*;
pub use render_command::{CapturedCommand, RenderCommand};
pub use style_builder::{StyleBuilder, StyleExt};
pub use text_field_state::{FieldKey, FieldKeyResult, TextFieldState};
pub use theme::{Color, Theme};
pub use widgets::widget_meta::{PaletteCategory, WidgetCategory, WidgetKind, WIDGET_REGISTRY};
pub use widgets::*;

// New re-exports from extracted modules
pub use context::{
    BuildContext, Component, DrawContext, EventContext, LayoutContext, UpdateContext,
};
pub use ui_tree::{CmdQueue, DirtyFlags, Node, NodeId, UiTree};
pub use widget_trait::Widget;
