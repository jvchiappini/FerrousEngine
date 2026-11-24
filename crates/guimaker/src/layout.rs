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
use taffy::geometry::Line;
use taffy::prelude::{
    AlignItems, AlignSelf, AvailableSpace, Dimension, Display, FlexDirection, FlexWrap,
    JustifyContent, LengthPercentage, LengthPercentageAuto, Position, Size as TSize,
    Style as TaffyStyle, TaffyTree,
};
use taffy::style::{
    GridPlacement, GridTrackRepetition, MaxTrackSizingFunction, MinTrackSizingFunction,
    NonRepeatedTrackSizingFunction, Overflow, TrackSizingFunction,
};
use taffy::style_helpers::TaffyGridLine;
use taffy::Point;

use crate::geom::{Rect, Size};
use crate::hash::Fnv;
use crate::store::{RepaintReason, Store};
use crate::style::{
    Align, ColumnTrack, Columns, Direction, Justify, ResolvedStyle, ScrollAxes, SizeSpec,
};
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
    /// A fluid size won: `clamp(min, fraction * container, max)`.
    Fluid(f32),
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
            Self::Fluid(fraction) => {
                format!("a fluid {:.0}% of the container, clamped", fraction * 100.0)
            }
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
    /// Layout passes this frame: 1 normally, 2 when a responsive value moved
    /// after the first pass measured the containers.
    pub responsive_passes: u8,
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
    content_sizes: Vec<Size>,
    previous: Vec<Rect>,
    damage: Vec<Rect>,
    repaints: Vec<RepaintReason>,
    causes: Vec<AxisCause>,
    viewport: Size,
    scale: f32,
    structure_hash: u64,
    first_frame: bool,
    viewport_dirty: bool,
    containers: Vec<f32>,
    restyled: Vec<bool>,
    parent_directions: Vec<Option<Direction>>,
    has_responsive: bool,
    host_capabilities: crate::overlay::HostCapabilities,
    /// The monitor work area in window-local logical pixels, when the host
    /// reports one. A popup-hosted overlay is solved against it; an in-window
    /// overlay still clamps to the viewport.
    work_area: Option<Rect>,
    overlay_hosts: Vec<(NodeId, crate::overlay::OverlayHost, Rect)>,
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
            content_sizes: Vec::new(),
            previous: Vec::new(),
            damage: Vec::new(),
            repaints: Vec::new(),
            causes: Vec::new(),
            viewport: Size::ZERO,
            scale: 1.0,
            structure_hash: 0,
            first_frame: true,
            viewport_dirty: true,
            containers: Vec::new(),
            restyled: Vec::new(),
            parent_directions: Vec::new(),
            has_responsive: false,
            host_capabilities: crate::overlay::HostCapabilities::default(),
            work_area: None,
            overlay_hosts: Vec::new(),
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

    /// Sets what the host can do with overlays that do not fit.
    pub fn set_host_capabilities(&mut self, capabilities: crate::overlay::HostCapabilities) {
        self.host_capabilities = capabilities;
    }

    /// Sets the monitor work area, in window-local logical pixels.
    ///
    /// A popup-hosted overlay is flipped, shifted and clamped against this
    /// rectangle (the monitor minus its taskbar), so a menu next to a screen
    /// edge stays fully on screen. `None` falls back to the window viewport.
    pub fn set_work_area(&mut self, work_area: Option<Rect>) -> bool {
        if self.work_area == work_area {
            return false;
        }
        self.work_area = work_area;
        self.viewport_dirty = true;
        true
    }

    /// Where the layout decided to host the anchored overlay at `node`.
    #[must_use]
    pub fn overlay_host(&self, node: NodeId) -> Option<crate::overlay::OverlayHost> {
        self.overlay_hosts
            .iter()
            .rev()
            .find(|(id, _, _)| *id == node)
            .map(|(_, host, _)| *host)
    }

    /// The unclamped popup rect for the anchored overlay at `node`.
    #[must_use]
    pub fn overlay_popup_rect(&self, node: NodeId) -> Option<Rect> {
        self.overlay_hosts
            .iter()
            .rev()
            .find(|(id, _, _)| *id == node)
            .map(|(_, _, rect)| *rect)
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

    /// The content size of a node (the union of its children's rectangles,
    /// measured from the content origin). A scroll container clamps its offset
    /// with `content_size - rect.size`.
    #[must_use]
    pub fn content_size(&self, id: NodeId) -> Size {
        self.content_sizes
            .get(id.index())
            .copied()
            .unwrap_or(Size::ZERO)
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
    /// Responsive values resolve against the container width, which is only
    /// known once the parents are laid out. The first pass uses the previous
    /// frame's rectangles (the viewport on a cold frame); if that moved any
    /// responsive value the engine runs a second pass with the fresh widths.
    /// A view without a single responsive modifier never pays for the second
    /// pass, so the phase 2 single-pass cost is unchanged.
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
        self.has_responsive = false;
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

        let Some(root) = tree.root else {
            return;
        };
        let available = TSize {
            width: AvailableSpace::Definite(self.viewport.width),
            height: AvailableSpace::Definite(self.viewport.height),
        };

        let mut shapes = std::mem::take(&mut self.shapes);
        let mut shape_keys = std::mem::take(&mut self.shape_keys);
        let mut resolved = std::mem::take(&mut self.resolved);
        let mut changed = std::mem::take(&mut self.changed);
        let mut restyled = std::mem::take(&mut self.restyled);
        let mut parents = std::mem::take(&mut self.parent_directions);
        let texts = std::mem::take(&mut self.texts);
        let hairline = theme.borders.hairline;
        let mut counters = Counters::default();

        self.resolve_containers(tree);
        self.resolve_styles(
            tree,
            theme,
            false,
            &mut resolved,
            &mut changed,
            &mut restyled,
        );
        Self::apply_inherited_tones(tree, theme, &mut resolved);
        self.stats.responsive_passes = 1;
        self.pass(
            tree,
            root,
            available,
            hairline,
            &resolved,
            &restyled,
            &mut parents,
            &texts,
            &mut shapes,
            &mut shape_keys,
            text_system,
            store,
            false,
            &mut counters,
        );

        if self.has_responsive {
            self.resolve_containers(tree);
            if self.resolve_styles(
                tree,
                theme,
                true,
                &mut resolved,
                &mut changed,
                &mut restyled,
            ) {
                Self::apply_inherited_tones(tree, theme, &mut resolved);
                self.stats.responsive_passes = 2;
                self.pass(
                    tree,
                    root,
                    available,
                    hairline,
                    &resolved,
                    &restyled,
                    &mut parents,
                    &texts,
                    &mut shapes,
                    &mut shape_keys,
                    text_system,
                    store,
                    true,
                    &mut counters,
                );
            }
        }

        self.shapes = shapes;
        self.shape_keys = shape_keys;
        self.resolved = resolved;
        self.restyled = restyled;
        self.parent_directions = parents;
        self.texts = texts;
        self.stats.style_updates = counters.style_updates;
        self.stats.nodes_created = counters.nodes_created;
        self.stats.measure_calls = counters.measure_calls;
        self.stats.reused = counters.reused;
        self.finish_frame(tree, structure_changed, &changed);
        self.changed = changed;
    }

    #[allow(clippy::too_many_arguments)] // reason: the frame pipeline passes its owned scratch buffers explicitly
    fn pass<Msg>(
        &mut self,
        tree: &Tree<Msg>,
        root: NodeId,
        available: TSize<AvailableSpace>,
        hairline: f32,
        resolved: &[ResolvedStyle],
        restyled: &[bool],
        parents: &mut Vec<Option<Direction>>,
        texts: &[Option<Arc<str>>],
        shapes: &mut Vec<Option<TextShape>>,
        shape_keys: &mut Vec<u64>,
        text_system: &mut dyn TextSystem,
        store: &Store,
        visit_all: bool,
        counters: &mut Counters,
    ) {
        let taffy_root = self.sync(
            tree,
            root,
            Direction::Column,
            false,
            resolved,
            restyled,
            parents,
            hairline,
            visit_all,
            counters,
        );
        let Some(taffy_root) = taffy_root else {
            return;
        };
        self.root = Some(taffy_root);
        let shape_call_count = &mut counters.measure_calls;
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
                key.write_f32(resolved_text.text.style.size);
                key.write_f32(resolved_text.text.style.line_height);
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
                    *shape_call_count += 1;
                }
                let shape = shapes[node_id.index()].as_ref().expect("just shaped");
                TSize {
                    width: known.width.unwrap_or(shape.width),
                    height: shape.height,
                }
            },
        );
        self.overlay_hosts.clear();
        self.collect(tree, root, Vec2::ZERO, store, resolved);
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
        self.containers.clear();
        self.containers.resize(nodes, self.viewport.width);
        self.restyled.clear();
        self.restyled.resize(nodes, false);
        self.parent_directions.clear();
        self.parent_directions.resize(nodes, None);
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

    /// Fills `containers` with each node's parent content width.
    ///
    /// The root's container is the viewport; every other node resolves against
    /// its parent's laid-out width minus the parent's horizontal padding. The
    /// first pass of a frame reads the previous rectangles (the viewport on a
    /// cold frame) and the second pass reads the fresh ones.
    fn resolve_containers<Msg>(&mut self, tree: &Tree<Msg>) {
        if self.containers.len() != tree.len() {
            self.containers.resize(tree.len(), self.viewport.width);
        }
        for node in &tree.nodes {
            let width = match node.parent {
                None => self.viewport.width,
                Some(parent) => {
                    let parent_rect = self
                        .rects
                        .get(parent.index())
                        .copied()
                        .unwrap_or(Rect::ZERO);
                    let padding = self
                        .resolved
                        .get(parent.index())
                        .map_or(0.0, |style| style.padding.left + style.padding.right);
                    (parent_rect.width - padding).max(0.0)
                }
            };
            self.containers[node.id.index()] = width;
        }
    }

    /// Resolves the styles of every node whose content or container changed.
    ///
    /// Returns whether any resolved style moved, which is the trigger for the
    /// second responsive pass.
    #[allow(clippy::too_many_arguments)] // reason: the scratch buffers are owned by `run`
    fn resolve_styles<Msg>(
        &mut self,
        tree: &Tree<Msg>,
        theme: &Theme,
        all: bool,
        resolved: &mut Vec<ResolvedStyle>,
        changed: &mut Vec<bool>,
        restyled: &mut Vec<bool>,
    ) -> bool {
        if resolved.len() != tree.len() {
            let default = crate::style::Style::default().resolve(theme, self.viewport.width);
            resolved.resize(tree.len(), default);
        }
        if changed.len() != tree.len() {
            changed.resize(tree.len(), true);
        }
        if restyled.len() != tree.len() {
            restyled.resize(tree.len(), false);
        }
        let mut moved = false;
        for (index, node) in tree.nodes.iter().enumerate() {
            self.has_responsive |= node.style.is_responsive();
            let content_changed = self.hashes.get(index).copied() != Some(node.content_hash);
            if !content_changed && !all {
                changed[index] = false;
                restyled[index] = false;
                continue;
            }
            let container = self
                .containers
                .get(index)
                .copied()
                .unwrap_or(self.viewport.width);
            let next = node.style.resolve(theme, container);
            let restyled_here = next != resolved[index];
            if restyled_here {
                resolved[index] = next;
                moved = true;
            }
            changed[index] = content_changed;
            restyled[index] = restyled_here;
        }
        moved
    }

    /// Propagates a container's tone to descendants that did not set one.
    ///
    /// Icons and text draw in the resolved tone, so an icon inside a muted row
    /// (or a label that never calls `.tone`) follows its context instead of
    /// silently falling back to the primary text colour.
    fn apply_inherited_tones<Msg>(tree: &Tree<Msg>, theme: &Theme, resolved: &mut [ResolvedStyle]) {
        for node in &tree.nodes {
            let index = node.id.index();
            if index >= resolved.len() {
                continue;
            }
            let parent_tone = node
                .parent
                .and_then(|parent| resolved.get(parent.index()))
                .map_or(crate::style::Tone::Default, |style| style.tone);
            if !node.style.tone_explicit {
                let tone = parent_tone.paired_with(node.style.bg);
                let style = &mut resolved[index];
                if style.tone != tone {
                    style.tone = tone;
                    style.text.color = tone.color(theme);
                }
            }
        }
    }

    #[allow(clippy::too_many_arguments)] // reason: the counters and scratch buffers are explicit and local
    fn sync<Msg>(
        &mut self,
        tree: &Tree<Msg>,
        id: NodeId,
        parent_direction: Direction,
        absolute: bool,
        resolved: &[ResolvedStyle],
        restyled: &[bool],
        parents: &mut Vec<Option<Direction>>,
        hairline: f32,
        visit_all: bool,
        counters: &mut Counters,
    ) -> Option<taffy::NodeId> {
        let node = &tree.nodes[id.index()];
        let index = id.index();
        if parents.len() <= index {
            parents.resize(index + 1, None);
        }
        let direction_changed = node.parent.is_some() && parents[index] != Some(parent_direction);
        parents[index] = Some(parent_direction);
        let content_changed = self.hashes.get(index).copied() != Some(node.content_hash);
        let changed =
            content_changed || restyled.get(index).copied().unwrap_or(false) || direction_changed;
        // A clean subtree is skipped, except on the second responsive pass: a
        // descendant may have restyled under a clean parent, so that pass
        // visits everything and only writes the styles that actually moved.
        if !changed && !visit_all {
            if let Some(entry) = self.entries.get(index).and_then(|entry| *entry) {
                counters.reused += 1;
                self.hashes[index] = node.content_hash;
                return Some(entry.id);
            }
        }

        let mut children = Vec::with_capacity(node.children.len());
        for (child_index, &child) in node.children.iter().enumerate() {
            let child_absolute =
                node.kind == NodeKind::Stack || (node.kind == NodeKind::Overlay && child_index > 0);
            let child_id = self.sync(
                tree,
                child,
                resolved[index].direction,
                child_absolute,
                resolved,
                restyled,
                parents,
                hairline,
                visit_all,
                counters,
            )?;
            children.push(child_id);
        }

        let style = taffy_style(
            node.kind,
            &resolved[index],
            parent_direction,
            absolute,
            node.parent.is_none(),
            hairline,
        );
        let taffy_id = if let Some(entry) = self.entries.get(index).and_then(|entry| *entry) {
            if changed || entry.hash != node.content_hash {
                let _ = self.taffy.set_style(entry.id, style);
                let _ = self.taffy.set_children(entry.id, &children);
                counters.style_updates += 1;
                if let Some(slot) = self.entries[index].as_mut() {
                    slot.hash = node.content_hash;
                }
            }
            entry.id
        } else {
            let taffy_id = self.taffy.new_with_children(style, &children).ok()?;
            counters.nodes_created += 1;
            self.entries[index] = Some(TaffyEntry {
                id: taffy_id,
                hash: node.content_hash,
            });
            taffy_id
        };
        let _ = self.taffy.set_node_context(taffy_id, Some(id));
        self.hashes[index] = node.content_hash;
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
        let mut rect = Rect::new(
            origin.x + layout.location.x,
            origin.y + layout.location.y,
            layout.size.width,
            layout.size.height,
        );
        if let Some(anchor) = resolved[id.index()].anchor {
            // An anchored overlay never participates in the parent's layout;
            // it is positioned against a sibling's rect after the normal pass.
            if let Some(parent) = node.parent {
                if let Some(sibling) = tree.nodes[parent.index()]
                    .children
                    .get(usize::from(anchor.sibling))
                {
                    let sibling_rect = self
                        .rects
                        .get(sibling.index())
                        .copied()
                        .unwrap_or(Rect::ZERO);
                    // A pure solver decides the side, the flip, the shift and
                    // the internal scroll. The window viewport decides whether
                    // the overlay fits in the window; a popup is then solved
                    // and clamped against the monitor work area instead, so it
                    // may extend past the window but never past the screen.
                    let window_area =
                        Rect::new(0.0, 0.0, self.viewport.width, self.viewport.height);
                    let content = Size::new(rect.width, rect.height);
                    let window_placement =
                        crate::overlay::place(sibling_rect, anchor.side, content, window_area);
                    let host = window_placement.hosted_in(window_area, self.host_capabilities);
                    let area = if host == crate::overlay::OverlayHost::Popup {
                        self.work_area.unwrap_or(window_area)
                    } else {
                        window_area
                    };
                    let placement = if host == crate::overlay::OverlayHost::Popup {
                        crate::overlay::place(sibling_rect, anchor.side, content, area)
                    } else {
                        window_placement
                    };
                    let gap = if placement.side.is_horizontal() {
                        anchor.dx
                    } else {
                        anchor.dy
                    };
                    let mut target = placement.rect;
                    match placement.side {
                        crate::overlay::Side::Right => target.x += gap,
                        crate::overlay::Side::Left => target.x -= gap,
                        crate::overlay::Side::Bottom => target.y += gap,
                        crate::overlay::Side::Top => target.y -= gap,
                    }
                    // The gap is applied after the solve so the flip anchors on
                    // the sibling's opposite edge; re-clamp in case it pushed
                    // the overlay past the area edge.
                    target = clamp_within(target, area);
                    rect = target;
                    self.overlay_hosts.push((id, host, target));
                }
            }
        }
        let rect = rect.snap(self.scale);
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
        let mut content = Size::ZERO;
        for &child in &node.children {
            self.collect(tree, child, child_origin, store, resolved);
            let child_rect = self.rects.get(child.index()).copied().unwrap_or(Rect::ZERO);
            content.width = content.width.max(child_rect.right() - child_origin.x);
            content.height = content.height.max(child_rect.bottom() - child_origin.y);
        }
        if self.content_sizes.len() <= id.index() {
            self.content_sizes.resize(id.index() + 1, Size::ZERO);
        }
        self.content_sizes[id.index()] = content;
    }

    fn finish_frame<Msg>(&mut self, tree: &Tree<Msg>, structure_changed: bool, changed: &[bool]) {
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
            } else if changed.get(index).copied().unwrap_or(false) {
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

/// The counters one layout pass reports.
#[derive(Clone, Copy, Debug, Default)]
struct Counters {
    style_updates: usize,
    nodes_created: usize,
    reused: usize,
    measure_calls: usize,
}
/// Clamps `rect` inside `area` on both axes, keeping its size.
fn clamp_within(mut rect: Rect, area: Rect) -> Rect {
    if area.width > 0.0 {
        let max_x = (area.x + area.width - rect.width).max(area.x);
        rect.x = rect.x.clamp(area.x, max_x);
    }
    if area.height > 0.0 {
        let max_y = (area.y + area.height - rect.height).max(area.y);
        rect.y = rect.y.clamp(area.y, max_y);
    }
    rect
}

fn taffy_style(
    kind: NodeKind,
    style: &ResolvedStyle,
    parent_direction: Direction,
    absolute: bool,
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
    output.align_self = style.align_self.map(|align| match align {
        Align::Start => AlignSelf::Start,
        Align::Center => AlignSelf::Center,
        Align::End => AlignSelf::End,
        Align::Stretch => AlignSelf::Stretch,
        Align::Baseline => AlignSelf::Baseline,
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
    // A priority lets a label give way to its siblings: 9 shrinks last, 1 first.
    // Priority nodes also drop the automatic content min-size, which is what
    // otherwise stops a text run from ever getting narrower than its content.
    let priority_shrink = style.priority > 0;
    output.flex_shrink = if priority_shrink {
        f32::from(10u8.saturating_sub(style.priority.min(9)))
    } else {
        style.shrink
    };
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
        width: if priority_shrink {
            Dimension::Length(0.0)
        } else {
            optional_dimension(style.min_width)
        },
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
    if absolute {
        output.position = Position::Absolute;
        if style.anchor.is_none() {
            // A plain stack child fills the stack. An anchored overlay keeps
            // auto insets and is sized by its content, then moved in `collect`.
            output.inset = taffy::Rect {
                left: LengthPercentageAuto::Length(0.0),
                right: LengthPercentageAuto::Length(0.0),
                top: LengthPercentageAuto::Length(0.0),
                bottom: LengthPercentageAuto::Length(0.0),
            };
        }
    }
    if let Some(columns) = &style.columns {
        output.grid_template_columns = match columns {
            Columns::Fixed(count) => taffy::style_helpers::evenly_sized_tracks((*count).max(1)),
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
            Columns::Tracks(tracks) => tracks
                .iter()
                .map(|track| {
                    let (min, max) = match *track {
                        ColumnTrack::Fixed(width) => (
                            MinTrackSizingFunction::Fixed(LengthPercentage::Length(width.px())),
                            MaxTrackSizingFunction::Fixed(LengthPercentage::Length(width.px())),
                        ),
                        ColumnTrack::Fraction(fraction) => (
                            MinTrackSizingFunction::Auto,
                            MaxTrackSizingFunction::Fraction(fraction.max(0.0)),
                        ),
                        ColumnTrack::Fit => {
                            (MinTrackSizingFunction::Auto, MaxTrackSizingFunction::Auto)
                        }
                    };
                    TrackSizingFunction::Single(NonRepeatedTrackSizingFunction { min, max })
                })
                .collect(),
        };
    }
    output.grid_row = grid_axis(style.grid_row, None);
    output.grid_column = grid_axis(style.grid_column, style.grid_column_span);
    output
}

/// Builds one taffy grid axis from a one-based start line and an optional span.
///
/// A line that does not fit `i16` falls back to `Auto`, so an out-of-range
/// placement never panics; taffy places the item with its auto-placement
/// algorithm instead.
fn grid_axis(line: Option<u16>, span: Option<u16>) -> Line<GridPlacement> {
    let start = line
        .and_then(|value| i16::try_from(value).ok())
        .map_or(GridPlacement::Auto, GridPlacement::from_line_index);
    let end = span.map_or(GridPlacement::Auto, GridPlacement::Span);
    Line { start, end }
}

fn dimension(spec: SizeSpec, fill: bool) -> Dimension {
    match spec {
        // A fluid size is resolved to `Px` by `Style::resolve`, so a resolved
        // style never carries one; keep the arm total without a second guess.
        SizeSpec::Auto | SizeSpec::Hug | SizeSpec::Fluid(_) => Dimension::Auto,
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
    } else if let Some(fraction) = style.fluid_width {
        Some(Cause::Fluid(fraction))
    } else {
        match style.width {
            SizeSpec::Token(spacing) => Some(Cause::Token(spacing)),
            SizeSpec::Px(_) | SizeSpec::Auto | SizeSpec::Fluid(_) => None,
            SizeSpec::Hug => Some(Cause::Content),
            SizeSpec::Fill => Some(Cause::Grow),
            SizeSpec::Fraction(value) => Some(Cause::Fraction(value)),
        }
    };
    let height = if node.parent.is_none() {
        Some(Cause::Viewport)
    } else if let Some(fraction) = style.fluid_height {
        Some(Cause::Fluid(fraction))
    } else {
        match style.height {
            SizeSpec::Token(spacing) => Some(Cause::Token(spacing)),
            SizeSpec::Px(_) | SizeSpec::Auto | SizeSpec::Fluid(_) => None,
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
