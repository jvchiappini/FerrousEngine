//! Runtime overdraw debug view for the GUI/2D pass (prompt 18 P5).
//!
//! This module is the public façade for the overdraw view.  The implementation
//! lives in [`crate::pipelines::overdraw`] so it is compiled as part of the
//! existing `pipelines` module; an orchestrator that wires the view into
//! `GuiRenderer` / `Renderer2dConfig` can add one line to `lib.rs`:
//!
//! ```ignore
//! pub mod overdraw;
//! ```
//!
//! and then use `ferrous_ui_render::overdraw::{OverdrawConfig, OverdrawView,
//! OverdrawCounters, create_overdraw_pipeline,
//! create_overdraw_text_pipeline}`.
//!
//! The view is default-off (`OverdrawConfig::default().enabled == false`), so
//! adding the module declaration alone changes no rendering behaviour.

pub use crate::pipelines::overdraw::{
    create_overdraw_pipeline, create_overdraw_text_pipeline, OverdrawConfig, OverdrawCounters,
    OverdrawView, OVERDRAW_HEAT,
};
