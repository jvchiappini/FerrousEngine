//! The frame loop: build, layout, route input, paint and report debug data.
//!
//! [`Ui`] owns the persistent state (theme, taffy tree, keyed store, hit index,
//! cached commands). A frame is a pure function of the view, that state, the
//! viewport, the scale and the injected clock; the same inputs produce the same
//! display list and damage. Nothing runs when nothing changes: an unchanged
//! view, no input and no store change reuse the previous display list and
//! report no damage.

#![allow(clippy::too_many_lines)] // reason: the paint pipeline is one linear pass
use std::sync::Arc;

use crate::debug::{CacheState, FrameDebug, NodeDebug};
use crate::display_list::{DisplayList, DrawCommand, ImageFit, TextRun, TextSpanStyle};
use crate::geom::{Rect, Size};
use crate::hash::Fnv;
use crate::input::{route, InputEvent, InputState};
use crate::layout::LayoutEngine;
use crate::spatial::HitIndex;
use crate::store::{RepaintReason, Store};
use crate::text::TextSystem;
use crate::theme::{AccentState, Color, ColorRole, Theme};
use crate::view::{Element, NodeId, NodeKind, Tree};

/// The counters for one frame.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct FrameStats {
    /// Layout counters.
    pub layout: crate::layout::LayoutStats,
    /// Display commands produced.
    pub commands: usize,
    /// Damage rectangles.
    pub damage: usize,
    /// Messages emitted.
    pub messages: usize,
    /// Whether the display list was rebuilt this frame.
    pub repainted: bool,
}

/// The reusable output of one frame.
#[derive(Clone, Debug)]
pub struct Frame<Msg> {
    /// The flat display list, in paint order.
    pub display_list: DisplayList,
    /// Messages emitted by this frame's input.
    pub messages: Vec<Msg>,
    /// This frame's counters.
    pub stats: FrameStats,
}

impl<Msg> Default for Frame<Msg> {
    fn default() -> Self {
        Self {
            display_list: DisplayList::new(),
            messages: Vec::new(),
            stats: FrameStats::default(),
        }
    }
}

impl<Msg> Frame<Msg> {
    /// Builds an empty frame.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// The union of this frame's damage.
    #[must_use]
    pub fn damage_union(&self) -> Option<Rect> {
        self.display_list.damage_union()
    }
}

/// The library instance: theme, persistent layout and interaction state.
pub struct Ui<Msg> {
    theme: Theme,
    viewport: Size,
    scale: f32,
    engine: LayoutEngine,
    index: HitIndex,
    store: Store,
    input: InputState,
    tree: Tree<Msg>,
    view_hash: u64,
    index_structure: u64,
    pending: Vec<InputEvent>,
    commands: Vec<Vec<DrawCommand>>,
    command_hashes: Vec<u64>,
    command_counts: Vec<usize>,
    painted: bool,
    last_repainted: bool,
    last_messages: usize,
    layout_needed: bool,
    last_scroll_generation: u64,
    last_damage: Vec<Rect>,
}

impl<Msg: Clone> Ui<Msg> {
    /// Builds a UI with a theme.
    #[must_use]
    pub fn new(theme: Theme) -> Self {
        Self {
            theme,
            viewport: Size::ZERO,
            scale: 1.0,
            engine: LayoutEngine::new(),
            index: HitIndex::new(),
            store: Store::new(),
            input: InputState::new(),
            tree: Tree::build(&Element::column()),
            view_hash: 0,
            index_structure: 0,
            pending: Vec::new(),
            commands: Vec::new(),
            command_hashes: Vec::new(),
            command_counts: Vec::new(),
            painted: false,
            last_repainted: true,
            last_messages: 0,
            layout_needed: true,
            last_scroll_generation: 0,
            last_damage: Vec::new(),
        }
    }

    /// The active theme.
    #[must_use]
    pub fn theme(&self) -> &Theme {
        &self.theme
    }

    /// Replaces the theme; the next frame repaints everything.
    pub fn set_theme(&mut self, theme: Theme) {
        self.theme = theme;
        self.view_hash = 0;
        self.layout_needed = true;
    }

    /// Sets the viewport and scale factor.
    pub fn set_viewport(&mut self, viewport: Size, scale: f32) {
        self.viewport = viewport;
        self.scale = scale;
        self.engine.set_viewport(viewport, scale);
        self.layout_needed = true;
    }

    /// Queues an input event. A run of pointer moves coalesces to the latest.
    pub fn push_event(&mut self, event: InputEvent) {
        if let (Some(InputEvent::PointerMoved { .. }), InputEvent::PointerMoved { .. }) =
            (self.pending.last(), &event)
        {
            if let Some(last) = self.pending.last_mut() {
                *last = event;
                return;
            }
        }
        self.pending.push(event);
    }

    /// The built tree.
    #[must_use]
    pub fn tree(&self) -> &Tree<Msg> {
        &self.tree
    }

    /// The layout engine.
    #[must_use]
    pub fn layout(&self) -> &LayoutEngine {
        &self.engine
    }

    /// The keyed store.
    #[must_use]
    pub fn store(&self) -> &Store {
        &self.store
    }

    /// The keyed store, mutably (an app may set a scroll offset or focus).
    pub fn store_mut(&mut self) -> &mut Store {
        &mut self.store
    }

    /// The hit index.
    #[must_use]
    pub fn hit_index(&self) -> &HitIndex {
        &self.index
    }

    /// The hit index, mutably (a query updates the last-hit cache).
    pub fn hit_index_mut(&mut self) -> &mut HitIndex {
        &mut self.index
    }

    /// Builds, lays out, routes input and paints one frame.
    pub fn paint(
        &mut self,
        root: &Element<Msg>,
        frame: &mut Frame<Msg>,
        text_system: &mut dyn TextSystem,
    ) {
        frame.messages.clear();
        self.store.begin_frame();
        self.last_damage.clear();

        let view_hash = crate::view::hash_element(root);
        let rebuilt = view_hash != self.view_hash || self.tree.is_empty();
        if rebuilt {
            self.tree.rebuild(root);
            self.view_hash = self.tree.view_hash;
            self.painted = false;
            self.layout_needed = true;
        }

        let layout_needed = self.layout_needed
            || self.tree.is_empty()
            || self.store.layout_generation() != self.last_scroll_generation;
        if layout_needed {
            self.engine.set_viewport(self.viewport, self.scale);
            self.engine
                .run(&self.tree, &self.theme, text_system, &self.store);
            self.last_scroll_generation = self.store.layout_generation();
            self.last_damage.extend_from_slice(self.engine.damage());
            self.engine.clear_damage();
            self.sync_store_rects();
            self.layout_needed = false;
        } else {
            self.engine.mark_reused(self.tree.len());
        }

        let layout_damage = !self.last_damage.is_empty();
        let structure_changed = self.tree.structure_hash != self.index_structure;
        if rebuilt || layout_damage || structure_changed || self.index.rebuilds() == 0 {
            self.index.rebuild(&self.tree, self.engine.rects());
            self.index_structure = self.tree.structure_hash;
            self.input.invalidate();
        }

        if !self.pending.is_empty() {
            let mut events = std::mem::take(&mut self.pending);
            for event in events.drain(..) {
                route(
                    event,
                    &self.tree,
                    &mut self.index,
                    &mut self.store,
                    &mut self.input,
                    &mut frame.messages,
                );
            }
            self.pending = events;
        }

        frame.display_list.damage.clear();
        frame
            .display_list
            .damage
            .extend_from_slice(&self.last_damage);
        self.collect_store_damage(&mut frame.display_list.damage);
        frame.display_list.damage.sort_by(|a, b| {
            a.x.total_cmp(&b.x)
                .then(a.y.total_cmp(&b.y))
                .then(a.width.total_cmp(&b.width))
                .then(a.height.total_cmp(&b.height))
        });
        frame.display_list.damage.dedup();

        let repaint = !self.painted
            || !frame.display_list.damage.is_empty()
            || !frame.messages.is_empty()
            || layout_damage;

        if repaint {
            self.paint_display_list(frame);
            frame.stats.commands = frame.display_list.commands.len();
            self.painted = true;
        }
        frame.stats.layout = self.engine.stats();
        frame.stats.damage = frame.display_list.damage.len();
        frame.stats.messages = frame.messages.len();
        frame.stats.repainted = repaint;
        self.last_repainted = repaint;
        self.last_messages = frame.messages.len();
    }

    /// An indented dump of the laid-out tree.
    #[must_use]
    pub fn inspect(&self) -> String {
        crate::inspect::inspect(&self.tree, &self.engine)
    }

    /// Explains why a keyed node has the size it has.
    #[must_use]
    pub fn explain(&self, key: &str) -> Option<String> {
        crate::inspect::explain(&self.tree, &self.engine, key)
    }

    /// The prompt 19 debug data for the last frame.
    #[must_use]
    pub fn debug(&self) -> FrameDebug {
        let mut nodes = Vec::with_capacity(self.tree.len());
        for node in &self.tree.nodes {
            let path = self.tree.path(node.id);
            let store_state = self.store.get(node.path_hash);
            let layout_repaint = self.engine.repaint(node.id);
            let interaction = store_state.map_or(RepaintReason::None, |state| state.repaint);
            let repaint = if interaction.repaints() {
                interaction
            } else {
                layout_repaint
            };
            nodes.push(NodeDebug {
                id: node.id,
                path,
                kind: node.kind.name(),
                rect: self.engine.rect(node.id),
                resolved: self
                    .engine
                    .resolved(node.id)
                    .cloned()
                    .unwrap_or_else(|| crate::style::Style::default().resolve(&self.theme)),
                cache: CacheState {
                    layout_reused: !self.last_repainted || !layout_repaint.repaints(),
                    text_reused: !layout_repaint.repaints(),
                    paint_reused: !repaint.repaints(),
                },
                repaint,
                commands: self
                    .command_counts
                    .get(node.id.index())
                    .copied()
                    .unwrap_or(0),
                text: node.text.as_ref().map(crate::style::TextSpec::text),
            });
        }
        FrameDebug {
            viewport: self.viewport,
            scale: self.scale,
            mode: self.theme.mode,
            density: self.theme.density,
            nodes,
            command_count: self.command_counts.iter().sum(),
            damage: self.last_damage.clone(),
            stats: self.engine.stats(),
            messages: self.last_messages,
        }
    }

    /// Whether the last frame rebuilt the display list.
    #[must_use]
    pub fn repainted(&self) -> bool {
        self.last_repainted
    }

    fn sync_store_rects(&mut self) {
        for node in &self.tree.nodes {
            let rect = self.engine.rect(node.id);
            self.store.set_rect(node.path_hash, rect);
        }
    }

    fn collect_store_damage(&mut self, damage: &mut Vec<Rect>) {
        for (_path, state) in self.store.iter() {
            if state.repaint.repaints() && !state.rect.is_empty() {
                damage.push(state.rect);
            }
        }
    }

    fn paint_display_list(&mut self, frame: &mut Frame<Msg>) {
        frame.display_list.commands.clear();
        if self.tree.root.is_none() {
            return;
        }
        let root = self.tree.root.expect("non-empty tree");
        let mut output = std::mem::take(&mut frame.display_list.commands);
        output.clear();
        if self.commands.len() != self.tree.len() {
            self.commands.resize_with(self.tree.len(), Vec::new);
            self.command_hashes.resize(self.tree.len(), 0);
            self.command_counts.resize(self.tree.len(), 0);
        }
        self.paint_node(root, &mut output);
        frame.display_list.commands = output;
    }

    fn paint_node(&mut self, id: NodeId, output: &mut Vec<DrawCommand>) {
        let resolved = self
            .engine
            .resolved(id)
            .cloned()
            .unwrap_or_else(|| crate::style::Style::default().resolve(&self.theme));
        if resolved.hidden || resolved.opacity <= 0.0 {
            return;
        }
        let rect = self.engine.rect(id);
        let clip = resolved.clip;
        let opacity = resolved.opacity;
        let corners = corners(rect, resolved.radius, &self.theme);
        if clip {
            output.push(DrawCommand::ClipPush {
                node: id,
                rect,
                radii: corners,
            });
        }
        if opacity < 1.0 {
            output.push(DrawCommand::LayerPush { node: id, opacity });
        }
        self.emit_own(id, rect, &resolved, output);
        let child_count = self.tree.nodes[id.index()].children.len();
        for index in 0..child_count {
            let child = self.tree.nodes[id.index()].children[index];
            self.paint_node(child, output);
        }
        if opacity < 1.0 {
            output.push(DrawCommand::LayerPop { node: id });
        }
        if clip {
            output.push(DrawCommand::ClipPop { node: id });
        }
    }

    fn emit_own(
        &mut self,
        id: NodeId,
        rect: Rect,
        resolved: &crate::style::ResolvedStyle,
        output: &mut Vec<DrawCommand>,
    ) {
        let node = &self.tree.nodes[id.index()];
        let state = self.store.get(node.path_hash).copied().unwrap_or_default();
        let mut hash = Fnv::new();
        hash.write_u64(node.content_hash);
        hash.write_bool(state.hovered);
        hash.write_bool(state.pressed);
        hash.write_bool(state.focused);
        hash.write_bool(node.style.disabled);
        hash.write_u8(match self.theme.mode {
            crate::theme::ThemeMode::Dark => 0,
            crate::theme::ThemeMode::Light => 1,
        });
        let hash = hash.finish();
        let reused = self.command_hashes.get(id.index()).copied() == Some(hash)
            && self
                .commands
                .get(id.index())
                .is_some_and(|commands| !commands.is_empty());
        if reused {
            let commands = &self.commands[id.index()];
            output.extend_from_slice(commands);
            if let Some(count) = self.command_counts.get_mut(id.index()) {
                *count = commands.len();
            }
            return;
        }

        let mut commands = std::mem::take(&mut self.commands[id.index()]);
        commands.clear();
        if let Some(color) = resolved.bg {
            let color = self.state_color(resolved, color, &state);
            commands.push(DrawCommand::Rect {
                node: id,
                rect,
                color,
                radii: corners(rect, resolved.radius, &self.theme),
            });
        }
        if node.kind == NodeKind::Divider {
            commands.push(DrawCommand::Rect {
                node: id,
                rect,
                color: self.theme.role(ColorRole::Border),
                radii: corners(rect, resolved.radius, &self.theme),
            });
        }
        if let Some(color) = resolved.border {
            commands.push(DrawCommand::Border {
                node: id,
                rect,
                color,
                width: resolved.border_width,
                radii: corners(rect, resolved.radius, &self.theme),
            });
        }
        if resolved.elevation_level != crate::theme::ElevationLevel::E0 {
            commands.push(DrawCommand::Shadow {
                node: id,
                rect,
                color: resolved.elevation.key.color,
                offset: glam::Vec2::new(
                    resolved.elevation.key.offset_x,
                    resolved.elevation.key.offset_y,
                ),
                blur: resolved.elevation.key.blur,
                spread: resolved.elevation.key.spread,
                radii: corners(rect, resolved.radius, &self.theme),
            });
            commands.push(DrawCommand::Shadow {
                node: id,
                rect,
                color: resolved.elevation.ambient.color,
                offset: glam::Vec2::new(
                    resolved.elevation.ambient.offset_x,
                    resolved.elevation.ambient.offset_y,
                ),
                blur: resolved.elevation.ambient.blur,
                spread: resolved.elevation.ambient.spread,
                radii: corners(rect, resolved.radius, &self.theme),
            });
        }
        if let Some(image) = node.image {
            commands.push(DrawCommand::Image {
                node: id,
                rect,
                source: image,
                fit: ImageFit::Cover,
                radii: corners(rect, resolved.radius, &self.theme),
            });
        }
        if let Some(text) = &node.text {
            if let Some(shape) = self.engine.shape(id) {
                let mut spans = Vec::with_capacity(text.spans.len());
                let mut cursor = 0usize;
                for span in &text.spans {
                    let end = cursor + span.text.len();
                    spans.push(TextSpanStyle {
                        start: cursor,
                        end,
                        color: span.tone.unwrap_or(resolved.tone).color(&self.theme),
                        strong: span.strong,
                        mono: span.mono,
                        link: span.link,
                    });
                    cursor = end;
                }
                let text: Arc<str> = Arc::from(text.text());
                commands.push(DrawCommand::Text {
                    node: id,
                    rect,
                    run: TextRun {
                        text,
                        lines: Arc::from(shape.lines.as_slice()),
                        style: resolved.text.style,
                        spans: Arc::from(spans),
                    },
                });
            }
        }
        output.extend_from_slice(&commands);
        if let Some(slot) = self.command_hashes.get_mut(id.index()) {
            *slot = hash;
        }
        if let Some(count) = self.command_counts.get_mut(id.index()) {
            *count = commands.len();
        }
        self.commands[id.index()] = commands;
    }

    fn state_color(
        &self,
        resolved: &crate::style::ResolvedStyle,
        base: Color,
        state: &crate::store::NodeState,
    ) -> Color {
        // Filled actions switch through the theme's fill roles; other colours
        // keep their role until a component opts into a state table in phase 6.
        if resolved.bg == Some(self.theme.colors.accent_fill)
            || base == self.theme.colors.accent_fill
        {
            let accent = if state.pressed {
                AccentState::Pressed
            } else if state.hovered {
                AccentState::Hover
            } else {
                AccentState::Rest
            };
            return self.theme.filled_action(accent).fill;
        }
        base
    }
}

fn corners(rect: Rect, radius: crate::theme::Radius, theme: &Theme) -> [f32; 4] {
    let min_side = rect.width.min(rect.height);
    let value = theme.radii.resolve(radius, min_side);
    [value; 4]
}
