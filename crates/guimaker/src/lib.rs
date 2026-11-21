//! `GUIMaker`: a UI library that is beautiful by default.
//!
//! `GUIMaker` turns structure into a screen. A view is built from tokens —
//! spacing, type roles, radii, elevation, colour roles, motion and density —
//! laid out by the library, routed by the library, and drawn by `FerrousEngine`.
//! The app writes what the screen *is*; it never computes where anything goes.
//!
//! The crate is deliberately headless: it contains no shader, no rasteriser
//! and no GPU code. Its output is a list of engine-agnostic draw commands that
//! `guimaker_ferrous` (the adapter crate) maps onto the engine's batch.
//!
//! # Design values live in one module
//!
//! [`theme`] is the only module with literal design values. Styling APIs take
//! tokens (`pad(S4)`, `radius(Md)`, `text(Body)`, `Tone::Muted`); the pixel
//! escape hatch is explicit (`Px(13.0)`) and reported by the design lint.
//!
//! ```
//! use guimaker::theme::{ColorRole, Theme};
//!
//! let theme = Theme::dark();
//! assert_eq!(theme.role(ColorRole::Accent), theme.colors.accent);
//! assert!(
//!     theme.colors.text.relative_luminance()
//!         > theme.colors.surface.relative_luminance()
//! );
//! ```
//!
//! # Determinism
//!
//! Layout and paint depend only on the view, the theme, the viewport size, the
//! scale factor and an injected clock. `Instant::now()` is never read inside
//! the library, so the same inputs give a byte-identical display list and
//! therefore a byte-identical screenshot.

#![forbid(unsafe_code)]
#![warn(missing_docs)]

pub mod theme;
