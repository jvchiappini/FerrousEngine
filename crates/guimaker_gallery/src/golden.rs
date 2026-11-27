//! Tolerance-based golden comparison.
//!
//! A golden is a PNG under `crates/guimaker_gallery/goldens/`. Comparison is
//! two-dimensional on purpose:
//!
//! - `max_channel_delta` is the largest per-channel (RGBA) difference a pixel
//!   may have and still count as "equal". This absorbs the sub-pixel
//!   anti-aliasing differences between GPU drivers without hiding a colour
//!   change.
//! - `max_diff_pixels` is the absolute budget of pixels that exceed that
//!   channel delta. This catches a layout shift (many pixels change) while
//!   allowing a few isolated glyph edges to move.
//!
//! The gallery default is [`Tolerance::gallery`]: a 12/255 channel delta and
//! 0.2 % of the total pixels. Both the absolute count and the fraction are
//! printed by the CLI and by the test failure message.
//!
//! Updating a golden always requires a reason; [`append_update_log`] records
//! it next to the PNGs in `goldens/UPDATE_LOG.md`.

use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::shot::RenderedImage;
use crate::{GalleryError, Result};

/// How much difference a comparison tolerates.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Tolerance {
    /// Largest per-channel (R, G, B or A) difference, in 8-bit steps, that a
    /// pixel may have and still count as equal.
    pub max_channel_delta: u8,
    /// Maximum number of pixels allowed to exceed `max_channel_delta`.
    pub max_diff_pixels: usize,
}

impl Tolerance {
    /// The channel delta the gallery uses, in 8-bit steps.
    pub const GALLERY_CHANNEL_DELTA: u8 = 12;

    /// The fraction of pixels the gallery allows to differ.
    pub const GALLERY_DIFF_FRACTION: f64 = 0.002;

    /// The gallery tolerance for an image of `pixel_count` pixels.
    ///
    /// This is the exact rule both the CLI and the test use:
    /// `max_channel_delta = 12`, `max_diff_pixels = ceil(0.2 % of pixels)`,
    /// with a floor of one pixel.
    #[must_use]
    #[allow(clippy::cast_precision_loss)] // reason: image pixel counts stay below 2^52
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    // reason: the budget is the ceil of a positive fraction of a usize count
    pub fn gallery(pixel_count: usize) -> Self {
        let budget = (pixel_count as f64 * Self::GALLERY_DIFF_FRACTION).ceil();
        Self {
            max_channel_delta: Self::GALLERY_CHANNEL_DELTA,
            max_diff_pixels: (budget as usize).max(1),
        }
    }
}

/// The outcome of one comparison.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Comparison {
    /// The largest per-channel difference found, over every pixel.
    pub max_channel_delta: u8,
    /// Pixels whose largest channel difference exceeded the tolerance.
    pub diff_pixels: usize,
    /// Total pixels in the image.
    pub total_pixels: usize,
    /// `diff_pixels / total_pixels`.
    pub diff_fraction: f64,
    /// The tolerance that was applied.
    pub tolerance: Tolerance,
    /// Whether `diff_pixels <= tolerance.max_diff_pixels`.
    pub within_tolerance: bool,
}

impl Comparison {
    /// A one-line, human-readable summary.
    #[must_use]
    pub fn summary(&self) -> String {
        format!(
            "max channel delta {} (limit {}), differing pixels {} of {} ({:.4} %, limit {})",
            self.max_channel_delta,
            self.tolerance.max_channel_delta,
            self.diff_pixels,
            self.total_pixels,
            self.diff_fraction * 100.0,
            self.tolerance.max_diff_pixels,
        )
    }
}

/// Loads a PNG as tightly packed RGBA8.
///
/// # Errors
///
/// Returns [`GalleryError::Io`] when the file cannot be read,
/// [`GalleryError::Image`] when it is not an 8-bit RGBA PNG and
/// [`GalleryError::Image`] when the decoded size disagrees with the header.
// Phase 9D Lane C: PNG decoding is gated so a binary that never compares
// goldens does not link the `png` codec.
#[cfg(feature = "png")]
pub fn load_png(path: impl AsRef<Path>) -> Result<RenderedImage> {
    let path = path.as_ref();
    let file = std::fs::File::open(path)?;
    let decoder = png::Decoder::new(file);
    let mut reader = decoder
        .read_info()
        .map_err(|error| GalleryError::Image(format!("{}: {error}", path.display())))?;
    let mut buffer = vec![0; reader.output_buffer_size()];
    let info = reader
        .next_frame(&mut buffer)
        .map_err(|error| GalleryError::Image(format!("{}: {error}", path.display())))?;
    buffer.truncate(info.buffer_size());

    if info.bit_depth != png::BitDepth::Eight {
        return Err(GalleryError::Image(format!(
            "{}: expected 8-bit RGBA, found {:?} {:?}",
            path.display(),
            info.bit_depth,
            info.color_type
        )));
    }
    let rgba = match info.color_type {
        png::ColorType::Rgba => buffer,
        png::ColorType::Rgb => {
            let mut rgba = Vec::with_capacity(buffer.len() / 3 * 4);
            for pixel in buffer.chunks_exact(3) {
                rgba.extend_from_slice(&[pixel[0], pixel[1], pixel[2], 255]);
            }
            rgba
        }
        other => {
            return Err(GalleryError::Image(format!(
                "{}: expected RGBA or RGB, found {other:?}",
                path.display()
            )));
        }
    };

    let image = RenderedImage {
        width: info.width,
        height: info.height,
        rgba,
    };
    if !image.is_consistent() {
        return Err(GalleryError::Image(format!(
            "{}: decoded {} bytes for a {}x{} image",
            path.display(),
            image.rgba.len(),
            image.width,
            image.height
        )));
    }
    Ok(image)
}

/// Compares two images and reports the exact deltas.
///
/// # Errors
///
/// Returns [`GalleryError::Golden`] when the dimensions differ, so a resized
/// screenshot can never pass against a stale golden.
#[allow(clippy::cast_precision_loss)] // reason: image pixel counts stay below 2^52
pub fn compare(
    expected: &RenderedImage,
    actual: &RenderedImage,
    tolerance: Tolerance,
) -> Result<Comparison> {
    if expected.width != actual.width || expected.height != actual.height {
        return Err(GalleryError::Golden(format!(
            "size mismatch: golden is {}x{}, render is {}x{}",
            expected.width, expected.height, actual.width, actual.height
        )));
    }
    if !expected.is_consistent() || !actual.is_consistent() {
        return Err(GalleryError::Golden(
            "image buffer length does not match its dimensions".to_string(),
        ));
    }

    let mut max_channel_delta = 0u8;
    let mut diff_pixels = 0usize;
    for (expected_pixel, actual_pixel) in expected
        .rgba
        .chunks_exact(4)
        .zip(actual.rgba.chunks_exact(4))
    {
        let mut pixel_delta = 0u8;
        for channel in 0..4 {
            let delta = expected_pixel[channel].abs_diff(actual_pixel[channel]);
            max_channel_delta = max_channel_delta.max(delta);
            pixel_delta = pixel_delta.max(delta);
        }
        if pixel_delta > tolerance.max_channel_delta {
            diff_pixels += 1;
        }
    }

    let total_pixels = expected.pixel_count();
    Ok(Comparison {
        max_channel_delta,
        diff_pixels,
        total_pixels,
        diff_fraction: diff_pixels as f64 / total_pixels as f64,
        tolerance,
        within_tolerance: diff_pixels <= tolerance.max_diff_pixels,
    })
}

/// Loads `golden_path` and compares it with `actual`.
///
/// # Errors
///
/// Propagates [`load_png`] and [`compare`] errors.
#[cfg(feature = "png")]
pub fn compare_png(
    golden_path: impl AsRef<Path>,
    actual: &RenderedImage,
    tolerance: Tolerance,
) -> Result<Comparison> {
    let expected = load_png(golden_path)?;
    compare(&expected, actual, tolerance)
}

/// Appends an update reason to `goldens/UPDATE_LOG.md` next to `golden_path`.
///
/// The log is plain UTF-8 with `\n` line endings. It is best-effort: a failure
/// to append is reported as an I/O error, because an unexplained golden update
/// is worse than a failed command.
///
/// # Errors
///
/// Returns [`GalleryError::Io`] when the log cannot be written.
pub fn append_update_log(golden_path: impl AsRef<Path>, reason: &str) -> Result<PathBuf> {
    let golden_path = golden_path.as_ref();
    let directory = golden_path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    let log_path = directory.join("UPDATE_LOG.md");
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |duration| duration.as_secs());
    let name = golden_path.file_name().map_or_else(
        || "?".to_string(),
        |name| name.to_string_lossy().into_owned(),
    );
    let line = format!("- {timestamp} `{name}`: {reason}\n");
    let mut existing = String::new();
    if log_path.exists() {
        existing = std::fs::read_to_string(&log_path)?;
    }
    let mut contents = if existing.is_empty() {
        "# Golden updates\n\nEvery rewrite records its reason here.\n\n".to_string()
    } else {
        existing
    };
    contents.push_str(&line);
    std::fs::write(&log_path, contents)?;
    Ok(log_path)
}

#[cfg(test)]
mod tests {
    use super::{compare, Tolerance};
    use crate::shot::RenderedImage;

    fn solid(width: u32, height: u32, pixel: [u8; 4]) -> RenderedImage {
        let mut rgba = Vec::with_capacity(width as usize * height as usize * 4);
        for _ in 0..width * height {
            rgba.extend_from_slice(&pixel);
        }
        RenderedImage {
            width,
            height,
            rgba,
        }
    }

    #[test]
    fn identical_images_pass() {
        let image = solid(4, 4, [10, 20, 30, 255]);
        let report = compare(&image, &image, Tolerance::gallery(16)).expect("same size");
        assert!(report.within_tolerance);
        assert_eq!(report.diff_pixels, 0);
        assert_eq!(report.max_channel_delta, 0);
    }

    #[test]
    fn a_small_delta_is_tolerated_but_counted() {
        let expected = solid(4, 4, [10, 20, 30, 255]);
        let mut actual = expected.clone();
        actual.rgba[0] = 15;
        let report = compare(&expected, &actual, Tolerance::gallery(16)).expect("same size");
        assert!(report.within_tolerance);
        assert_eq!(report.max_channel_delta, 5);
        assert_eq!(report.diff_pixels, 0);
    }

    #[test]
    fn a_large_delta_fails() {
        let expected = solid(4, 4, [10, 20, 30, 255]);
        let mut actual = expected.clone();
        actual.rgba[4] = 200;
        let tolerance = Tolerance {
            max_channel_delta: Tolerance::GALLERY_CHANNEL_DELTA,
            max_diff_pixels: 0,
        };
        let report = compare(&expected, &actual, tolerance).expect("same size");
        assert!(!report.within_tolerance);
        assert_eq!(report.diff_pixels, 1);
        assert!(report.max_channel_delta > Tolerance::GALLERY_CHANNEL_DELTA);
    }

    #[test]
    fn a_size_mismatch_is_an_error() {
        let expected = solid(4, 4, [0, 0, 0, 255]);
        let actual = solid(5, 4, [0, 0, 0, 255]);
        assert!(compare(&expected, &actual, Tolerance::gallery(20)).is_err());
    }

    #[test]
    fn the_gallery_budget_is_a_fraction_with_a_floor() {
        assert_eq!(Tolerance::gallery(1).max_diff_pixels, 1);
        assert_eq!(Tolerance::gallery(2_000_000).max_diff_pixels, 4_000);
        assert_eq!(
            Tolerance::gallery(10).max_channel_delta,
            Tolerance::GALLERY_CHANNEL_DELTA
        );
    }
}
