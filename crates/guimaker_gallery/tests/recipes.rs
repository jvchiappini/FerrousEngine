//! The Phase 9 screen recipes: each one paints, lints clean and keeps its name.
//!
//! A recipe is a whole screen, so the assertions are the same contract the start
//! recipe already follows: a non-empty display list at the desktop viewport, no
//! blocking design-lint finding at that size or across the responsive sweep, and
//! a stable CLI name.

use guimaker::geom::Size;
use guimaker::lint::lint;
use guimaker::sweep::sweep;
use guimaker::text::MetricsTextSystem;
use guimaker::theme::Theme;
use guimaker_gallery::story::Story;

fn painted(view: &guimaker::view::Element<()>, theme: &Theme) -> guimaker::Ui<()> {
    let mut ui = guimaker::Ui::new(theme.clone());
    ui.set_viewport(Size::new(1280.0, 800.0), 1.5);
    let _ = ui.set_time_ms(0.0);
    let mut frame = guimaker::Frame::new();
    let mut text = MetricsTextSystem::new();
    ui.paint(view, &mut frame, &mut text);
    ui
}

#[test]
fn every_recipe_paints_a_non_empty_list() {
    for story in Story::RECIPES {
        for theme in [Theme::light(), Theme::dark()] {
            let list = story.display_list(&theme, Size::new(1280.0, 800.0), 1.5);
            assert!(
                !list.commands.is_empty(),
                "{} painted no commands",
                story.name()
            );
        }
    }
}

#[test]
fn every_recipe_lints_clean_at_the_desktop_size() {
    let theme = Theme::dark();
    for story in Story::RECIPES {
        let ui = painted(&story.view(), &theme);
        let blocking: Vec<String> = lint(&ui)
            .into_iter()
            .filter(|finding| finding.severity.is_blocking())
            .map(|finding| {
                format!(
                    "{}: {:?} `{}` -- {}",
                    story.name(),
                    finding.rule,
                    finding.path,
                    finding.sentence()
                )
            })
            .collect();
        assert!(
            blocking.is_empty(),
            "blocking findings:\n{}",
            blocking.join("\n")
        );
    }
}

#[test]
fn every_recipe_lints_clean_across_the_sweep() {
    let theme = Theme::dark();
    for story in Story::RECIPES {
        let report = sweep(&story.view(), &theme);
        let blocking: Vec<String> = report
            .cases
            .iter()
            .flat_map(|case| {
                case.lint.iter().filter_map(move |finding| {
                    finding.severity.is_blocking().then_some(format!(
                        "{} @ {}x: {:?} `{}` -- {}",
                        story.name(),
                        case.width,
                        finding.rule,
                        finding.path,
                        finding.sentence()
                    ))
                })
            })
            .collect();
        assert!(
            blocking.is_empty(),
            "blocking sweep findings:\n{}",
            blocking.join("\n")
        );
    }
}

#[test]
fn every_recipe_has_a_stable_name() {
    for story in Story::RECIPES {
        assert_eq!(Story::parse(story.name()), Some(story));
    }
}
