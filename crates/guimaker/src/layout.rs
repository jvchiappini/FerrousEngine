//! Layout: taffy 0.7.7, memoised by a content hash, with per-node damage.
//!
//! The engine keeps the taffy tree between frames. A node whose
//! [`crate::view::Node::content_hash`] and available size are unchanged is not re-synced and
//! is not visited at all when its whole subtree is unchanged, so a still view
//! does no layout work. A changed node re-syncs its style and children and
//! taffy recomputes only what its cache invalidates. The result is snapped to
//! the physical pixel grid of the host scale, and the previous rectangles are
//! diffed into the frame's damage list.

#![allow(clippy::too_many_lines, clippy::field_reassign_with_default)] // reason: the taffy style mapping and the frame pipeline are one linear pass each
use std::sync::Arc;

use glam::Vec2;
use taffy::prelude::{
    AlignItems, AvailableSpace, Dimension, Display, FlexDirection, FlexWrap, JustifyContent,
    LengthPercentage, LengthPercentageAuto, Position, Size as TSize, Style as TaffyStyle,
    TaffyTree,
};
use taffy::style::{
    GridTrackRepetition, MaxTrackSizingFunction, MinTrackSizingFunction,
    NonRepeatedTrackSizingFunction, Overflow, TrackSizingFunction,
};
use taffy::Point;

use crate::geom::{Rect, Size};
use crate::hash::Fnv;
use crate::store::{RepaintReason, Store};
use crate::style::{Align, Columns, Direction, Justify, ResolvedStyle, ScrollAxes, SizeSpec};
use crate::text::{TextShape, TextSystem};
use crate::theme::{Spacing, Theme};
use crate::view::{NodeId, NodeKind, Tree};

/// What won for one axis of a node.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Cause {
    /// A fixed size (`Px` or a spacing token).
    Fixed,
    /// A spacing token.
    Token(Spacing),
    /// A fraction of the parent.
    Fraction(f32),
    /// A grow share of the free space.
    Grow,
    /// The content's own size.
    Content,
    /// A minimum clamped the size.
    Min,
    /// A maximum clamped the size.
    Max,
    /// The root filled the viewport.
    Viewport,
}

impl Cause {
    /// A plain-English description.
    #[must_use]
    pub fn describe(self) -> String {
        match self {
            Self::Fixed => "a fixed size".to_string(),
            Self::Token(spacing) => format!("the {} spacing token", spacing.name()),
            Self::Fraction(value) => format!("a {:.0}% fraction", value * 100.0),
            Self::Grow => "its grow share of the free space".to_string(),
            Self::Content => "its content size".to_string(),
            Self::Min => "a minimum that is larger than the content".to_string(),
            Self::Max => "a maximum that is smaller than the content".to_string(),
            Self::Viewport => "the viewport".to_string(),
        }
    }
}

/// The inferred causes of a node's width and height.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct AxisCause {
    /// Why the width is what it is.
    pub width: Option<Cause>,
    /// Why the height is what it is.
    pub height: Option<Cause>,
}

/// Layout counters for one frame.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct LayoutStats {
    /// Nodes in the tree.
    pub nodes: usize,
    /// Taffy styles written this frame.
    pub style_updates: usize,
    /// Taffy nodes created (a tree rebuild).
    pub nodes_created: usize,
    /// Text measurements this frame.
    pub measure_calls: usize,
    /// Nodes skipped because their whole subtree was unchanged.
    pub reused: usize,
    /// Rectangles in the damage list.
    pub damage_rects: usize,
}

#[derive(Clone, Copy, Debug)]
struct TaffyEntry {
    id: taffy::NodeId,
    hash: u64,
}

/// The layout engine. One per [`crate::Ui`], reused across frames.
pub struct LayoutEngine {
    taffy: TaffyTree<NodeId>,
    root: Option<taffy::NodeId>,
    entries: Vec<Option<TaffyEntry>>,
    hashes: Vec<u64>,
    changed: Vec<bool>,
    resolved: Vec<ResolvedStyle>,
    texts: Vec<Option<Arc<str>>>,
    shapes: Vec<Option<TextShape>>,
    shape_keys: Vec<u64>,
    rects: Vec<Rect>,
    previous: Vec<Rect>,
    damage: Vec<Rect>,
    repaints: Vec<RepaintReason>,
    causes: Vec<AxisCause>,
    viewport: Size,
    scale: f32,
    structure_hash: u64,
    first_frame: bool,
    viewport_dirty: bool,
    stats: LayoutStats,
}

impl Default for LayoutEngine {
    fn default() -> Self {
        Self::new()
    }
}

impl LayoutEngine {
    /// Builds an empty engine.
    #[must_use]
    pub fn new() -> Self {
        Self {
            taffy: TaffyTree::new(),
            root: None,
            entries: Vec::new(),
            hashes: Vec::new(),
            changed: Vec::new(),
            resolved: Vec::new(),
            texts: Vec::new(),
            shapes: Vec::new(),
            shape_keys: Vec::new(),
            rects: Vec::new(),
            previous: Vec::new(),
            damage: Vec::new(),
            repaints: Vec::new(),
            causes: Vec::new(),
            viewport: Size::ZERO,
            scale: 1.0,
            structure_hash: 0,
            first_frame: true,
            viewport_dirty: true,
            stats: LayoutStats::default(),
        }
    }

    /// Sets the viewport and scale; a change invalidates all layout.
    pub fn set_viewport(&mut self, viewport: Size, scale: f32) {
        if viewport != self.viewport || (scale - self.scale).abs() > f32::EPSILON {
            self.viewport = viewport;
            self.scale = scale;
            self.viewport_dirty = true;
        }
    }

    /// The current rectangles, indexed by [`NodeId`].
    #[must_use]
    pub fn rects(&self) -> &[Rect] {
        &self.rects
    }

    /// The rectangle of a node.
    #[must_use]
    pub fn rect(&self, id: NodeId) -> Rect {
        self.rects.get(id.index()).copied().unwrap_or(Rect::ZERO)
    }

    /// This frame's damage.
    #[must_use]
    pub fn damage(&self) -> &[Rect] {
        &self.damage
    }

    /// Clears the damage after the caller consumed it.
    pub fn clear_damage(&mut self) {
        self.damage.clear();
    }

    /// Records a frame that reused the previous layout.
    pub fn mark_reused(&mut self, nodes: usize) {
        self.stats = LayoutStats {
            nodes,
            reused: nodes,
            ..LayoutStats::default()
        };
    }

    /// Why a node repainted this frame.
    #[must_use]
    pub fn repaint(&self, id: NodeId) -> RepaintReason {
        self.repaints
            .get(id.index())
            .copied()
            .unwrap_or(RepaintReason::None)
    }

    /// The resolved style of a node.
    #[must_use]
    pub fn resolved(&self, id: NodeId) -> Option<&ResolvedStyle> {
        self.resolved.get(id.index())
    }

    /// The shaped text of a text node.
    #[must_use]
    pub fn shape(&self, id: NodeId) -> Option<&TextShape> {
        self.shapes.get(id.index()).and_then(Option::as_ref)
    }

    /// The inferred cause of a node's size.
    #[must_use]
    pub fn cause(&self, id: NodeId) -> AxisCause {
        self.causes.get(id.index()).copied().unwrap_or_default()
    }

    /// This frame's counters.
    #[must_use]
    pub fn stats(&self) -> LayoutStats {
        self.stats
    }

    /// Runs layout for a tree.
    ///
    /// # Panics
    ///
    /// Panics only if an internal vector is shorter than the tree's arena, an
    /// invariant restored at the start of the function.
    pub fn run<Msg>(
        &mut self,
        tree: &Tree<Msg>,
        theme: &Theme,
        text_system: &mut dyn TextSystem,
        store: &Store,
    ) {
        self.stats = LayoutStats {
            nodes: tree.len(),
            ..LayoutStats::default()
        };
        if tree.is_empty() {
            self.rects.clear();
            self.damage.clear();
            return;
        }

        let structure_changed = self.structure_hash != tree.structure_hash;
        if structure_changed {
            self.rebuild(tree.len());
            self.structure_hash = tree.structure_hash;
            self.viewport_dirty = true;
        } else if self.entries.len() != tree.len() {
            self.rebuild(tree.len());
            self.viewport_dirty = true;
        }

        if self.viewport_dirty {
            self.hashes.iter_mut().for_each(|hash| *hash = 0);
            self.viewport_dirty = false;
        }

        self.ensure_texts(tree);
        self.resolve_styles(tree, theme);

        let Some(root) = tree.root else {
            return;
        };
        let available = TSize {
            width: AvailableSpace::Definite(self.viewport.width),
            height: AvailableSpace::Definite(self.viewport.height),
        };

        let mut shapes = std::mem::take(&mut self.shapes);
        let mut shape_keys = std::mem::take(&mut self.shape_keys);
        let resolved = std::mem::take(&mut self.resolved);
        let texts = std::mem::take(&mut self.texts);
        let mut measure_calls = 0usize;
        let mut style_updates = 0usize;
        let mut nodes_created = 0usize;
        let mut reused = 0usize;

        let taffy_root = self.sync(
            tree,
            root,
            None,
            Direction::Column,
            &resolved,
            theme.borders.hairline,
            &mut style_updates,
            &mut nodes_created,
            &mut reused,
        );

        if let Some(taffy_root) = taffy_root {
            self.root = Some(taffy_root);
            let _ = self.taffy.compute_layout_with_measure(
                taffy_root,
                available,
                |known, available, _id, node_id, _style| {
                    let Some(node_id) = node_id else {
                        return TSize::ZERO;
                    };
                    let node_id = *node_id;
                    let node = &tree.nodes[node_id.index()];
                    let Some(text) = texts.get(node_id.index()).and_then(Option::as_ref) else {
                        return TSize::ZERO;
                    };
                    let resolved_text = &resolved[node_id.index()];
                    let width = known.width.or(match available.width {
                        AvailableSpace::Definite(width) => Some(width),
                        AvailableSpace::MinContent | AvailableSpace::MaxContent => None,
                    });
                    let mut key = Fnv::new();
                    key.write_u64(node.content_hash);
                    key.write_f32(width.unwrap_or(f32::NEG_INFINITY));
                    key.write_u8(u8::from(resolved_text.text.style.tabular));
                    let key = key.finish();
                    if shape_keys.get(node_id.index()).copied() != Some(key)
                        || shapes.get(node_id.index()).is_none_or(Option::is_none)
                    {
                        let shape = text_system.shape_clamped(
                            text,
                            resolved_text.text.style,
                            width,
                            resolved_text.max_lines,
                            resolved_text.ellipsis,
                        );
                        if shapes.len() <= node_id.index() {
                            shapes.resize(node_id.index() + 1, None);
                        }
                        shapes[node_id.index()] = Some(shape);
                        if shape_keys.len() <= node_id.index() {
                            shape_keys.resize(node_id.index() + 1, 0);
                        }
                        shape_keys[node_id.index()] = key;
                        measure_calls += 1;
                    }
                    let shape = shapes[node_id.index()].as_ref().expect("just shaped");
                    TSize {
                        width: known.width.unwrap_or(shape.width),
                        height: shape.height,
                    }
                },
            );
            self.collect(tree, root, Vec2::ZERO, store, &resolved);
        }

        self.shapes = shapes;
        self.shape_keys = shape_keys;
        self.resolved = resolved;
        self.texts = texts;
        self.stats.style_updates = style_updates;
        self.stats.nodes_created = nodes_created;
        self.stats.measure_calls = measure_calls;
        self.stats.reused = reused;
        self.finish_frame(tree, structure_changed);
    }

    fn rebuild(&mut self, nodes: usize) {
        self.taffy = TaffyTree::new();
        self.root = None;
        self.entries.clear();
        self.entries.resize(nodes, None);
        self.hashes.clear();
        self.hashes.resize(nodes, 0);
        self.changed.clear();
        self.changed.resize(nodes, true);
        self.rects.clear();
        self.rects.resize(nodes, Rect::ZERO);
        self.previous.clear();
        self.previous.resize(nodes, Rect::ZERO);
        self.repaints.clear();
        self.repaints.resize(nodes, RepaintReason::None);
        self.causes.clear();
        self.causes.resize(nodes, AxisCause::default());
        self.shapes.clear();
        self.shapes.resize(nodes, None);
        self.shape_keys.clear();
        self.shape_keys.resize(nodes, 0);
        self.first_frame = true;
    }

    fn ensure_texts<Msg>(&mut self, tree: &Tree<Msg>) {
        if self.texts.len() != tree.len() {
            self.texts.resize(tree.len(), None);
        }
        for (index, node) in tree.nodes.iter().enumerate() {
            if let Some(text) = &node.text {
                let changed = self.hashes.get(index).copied() != Some(node.content_hash);
                if changed || self.texts[index].is_none() {
                    self.texts[index] = Some(Arc::from(text.text()));
                }
            } else {
                self.texts[index] = None;
            }
        }
    }

    fn resolve_styles<Msg>(&mut self, tree: &Tree<Msg>, theme: &Theme) {
        if self.resolved.len() != tree.len() {
            let default = crate::style::Style::default().resolve(theme);
            self.resolved.resize(tree.len(), default);
        }
        for (index, node) in tree.nodes.iter().enumerate() {
            if self.hashes.get(index).copied() != Some(node.content_hash) {
                self.resolved[index] = node.style.resolve(theme);
                if self.changed.len() <= index {
                    self.changed.resize(index + 1, false);
                }
                self.changed[index] = true;
            } else if self.changed.len() <= index {
                self.changed.resize(index + 1, false);
                self.changed[index] = false;
            } else {
                self.changed[index] = false;
            }
        }
    }

    #[allow(clippy::too_many_arguments)] // reason: the counters are explicit and local
    fn sync<Msg>(
        &mut self,
        tree: &Tree<Msg>,
        id: NodeId,
        parent_kind: Option<NodeKind>,
        parent_direction: Direction,
        resolved: &[ResolvedStyle],
        hairline: f32,
        style_updates: &mut usize,
        nodes_created: &mut usize,
        reused: &mut usize,
    ) -> Option<taffy::NodeId> {
        let node = &tree.nodes[id.index()];
        let changed = self.hashes.get(id.index()).copied() != Some(node.content_hash);
        if !changed {
            if let Some(entry) = self.entries.get(id.index()).and_then(|entry| *entry) {
                *reused += 1;
                self.hashes[id.index()] = node.content_hash;
                return Some(entry.id);
            }
        }

        let mut children = Vec::with_capacity(node.children.len());
        for &child in &node.children {
            let child_id = self.sync(
                tree,
                child,
                Some(node.kind),
                resolved[id.index()].direction,
                resolved,
                hairline,
                style_updates,
                nodes_created,
                reused,
            )?;
            children.push(child_id);
        }

        let style = taffy_style(
            node.kind,
            &resolved[id.index()],
            parent_kind,
            parent_direction,
            node.parent.is_none(),
            hairline,
        );
        let taffy_id = if let Some(entry) = self.entries.get(id.index()).and_then(|entry| *entry) {
            if changed || entry.hash != node.content_hash {
                let _ = self.taffy.set_style(entry.id, style);
                let _ = self.taffy.set_children(entry.id, &children);
                *style_updates += 1;
                if let Some(slot) = self.entries[id.index()].as_mut() {
                    slot.hash = node.content_hash;
                }
            }
            entry.id
        } else {
            let taffy_id = self.taffy.new_with_children(style, &children).ok()?;
            *nodes_created += 1;
            self.entries[id.index()] = Some(TaffyEntry {
                id: taffy_id,
                hash: node.content_hash,
            });
            taffy_id
        };
        let _ = self.taffy.set_node_context(taffy_id, Some(id));
        self.hashes[id.index()] = node.content_hash;
        Some(taffy_id)
    }

    fn collect<Msg>(
        &mut self,
        tree: &Tree<Msg>,
        id: NodeId,
        origin: Vec2,
        store: &Store,
        resolved: &[ResolvedStyle],
    ) {
        let Some(entry) = self.entries.get(id.index()).and_then(|entry| *entry) else {
            return;
        };
        let Ok(layout) = self.taffy.layout(entry.id) else {
            return;
        };
        let layout = *layout;
        let node = &tree.nodes[id.index()];
        let rect = Rect::new(
            origin.x + layout.location.x,
            origin.y + layout.location.y,
            layout.size.width,
            layout.size.height,
        )
        .snap(self.scale);
        if self.rects.len() <= id.index() {
            self.rects.resize(id.index() + 1, Rect::ZERO);
        }
        self.rects[id.index()] = rect;
        self.causes[id.index()] = infer_causes(node, resolved.get(id.index()));
        let scroll = if resolved[id.index()].scroll.clips() {
            store.scroll(node.path_hash)
        } else {
            Vec2::ZERO
        };
        let child_origin = Vec2::new(rect.x - scroll.x, rect.y - scroll.y);
        for &child in &node.children {
            self.collect(tree, child, child_origin, store, resolved);
        }
    }

    fn finish_frame<Msg>(&mut self, tree: &Tree<Msg>, structure_changed: bool) {
        self.damage.clear();
        if self.first_frame || structure_changed {
            self.damage.push(Rect::new(
                0.0,
                0.0,
                self.viewport.width,
                self.viewport.height,
            ));
        }
        for (index, _node) in tree.nodes.iter().enumerate() {
            let rect = self.rects.get(index).copied().unwrap_or(Rect::ZERO);
            let previous = self.previous.get(index).copied().unwrap_or(Rect::ZERO);
            let reason = if self.first_frame || structure_changed {
                RepaintReason::FirstFrame
            } else if self.changed.get(index).copied().unwrap_or(false) {
                RepaintReason::ViewChanged
            } else if rect != previous {
                RepaintReason::LayoutChanged
            } else {
                RepaintReason::None
            };
            if self.repaints.len() <= index {
                self.repaints.resize(index + 1, RepaintReason::None);
            }
            self.repaints[index] = reason;
            if !self.first_frame && !structure_changed && reason.repaints() && !rect.is_empty() {
                self.damage.push(previous.union(rect));
            }
        }
        self.previous.clear();
        self.previous.extend_from_slice(&self.rects);
        self.stats.damage_rects = self.damage.len();
        self.first_frame = false;
    }
}

fn taffy_style(
    kind: NodeKind,
    style: &ResolvedStyle,
    parent_kind: Option<NodeKind>,
    parent_direction: Direction,
    is_root: bool,
    hairline: f32,
) -> TaffyStyle {
    let mut output = TaffyStyle::default();
    output.display = if style.hidden {
        Display::None
    } else if kind == NodeKind::Grid {
        Display::Grid
    } else {
        Display::Flex
    };
    output.flex_direction = match style.direction {
        Direction::Row => FlexDirection::Row,
        Direction::Column => FlexDirection::Column,
    };
    output.flex_wrap = if style.wrap {
        FlexWrap::Wrap
    } else {
        FlexWrap::NoWrap
    };
    output.align_items = Some(match style.align {
        Align::Start => AlignItems::Start,
        Align::Center => AlignItems::Center,
        Align::End => AlignItems::End,
        Align::Stretch => AlignItems::Stretch,
        Align::Baseline => AlignItems::Baseline,
    });
    output.justify_content = Some(match style.justify {
        Justify::Start => JustifyContent::Start,
        Justify::Center => JustifyContent::Center,
        Justify::End => JustifyContent::End,
        Justify::SpaceBetween => JustifyContent::SpaceBetween,
    });
    output.gap = TSize {
        width: LengthPercentage::Length(style.gap),
        height: LengthPercentage::Length(style.gap),
    };
    output.padding = taffy::Rect {
        left: LengthPercentage::Length(style.padding.left),
        right: LengthPercentage::Length(style.padding.right),
        top: LengthPercentage::Length(style.padding.top),
        bottom: LengthPercentage::Length(style.padding.bottom),
    };
    let fill_width = matches!(style.width, SizeSpec::Fill) && parent_direction == Direction::Row;
    let fill_height =
        matches!(style.height, SizeSpec::Fill) && parent_direction == Direction::Column;
    output.flex_grow = style.grow + if fill_width || fill_height { 1.0 } else { 0.0 };
    output.flex_shrink = style.shrink;
    let mut width = dimension(style.width, fill_width);
    let mut height = dimension(style.height, fill_height);
    if is_root {
        if matches!(style.width, SizeSpec::Auto | SizeSpec::Hug | SizeSpec::Fill) {
            width = Dimension::Percent(1.0);
        }
        if matches!(
            style.height,
            SizeSpec::Auto | SizeSpec::Hug | SizeSpec::Fill
        ) {
            height = Dimension::Percent(1.0);
        }
    }
    if kind == NodeKind::Divider {
        if parent_direction == Direction::Row {
            width = Dimension::Length(hairline);
            height = Dimension::Percent(1.0);
        } else {
            width = Dimension::Percent(1.0);
            height = Dimension::Length(hairline);
        }
    }
    output.size = TSize { width, height };
    output.min_size = TSize {
        width: optional_dimension(style.min_width),
        height: optional_dimension(style.min_height),
    };
    output.max_size = TSize {
        width: optional_dimension(style.max_width),
        height: optional_dimension(style.max_height),
    };
    output.aspect_ratio = style.aspect;
    output.overflow = Point {
        x: overflow(style.clip, style.scroll),
        y: overflow(style.clip, style.scroll),
    };
    if parent_kind == Some(NodeKind::Stack) {
        output.position = Position::Absolute;
        output.inset = taffy::Rect {
            left: LengthPercentageAuto::Length(0.0),
            right: LengthPercentageAuto::Length(0.0),
            top: LengthPercentageAuto::Length(0.0),
            bottom: LengthPercentageAuto::Length(0.0),
        };
    }
    if let Some(columns) = style.columns {
        output.grid_template_columns = match columns {
            Columns::Fixed(count) => taffy::style_helpers::evenly_sized_tracks(count.max(1)),
            Columns::AutoFill(width) => {
                let track = NonRepeatedTrackSizingFunction {
                    min: MinTrackSizingFunction::Fixed(LengthPercentage::Length(width.px())),
                    max: MaxTrackSizingFunction::Fraction(1.0),
                };
                vec![TrackSizingFunction::Repeat(
                    GridTrackRepetition::AutoFill,
                    vec![track],
                )]
            }
        };
    }
    output
}

fn dimension(spec: SizeSpec, fill: bool) -> Dimension {
    match spec {
        SizeSpec::Auto | SizeSpec::Hug => Dimension::Auto,
        SizeSpec::Fill => {
            if fill {
                Dimension::Auto
            } else {
                Dimension::Percent(1.0)
            }
        }
        SizeSpec::Fraction(value) => Dimension::Percent(value.clamp(0.0, 1.0)),
        SizeSpec::Token(spacing) => Dimension::Length(spacing.px()),
        SizeSpec::Px(value) => Dimension::Length(value),
    }
}

fn optional_dimension(value: Option<f32>) -> Dimension {
    value.map_or(Dimension::Auto, Dimension::Length)
}

fn overflow(clip: bool, scroll: ScrollAxes) -> Overflow {
    if scroll != ScrollAxes::None {
        Overflow::Scroll
    } else if clip {
        Overflow::Hidden
    } else {
        Overflow::Visible
    }
}

fn infer_causes<Msg>(node: &crate::view::Node<Msg>, style: Option<&ResolvedStyle>) -> AxisCause {
    let Some(style) = style else {
        return AxisCause::default();
    };
    let width = if node.parent.is_none() {
        Some(Cause::Viewport)
    } else {
        match style.width {
            SizeSpec::Token(spacing) => Some(Cause::Token(spacing)),
            SizeSpec::Px(_) | SizeSpec::Auto => None,
            SizeSpec::Hug => Some(Cause::Content),
            SizeSpec::Fill => Some(Cause::Grow),
            SizeSpec::Fraction(value) => Some(Cause::Fraction(value)),
        }
    };
    let height = if node.parent.is_none() {
        Some(Cause::Viewport)
    } else {
        match style.height {
            SizeSpec::Token(spacing) => Some(Cause::Token(spacing)),
            SizeSpec::Px(_) | SizeSpec::Auto => None,
            SizeSpec::Hug => Some(Cause::Content),
            SizeSpec::Fill => Some(Cause::Grow),
            SizeSpec::Fraction(value) => Some(Cause::Fraction(value)),
        }
    };
    AxisCause { width, height }
}

/// The union of this frame's damage, for the adapter's present path.
#[must_use]
pub fn damage_union(damage: &[Rect]) -> Option<Rect> {
    let mut iter = damage.iter().copied().filter(|rect| !rect.is_empty());
    let first = iter.next()?;
    Some(iter.fold(first, Rect::union))
}

#[cfg(test)]
mod tests {
    use super::{damage_union, dimension};
    use crate::geom::Rect;
    use crate::style::SizeSpec;
    use crate::theme::Spacing;
    use taffy::prelude::Dimension;

    #[test]
    fn token_sizes_resolve() {
        assert_eq!(
            dimension(SizeSpec::Token(Spacing::S4), false),
            Dimension::Length(16.0)
        );
        assert_eq!(
            dimension(SizeSpec::Fraction(0.5), false),
            Dimension::Percent(0.5)
        );
        assert_eq!(dimension(SizeSpec::Fill, true), Dimension::Auto);
        assert_eq!(
            dimension(SizeSpec::Px(13.0), false),
            Dimension::Length(13.0)
        );
    }

    #[test]
    fn damage_union_ignores_empty_rects() {
        assert_eq!(damage_union(&[]), None);
        assert_eq!(
            damage_union(&[
                Rect::new(0.0, 0.0, 10.0, 10.0),
                Rect::new(5.0, 5.0, 10.0, 10.0),
            ]),
            Some(Rect::new(0.0, 0.0, 15.0, 15.0))
        );
    }
}
