//! `GUIMaker`'s `FerrousEngine` backend.
//!
//! The `guimaker` core is engine-agnostic: it produces a [`DisplayList`] of
//! rounded rectangles, borders, shadows, gradients, text runs, images, icons,
//! clips and layers. This crate maps every command onto [`GuiBatch`] and shapes
//! text with `ferrous_font`; it is a mapping, not a second drawing engine.
//!
//! # Two entry points
//!
//! - [`GuiMakerRenderer`] owns the image [`TextureMap`] and the opacity stack.
//!   Use it when the display list contains [`DrawCommand::Image`] commands or
//!   when a host wants to reuse the same texture registry across frames.
//! - [`encode`] is the stateless convenience: it builds a temporary renderer
//!   with an empty texture map and maps the list. Images without a registered
//!   texture are skipped (a documented fallback); everything else is drawn.
//!
//! [`FerrousTextSystem`] is the [`guimaker::text::TextSystem`] a host passes to
//! [`guimaker::Ui::paint`]. It uses the real `ferrous_font` layout tables
//! (advances, `kern` and `GPOS` pair kerning, tracking and line wrapping) and
//! never touches a GPU.
//!
//! # Documented fallbacks
//!
//! The pinned engine (`dd382bd`) already exposes rounded clip shapes, a
//! backdrop-blur request helper and gradient borders, but not every one has a
//! complete GPU path. The exact mapping and the remaining fallbacks are listed
//! in `docs/backend.md` and in the README.
//!
//! [`DisplayList`]: guimaker::display_list::DisplayList
//! [`DrawCommand::Image`]: guimaker::display_list::DrawCommand::Image
//! [`GuiBatch`]: ferrous_ui_render::GuiBatch

#![forbid(unsafe_code)]
#![warn(missing_docs)]

mod mapper;
mod text;
mod texture;

pub use mapper::{encode, GuiMakerRenderer};
pub use text::{BundledGlyphs, FerrousTextSystem, GlyphSource};
pub use texture::TextureMap;
