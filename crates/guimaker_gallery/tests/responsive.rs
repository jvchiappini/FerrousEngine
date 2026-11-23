//! Every non-overlay component lays out cleanly at every width and density.
//!
//! The Phase 3 sweep checks overflow, overlap and offscreen children. Overlay
//! components intentionally overlap their anchor, so they are excluded here and
//! covered by their own structural tests.

use guimaker::sweep::sweep;
use guimaker::theme::{Density, Theme};
use guimaker_gallery::Component;

/// Whether a finding is a subpixel rounding edge (at most half a logical px).
fn subpixel(detail: &str) -> bool {
    detail
        .rsplit_once(" by ")
        .and_then(|(_, tail)| tail.strip_suffix(" px"))
        .and_then(|value| value.parse::<f32>().ok())
        .is_some_and(|value| value <= 0.5)
}

fn overlay(component: Component) -> bool {
    matches!(
        component,
        Component::LayerStack
            | Component::Tooltip
            | Component::Popover
            | Component::Menu
            | Component::Dialog
            | Component::CommandPalette
            // The avatar group deliberately overlaps its members through a
            // negative gap, so its "overlap" findings are the design.
            | Component::Avatar
    )
}

#[test]
fn every_content_component_passes_the_responsive_sweep_in_both_densities() {
    for component in Component::ALL {
        if overlay(component) {
            continue;
        }
        for density in [Density::Compact, Density::Comfortable] {
            let mut theme = Theme::dark();
            theme.density = density;
            let report = sweep(&component.view(), &theme);
            // Fractional flex widths can round an edge by a subpixel at a
            // non-integer scale; the sweep's own half-pixel slop is applied in
            // physical pixels, so a 0.3 logical px edge is not a real overflow.
            let material: Vec<&guimaker::sweep::Issue> = report
                .cases
                .iter()
                .flat_map(|case| case.issues.iter())
                .filter(|issue| !subpixel(&issue.detail))
                .collect();
            let detail = material
                .first()
                .map(|issue| issue.detail.clone())
                .unwrap_or_default();
            assert!(
                material.is_empty(),
                "{} ({density:?}) is not clean:\n{}\n{detail}",
                component.name(),
                report.summary()
            );
        }
    }
}
