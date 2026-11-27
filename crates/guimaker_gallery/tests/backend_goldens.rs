//! Renders a curated story set on the backend selected by `WGPU_BACKEND` and
//! compares it with the committed (Vulkan-authored) goldens.
//!
//! `ferrous_gpu::EngineContext::new` honours `WGPU_BACKEND`, so the whole
//! gallery harness follows it: `WGPU_BACKEND=vulkan`, `dx12` or `gl`.
//!
//! # Tolerance
//!
//! - Vulkan: the committed goldens are the reference, so the comparison must be
//!   **byte-identical** (`max_channel_delta == 0`, `diff_pixels == 0`).
//! - DX12/GL: a documented cross-backend tolerance of **32/255 per channel**
//!   and **2 % of pixels** (see [`cross_backend`]).  GL uses the
//!   separate-texture shader (`gui_web.wgsl`) with a different image sample
//!   path (`textureSampleLevel`) and MSAA/text AA differs per driver, so the
//!   gallery 12/255 rule is too tight across APIs.  That shader also has no
//!   backdrop-blur branch and only eight texture slots; that is why this test
//!   covers the `Start`/`Components` stories (no backdrop blur, few textures)
//!   rather than the blur-heavy recipes.
//!
//! The test skips cleanly when the selected backend has no adapter.

// The gallery test target is auto-discovered; gate it on the PNG codec like
// the committed `goldens` target so a `--no-default-features` build still
// compiles (the Cargo.toml entry cannot be edited by this lane).
#![cfg(feature = "png")]

use std::path::PathBuf;

use guimaker::geom::Size;
use guimaker::theme::Theme;
use guimaker_gallery::golden::{compare_png, Tolerance};
use guimaker_gallery::shot::StoryTarget;
use guimaker_gallery::story::Story;

fn golden_path(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("goldens")
        .join(format!("{name}.png"))
}

/// The documented DX12/GL tolerance: 32/255 per channel and 2 % of pixels.
#[must_use]
#[allow(
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss
)]
fn cross_backend(pixel_count: usize) -> Tolerance {
    let budget = (pixel_count as f64 * 0.02).ceil() as usize;
    Tolerance {
        max_channel_delta: 32,
        max_diff_pixels: budget.max(1),
    }
}

/// Whether this run asked for Vulkan via `WGPU_BACKEND`.
///
/// Unset means "driver order" (Vulkan first on Windows); in that case the
/// tolerant comparison is used because the active backend is not pinned.
fn requested_vulkan() -> bool {
    std::env::var("WGPU_BACKEND").is_ok_and(|value| value.eq_ignore_ascii_case("vulkan"))
}

/// The stories compared on every backend.
///
/// No backdrop blur and at most a font atlas plus the solid test texture, so
/// the GL separate-binding path renders the same shapes.
fn cases() -> Vec<(String, Story, Theme)> {
    let mut cases = Vec::new();
    for (story, stem) in [(Story::Start, "start"), (Story::Components, "components")] {
        for (mode, theme) in [("light", Theme::light()), ("dark", Theme::dark())] {
            cases.push((format!("{stem}-{mode}-1280x800-1.5"), story, theme));
        }
    }
    cases
}

#[test]
fn the_stories_match_the_committed_goldens_on_the_selected_backend() {
    let strict_vulkan = requested_vulkan();

    let logical = Size::new(1280.0, 800.0);
    let scale = 1.5;
    let mut target = match StoryTarget::new(logical, scale) {
        Ok(target) => target,
        Err(error) => {
            eprintln!("skipping: no headless GPU device ({error})");
            return;
        }
    };

    for (name, story, theme) in cases() {
        let path = golden_path(&name);
        assert!(
            path.is_file(),
            "missing golden {}; the committed goldens are the reference",
            path.display()
        );

        let image = target
            .render_story(story, &theme, logical, scale)
            .expect("golden render");
        let tolerance = if strict_vulkan {
            Tolerance::gallery(image.pixel_count())
        } else {
            cross_backend(image.pixel_count())
        };
        let report = compare_png(&path, &image, tolerance).expect("golden comparison");

        if strict_vulkan {
            assert_eq!(
                report.diff_pixels,
                0,
                "Vulkan golden `{name}` must stay byte-identical: {}",
                report.summary()
            );
            assert_eq!(
                report.max_channel_delta,
                0,
                "Vulkan golden `{name}` must stay byte-identical: {}",
                report.summary()
            );
        } else {
            assert!(
                report.within_tolerance,
                "golden `{name}` on the selected backend is out of tolerance: {}",
                report.summary()
            );
        }
    }
}
