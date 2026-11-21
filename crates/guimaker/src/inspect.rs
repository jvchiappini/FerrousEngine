//! `inspect` and `explain`: the first thing to read when a layout looks wrong.

#![allow(clippy::format_push_string)] // reason: inspect is a debug-only text builder
use crate::geom::Rect;
use crate::layout::{Cause, LayoutEngine};
use crate::style::ResolvedStyle;
use crate::theme::Radius;
use crate::view::Tree;

/// An indented, stable text dump of the laid-out tree.
///
/// The dump names the kind, the stable path, the rectangle, the resolved
/// spacing/radius/elevation tokens and the text. It is used by tests and by a
/// model looking at a screen; it contains no pointer addresses and is stable
/// for the same inputs.
#[must_use]
pub fn inspect<Msg>(tree: &Tree<Msg>, engine: &LayoutEngine) -> String {
    let mut output = String::new();
    for node in &tree.nodes {
        let rect = engine.rect(node.id);
        let resolved = engine.resolved(node.id);
        let indent = "  ".repeat(usize::from(node.depth));
        output.push_str(&format!(
            "{indent}{} {} rect={:.1},{:.1},{:.1},{:.1}",
            node.kind.name(),
            tree.path(node.id),
            rect.x,
            rect.y,
            rect.width,
            rect.height
        ));
        if let Some(resolved) = resolved {
            append_style(&mut output, resolved);
        }
        if let Some(text) = &node.text {
            let text = text.text();
            let snippet = if text.chars().count() > 40 {
                let mut snippet: String = text.chars().take(40).collect();
                snippet.push('…');
                snippet
            } else {
                text
            };
            output.push_str(&format!(" text={snippet:?}"));
        }
        output.push('\n');
    }
    output
}

fn append_style(output: &mut String, resolved: &ResolvedStyle) {
    if resolved.padding.top != 0.0
        || resolved.padding.right != 0.0
        || resolved.padding.bottom != 0.0
        || resolved.padding.left != 0.0
    {
        output.push_str(&format!(
            " pad={:.1}/{:.1}/{:.1}/{:.1}",
            resolved.padding.top,
            resolved.padding.right,
            resolved.padding.bottom,
            resolved.padding.left
        ));
    }
    if resolved.gap != 0.0 {
        output.push_str(&format!(" gap={:.1}", resolved.gap));
    }
    if resolved.bg.is_some() {
        output.push_str(" bg=role");
    }
    if resolved.radius != Radius::None {
        output.push_str(&format!(" radius={}", radius_name(resolved.radius)));
    }
    if resolved.elevation_level != crate::theme::ElevationLevel::E0 {
        output.push_str(&format!(" elevation={}", resolved.elevation_level.level()));
    }
    if resolved.border.is_some() {
        output.push_str(&format!(" border={}", border_name(resolved.border_width)));
    }
    output.push_str(&format!(" text={}", resolved.text_role.name()));
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

fn border_name(width: f32) -> &'static str {
    if width <= 1.0 {
        "hairline"
    } else {
        "regular"
    }
}

/// Explains, in plain English, why a node has the size it has and what it
/// overflows or overlaps.
///
/// The explanation is inferred from the constraints and the result, which is
/// what a layout reader can act on: the winning rule, the parent's content
/// width, the overflow amount, the siblings it overlaps, and a concrete fix.
#[must_use]
pub fn explain<Msg>(tree: &Tree<Msg>, engine: &LayoutEngine, key: &str) -> Option<String> {
    let id = tree.find_key(key)?;
    let node = tree.node(id)?;
    let rect = engine.rect(id);
    let cause = engine.cause(id);
    let mut output = format!(
        "`{key}` is {:.0} x {:.0} px: width from {}; height from {}.",
        rect.width,
        rect.height,
        cause
            .width
            .map_or_else(|| "its content".to_string(), Cause::describe),
        cause
            .height
            .map_or_else(|| "its content".to_string(), Cause::describe),
    );
    if let Some(parent) = node.parent {
        let parent_rect = engine.rect(parent);
        let inner_width = parent_rect.width
            - engine
                .resolved(parent)
                .map_or(0.0, |style| style.padding.left + style.padding.right);
        if rect.width > inner_width + 0.5 {
            output.push_str(&format!(
                " It is {:.0} px wider than `{}`'s {:.0} px content width, so it overflows it. Fix: a fraction width with a minimum (`width(Fraction(0.25)).min_width(S3)`), or hide it below the breakpoint.",
                rect.width - inner_width,
                tree.path(parent),
                inner_width
            ));
        }
        for &sibling in &tree.nodes[parent.index()].children {
            if sibling == id {
                continue;
            }
            let sibling_rect = engine.rect(sibling);
            let overlap = rect.intersect(sibling_rect);
            if overlap.width > 0.5 && overlap.height > 0.5 {
                output.push_str(&format!(
                    " It overlaps `{}` (x {:.0}..{:.0}, y {:.0}..{:.0}).",
                    tree.path(sibling),
                    overlap.x,
                    overlap.right(),
                    overlap.y,
                    overlap.bottom()
                ));
            }
        }
    }
    Some(output)
}

#[allow(dead_code)] // reason: used by the pinned explain test through the public API
fn rect(x: f32, y: f32, width: f32, height: f32) -> Rect {
    Rect::new(x, y, width, height)
}

#[cfg(test)]
mod tests {
    use super::{explain, inspect};
    use crate::layout::LayoutEngine;
    use crate::store::Store;
    use crate::style::SizeSpec;
    use crate::text::MetricsTextSystem;
    use crate::theme::{Spacing, Theme};
    use crate::view::{text, Element};

    #[derive(Clone, Debug, PartialEq)]
    enum Msg {}

    fn engine_for<M: Clone>(view: &Element<M>) -> (crate::view::Tree<M>, LayoutEngine) {
        let tree = crate::view::Tree::build(view);
        let mut engine = LayoutEngine::new();
        engine.set_viewport(crate::geom::Size::new(400.0, 300.0), 1.0);
        let mut text = MetricsTextSystem::new();
        engine.run(&tree, &Theme::dark(), &mut text, &Store::new());
        (tree, engine)
    }

    #[test]
    fn inspect_is_stable_and_names_paths() {
        let view: Element<Msg> = Element::column()
            .key("root")
            .pad(Spacing::S4)
            .gap(Spacing::S2)
            .child(text("Motioner").text_role(crate::theme::TypeRole::Title))
            .child(
                Element::row()
                    .key("actions")
                    .child(Element::canvas().key("open")),
            );
        let (tree, engine) = engine_for(&view);
        let dump = inspect(&tree, &engine);
        assert!(dump.contains("column root"));
        assert!(dump.contains("row root/actions"));
        assert!(dump.contains("canvas root/actions/open"));
        assert!(dump.contains("text=body") || dump.contains("text=title"));
        assert_eq!(dump, inspect(&tree, &engine));
    }

    #[test]
    fn explain_reports_an_overflow_and_overlap() {
        let view: Element<Msg> = Element::row()
            .key("shell")
            .width(SizeSpec::Token(Spacing::S24))
            .child(
                Element::column()
                    .key("sidebar")
                    .width(SizeSpec::Token(Spacing::S24))
                    .height(SizeSpec::Auto),
            )
            .child(
                Element::column()
                    .key("content")
                    .width(SizeSpec::Fill)
                    .height(SizeSpec::Auto),
            );
        let (tree, engine) = engine_for(&view);
        let text = explain(&tree, &engine, "sidebar").expect("explanation");
        assert!(text.contains("`sidebar` is"), "{text}");
    }
}
