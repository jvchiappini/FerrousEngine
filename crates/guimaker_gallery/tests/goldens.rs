//! Regenerates the committed goldens and compares them with the tolerance.
//!
//! The tolerance is the gallery default: a 12/255 per-channel delta and 0.2 %
//! of pixels. The test skips (with a message) when the machine has no headless
//! GPU adapter, so a CPU-only CI job does not report a false regression; on a
//! machine with an adapter it is a real pixel comparison.

use std::path::PathBuf;

use guimaker::geom::Size;
use guimaker::theme::Theme;
use guimaker_gallery::golden::{compare_png, Tolerance};
use guimaker_gallery::shot::StoryTarget;
use guimaker_gallery::story::Story;
use guimaker_gallery::Component;

fn golden_path(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("goldens")
        .join(format!("{name}.png"))
}

fn cases() -> Vec<(String, Story, Theme)> {
    let mut cases = vec![
        (
            "start-light-1280x800-1.5".to_string(),
            Story::Start,
            Theme::light(),
        ),
        (
            "start-dark-1280x800-1.5".to_string(),
            Story::Start,
            Theme::dark(),
        ),
        (
            "components-light-1280x800-1.5".to_string(),
            Story::Components,
            Theme::light(),
        ),
        (
            "components-dark-1280x800-1.5".to_string(),
            Story::Components,
            Theme::dark(),
        ),
    ];
    for component in Component::ALL {
        for (mode, theme) in [("light", Theme::light()), ("dark", Theme::dark())] {
            cases.push((
                format!("{}-{mode}-1280x800-1.5", component.name()),
                Story::Component(component),
                theme,
            ));
        }
    }
    for story in Story::RECIPES {
        for (mode, theme) in [("light", Theme::light()), ("dark", Theme::dark())] {
            cases.push((
                format!("{}-{mode}-1280x800-1.5", recipe_golden_name(story)),
                story,
                theme,
            ));
        }
    }
    cases
}

/// The golden file stem for a recipe.
///
/// Two recipe CLI names (`empty-state`, `command-palette`) are also component
/// story names, and the component goldens already own those file stems. The
/// recipe goldens take a `-screen` suffix so neither set of committed PNGs is
/// rewritten.
fn recipe_golden_name(story: Story) -> &'static str {
    match story {
        Story::EmptyState => "empty-state-screen",
        Story::CommandPalette => "command-palette-screen",
        other => other.name(),
    }
}

#[test]
fn committed_goldens_match_the_current_render() {
    // One device, GUI renderer and target for the whole batch: creating a
    // headless harness per case was the dominant cost of this test.
    let logical = Size::new(1280.0, 800.0);
    let scale = 1.5;
    let mut target = match StoryTarget::new(logical, scale) {
        Ok(target) => target,
        Err(error) => {
            eprintln!("skipping every golden: no headless GPU ({error})");
            return;
        }
    };

    for (name, story, theme) in cases() {
        let path = golden_path(&name);
        assert!(
            path.is_file(),
            "missing golden {}; run `cargo run -p guimaker_gallery -- golden {} --size 1280x800 --scale 1.5 --theme {} --update --reason ...`",
            path.display(),
            story.name(),
            theme.mode_name(),
        );

        let image = target
            .render_story(story, &theme, logical, scale)
            .expect("golden render");

        let tolerance = Tolerance::gallery(image.pixel_count());
        let report = compare_png(&path, &image, tolerance).expect("golden comparison");
        assert!(
            report.within_tolerance,
            "golden `{name}` is out of tolerance: {}",
            report.summary()
        );
    }
}

/// A tiny helper so the assertion message names the theme mode.
trait ThemeModeName {
    fn mode_name(&self) -> &'static str;
}

impl ThemeModeName for Theme {
    fn mode_name(&self) -> &'static str {
        match self.mode {
            guimaker::theme::ThemeMode::Light => "light",
            guimaker::theme::ThemeMode::Dark => "dark",
        }
    }
}
