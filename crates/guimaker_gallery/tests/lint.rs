//! Every gallery story must lint clean at the desktop size and across the
//! responsive sweep.
//!
//! A mechanical finding is a bug in the story, not a licence to silence a rule:
//! the failure message names the story, the case, the rule and the sentence the
//! lint produced, so the fix is a layout/structure change. Warnings and notes
//! are printed for the report and do not fail the test.

use guimaker::geom::Size;
use guimaker::lint::{lint, Finding, Severity};
use guimaker::sweep::sweep;
use guimaker::text::MetricsTextSystem;
use guimaker::theme::Theme;
use guimaker::view::Element;
use guimaker::{Frame, Ui};
use guimaker_gallery::story::Story;

fn painted(view: &Element<()>, theme: &Theme) -> Ui<()> {
    let mut ui: Ui<()> = Ui::new(theme.clone());
    ui.set_viewport(Size::new(1280.0, 800.0), 1.0);
    let _ = ui.set_time_ms(0.0);
    let mut frame = Frame::new();
    let mut text = MetricsTextSystem::new();
    ui.paint(view, &mut frame, &mut text);
    ui
}

#[allow(clippy::trivially_copy_pass_by_ref)] // reason: severities are read from findings behind a shared reference
fn severity_name(severity: &Severity) -> &'static str {
    match severity {
        Severity::Error => "error",
        Severity::Warning => "warning",
        Severity::Info => "info",
    }
}

fn describe(story: Story, finding: &Finding) -> String {
    let rule = format!("{:?}", finding.rule);
    format!(
        "{}: [{}] {rule} `{}` -- {}",
        story.name(),
        severity_name(&finding.severity),
        finding.path,
        finding.sentence()
    )
}

#[test]
fn every_story_lints_clean_at_the_desktop_size() {
    let theme = Theme::dark();
    let mut blocking = Vec::new();
    for story in Story::all() {
        let ui = painted(&story.view(), &theme);
        for finding in lint(&ui) {
            eprintln!("{}", describe(story, &finding));
            if finding.severity.is_blocking() {
                blocking.push(describe(story, &finding));
            }
        }
    }
    assert!(
        blocking.is_empty(),
        "blocking design-lint findings:\n{}",
        blocking.join("\n")
    );
}

#[test]
fn every_story_lints_clean_across_the_sweep() {
    let theme = Theme::dark();
    let mut blocking = Vec::new();
    for story in Story::all() {
        let report = sweep(&story.view(), &theme);
        for case in &report.cases {
            for finding in &case.lint {
                eprintln!("{} @ {}x", describe(story, finding), case.width);
                if finding.severity.is_blocking() {
                    blocking.push(format!("{} @ {}x", describe(story, finding), case.width));
                }
            }
        }
        for entry in report.first_widths() {
            eprintln!(
                "{}: {} `{}` first appears at {} px",
                story.name(),
                entry.rule,
                entry.path,
                entry.width
            );
        }
    }
    assert!(
        blocking.is_empty(),
        "blocking design-lint findings across the sweep:\n{}",
        blocking.join("\n")
    );
}
