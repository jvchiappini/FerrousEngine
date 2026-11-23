//! The design lint: a deterministic list of layout and design-system findings
//! for one painted screen.
//!
//! The lint reads only the data a frame already produced ([`Ui::tree`],
//! [`Ui::layout`], [`Ui::theme`], [`Ui::input`], [`Ui::shortcuts`] and
//! [`Ui::focus_order`]). It never runs on the paint path: `Ui::paint` does not
//! call it, and no rule touches the clock or the frame's draw commands. The same
//! view, theme and viewport therefore always produce the same findings in the
//! same order.
//!
//! Every finding names a stable key path, the rectangle that broke and, where it
//! helps, the sibling or parent rectangle it conflicts with.
//! [`Finding::sentence`] renders the readable form a model can act on, including
//! the inferred cause and a concrete fix.
//!
//! Rules report `Error`/`Warning` for real breakage and `Info` for an escape
//! hatch or a near miss. [`assert_clean`] is the test entry point: it paints a
//! view at `1280x800` and panics with the sentences of every blocking finding.

#![allow(clippy::cast_precision_loss)] // reason: rule thresholds and reported pixels are logical `f32` values
#![allow(clippy::format_push_string)] // reason: findings are readable sentences built incrementally

use crate::frame::{Frame, Ui};
use crate::geom::{Rect, Size};
use crate::input::shortcut::{Scope, ShortcutKey};
use crate::layout::{Cause, LayoutEngine};
use crate::style::{ResolvedStyle, ScrollAxes, SizeSpec, Tone};
use crate::text::MetricsTextSystem;
use crate::theme::{Color, ColorRole, Radius, Theme};
use crate::view::{Element, NodeId, NodeKind, Tree};

/// How serious a finding is.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Severity {
    /// A documented escape hatch or a near miss; safe to ship.
    Info,
    /// A real design problem that a user would notice.
    Warning,
    /// The screen is structurally broken at this viewport.
    Error,
}

impl Severity {
    /// The severity's stable name for reports and tests.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Info => "info",
            Self::Warning => "warning",
            Self::Error => "error",
        }
    }

    /// Whether the severity blocks a clean screen.
    #[must_use]
    pub const fn is_blocking(self) -> bool {
        matches!(self, Self::Warning | Self::Error)
    }
}

/// A design rule the lint enforces.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Rule {
    /// Two siblings that are not in a stack overlap materially.
    Overlap,
    /// A child escapes its parent's padding-aware content box.
    Overflow,
    /// A text node's shaped text does not fit and is silently clipped.
    ClippedText,
    /// Text or an icon does not meet its role's contrast minimum.
    LowContrast,
    /// A size or gap uses the explicit pixel escape hatch.
    PxEscapeHatch,
    /// A resolved spacing value is off the four-pixel grid.
    OffScaleSpacing,
    /// Sibling edges almost align (one to three pixels apart).
    NearMissAlignment,
    /// Siblings of the same kind use different radius tokens.
    InconsistentRadius,
    /// Siblings of the same kind use different elevation tokens.
    InconsistentElevation,
    /// An interactive target is smaller than 24 by 24 logical pixels.
    SmallHitTarget,
    /// An icon-only action has no tooltip.
    IconOnlyWithoutTooltip,
    /// More than one filled-accent action shares a parent.
    MultiplePrimaryActions,
    /// An empty layout container has no content and no visual.
    EmptyContainer,
    /// A node is wider than its own maximum width, or a content column's
    /// content is.
    MaxWidthOverflow,
    /// A child's right edge escapes the viewport.
    HorizontalOverflow,
    /// A trapping focus scope has no overlay-scope `Escape` binding.
    FocusTrapWithoutExit,
    /// An interactive node cannot be reached by the keyboard.
    UnreachableFocusable,
    /// The same chord is bound twice in the same scope.
    DuplicateShortcut,
}

impl Rule {
    /// The rule's stable name for reports and tests.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Overlap => "overlap",
            Self::Overflow => "overflow",
            Self::ClippedText => "clipped_text",
            Self::LowContrast => "low_contrast",
            Self::PxEscapeHatch => "px_escape_hatch",
            Self::OffScaleSpacing => "off_scale_spacing",
            Self::NearMissAlignment => "near_miss_alignment",
            Self::InconsistentRadius => "inconsistent_radius",
            Self::InconsistentElevation => "inconsistent_elevation",
            Self::SmallHitTarget => "small_hit_target",
            Self::IconOnlyWithoutTooltip => "icon_only_without_tooltip",
            Self::MultiplePrimaryActions => "multiple_primary_actions",
            Self::EmptyContainer => "empty_container",
            Self::MaxWidthOverflow => "max_width_overflow",
            Self::HorizontalOverflow => "horizontal_overflow",
            Self::FocusTrapWithoutExit => "focus_trap_without_exit",
            Self::UnreachableFocusable => "unreachable_focusable",
            Self::DuplicateShortcut => "duplicate_shortcut",
        }
    }

    /// A short human title.
    #[must_use]
    pub const fn title(self) -> &'static str {
        match self {
            Self::Overlap => "Sibling overlap",
            Self::Overflow => "Child overflow",
            Self::ClippedText => "Clipped text",
            Self::LowContrast => "Low contrast",
            Self::PxEscapeHatch => "Pixel escape hatch",
            Self::OffScaleSpacing => "Off-grid spacing",
            Self::NearMissAlignment => "Near-miss alignment",
            Self::InconsistentRadius => "Inconsistent radius",
            Self::InconsistentElevation => "Inconsistent elevation",
            Self::SmallHitTarget => "Small hit target",
            Self::IconOnlyWithoutTooltip => "Icon without a tooltip",
            Self::MultiplePrimaryActions => "Multiple primary actions",
            Self::EmptyContainer => "Empty container",
            Self::MaxWidthOverflow => "Maximum-width overflow",
            Self::HorizontalOverflow => "Horizontal overflow",
            Self::FocusTrapWithoutExit => "Focus trap without exit",
            Self::UnreachableFocusable => "Unreachable focusable",
            Self::DuplicateShortcut => "Duplicate shortcut",
        }
    }
}

/// One finding: what broke, where, why and how to fix it.
#[derive(Clone, Debug, PartialEq)]
pub struct Finding {
    /// How serious it is.
    pub severity: Severity,
    /// The rule that produced it.
    pub rule: Rule,
    /// The stable key path of the offending node.
    pub path: String,
    /// The offending rectangle, in logical pixels.
    pub rect: Rect,
    /// Other rectangles the finding refers to (a sibling, a parent).
    pub related: Vec<Rect>,
    /// The inferred cause, in plain English.
    pub cause: String,
    /// What is wrong, with the numbers.
    pub message: String,
    /// A concrete fix hint written for a model or a person to act on.
    pub fix: String,
}

impl Finding {
    /// The readable one-sentence form: path, rect, numbers, cause and fix.
    ///
    /// Every finding produces a complete sentence even when a rule has only one
    /// rectangle to show.
    #[must_use]
    pub fn sentence(&self) -> String {
        let mut out = format!("`{}` {}", self.path, self.message);
        out.push_str(&format!(" ({})", rect_text(self.rect)));
        let mut related = String::new();
        for rect in &self.related {
            if !related.is_empty() {
                related.push_str("; ");
            }
            related.push_str(&rect_text(*rect));
        }
        if !related.is_empty() {
            out.insert_str(out.len() - 1, &format!("; related: {related}"));
        }
        out.push_str(&format!(
            "; rule `{}` ({});",
            self.rule.name(),
            self.severity.name()
        ));
        if !self.cause.is_empty() {
            out.push_str(&format!(" because {};", self.cause));
        }
        out.push_str(&format!(" fix: {}.", self.fix));
        out
    }
}

/// The minimum contrast ratio a role must reach against its fill.
///
/// These are the documented theme thresholds: primary text is 7:1, secondary
/// text and on-accent labels are 4.5:1, tertiary text and the focus ring are
/// 3:1.
fn contrast_threshold(role: ColorRole) -> Option<f32> {
    match role {
        ColorRole::Text => Some(7.0),
        ColorRole::TextMuted | ColorRole::TextOnAccent | ColorRole::TextOnAccentFill => Some(4.5),
        ColorRole::TextFaint | ColorRole::FocusRing => Some(3.0),
        _ => None,
    }
}

/// The roles the contrast rule understands, in priority order.
const CONTRAST_ROLES: [ColorRole; 6] = [
    ColorRole::Text,
    ColorRole::TextMuted,
    ColorRole::TextFaint,
    ColorRole::TextOnAccent,
    ColorRole::TextOnAccentFill,
    ColorRole::FocusRing,
];

/// The physical-pixel slop a snapped rectangle may overshoot.
const EDGE_SLOP: f32 = 1.5;

/// The smallest overlap, on both axes, that counts as material.
const MATERIAL_OVERLAP: f32 = 1.0;

/// The spacing grid, in logical pixels.
const GRID: f32 = 4.0;

/// How close a value must be to the grid to count as on it.
const GRID_EPSILON: f32 = 0.01;

/// The smallest near miss worth reporting.
const NEAR_MIN: f32 = 1.0;

/// The largest near miss worth reporting.
const NEAR_MAX: f32 = 3.0;

/// The minimum interactive target, in logical pixels (WCAG 2.5.8).
const MIN_TARGET: f32 = 24.0;

/// How much float jitter a contrast comparison tolerates.
const CONTRAST_EPSILON: f32 = 0.001;

/// The read-only data a rule needs.
struct Ctx<'a, Msg> {
    tree: &'a Tree<Msg>,
    layout: &'a LayoutEngine,
    theme: &'a Theme,
}

impl<Msg> Ctx<'_, Msg> {
    fn rect(&self, id: NodeId) -> Rect {
        self.layout.rect(id)
    }

    fn resolved(&self, id: NodeId) -> Option<&ResolvedStyle> {
        self.layout.resolved(id)
    }

    fn path(&self, id: NodeId) -> String {
        self.tree.path(id)
    }

    /// Whether a node is painted at all (not hidden, not fully transparent).
    fn is_visible(&self, id: NodeId) -> bool {
        self.resolved(id)
            .is_some_and(|style| !style.hidden && style.opacity > 0.0)
    }

    /// The rectangle a node's children live in, inset by its padding.
    fn content_box(&self, id: NodeId) -> Rect {
        let rect = self.rect(id);
        let Some(style) = self.resolved(id) else {
            return rect;
        };
        let padding = style.padding;
        Rect::new(
            rect.x + padding.left,
            rect.y + padding.top,
            (rect.width - padding.left - padding.right).max(0.0),
            (rect.height - padding.top - padding.bottom).max(0.0),
        )
    }

    /// Why a node has the size it has, with `Px` values named explicitly.
    fn cause_text(&self, id: NodeId) -> String {
        let cause = self.layout.cause(id);
        let resolved = self.resolved(id);
        let width = cause.width.map_or_else(
            || size_label(resolved.map(|style| style.width), false),
            Cause::describe,
        );
        let height = cause.height.map_or_else(
            || size_label(resolved.map(|style| style.height), true),
            Cause::describe,
        );
        format!("width from {width}; height from {height}")
    }
}

fn size_label(size: Option<SizeSpec>, height: bool) -> String {
    match size {
        Some(SizeSpec::Px(value)) => {
            let axis = if height { "height" } else { "width" };
            format!("a fixed {axis}(Px({value:.0}))")
        }
        _ => "its content".to_string(),
    }
}

fn rect_text(rect: Rect) -> String {
    format!(
        "x {:.0}..{:.0}, y {:.0}..{:.0} ({:.0}x{:.0} px)",
        rect.x,
        rect.right(),
        rect.y,
        rect.bottom(),
        rect.width,
        rect.height
    )
}

/// Runs every rule against a painted [`Ui`].
///
/// The result is deterministic: rules run in a fixed order and each walks the
/// arena in tree order (or the shortcut registry in bind order).
#[must_use]
pub fn lint<Msg: Clone>(ui: &Ui<Msg>) -> Vec<Finding> {
    let ctx = Ctx {
        tree: ui.tree(),
        layout: ui.layout(),
        theme: ui.theme(),
    };
    let mut findings = Vec::new();
    check_overlap(&ctx, &mut findings);
    check_overflow(&ctx, &mut findings);
    check_clipped_text(&ctx, &mut findings);
    check_low_contrast(&ctx, &mut findings);
    check_px_escape(&ctx, &mut findings);
    check_off_scale_spacing(&ctx, &mut findings);
    check_near_miss(&ctx, &mut findings);
    check_inconsistent_radius(&ctx, &mut findings);
    check_inconsistent_elevation(&ctx, &mut findings);
    check_small_hit_target(&ctx, &mut findings);
    check_icon_only(&ctx, &mut findings);
    check_multiple_primary(&ctx, &mut findings);
    check_empty_container(&ctx, &mut findings);
    check_max_width(&ctx, &mut findings);
    check_horizontal_overflow(&ctx, &mut findings);
    check_focus_trap(ui, &ctx, &mut findings);
    check_unreachable_focusable(&ctx, &mut findings);
    check_duplicate_shortcuts(ui, &ctx, &mut findings);
    findings
}

/// Paints a view at `1280x800`, lints it and panics on a blocking finding.
///
/// The returned list includes the informational findings, so a caller can
/// inspect escape hatches and near misses even when the screen is clean.
///
/// # Panics
///
/// Panics with every blocking [`Finding::sentence`] when the painted view has a
/// `Warning` or `Error` finding.
#[must_use]
pub fn assert_clean<Msg: Clone>(view: &Element<Msg>, theme: &Theme) -> Vec<Finding> {
    let mut ui: Ui<Msg> = Ui::new(theme.clone());
    ui.set_viewport(Size::new(1280.0, 800.0), 1.0);
    let mut frame: Frame<Msg> = Frame::new();
    let mut text = MetricsTextSystem::new();
    ui.paint(view, &mut frame, &mut text);
    let findings = lint(&ui);
    let blocking: Vec<&Finding> = findings
        .iter()
        .filter(|finding| finding.severity.is_blocking())
        .collect();
    if !blocking.is_empty() {
        let mut report = format!(
            "design lint: {} blocking finding(s) at 1280x800:\n",
            blocking.len()
        );
        for finding in &blocking {
            report.push_str(&finding.sentence());
            report.push('\n');
        }
        panic!("{report}");
    }
    findings
}

fn check_overlap<Msg>(ctx: &Ctx<'_, Msg>, findings: &mut Vec<Finding>) {
    for parent in &ctx.tree.nodes {
        if matches!(parent.kind, NodeKind::Stack | NodeKind::Overlay) {
            continue;
        }
        let Some(parent_style) = ctx.resolved(parent.id) else {
            continue;
        };
        // A negative gap is the documented avatar-group overlap mechanism.
        if parent_style.gap < 0.0 {
            continue;
        }
        let children = &parent.children;
        for (index, &first) in children.iter().enumerate() {
            if !ctx.is_visible(first) || ctx.tree.nodes[first.index()].style.overlap_layer {
                continue;
            }
            let first_rect = ctx.rect(first);
            if first_rect.is_empty() {
                continue;
            }
            for &second in &children[index + 1..] {
                if !ctx.is_visible(second) || ctx.tree.nodes[second.index()].style.overlap_layer {
                    continue;
                }
                let second_rect = ctx.rect(second);
                if second_rect.is_empty() {
                    continue;
                }
                let overlap = first_rect.intersect(second_rect);
                if overlap.width <= MATERIAL_OVERLAP || overlap.height <= MATERIAL_OVERLAP {
                    continue;
                }
                let parent_path = ctx.path(parent.id);
                let content = ctx.layout.content_size(parent.id);
                let content_box = ctx.content_box(parent.id);
                findings.push(Finding {
                    severity: Severity::Error,
                    rule: Rule::Overlap,
                    path: ctx.path(first),
                    rect: first_rect,
                    related: vec![second_rect],
                    cause: format!(
                        "the parent `{parent_path}` has {}; its children need {:.0} px of height in a {:.0} px content box",
                        ctx.cause_text(parent.id),
                        content.height,
                        content_box.height
                    ),
                    message: format!(
                        "overlaps `{}` by {:.0} px horizontally and {:.0} px vertically",
                        ctx.path(second),
                        overlap.width,
                        overlap.height
                    ),
                    fix: "remove the fixed height or the anchored offset that makes the two share space; if the overlap is intentional, put both in a `stack` and mark the back layer `overlap_layer(true)` so the lint knows.".to_string(),
                });
            }
        }
    }
}

fn check_overflow<Msg>(ctx: &Ctx<'_, Msg>, findings: &mut Vec<Finding>) {
    for node in &ctx.tree.nodes {
        let Some(parent) = node.parent else {
            continue;
        };
        if !ctx.is_visible(node.id) || !ctx.is_visible(parent) {
            continue;
        }
        let Some(parent_style) = ctx.resolved(parent) else {
            continue;
        };
        // A clip or scroll container is allowed to receive larger children.
        if parent_style.clip || parent_style.scroll != ScrollAxes::None {
            continue;
        }
        // An anchored overlay and a background layer are placed on purpose.
        if node.style.anchor.is_some() || node.style.overlap_layer {
            continue;
        }
        // An anchored overlay may intentionally extend past its anchor; its
        // descendants are part of that overlay, not a broken child.
        if has_anchored_ancestor(ctx, node.id) {
            continue;
        }
        let rect = ctx.rect(node.id);
        if rect.is_empty() {
            continue;
        }
        let content_box = ctx.content_box(parent);
        if content_box.is_empty() {
            continue;
        }
        let escapes = [
            ("left", content_box.x - rect.x),
            ("top", content_box.y - rect.y),
            ("right", rect.right() - content_box.right()),
            ("bottom", rect.bottom() - content_box.bottom()),
        ];
        let Some((edge, amount)) = escapes
            .iter()
            .copied()
            .filter(|(_, amount)| *amount > EDGE_SLOP)
            .max_by(|left, right| left.1.total_cmp(&right.1))
        else {
            continue;
        };
        let parent_path = ctx.path(parent);
        findings.push(Finding {
            severity: Severity::Error,
            rule: Rule::Overflow,
            path: ctx.path(node.id),
            rect,
            related: vec![content_box],
            cause: format!(
                "{}; `{parent_path}`'s content box is {}",
                ctx.cause_text(node.id),
                rect_text(content_box)
            ),
            message: format!(
                "escapes `{parent_path}`'s content box by {amount:.0} px on the {edge}"
            ),
            fix: "give the child a size that fits (`Fill`, `Fraction`, `Hug`, or a smaller token), reduce its padding or the parent's gap, or wrap the parent in `scroll` so the overflow is scrollable instead of clipped.".to_string(),
        });
    }
}

fn check_clipped_text<Msg>(ctx: &Ctx<'_, Msg>, findings: &mut Vec<Finding>) {
    for node in &ctx.tree.nodes {
        if node.kind != NodeKind::Text || !ctx.is_visible(node.id) {
            continue;
        }
        let Some(spec) = &node.text else {
            continue;
        };
        if spec.max_lines.is_some() || spec.ellipsis != crate::text::Ellipsis::None {
            continue;
        }
        let Some(shape) = ctx.layout.shape(node.id) else {
            continue;
        };
        let content_box = ctx.content_box(node.id);
        if content_box.is_empty() {
            continue;
        }
        let over_width = shape.width - content_box.width;
        let over_height = shape.height - content_box.height;
        if over_width <= EDGE_SLOP && over_height <= EDGE_SLOP {
            continue;
        }
        findings.push(Finding {
            severity: Severity::Warning,
            rule: Rule::ClippedText,
            path: ctx.path(node.id),
            rect: ctx.rect(node.id),
            related: vec![],
            cause: format!(
                "{}; the shaped text is {:.0}x{:.0} px inside a {:.0}x{:.0} px content box",
                ctx.cause_text(node.id),
                shape.width,
                shape.height,
                content_box.width,
                content_box.height
            ),
            message: format!(
                "shapes to {:.0}x{:.0} px but its content box is {:.0}x{:.0} px, so the last lines are silently clipped",
                shape.width, shape.height, content_box.width, content_box.height
            ),
            fix: "add `lines(n)` with `ellipsis(Ellipsis::End)`, give the text a wider parent, or lower the type role; do not leave text that cannot fit without an explicit clamp.".to_string(),
        });
    }
}

fn check_low_contrast<Msg>(ctx: &Ctx<'_, Msg>, findings: &mut Vec<Finding>) {
    for node in &ctx.tree.nodes {
        if !ctx.is_visible(node.id) {
            continue;
        }
        let Some(style) = ctx.resolved(node.id) else {
            continue;
        };
        if node.kind == NodeKind::Icon {
            let color = style.tone.color(ctx.theme);
            if let Some(role) = role_for(ctx.theme, style.tone, color) {
                check_contrast(ctx, node.id, "icon", role, color, findings);
            }
        } else if node.kind == NodeKind::Text {
            let shaped = ctx
                .layout
                .shape(node.id)
                .is_some_and(|shape| !shape.glyphs.is_empty());
            if !shaped {
                continue;
            }
            let base = style.text.color;
            if let Some(role) = role_for(ctx.theme, style.tone, base) {
                check_contrast(ctx, node.id, "text", role, base, findings);
            }
            let Some(spec) = &node.text else {
                continue;
            };
            for (index, span) in spec.spans.iter().enumerate() {
                let Some(tone) = span.tone else {
                    continue;
                };
                let color = tone.color(ctx.theme);
                if color == base {
                    continue;
                }
                let Some(role) = role_for(ctx.theme, tone, color) else {
                    continue;
                };
                let path = format!("{}/span[{index}]", ctx.path(node.id));
                check_contrast_at(ctx, node.id, &path, "span", role, color, findings);
            }
        }
    }
}

/// The contrast role a tone declares, falling back to the documented colours.
///
/// A theme may reuse one colour for two roles (dark `text` is the same colour as
/// `text_on_accent_fill`), so the declared tone wins when its colour matches.
fn role_for(theme: &Theme, tone: Tone, color: Color) -> Option<ColorRole> {
    let declared = tone.role();
    if contrast_threshold(declared).is_some() && theme.role(declared) == color {
        return Some(declared);
    }
    CONTRAST_ROLES
        .into_iter()
        .find(|role| theme.role(*role) == color)
}

fn check_contrast<Msg>(
    ctx: &Ctx<'_, Msg>,
    id: NodeId,
    label: &str,
    role: ColorRole,
    color: Color,
    findings: &mut Vec<Finding>,
) {
    let path = ctx.path(id);
    check_contrast_at(ctx, id, &path, label, role, color, findings);
}

#[allow(clippy::too_many_arguments)] // reason: the readable-sentence fields are named at the call site
fn check_contrast_at<Msg>(
    ctx: &Ctx<'_, Msg>,
    id: NodeId,
    path: &str,
    label: &str,
    role: ColorRole,
    color: Color,
    findings: &mut Vec<Finding>,
) {
    let Some(threshold) = contrast_threshold(role) else {
        return;
    };
    let rect = ctx.rect(id);
    if rect.is_empty() {
        return;
    }
    let Some((fill, fill_role)) = composited_fill(ctx, id, rect) else {
        return;
    };
    let ratio = color.contrast_ratio(fill);
    if ratio + CONTRAST_EPSILON >= threshold {
        return;
    }
    let fill_name = fill_role.map_or_else(
        || "the composited background".to_string(),
        |role| format!("`{}`", role.name()),
    );
    findings.push(Finding {
        severity: Severity::Warning,
        rule: Rule::LowContrast,
        path: path.to_string(),
        rect,
        related: vec![],
        cause: format!(
            "the {label} colour of role `{}` reaches {ratio:.2}:1 against {fill_name}, and the role needs {threshold:.1}:1",
            role.name()
        ),
        message: format!(
            "{label} role `{}` is below the {threshold:.1}:1 minimum on {fill_name}",
            role.name()
        ),
        fix: "use a stronger tone (`Tone::Default` -> 7:1, `Tone::Muted` -> 4.5:1, `Tone::Faint` -> 3:1) or move the run onto a surface role that pairs with it.".to_string(),
    });
}

fn composited_fill<Msg>(
    ctx: &Ctx<'_, Msg>,
    id: NodeId,
    target: Rect,
) -> Option<(Color, Option<ColorRole>)> {
    let mut fill = ctx.theme.role(ColorRole::Bg);
    let mut role = None;
    for node in ctx.tree.nodes.iter().take(id.index() + 1) {
        let rect = ctx.rect(node.id);
        if rect.is_empty() || rect.intersect(target).is_empty() {
            continue;
        }
        if !ctx.is_visible(node.id) {
            continue;
        }
        // An image, a canvas or a blurred backdrop has no colour a lint can
        // composite, so the fill is unknown and the rule is skipped.
        if matches!(node.kind, NodeKind::Image | NodeKind::Canvas)
            || node.style.backdrop_blur.is_some()
        {
            return None;
        }
        if let Some(bg_role) = node.style.bg {
            let bg = ctx.theme.role(bg_role);
            fill = fill.lerp(bg, bg.alpha());
            role = Some(bg_role);
        }
    }
    Some((fill, role))
}

fn check_px_escape<Msg>(ctx: &Ctx<'_, Msg>, findings: &mut Vec<Finding>) {
    for node in &ctx.tree.nodes {
        let mut items: Vec<String> = Vec::new();
        node.style.width.for_each(|_, spec| {
            if let SizeSpec::Px(value) = *spec {
                items.push(format!("width(Px({value:.0}))"));
            }
        });
        node.style.height.for_each(|_, spec| {
            if let SizeSpec::Px(value) = *spec {
                items.push(format!("height(Px({value:.0}))"));
            }
        });
        if let Some(rule) = &node.style.gap_px {
            rule.for_each(|_, value| items.push(format!("gap_px({value:.0})")));
        }
        if items.is_empty() {
            continue;
        }
        findings.push(Finding {
            severity: Severity::Info,
            rule: Rule::PxEscapeHatch,
            path: ctx.path(node.id),
            rect: ctx.rect(node.id),
            related: vec![],
            cause: "the node uses explicit pixel values instead of spacing, size or type tokens"
                .to_string(),
            message: format!("uses the pixel escape hatch: {}", items.join(", ")),
            fix: "replace each pixel with the nearest token (`Spacing`, `SizeSpec::Token`, or a `Fluid` clamp); keep `Px` only for a value measured at runtime and add a comment explaining why.".to_string(),
        });
    }
}

fn check_off_scale_spacing<Msg>(ctx: &Ctx<'_, Msg>, findings: &mut Vec<Finding>) {
    for node in &ctx.tree.nodes {
        let Some(style) = ctx.resolved(node.id) else {
            continue;
        };
        let mut off: Vec<String> = Vec::new();
        let padding = style.padding;
        push_off_grid("padding.top", padding.top, &mut off);
        push_off_grid("padding.right", padding.right, &mut off);
        push_off_grid("padding.bottom", padding.bottom, &mut off);
        push_off_grid("padding.left", padding.left, &mut off);
        // A negative gap is the documented avatar-group overlap, not drift.
        if style.gap >= 0.0 {
            push_off_grid("gap", style.gap, &mut off);
        }
        for (name, value) in [
            ("min_width", style.min_width),
            ("min_height", style.min_height),
            ("max_width", style.max_width),
            ("max_height", style.max_height),
        ] {
            if let Some(value) = value {
                push_off_grid(name, value, &mut off);
            }
        }
        if off.is_empty() {
            continue;
        }
        findings.push(Finding {
            severity: Severity::Info,
            rule: Rule::OffScaleSpacing,
            path: ctx.path(node.id),
            rect: ctx.rect(node.id),
            related: vec![],
            cause: "raw pixel spacing and size values bypass the four-pixel token scale".to_string(),
            message: format!("resolves off the 4 px grid: {}", off.join(", ")),
            fix: "round each value to the nearest multiple of 4 and move it to a `Spacing` or theme token; keep only the documented negative `gap_px` overlap as an explicit exception.".to_string(),
        });
    }
}

fn push_off_grid(name: &str, value: f32, off: &mut Vec<String>) {
    if value.abs() <= f32::EPSILON {
        return;
    }
    let remainder = value.rem_euclid(GRID);
    if remainder <= GRID_EPSILON || (GRID - remainder) <= GRID_EPSILON {
        return;
    }
    off.push(format!("{name}={value:.0}"));
}

/// One rectangle edge accessor for the near-miss scan.
type Edge = fn(Rect) -> f32;

fn check_near_miss<Msg>(ctx: &Ctx<'_, Msg>, findings: &mut Vec<Finding>) {
    let axes: [(&'static str, Edge); 4] = [
        ("left", |rect| rect.x),
        ("right", Rect::right),
        ("top", |rect| rect.y),
        ("bottom", Rect::bottom),
    ];
    for parent in &ctx.tree.nodes {
        if parent.children.len() < 2
            || matches!(
                parent.kind,
                NodeKind::Stack | NodeKind::Overlay | NodeKind::Split
            )
        {
            continue;
        }
        let children: Vec<NodeId> = parent
            .children
            .iter()
            .copied()
            .filter(|id| {
                let node = &ctx.tree.nodes[id.index()];
                ctx.is_visible(*id) && node.style.anchor.is_none() && !node.style.overlap_layer
            })
            .collect();
        let mut best: Option<(f32, &'static str, NodeId, NodeId)> = None;
        for (edge_name, edge) in axes {
            for (index, &first) in children.iter().enumerate() {
                for &second in &children[index + 1..] {
                    let delta = (edge(ctx.rect(first)) - edge(ctx.rect(second))).abs();
                    if !(NEAR_MIN..=NEAR_MAX).contains(&delta) {
                        continue;
                    }
                    if best.is_none_or(|(best_gap, ..)| delta < best_gap) {
                        best = Some((delta, edge_name, first, second));
                    }
                }
            }
        }
        let Some((separation, edge_name, first, second)) = best else {
            continue;
        };
        findings.push(Finding {
            severity: Severity::Info,
            rule: Rule::NearMissAlignment,
            path: ctx.path(parent.id),
            rect: ctx.rect(parent.id),
            related: vec![ctx.rect(first), ctx.rect(second)],
            cause: format!(
                "the `{edge_name}` edges of `{}` and `{}` differ by {separation:.0} px",
                ctx.path(first),
                ctx.path(second)
            ),
            message: format!(
                "siblings `{}` and `{}` nearly align on their {edge_name} edges ({separation:.0} px apart)",
                ctx.path(first),
                ctx.path(second)
            ),
            fix: "give the siblings the same `align`/`justify`, padding, or size so the edges match exactly; a one-to-three pixel difference is a rounding slip, not a design choice.".to_string(),
        });
    }
}

/// One group of siblings that share a kind under the same parent.
type SiblingGroup = (NodeId, NodeKind, Vec<NodeId>);

/// Groups visible children by kind, skipping the kinds whose shape is not a
/// surface (text, icons, paths, spacers and dividers).
fn sibling_groups<Msg>(ctx: &Ctx<'_, Msg>) -> Vec<SiblingGroup> {
    let mut all: Vec<SiblingGroup> = Vec::new();
    for parent in &ctx.tree.nodes {
        let mut local: Vec<SiblingGroup> = Vec::new();
        for &child in &parent.children {
            if !ctx.is_visible(child) {
                continue;
            }
            let node = &ctx.tree.nodes[child.index()];
            if matches!(
                node.kind,
                NodeKind::Text
                    | NodeKind::Icon
                    | NodeKind::Path
                    | NodeKind::Spacer
                    | NodeKind::Divider
            ) {
                continue;
            }
            if let Some((_, _, list)) = local.iter_mut().find(|(_, kind, _)| *kind == node.kind) {
                list.push(child);
            } else {
                local.push((parent.id, node.kind, vec![child]));
            }
        }
        all.extend(local);
    }
    all
}

fn check_inconsistent_radius<Msg>(ctx: &Ctx<'_, Msg>, findings: &mut Vec<Finding>) {
    for (parent, kind, list) in sibling_groups(ctx) {
        if list.len() < 2 {
            continue;
        }
        let first = list[0];
        let Some(first_style) = ctx.resolved(first) else {
            continue;
        };
        let Some(outlier) = list.iter().copied().find(|id| {
            ctx.resolved(*id)
                .is_some_and(|style| style.radius != first_style.radius)
        }) else {
            continue;
        };
        let Some(style) = ctx.resolved(outlier) else {
            continue;
        };
        findings.push(Finding {
            severity: Severity::Info,
            rule: Rule::InconsistentRadius,
            path: ctx.path(outlier),
            rect: ctx.rect(outlier),
            related: list.iter().map(|id| ctx.rect(*id)).collect(),
            cause: format!(
                "`{}` siblings under `{}` resolve different radius tokens: `{}` vs `{}`",
                kind.name(),
                ctx.path(parent),
                radius_name(style.radius),
                radius_name(first_style.radius)
            ),
            message: format!(
                "radius `{}` differs from its `{}` siblings, which use `{}`",
                radius_name(style.radius),
                kind.name(),
                radius_name(first_style.radius)
            ),
            fix: "give every sibling of the same kind the same `radius` token; if the difference is intentional, put the outlier in a different kind (a `stack` wrapper) so the grouping is explicit.".to_string(),
        });
    }
}

fn check_inconsistent_elevation<Msg>(ctx: &Ctx<'_, Msg>, findings: &mut Vec<Finding>) {
    for (parent, kind, list) in sibling_groups(ctx) {
        if list.len() < 2 {
            continue;
        }
        let first = list[0];
        let Some(first_style) = ctx.resolved(first) else {
            continue;
        };
        let Some(outlier) = list.iter().copied().find(|id| {
            ctx.resolved(*id)
                .is_some_and(|style| style.elevation_level != first_style.elevation_level)
        }) else {
            continue;
        };
        let Some(style) = ctx.resolved(outlier) else {
            continue;
        };
        findings.push(Finding {
            severity: Severity::Info,
            rule: Rule::InconsistentElevation,
            path: ctx.path(outlier),
            rect: ctx.rect(outlier),
            related: list.iter().map(|id| ctx.rect(*id)).collect(),
            cause: format!(
                "`{}` siblings under `{}` resolve different elevation tokens: E{} vs E{}",
                kind.name(),
                ctx.path(parent),
                style.elevation_level.level(),
                first_style.elevation_level.level()
            ),
            message: format!(
                "elevation E{} differs from its `{}` siblings, which use E{}",
                style.elevation_level.level(),
                kind.name(),
                first_style.elevation_level.level()
            ),
            fix: "give every sibling of the same kind the same `elevation`; if the difference is intentional, make it a hover/selected role instead of a different rest level.".to_string(),
        });
    }
}

fn radius_name(radius: Radius) -> &'static str {
    match radius {
        Radius::None => "none",
        Radius::Xs => "xs",
        Radius::Sm => "sm",
        Radius::Md => "md",
        Radius::Lg => "lg",
        Radius::Xl => "xl",
        Radius::Full => "full",
    }
}

fn check_small_hit_target<Msg>(ctx: &Ctx<'_, Msg>, findings: &mut Vec<Finding>) {
    for node in &ctx.tree.nodes {
        if !(node.action.is_some() || node.style.interactive) || !ctx.is_visible(node.id) {
            continue;
        }
        let rect = ctx.rect(node.id);
        if rect.is_empty() {
            continue;
        }
        let narrow = rect.width < MIN_TARGET;
        let short = rect.height < MIN_TARGET;
        if !narrow && !short {
            continue;
        }
        let axis = match (narrow, short) {
            (true, true) => "width and height",
            (true, false) => "width",
            _ => "height",
        };
        findings.push(Finding {
            severity: Severity::Warning,
            rule: Rule::SmallHitTarget,
            path: ctx.path(node.id),
            rect,
            related: vec![],
            cause: format!(
                "content plus padding resolves to {:.0}x{:.0} px",
                rect.width, rect.height
            ),
            message: format!(
                "is {:.0}x{:.0} px, below the {MIN_TARGET:.0} px minimum on {axis}",
                rect.width, rect.height
            ),
            fix: "add `min_width(Spacing::S6)` and `min_height(Spacing::S6)` (or enough `pad`) so the target is at least 24x24 logical pixels.".to_string(),
        });
    }
}

fn check_icon_only<Msg>(ctx: &Ctx<'_, Msg>, findings: &mut Vec<Finding>) {
    for node in &ctx.tree.nodes {
        if !(node.action.is_some() || node.style.interactive)
            || node.style.tooltip.is_some()
            || !ctx.is_visible(node.id)
        {
            continue;
        }
        if !icon_only(ctx.tree, node.id) {
            continue;
        }
        findings.push(Finding {
            severity: Severity::Warning,
            rule: Rule::IconOnlyWithoutTooltip,
            path: ctx.path(node.id),
            rect: ctx.rect(node.id),
            related: vec![],
            cause: "the interaction paints only an icon and has no tooltip".to_string(),
            message: "is icon-only with no tooltip, so its action has no accessible name".to_string(),
            fix: "add `.tooltip(\"...\")` (or a visible text label) so the icon action has an accessible name; a screen reader and a first-time user both need it.".to_string(),
        });
    }
}

fn icon_only<Msg>(tree: &Tree<Msg>, id: NodeId) -> bool {
    let node = &tree.nodes[id.index()];
    if node.text.is_some() {
        return false;
    }
    if node.kind == NodeKind::Icon {
        return true;
    }
    if node.children.is_empty() {
        return false;
    }
    let mut saw_icon = false;
    for &child in &node.children {
        match tree.nodes[child.index()].kind {
            NodeKind::Icon => saw_icon = true,
            NodeKind::Spacer => {}
            _ => return false,
        }
    }
    saw_icon
}

fn check_multiple_primary<Msg>(ctx: &Ctx<'_, Msg>, findings: &mut Vec<Finding>) {
    for parent in &ctx.tree.nodes {
        // A split button is one composite primary action (a label plus its
        // menu affordance); its two accent children are one group.
        if parent.style.primary_group {
            continue;
        }
        let primaries: Vec<NodeId> = parent
            .children
            .iter()
            .copied()
            .filter(|id| {
                let node = &ctx.tree.nodes[id.index()];
                (node.action.is_some() || node.style.interactive)
                    && ctx.is_visible(*id)
                    && !ctx.resolved(*id).is_some_and(|style| style.disabled)
                    && is_filled_accent(node)
            })
            .collect();
        if primaries.len() < 2 {
            continue;
        }
        let second = primaries[1];
        let mut names = String::new();
        for id in &primaries {
            if !names.is_empty() {
                names.push_str(", ");
            }
            names.push_str(&format!("`{}`", ctx.path(*id)));
        }
        findings.push(Finding {
            severity: Severity::Warning,
            rule: Rule::MultiplePrimaryActions,
            path: ctx.path(second),
            rect: ctx.rect(second),
            related: primaries.iter().map(|id| ctx.rect(*id)).collect(),
            cause: format!(
                "{} filled-accent actions share the parent `{}`",
                primaries.len(),
                ctx.path(parent.id)
            ),
            message: format!(
                "is one of {} filled-accent actions under the same parent ({names}); a screen should have one primary action per group",
                primaries.len()
            ),
            fix: "keep one filled accent action and demote the others to `ButtonVariant::Secondary`/`Ghost` (a `Surface` or `SurfaceRaised` background), so the primary path is unambiguous.".to_string(),
        });
    }
}

fn is_filled_accent<Msg>(node: &crate::view::Node<Msg>) -> bool {
    // Only the state the user actually sees counts: a resting accent fill, or
    // the accent fill of a selected toggle. The hover/pressed/disabled roles
    // are state variants of the same control, not a second primary action.
    is_accent_fill(node.style.bg)
        || (node.style.selected && is_accent_fill(node.style.visual.bg.selected))
}

fn is_accent_fill(role: Option<ColorRole>) -> bool {
    matches!(
        role,
        Some(ColorRole::AccentFill | ColorRole::AccentFillHover | ColorRole::AccentFillPressed)
    )
}

fn check_empty_container<Msg>(ctx: &Ctx<'_, Msg>, findings: &mut Vec<Finding>) {
    for node in &ctx.tree.nodes {
        if !matches!(
            node.kind,
            NodeKind::Column | NodeKind::Row | NodeKind::Stack | NodeKind::Grid | NodeKind::Scroll
        ) || !node.children.is_empty()
            || !ctx.is_visible(node.id)
        {
            continue;
        }
        if node.text.is_some()
            || node.style.bg.is_some()
            || node.has_visual
            || node.image.is_some()
            || node.icon_path.is_some()
            || node.path_points.is_some()
        {
            continue;
        }
        // A deliberately empty cell (a fixed track, a flexible spacer) is a
        // sizing primitive, not an unfinished container.
        if node.style.grow > 0.0
            || ctx.resolved(node.id).is_some_and(|style| {
                matches!(
                    style.width,
                    SizeSpec::Token(_) | SizeSpec::Px(_) | SizeSpec::Fraction(_)
                ) || matches!(
                    style.height,
                    SizeSpec::Token(_) | SizeSpec::Px(_) | SizeSpec::Fraction(_)
                )
            })
        {
            continue;
        }
        findings.push(Finding {
            severity: Severity::Warning,
            rule: Rule::EmptyContainer,
            path: ctx.path(node.id),
            rect: ctx.rect(node.id),
            related: vec![],
            cause: "the node has no children and no background, text, image or path".to_string(),
            message: format!("is an empty `{}` container", node.kind.name()),
            fix: "remove the node, or give it content or a background; an empty layout node still costs a taffy node and a paint slot.".to_string(),
        });
    }
}

fn check_max_width<Msg>(ctx: &Ctx<'_, Msg>, findings: &mut Vec<Finding>) {
    for node in &ctx.tree.nodes {
        let Some(style) = ctx.resolved(node.id) else {
            continue;
        };
        let Some(max_width) = style.max_width else {
            continue;
        };
        let rect = ctx.rect(node.id);
        if rect.width > max_width + EDGE_SLOP {
            findings.push(Finding {
                severity: Severity::Error,
                rule: Rule::MaxWidthOverflow,
                path: ctx.path(node.id),
                rect,
                related: vec![],
                cause: format!(
                    "{}; the resolved maximum is {max_width:.0} px",
                    ctx.cause_text(node.id)
                ),
                message: format!(
                    "is {:.0} px wide, wider than its own max_width of {max_width:.0} px",
                    rect.width
                ),
                fix: "remove the conflicting `min_width` or fixed `width`, or raise `max_width` to the token the content really needs.".to_string(),
            });
        } else if node.style.content_width.is_some() && !style.clip {
            let content_width = ctx.layout.content_size(node.id).width;
            if content_width > max_width + EDGE_SLOP {
                findings.push(Finding {
                    severity: Severity::Error,
                    rule: Rule::MaxWidthOverflow,
                    path: ctx.path(node.id),
                    rect,
                    related: node.children.iter().map(|id| ctx.rect(*id)).collect(),
                    cause: format!(
                        "the content column needs {content_width:.0} px but its max_width is {max_width:.0} px"
                    ),
                    message: format!(
                        "content is {content_width:.0} px wide inside a {max_width:.0} px maximum column"
                    ),
                    fix: "raise `max_width` to a wider `ContentWidth` token, remove the child's fixed width, or wrap the content in `scroll`.".to_string(),
                });
            }
        }
    }
}

fn check_horizontal_overflow<Msg>(ctx: &Ctx<'_, Msg>, findings: &mut Vec<Finding>) {
    let Some(root) = ctx.tree.root else {
        return;
    };
    let viewport_right = ctx.rect(root).right();
    for node in &ctx.tree.nodes {
        if node.id == root || !ctx.is_visible(node.id) {
            continue;
        }
        if node.style.anchor.is_some() || node.style.overlap_layer {
            continue;
        }
        if has_anchored_ancestor(ctx, node.id) {
            continue;
        }
        let rect = ctx.rect(node.id);
        if rect.is_empty() || rect.right() <= viewport_right + EDGE_SLOP {
            continue;
        }
        if has_clipping_ancestor(ctx, node.id) {
            continue;
        }
        findings.push(Finding {
            severity: Severity::Error,
            rule: Rule::HorizontalOverflow,
            path: ctx.path(node.id),
            rect,
            related: vec![ctx.rect(root)],
            cause: format!(
                "{}; the root's right edge is {viewport_right:.0} px",
                ctx.cause_text(node.id)
            ),
            message: format!(
                "right edge is {:.0} px, beyond the {viewport_right:.0} px viewport",
                rect.right()
            ),
            fix: "constrain the child with `Fill`, `Fraction` or `Hug`, add a `max_width(ContentWidth::...)`, or wrap its row in `scroll`.".to_string(),
        });
    }
}

fn has_anchored_ancestor<Msg>(ctx: &Ctx<'_, Msg>, id: NodeId) -> bool {
    let mut current = ctx.tree.nodes.get(id.index()).and_then(|node| node.parent);
    while let Some(parent) = current {
        if ctx.tree.nodes[parent.index()].style.anchor.is_some() {
            return true;
        }
        current = ctx.tree.nodes[parent.index()].parent;
    }
    false
}

fn has_clipping_ancestor<Msg>(ctx: &Ctx<'_, Msg>, id: NodeId) -> bool {
    let mut current = ctx.tree.nodes.get(id.index()).and_then(|node| node.parent);
    while let Some(parent) = current {
        if let Some(style) = ctx.resolved(parent) {
            if style.clip || style.scroll != ScrollAxes::None {
                return true;
            }
        }
        current = ctx
            .tree
            .nodes
            .get(parent.index())
            .and_then(|node| node.parent);
    }
    false
}

fn check_focus_trap<Msg: Clone>(ui: &Ui<Msg>, ctx: &Ctx<'_, Msg>, findings: &mut Vec<Finding>) {
    let bindings = ui.shortcuts().bindings();
    for scope in &ui.input().focus_scopes {
        if !scope.trap || ctx.tree.node(scope.root).is_none() {
            continue;
        }
        let has_escape = bindings.iter().any(|binding| {
            binding.scope == Scope::Overlay && binding.chord.key == ShortcutKey::Escape
        });
        if has_escape {
            continue;
        }
        findings.push(Finding {
            severity: Severity::Warning,
            rule: Rule::FocusTrapWithoutExit,
            path: ctx.path(scope.root),
            rect: ctx.rect(scope.root),
            related: vec![],
            cause: "a trapping scope wraps the tab order at its ends; without an `Escape` binding the keyboard cannot leave it".to_string(),
            message: "traps Tab in this focus scope but no `Escape` binding exists in the `Overlay` scope".to_string(),
            fix: "bind `Escape` in `Scope::Overlay` (and pop the scope in its handler) so the keyboard has a way out of the trap.".to_string(),
        });
    }
}

fn check_unreachable_focusable<Msg>(ctx: &Ctx<'_, Msg>, findings: &mut Vec<Finding>) {
    for node in &ctx.tree.nodes {
        if !node.style.interactive {
            continue;
        }
        let Some(style) = ctx.resolved(node.id) else {
            continue;
        };
        let mut reasons: Vec<&str> = Vec::new();
        if style.hidden {
            reasons.push("hidden");
        }
        // A disabled control is deliberately unreachable; it is a valid
        // state, not a dead end. Only hidden/transparent controls are
        // reported.
        if style.opacity <= 0.0 {
            reasons.push("fully transparent");
        }
        if !reasons.is_empty() {
            findings.push(Finding {
                severity: Severity::Warning,
                rule: Rule::UnreachableFocusable,
                path: ctx.path(node.id),
                rect: ctx.rect(node.id),
                related: vec![],
                cause: format!(
                    "the node is interactive but {}",
                    reasons.join(" and ")
                ),
                message: format!(
                    "is interactive but {} while still carrying an action",
                    reasons.join(" and ")
                ),
                fix: "remove the interaction, or keep the node visible and enabled while it can receive input; a focusable control a user cannot see or use is a dead end.".to_string(),
            });
        }
    }
}

fn check_duplicate_shortcuts<Msg: Clone>(
    ui: &Ui<Msg>,
    ctx: &Ctx<'_, Msg>,
    findings: &mut Vec<Finding>,
) {
    let platform = ui.shortcuts().platform();
    let root_rect = ctx.tree.root.map_or(Rect::ZERO, |id| ctx.rect(id));
    for conflict in ui.shortcuts().conflicts() {
        let chord = conflict.chord.display(platform);
        let scope = scope_name(conflict.scope);
        let labels = conflict.bindings.join(", ");
        findings.push(Finding {
            severity: Severity::Warning,
            rule: Rule::DuplicateShortcut,
            path: format!("{chord} ({scope} scope)"),
            rect: root_rect,
            related: vec![],
            cause: "two bindings claim the same chord in the same scope; the last registration wins and the earlier action becomes unreachable".to_string(),
            message: format!("is bound more than once: {labels}"),
            fix: "bind a different chord, use a narrower scope, or clear the previous binding; keep one action per chord per scope.".to_string(),
        });
    }
}

fn scope_name(scope: Scope) -> &'static str {
    match scope {
        Scope::Global => "global",
        Scope::Focused => "focused",
        Scope::Overlay => "overlay",
    }
}

#[cfg(test)]
mod tests {
    use super::{assert_clean, lint, Finding, Rule, Severity};
    use crate::frame::{Frame, Ui};
    use crate::geom::{Rect, Size};
    use crate::icons::Icon;
    use crate::input::focus::FocusScope;
    use crate::input::shortcut::{Chord, Scope, ShortcutKey};
    use crate::recipe::start_screen;
    use crate::style::{Align, SizeSpec, Tone};
    use crate::text::MetricsTextSystem;
    use crate::theme::{ColorRole, ContentWidth, ElevationLevel, Radius, Spacing, Theme, TypeRole};
    use crate::view::{text, Element, NodeId};
    use glam::Vec2;

    fn root<Msg>() -> Element<Msg> {
        Element::column()
            .key("root")
            .width(SizeSpec::Fill)
            .height(SizeSpec::Fill)
            .bg(ColorRole::Bg)
            .pad(Spacing::S4)
    }

    fn clean_view() -> Element<()> {
        root()
            .gap(Spacing::S2)
            .child(text("Hello").text_role(TypeRole::Title))
            .child(text("Body copy").tone(Tone::Muted))
    }

    fn run<Msg: Clone>(view: &Element<Msg>) -> Vec<Finding> {
        let mut ui: Ui<Msg> = Ui::new(Theme::dark());
        ui.set_viewport(Size::new(1280.0, 800.0), 1.0);
        let mut frame: Frame<Msg> = Frame::new();
        let mut text = MetricsTextSystem::new();
        ui.paint(view, &mut frame, &mut text);
        lint(&ui)
    }

    fn has_rule(findings: &[Finding], rule: Rule) -> bool {
        findings.iter().any(|finding| finding.rule == rule)
    }

    fn escape_chord() -> Chord {
        Chord {
            key: ShortcutKey::Escape,
            control: false,
            shift: false,
            alt: false,
            super_key: false,
        }
    }

    #[test]
    fn severity_and_rule_names_are_stable() {
        assert_eq!(Severity::Info.name(), "info");
        assert_eq!(Severity::Warning.name(), "warning");
        assert_eq!(Severity::Error.name(), "error");
        assert!(!Severity::Info.is_blocking());
        assert!(Severity::Warning.is_blocking());
        assert!(Severity::Error.is_blocking());
        assert_eq!(Rule::LowContrast.name(), "low_contrast");
        assert_eq!(Rule::LowContrast.title(), "Low contrast");
    }

    #[test]
    fn a_sentence_names_path_rect_numbers_cause_and_fix() {
        let finding = Finding {
            severity: Severity::Error,
            rule: Rule::Overlap,
            path: "recent_card".to_string(),
            rect: Rect::new(10.0, 20.0, 100.0, 50.0),
            related: vec![Rect::new(10.0, 60.0, 100.0, 50.0)],
            cause: "the parent has a fixed height(Px(300))".to_string(),
            message: "overlaps `examples_row` by 18 px vertically".to_string(),
            fix: "remove the fixed height".to_string(),
        };
        let sentence = finding.sentence();
        assert!(sentence.contains("recent_card"), "{sentence}");
        assert!(sentence.contains("18 px"), "{sentence}");
        assert!(sentence.contains("x 10..110"), "{sentence}");
        assert!(sentence.contains("because"), "{sentence}");
        assert!(sentence.contains("fix:"), "{sentence}");
    }

    #[test]
    fn a_minimal_view_lints_clean() {
        let findings = assert_clean(&clean_view(), &Theme::dark());
        let blocking: Vec<&Finding> = findings
            .iter()
            .filter(|finding| finding.severity.is_blocking())
            .collect();
        assert!(
            blocking.is_empty(),
            "unexpected blocking findings: {blocking:?}"
        );
    }

    #[test]
    fn a_sibling_overlap_is_an_error() {
        let view: Element<()> = root()
            .child(
                Element::row()
                    .key("first")
                    .width(SizeSpec::Fill)
                    .height(SizeSpec::Token(Spacing::S4)),
            )
            .child(
                Element::row()
                    .key("second")
                    .width(SizeSpec::Token(Spacing::S16))
                    .height(SizeSpec::Token(Spacing::S4))
                    .overlay_to(0, Vec2::new(-8.0, 0.0)),
            );
        assert!(has_rule(&run(&view), Rule::Overlap));

        let clean: Element<()> = root()
            .child(
                Element::row()
                    .key("first")
                    .width(SizeSpec::Fill)
                    .height(SizeSpec::Token(Spacing::S4)),
            )
            .child(
                Element::row()
                    .key("second")
                    .width(SizeSpec::Fill)
                    .height(SizeSpec::Token(Spacing::S4)),
            );
        assert!(!has_rule(&run(&clean), Rule::Overlap));
    }

    #[test]
    fn a_child_taller_than_its_parent_overflows() {
        let view: Element<()> = root().child(
            Element::column()
                .key("box")
                .width(SizeSpec::Fill)
                .height(SizeSpec::Px(40.0))
                .child(
                    Element::row()
                        .key("tall")
                        .width(SizeSpec::Fill)
                        .height(SizeSpec::Px(200.0))
                        .shrink(0.0)
                        .bg(ColorRole::Surface),
                ),
        );
        assert!(has_rule(&run(&view), Rule::Overflow));

        let clean: Element<()> = root().child(
            Element::column()
                .key("box")
                .width(SizeSpec::Fill)
                .height(SizeSpec::Token(Spacing::S12))
                .child(
                    Element::row()
                        .key("short")
                        .width(SizeSpec::Fill)
                        .height(SizeSpec::Token(Spacing::S4))
                        .bg(ColorRole::Surface),
                ),
        );
        assert!(!has_rule(&run(&clean), Rule::Overflow));
    }

    #[test]
    fn text_in_a_short_box_is_clipped() {
        let view: Element<()> = root().child(
            text("A rather long label that needs two lines")
                .key("label")
                .width(SizeSpec::Px(120.0))
                .height(SizeSpec::Px(6.0)),
        );
        assert!(has_rule(&run(&view), Rule::ClippedText));

        let clean: Element<()> = root()
            .child(text("A rather long label that needs two lines").width(SizeSpec::Px(120.0)));
        assert!(!has_rule(&run(&clean), Rule::ClippedText));
    }

    #[test]
    fn faint_text_on_the_accent_fill_is_low_contrast() {
        let view: Element<()> = root().child(
            text("Low contrast")
                .key("low")
                .tone(Tone::Faint)
                .bg(ColorRole::AccentFill)
                .pad(Spacing::S2),
        );
        assert!(has_rule(&run(&view), Rule::LowContrast));

        let clean: Element<()> = root().child(
            text("On accent")
                .tone(Tone::OnAccentFill)
                .bg(ColorRole::AccentFill)
                .pad(Spacing::S2),
        );
        assert!(!has_rule(&run(&clean), Rule::LowContrast));
    }

    #[test]
    fn a_pixel_size_is_an_escape_hatch() {
        let view: Element<()> = root().child(
            Element::row()
                .key("px")
                .width(SizeSpec::Px(120.0))
                .height(SizeSpec::Token(Spacing::S4))
                .bg(ColorRole::Surface),
        );
        assert!(has_rule(&run(&view), Rule::PxEscapeHatch));

        let clean: Element<()> = root().child(
            Element::row()
                .width(SizeSpec::Token(Spacing::S24))
                .height(SizeSpec::Token(Spacing::S4))
                .bg(ColorRole::Surface),
        );
        assert!(!has_rule(&run(&clean), Rule::PxEscapeHatch));
    }

    #[test]
    fn a_six_pixel_gap_is_off_the_grid_but_a_negative_gap_is_exempt() {
        let view: Element<()> = root().gap_px(6.0).child(text("one")).child(text("two"));
        assert!(has_rule(&run(&view), Rule::OffScaleSpacing));

        let overlap: Element<()> = root().gap_px(-6.0).child(text("one")).child(text("two"));
        assert!(!has_rule(&run(&overlap), Rule::OffScaleSpacing));

        let clean: Element<()> = root()
            .gap(Spacing::S2)
            .child(text("one"))
            .child(text("two"));
        assert!(!has_rule(&run(&clean), Rule::OffScaleSpacing));
    }

    #[test]
    fn centred_rows_two_pixels_apart_are_a_near_miss() {
        let view: Element<()> = Element::row()
            .key("root")
            .width(SizeSpec::Fill)
            .height(SizeSpec::Fill)
            .bg(ColorRole::Bg)
            .pad(Spacing::S4)
            .align(Align::Center)
            .child(
                Element::row()
                    .key("short")
                    .width(SizeSpec::Token(Spacing::S4))
                    .height(SizeSpec::Token(Spacing::S5))
                    .bg(ColorRole::Surface),
            )
            .child(
                Element::row()
                    .key("tall")
                    .width(SizeSpec::Token(Spacing::S4))
                    .height(SizeSpec::Token(Spacing::S6))
                    .bg(ColorRole::Surface),
            );
        assert!(has_rule(&run(&view), Rule::NearMissAlignment));

        let clean: Element<()> = Element::row()
            .key("root")
            .width(SizeSpec::Fill)
            .height(SizeSpec::Fill)
            .bg(ColorRole::Bg)
            .pad(Spacing::S4)
            .align(Align::Center)
            .child(
                Element::row()
                    .width(SizeSpec::Token(Spacing::S4))
                    .height(SizeSpec::Token(Spacing::S5))
                    .bg(ColorRole::Surface),
            )
            .child(
                Element::row()
                    .width(SizeSpec::Token(Spacing::S4))
                    .height(SizeSpec::Token(Spacing::S5))
                    .bg(ColorRole::Surface),
            );
        assert!(!has_rule(&run(&clean), Rule::NearMissAlignment));
    }

    #[test]
    fn siblings_with_different_radii_are_reported() {
        let card = |key: &'static str, radius: Radius| {
            Element::row()
                .key(key)
                .width(SizeSpec::Token(Spacing::S6))
                .height(SizeSpec::Token(Spacing::S6))
                .bg(ColorRole::Surface)
                .radius(radius)
        };
        let view: Element<()> = root()
            .child(card("a", Radius::Sm))
            .child(card("b", Radius::Md));
        assert!(has_rule(&run(&view), Rule::InconsistentRadius));

        let clean: Element<()> = root()
            .child(card("a", Radius::Sm))
            .child(card("b", Radius::Sm));
        assert!(!has_rule(&run(&clean), Rule::InconsistentRadius));
    }

    #[test]
    fn siblings_with_different_elevations_are_reported() {
        let card = |key: &'static str, level: ElevationLevel| {
            Element::row()
                .key(key)
                .width(SizeSpec::Token(Spacing::S6))
                .height(SizeSpec::Token(Spacing::S6))
                .bg(ColorRole::Surface)
                .elevation(level)
        };
        let view: Element<()> = root()
            .child(card("a", ElevationLevel::E1))
            .child(card("b", ElevationLevel::E2));
        assert!(has_rule(&run(&view), Rule::InconsistentElevation));

        let clean: Element<()> = root()
            .child(card("a", ElevationLevel::E1))
            .child(card("b", ElevationLevel::E1));
        assert!(!has_rule(&run(&clean), Rule::InconsistentElevation));
    }

    #[test]
    fn a_twenty_pixel_target_is_too_small() {
        let view: Element<()> = root().child(
            Element::row()
                .key("tiny")
                .width(SizeSpec::Px(20.0))
                .height(SizeSpec::Px(20.0))
                .bg(ColorRole::Surface)
                .on_click(()),
        );
        assert!(has_rule(&run(&view), Rule::SmallHitTarget));

        let clean: Element<()> = root().child(
            Element::row()
                .width(SizeSpec::Px(32.0))
                .height(SizeSpec::Px(32.0))
                .bg(ColorRole::Surface)
                .on_click(()),
        );
        assert!(!has_rule(&run(&clean), Rule::SmallHitTarget));
    }

    #[test]
    fn an_icon_action_without_a_tooltip_is_reported() {
        let view: Element<()> = root().child(
            Element::icon(Icon::Search.path())
                .key("icon")
                .width(SizeSpec::Token(Spacing::S6))
                .height(SizeSpec::Token(Spacing::S6))
                .on_click(()),
        );
        assert!(has_rule(&run(&view), Rule::IconOnlyWithoutTooltip));

        let clean: Element<()> = root().child(
            Element::icon(Icon::Search.path())
                .width(SizeSpec::Token(Spacing::S6))
                .height(SizeSpec::Token(Spacing::S6))
                .tooltip("Search")
                .on_click(()),
        );
        assert!(!has_rule(&run(&clean), Rule::IconOnlyWithoutTooltip));
    }

    #[test]
    fn two_filled_accent_actions_under_one_parent_are_reported() {
        let primary = |key: &'static str| {
            text(key)
                .pad_x(Spacing::S3)
                .pad_y(Spacing::S3)
                .on_accent_fill()
                .on_click(())
        };
        let view: Element<()> = root().child(primary("a")).child(primary("b"));
        assert!(has_rule(&run(&view), Rule::MultiplePrimaryActions));

        let clean: Element<()> = root().child(primary("a")).child(
            text("b")
                .pad_x(Spacing::S3)
                .pad_y(Spacing::S3)
                .bg(ColorRole::SurfaceRaised)
                .on_click(()),
        );
        assert!(!has_rule(&run(&clean), Rule::MultiplePrimaryActions));
    }

    #[test]
    fn an_empty_container_is_reported() {
        let view: Element<()> = root().child(Element::column().key("empty"));
        assert!(has_rule(&run(&view), Rule::EmptyContainer));

        // A fixed-size empty cell is a track/spacer, not an unfinished
        // container; the rule must not flood every menu shortcut column.
        let fixed: Element<()> = root().child(
            Element::column()
                .key("cell")
                .width(SizeSpec::Token(Spacing::S4))
                .height(SizeSpec::Token(Spacing::S4)),
        );
        assert!(!has_rule(&run(&fixed), Rule::EmptyContainer));

        let clean: Element<()> = root().child(
            Element::column()
                .width(SizeSpec::Fill)
                .height(SizeSpec::Token(Spacing::S4))
                .bg(ColorRole::Surface),
        );
        assert!(!has_rule(&run(&clean), Rule::EmptyContainer));
    }

    #[test]
    fn a_content_column_wider_than_its_maximum_is_reported() {
        let view: Element<()> = root().child(
            Element::column()
                .key("prose")
                .width(SizeSpec::Fill)
                .content_width(ContentWidth::Sm)
                .align(Align::Stretch)
                .child(
                    Element::row()
                        .key("wide")
                        .width(SizeSpec::Px(600.0))
                        .height(SizeSpec::Token(Spacing::S4))
                        .bg(ColorRole::Surface),
                ),
        );
        assert!(has_rule(&run(&view), Rule::MaxWidthOverflow));

        let clean: Element<()> = root().child(
            Element::column()
                .key("prose")
                .width(SizeSpec::Fill)
                .content_width(ContentWidth::Sm)
                .align(Align::Stretch)
                .child(
                    Element::row()
                        .key("wide")
                        .width(SizeSpec::Fill)
                        .height(SizeSpec::Token(Spacing::S4))
                        .bg(ColorRole::Surface),
                ),
        );
        assert!(!has_rule(&run(&clean), Rule::MaxWidthOverflow));
    }

    #[test]
    fn a_child_past_the_viewport_edge_is_reported() {
        let view: Element<()> = root().child(
            Element::row()
                .key("wide")
                .width(SizeSpec::Px(2000.0))
                .height(SizeSpec::Token(Spacing::S4))
                .bg(ColorRole::Surface),
        );
        assert!(has_rule(&run(&view), Rule::HorizontalOverflow));

        let clean: Element<()> = root().child(
            Element::row()
                .key("wide")
                .width(SizeSpec::Token(Spacing::S24))
                .height(SizeSpec::Token(Spacing::S4))
                .bg(ColorRole::Surface),
        );
        assert!(!has_rule(&run(&clean), Rule::HorizontalOverflow));
    }

    #[test]
    fn a_trapping_scope_needs_an_escape_binding() {
        let view: Element<()> = clean_view();
        let mut ui: Ui<()> = Ui::new(Theme::dark());
        ui.set_viewport(Size::new(1280.0, 800.0), 1.0);
        let mut frame: Frame<()> = Frame::new();
        let mut text = MetricsTextSystem::new();
        ui.paint(&view, &mut frame, &mut text);
        ui.input_mut().push_focus_scope(FocusScope {
            root: NodeId(0),
            trap: true,
        });
        assert!(has_rule(&lint(&ui), Rule::FocusTrapWithoutExit));

        ui.shortcuts_mut().bind(escape_chord(), Scope::Overlay, ());
        assert!(!has_rule(&lint(&ui), Rule::FocusTrapWithoutExit));
    }

    #[test]
    fn a_hidden_interactive_node_is_unreachable() {
        let view: Element<()> = root().child(
            Element::row()
                .key("hidden")
                .width(SizeSpec::Token(Spacing::S6))
                .height(SizeSpec::Token(Spacing::S6))
                .bg(ColorRole::Surface)
                .on_click(())
                .hidden(),
        );
        assert!(has_rule(&run(&view), Rule::UnreachableFocusable));

        let clean: Element<()> = root().child(
            Element::row()
                .width(SizeSpec::Token(Spacing::S6))
                .height(SizeSpec::Token(Spacing::S6))
                .bg(ColorRole::Surface)
                .on_click(()),
        );
        assert!(!has_rule(&run(&clean), Rule::UnreachableFocusable));
    }

    #[test]
    fn a_duplicate_shortcut_is_reported() {
        let view: Element<()> = clean_view();
        let mut ui: Ui<()> = Ui::new(Theme::dark());
        ui.set_viewport(Size::new(1280.0, 800.0), 1.0);
        let mut frame: Frame<()> = Frame::new();
        let mut text = MetricsTextSystem::new();
        ui.paint(&view, &mut frame, &mut text);
        let chord = escape_chord();
        ui.shortcuts_mut().bind(chord, Scope::Global, ());
        assert!(!has_rule(&lint(&ui), Rule::DuplicateShortcut));
        ui.shortcuts_mut().bind(chord, Scope::Global, ());
        assert!(has_rule(&lint(&ui), Rule::DuplicateShortcut));
    }

    #[test]
    fn the_start_screen_has_no_blocking_findings() {
        let findings = assert_clean(&start_screen(0.0), &Theme::dark());
        let blocking: Vec<&Finding> = findings
            .iter()
            .filter(|finding| finding.severity.is_blocking())
            .collect();
        assert!(blocking.is_empty(), "start screen findings: {blocking:?}");
    }
}
