//! The responsive sweep: one view, many widths and scales, checked for breakage.

#![allow(clippy::cast_precision_loss)] // reason: rect edges are logical `f32` pixels, and the grid check multiplies them by the scale for an integer comparison

use std::fmt::Write as _;

use crate::frame::{Frame, Ui};
use crate::geom::{Rect, Size};
use crate::layout::LayoutEngine;
use crate::style::ScrollAxes;
use crate::text::MetricsTextSystem;
use crate::theme::Theme;
use crate::view::{Element, NodeKind, Tree};

/// The default viewport widths, in logical pixels, from phone to `4K`.
pub const WIDTHS: [f32; 10] = [
    360.0, 480.0, 640.0, 768.0, 1024.0, 1280.0, 1440.0, 1920.0, 2560.0, 3840.0,
];

/// The default device scale factors.
pub const SCALES: [f32; 4] = [1.0, 1.25, 1.5, 2.0];

/// The default viewport height, in logical pixels.
pub const HEIGHT: f32 = 900.0;

/// How far an edge may sit from an integer physical pixel before it is a finding.
const SNAP_EPSILON: f32 = 1e-3;

/// The fixed part of the containment tolerance, in logical pixels.
///
/// Rectangles are snapped to the physical pixel grid, so an edge may overshoot
/// its parent by up to half a logical pixel without anything being wrong.
const RECT_SLOP: f32 = 0.5;

/// What the sweep found at one width and scale.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum IssueKind {
    /// A child's rectangle escapes its parent's content box.
    Overflow,
    /// Two siblings that are not in a stack overlap.
    Overlap,
    /// A rectangle edge is not on the physical pixel grid.
    Unsnapped,
    /// The root is wider than the viewport.
    Offscreen,
}

impl IssueKind {
    /// The kind's stable name for reports.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Overflow => "overflow",
            Self::Overlap => "overlap",
            Self::Unsnapped => "unsnapped",
            Self::Offscreen => "offscreen",
        }
    }
}

/// One finding.
#[derive(Clone, Debug, PartialEq)]
pub struct Issue {
    /// The viewport width this finding was seen at.
    pub width: f32,
    /// The scale factor this finding was seen at.
    pub scale: f32,
    /// What kind of finding it is.
    pub kind: IssueKind,
    /// Stable debug path of the offending node (`start/body/recent/2`).
    pub path: String,
    /// One human sentence with the numbers and the parent/sibling path.
    pub detail: String,
}

/// The result for one width and scale.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Case {
    /// The viewport width.
    pub width: f32,
    /// The device scale factor.
    pub scale: f32,
    /// The findings at this case, in discovery order.
    pub issues: Vec<Issue>,
    /// Nodes laid out at this case (0 when the view failed to build).
    pub nodes: usize,
}

impl Case {
    /// Whether this case is clean.
    #[must_use]
    pub fn passes(&self) -> bool {
        self.issues.is_empty()
    }
}

/// Every case of one sweep.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Sweep {
    /// One result per width and scale combination.
    pub cases: Vec<Case>,
}

impl Sweep {
    /// Cases that had no finding.
    #[must_use]
    pub fn passes(&self) -> usize {
        self.cases.iter().filter(|case| case.passes()).count()
    }

    /// Cases with at least one finding.
    #[must_use]
    pub fn failures(&self) -> usize {
        self.cases.iter().filter(|case| !case.passes()).count()
    }

    /// Whether every case passed.
    #[must_use]
    pub fn is_clean(&self) -> bool {
        self.cases.iter().all(Case::passes)
    }

    /// Total findings.
    #[must_use]
    pub fn issue_count(&self) -> usize {
        self.cases.iter().map(|case| case.issues.len()).sum()
    }

    /// How many cases failed at each width, in the order the widths were given.
    #[must_use]
    pub fn failures_by_width(&self) -> Vec<(f32, usize)> {
        let mut totals: Vec<(f32, usize)> = Vec::new();
        for case in &self.cases {
            let failed = usize::from(!case.passes());
            match totals
                .iter_mut()
                .find(|(width, _)| (*width - case.width).abs() < f32::EPSILON)
            {
                Some(entry) => entry.1 += failed,
                None => totals.push((case.width, failed)),
            }
        }
        totals
    }

    /// A short multi-line summary: one line per case (`1024 @ 1.5x: pass` or
    /// `360 @ 1x: 3 issue(s)`), then a total line.
    #[must_use]
    pub fn summary(&self) -> String {
        let mut out = String::new();
        for case in &self.cases {
            let width = case.width;
            let scale = case.scale;
            if case.passes() {
                let _ = writeln!(out, "{width} @ {scale}x: pass");
            } else {
                let count = case.issues.len();
                let _ = writeln!(out, "{width} @ {scale}x: {count} issue(s)");
            }
        }
        let cases = self.cases.len();
        let pass = self.passes();
        let fail = self.failures();
        let issues = self.issue_count();
        let _ = write!(
            out,
            "total: {cases} case(s), {pass} pass, {fail} fail, {issues} issue(s)"
        );
        out
    }
}

/// Runs the default sweep (`WIDTHS` x `SCALES` x `HEIGHT`) over a view.
#[must_use]
pub fn sweep<Msg: Clone>(view: &Element<Msg>, theme: &Theme) -> Sweep {
    sweep_with(view, theme, &WIDTHS, &SCALES, HEIGHT)
}

/// Runs a sweep over explicit widths and scales.
///
/// Each width/scale pair paints a fresh [`Ui`] with its own [`Frame`] and
/// [`MetricsTextSystem`], so no case inherits the previous case's cached
/// layout.
#[must_use]
pub fn sweep_with<Msg: Clone>(
    view: &Element<Msg>,
    theme: &Theme,
    widths: &[f32],
    scales: &[f32],
    height: f32,
) -> Sweep {
    let mut sweep = Sweep {
        cases: Vec::with_capacity(widths.len().saturating_mul(scales.len())),
    };
    for &width in widths {
        for &scale in scales {
            let mut ui: Ui<Msg> = Ui::new(theme.clone());
            ui.set_viewport(Size::new(width, height), scale);
            let mut frame: Frame<Msg> = Frame::new();
            let mut text = MetricsTextSystem::new();
            ui.paint(view, &mut frame, &mut text);
            // The case already knows its metrics, so it uses the shared checker
            // directly instead of rebuilding the UI's per-node debug snapshot.
            let issues = check_at(&ui, width, scale);
            let nodes = ui.tree().len();
            sweep.cases.push(Case {
                width,
                scale,
                issues,
                nodes,
            });
        }
    }
    sweep
}

/// Checks one already-painted frame and returns its findings.
///
/// The painted viewport and scale are read from the frame's [`Ui::debug`]
/// snapshot; the checks themselves only read [`Ui::tree`] and [`Ui::layout`].
#[must_use]
pub fn check<Msg: Clone>(ui: &Ui<Msg>) -> Vec<Issue> {
    let (width, scale) = {
        let info = ui.debug();
        (info.viewport.width, info.scale)
    };
    check_at(ui, width, scale)
}

fn check_at<Msg: Clone>(ui: &Ui<Msg>, width: f32, scale: f32) -> Vec<Issue> {
    let tree = ui.tree();
    let layout = ui.layout();
    let tolerance = tolerance_for(scale);
    let mut issues = Vec::new();
    check_offscreen(tree, layout, width, scale, tolerance, &mut issues);
    check_overflow(tree, layout, width, scale, tolerance, &mut issues);
    check_overlap(tree, layout, width, scale, tolerance, &mut issues);
    check_unsnapped(tree, layout, width, scale, &mut issues);
    issues
}

fn check_offscreen<Msg>(
    tree: &Tree<Msg>,
    layout: &LayoutEngine,
    width: f32,
    scale: f32,
    tolerance: f32,
    issues: &mut Vec<Issue>,
) {
    let Some(root) = tree.root else {
        return;
    };
    let rect = layout.rect(root);
    if rect.width <= width + tolerance {
        return;
    }
    let path = tree.path(root);
    let root_width = tidy(rect.width);
    let viewport_width = tidy(width);
    let detail = format!(
        "root `{path}` is {root_width} px wide, wider than the {viewport_width} px viewport"
    );
    issues.push(Issue {
        width,
        scale,
        kind: IssueKind::Offscreen,
        path,
        detail,
    });
}

fn check_overflow<Msg>(
    tree: &Tree<Msg>,
    layout: &LayoutEngine,
    width: f32,
    scale: f32,
    tolerance: f32,
    issues: &mut Vec<Issue>,
) {
    for node in &tree.nodes {
        let Some(parent) = node.parent else {
            continue;
        };
        let Some(parent_style) = layout.resolved(parent) else {
            continue;
        };
        if parent_style.clip || parent_style.scroll != ScrollAxes::None {
            continue;
        }
        let child_rect = layout.rect(node.id);
        if child_rect.is_empty() {
            continue;
        }
        let parent_rect = layout.rect(parent);
        if parent_rect.is_empty() {
            continue;
        }
        let edges = [
            ("left", parent_rect.x - tolerance - child_rect.x),
            ("top", parent_rect.y - tolerance - child_rect.y),
            (
                "right",
                child_rect.right() - parent_rect.right() - tolerance,
            ),
            (
                "bottom",
                child_rect.bottom() - parent_rect.bottom() - tolerance,
            ),
        ];
        if !edges.iter().any(|(_, amount)| *amount > 0.0) {
            continue;
        }
        let parent_path = tree.path(parent);
        let detail = overflow_detail(&parent_path, child_rect, parent_rect, &edges);
        issues.push(Issue {
            width,
            scale,
            kind: IssueKind::Overflow,
            path: tree.path(node.id),
            detail,
        });
    }
}

fn check_overlap<Msg>(
    tree: &Tree<Msg>,
    layout: &LayoutEngine,
    width: f32,
    scale: f32,
    tolerance: f32,
    issues: &mut Vec<Issue>,
) {
    for node in &tree.nodes {
        if node.kind == NodeKind::Stack {
            continue;
        }
        let children = &node.children;
        for (index, &first) in children.iter().enumerate() {
            if layout.resolved(first).is_some_and(|style| style.hidden) {
                continue;
            }
            let first_rect = layout.rect(first);
            if first_rect.is_empty() {
                continue;
            }
            for &second in &children[index + 1..] {
                if layout.resolved(second).is_some_and(|style| style.hidden) {
                    continue;
                }
                let second_rect = layout.rect(second);
                if second_rect.is_empty() {
                    continue;
                }
                let overlap = first_rect.intersect(second_rect);
                if overlap.width <= tolerance || overlap.height <= tolerance {
                    continue;
                }
                let first_path = tree.path(first);
                let second_path = tree.path(second);
                let detail =
                    overlap_detail(&first_path, first_rect, &second_path, second_rect, overlap);
                issues.push(Issue {
                    width,
                    scale,
                    kind: IssueKind::Overlap,
                    path: first_path,
                    detail,
                });
            }
        }
    }
}

fn check_unsnapped<Msg>(
    tree: &Tree<Msg>,
    layout: &LayoutEngine,
    width: f32,
    scale: f32,
    issues: &mut Vec<Issue>,
) {
    if !scale.is_finite() || scale <= 0.0 {
        return;
    }
    for node in &tree.nodes {
        let rect = layout.rect(node.id);
        let mut list = String::new();
        for (name, value) in [
            ("x", rect.x),
            ("y", rect.y),
            ("right", rect.right()),
            ("bottom", rect.bottom()),
        ] {
            let physical = value * scale;
            if (physical - physical.round()).abs() > SNAP_EPSILON {
                if !list.is_empty() {
                    list.push_str(", ");
                }
                let edge = tidy(value);
                let snapped = tidy(physical);
                let _ = write!(list, "{name} {edge} -> {snapped} physical px");
            }
        }
        if list.is_empty() {
            continue;
        }
        let path = tree.path(node.id);
        let detail = format!("`{path}` at {scale}x has an edge off the physical grid: {list}");
        issues.push(Issue {
            width,
            scale,
            kind: IssueKind::Unsnapped,
            path,
            detail,
        });
    }
}

fn overflow_detail(
    parent_path: &str,
    child: Rect,
    parent: Rect,
    edges: &[(&'static str, f32)],
) -> String {
    let mut escaped = String::new();
    for &(name, amount) in edges {
        if amount > 0.0 {
            if !escaped.is_empty() {
                escaped.push_str(" and ");
            }
            let amount = tidy(amount);
            let _ = write!(escaped, "on the {name} by {amount} px");
        }
    }
    let child_left = tidy(child.x);
    let child_right = tidy(child.right());
    let child_top = tidy(child.y);
    let child_bottom = tidy(child.bottom());
    let parent_left = tidy(parent.x);
    let parent_right = tidy(parent.right());
    let parent_top = tidy(parent.y);
    let parent_bottom = tidy(parent.bottom());
    format!(
        "child x {child_left}..{child_right}, y {child_top}..{child_bottom} escapes `{parent_path}` x {parent_left}..{parent_right}, y {parent_top}..{parent_bottom} {escaped}"
    )
}

fn overlap_detail(
    first_path: &str,
    first: Rect,
    second_path: &str,
    second: Rect,
    overlap: Rect,
) -> String {
    let first_left = tidy(first.x);
    let first_right = tidy(first.right());
    let first_top = tidy(first.y);
    let first_bottom = tidy(first.bottom());
    let second_left = tidy(second.x);
    let second_right = tidy(second.right());
    let second_top = tidy(second.y);
    let second_bottom = tidy(second.bottom());
    let overlap_width = tidy(overlap.width);
    let overlap_height = tidy(overlap.height);
    format!(
        "children `{first_path}` x {first_left}..{first_right}, y {first_top}..{first_bottom} and `{second_path}` x {second_left}..{second_right}, y {second_top}..{second_bottom} overlap by {overlap_width} x {overlap_height} px"
    )
}

fn tolerance_for(scale: f32) -> f32 {
    if scale.is_finite() && scale > 0.0 {
        1.0 / scale + RECT_SLOP
    } else {
        RECT_SLOP
    }
}

fn tidy(value: f32) -> f32 {
    (value * 100.0).round() / 100.0
}

#[cfg(test)]
mod tests {
    use super::{Case, Issue, IssueKind, Sweep};

    fn case(width: f32, scale: f32, issues: usize) -> Case {
        let issues = (0..issues)
            .map(|index| Issue {
                width,
                scale,
                kind: IssueKind::Overflow,
                path: format!("root/{index}"),
                detail: "detail".to_string(),
            })
            .collect();
        Case {
            width,
            scale,
            issues,
            nodes: 1,
        }
    }

    #[test]
    fn summary_has_one_line_per_case_and_a_total() {
        let sweep = Sweep {
            cases: vec![case(1024.0, 1.5, 0), case(360.0, 1.0, 3)],
        };
        let text = sweep.summary();
        let lines: Vec<&str> = text.lines().collect();
        assert_eq!(lines.len(), 3);
        assert_eq!(lines[0], "1024 @ 1.5x: pass");
        assert_eq!(lines[1], "360 @ 1x: 3 issue(s)");
        assert!(lines[2].starts_with("total:"));
        assert_eq!(sweep.passes(), 1);
        assert_eq!(sweep.failures(), 1);
        assert_eq!(sweep.issue_count(), 3);
        assert!(!sweep.is_clean());
    }

    #[test]
    #[allow(clippy::float_cmp)] // reason: the widths and counts are exact literals
    fn failures_by_width_keeps_the_given_order() {
        let sweep = Sweep {
            cases: vec![
                case(360.0, 1.0, 1),
                case(360.0, 2.0, 0),
                case(1024.0, 1.0, 0),
                case(1024.0, 2.0, 2),
            ],
        };
        assert_eq!(sweep.failures_by_width(), vec![(360.0, 1), (1024.0, 1)]);
    }
}
