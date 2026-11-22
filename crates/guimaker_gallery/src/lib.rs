//! `GUIMaker` gallery: stories, headless screenshots, tolerance goldens and the
//! `FerrousApp` host adapter.
//!
//! The crate has two independent halves:
//!
//! - [`story`] builds a `guimaker::Ui` at a viewport/scale and returns the flat
//!   display list. [`shot`] turns that list into RGBA8 (and a PNG) through the
//!   real GUI renderer, without a window. [`golden`] compares two images with a
//!   per-channel/pixel-count tolerance.
//! - [`host`] is the one-screen glue that lets any app adopt `GUIMaker` inside a
//!   `ferrous_engine::FerrousApp`: engine input becomes `guimaker::InputEvent`,
//!   `update` paints the view, `draw_ui` encodes it into the engine's
//!   `GuiBatch`.
//!
//! The command line in `src/main.rs` is the only binary; the library is what
//! tests and a host import.
//!
//! # Example
//!
//! ```no_run
//! use guimaker::geom::Size;
//! use guimaker::theme::Theme;
//! use guimaker_gallery::{shot, story::Story};
//!
//! let image = shot::render_story(
//!     Story::Start,
//!     &Theme::light(),
//!     Size::new(1280.0, 800.0),
//!     1.5,
//! )
//! .expect("headless GPU");
//! shot::write_png("out/start-light.png", &image).expect("write PNG");
//! ```

#![forbid(unsafe_code)]

pub mod golden;
pub mod host;
pub mod shot;
pub mod story;

pub use golden::{compare, compare_png, load_png, Tolerance};
pub use host::{GuiApp, GuiHost};
pub use shot::{render_story, write_png, RenderedImage};
pub use story::Story;

/// Everything that can go wrong in the gallery.
#[derive(Debug)]
pub enum GalleryError {
    /// Bad command-line arguments or an invalid render request.
    Usage(String),
    /// The headless GPU device or a render step failed.
    Gpu(String),
    /// A file could not be read or written.
    Io(std::io::Error),
    /// A PNG could not be encoded or decoded.
    Image(String),
    /// A golden is missing, has the wrong size or failed the tolerance.
    Golden(String),
}

impl std::fmt::Display for GalleryError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Usage(message) => write!(formatter, "{message}"),
            Self::Gpu(message) => write!(formatter, "headless GPU: {message}"),
            Self::Io(error) => write!(formatter, "I/O: {error}"),
            Self::Image(message) => write!(formatter, "PNG: {message}"),
            Self::Golden(message) => write!(formatter, "golden: {message}"),
        }
    }
}

impl std::error::Error for GalleryError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io(error) => Some(error),
            _ => None,
        }
    }
}

impl From<std::io::Error> for GalleryError {
    fn from(error: std::io::Error) -> Self {
        Self::Io(error)
    }
}

/// The gallery's result type.
pub type Result<T> = std::result::Result<T, GalleryError>;
