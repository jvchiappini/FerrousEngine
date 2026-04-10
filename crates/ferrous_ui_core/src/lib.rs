//! `ferrous_ui_core` — Data and logic core of the FerrousEngine UI system.
//!
//! This crate defines the fundamental structures for the "Retained Mode" UI system.
//! Unlike immediate mode, widgets here persist in a memory tree (`UiTree`),
//! allowing massive optimizations like deferred layout calculation and drawing command 
//! caching ("Zero Lag").

pub mod animation;
pub mod events;
pub mod primitives;
pub mod reactive;
pub mod reflect;
pub mod style_builder;
pub mod text_field_state;
pub mod theme;
pub mod widgets;
pub mod background;
pub mod render_command;
pub mod fui_loader;
pub mod controller;
pub mod ui_tree;
pub mod context;
pub mod widget_trait;
pub mod spatial_index;
pub mod render_collector;

// Re-export common types
pub use animation::{Animated, Spring, Tween, Easing, Lerp};
pub use events::*;
pub use ferrous_ui_macros::{ui, FerrousWidget};
pub use primitives::*;
pub use reactive::*;
pub use reflect::*;
pub use fui_loader::FuiLoader;
pub use style_builder::{StyleBuilder, StyleExt};
pub use controller::FerrousController;
pub use text_field_state::{FieldKey, FieldKeyResult, TextFieldState};
pub use theme::{Color, Theme};
pub use glam::Vec2;
pub use widgets::widget_meta::{PaletteCategory, WidgetCategory, WidgetKind, WIDGET_REGISTRY};
pub use widgets::*;
pub use background::{Background, GradientStop, GradientAngle, UvMode};
pub use render_command::{RenderCommand, CapturedCommand};

// New re-exports from extracted modules
pub use ui_tree::{NodeId, DirtyFlags, CmdQueue, Node, UiTree};
pub use context::{EventContext, BuildContext, Component, UpdateContext, LayoutContext, DrawContext};
pub use widget_trait::Widget;
pub use spatial_index::SpatialIndex;
