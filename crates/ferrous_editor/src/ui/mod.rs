//! Editor UI panels — Phase 13: User Interface & Feedback.
//!
//! Each sub-module is a self-contained panel.  Panels own their widget state
//! (sliders, pickers, etc.) and expose two methods:
//!
//! - `configure_ui(&mut self, ui: &mut Ui)` — registers widgets with the Ui
//!   system so that input events are dispatched to them.
//! - `draw(&mut self, gui, text, font, ctx)` — reads the current selection,
//!   draws the panel geometry and labels, and flushes any pending mutations
//!   back to the world + renderer.

pub mod global_light;
pub mod material_inspector;

pub use global_light::GlobalLightPanel;
pub use material_inspector::MaterialInspector;
