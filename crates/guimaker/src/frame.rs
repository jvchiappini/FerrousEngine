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
use crate::input::focus::FocusOrder;
use crate::input::shortcut::ShortcutRegistry;
use crate::input::{route, InputEvent, InputState, RouteContext};
use crate::layout::LayoutEngine;
use crate::spatial::HitIndex;
use crate::store::{RepaintReason, Store};
use crate::style::{ScrollAxes, Tone};
use crate::text::TextSystem;
use crate::theme::{ColorRole, Duration, Radius, Spacing, Theme};
use crate::view::{Element, NodeId, NodeKind, Tree};
use crate::visual::{VisualAnimator, BACKGROUND, BORDER, FOCUS_RING, FOREGROUND};

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
#[allow(clippy::struct_excessive_bools)] // reason: the flags are independent frame-level states
pub struct Ui<Msg> {
    theme: Theme,
    viewport: Size,
    scale: f32,
    engine: LayoutEngine,
    index: HitIndex,
    store: Store,
    input: InputState,
    focus: FocusOrder,
    shortcuts: ShortcutRegistry<Msg>,
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
    visuals: VisualAnimator,
    animation_damage: Vec<Rect>,
    ring_animating: bool,
    last_blink_phase: Option<bool>,
    frames_painted: u64,
    frames_repainted: u64,
}

impl<Msg: Clone> Ui<Msg> {
    /// Builds a UI with a theme.
    #[must_use]
    pub fn new(theme: Theme) -> Self {
        let mut input = InputState::new();
        input.scroll.set_smooth(!theme.prefers_reduced_motion);
        Self {
            theme,
            viewport: Size::ZERO,
            scale: 1.0,
            engine: LayoutEngine::new(),
            index: HitIndex::new(),
            store: Store::new(),
            input,
            focus: FocusOrder::new(),
            shortcuts: ShortcutRegistry::default(),
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
            visuals: VisualAnimator::new(),
            animation_damage: Vec::new(),
            ring_animating: false,
            last_blink_phase: None,
            frames_painted: 0,
            frames_repainted: 0,
        }
    }

    /// The active theme.
    #[must_use]
    pub fn theme(&self) -> &Theme {
        &self.theme
    }

    /// Replaces the theme; the next frame repaints everything.
    pub fn set_theme(&mut self, theme: Theme) {
        if theme.prefers_reduced_motion != self.theme.prefers_reduced_motion {
            self.input.scroll.set_smooth(!theme.prefers_reduced_motion);
        }
        self.theme = theme;
        self.view_hash = 0;
        self.layout_needed = true;
        self.command_hashes.iter_mut().for_each(|hash| *hash = 0);
    }

    /// Sets the viewport and scale factor.
    pub fn set_viewport(&mut self, viewport: Size, scale: f32) {
        self.viewport = viewport;
        self.scale = scale;
        self.engine.set_viewport(viewport, scale);
        self.layout_needed = true;
    }

    /// Declares what the host can do with overlays that do not fit.
    ///
    /// The default is [`crate::overlay::HostCapabilities::single_window`]: the
    /// solver clamps the overlay to the canvas and it is painted here. A host
    /// that really draws popup windows passes
    /// [`crate::overlay::HostCapabilities::desktop`]; then an anchored overlay
    /// that does not fit is hosted in a popup window and is **not** painted in
    /// the main window.
    pub fn set_host_capabilities(&mut self, capabilities: crate::overlay::HostCapabilities) {
        self.engine.set_host_capabilities(capabilities);
        self.layout_needed = true;
    }

    /// Where the layout decided to host the anchored overlay at `node`.
    #[must_use]
    pub fn overlay_host(&self, node: NodeId) -> Option<crate::overlay::OverlayHost> {
        self.engine.overlay_host(node)
    }

    /// The unclamped popup rect for the anchored overlay at `node`.
    #[must_use]
    pub fn overlay_popup_rect(&self, node: NodeId) -> Option<Rect> {
        self.engine.overlay_popup_rect(node)
    }

    /// Sets the injected clock instant, in milliseconds.
    ///
    /// Every timed decision uses it: double and triple click, long press, hover
    /// intent, scroll momentum and the caret blink. A host calls it once per
    /// frame with its monotonic clock; the library never reads time itself.
    /// Returns the elapsed milliseconds since the previous call.
    #[allow(clippy::cast_possible_truncation)] // reason: a frame delta is far below f32's range
    pub fn set_time_ms(&mut self, now_ms: f64) -> f64 {
        let dt = self.input.set_time_ms(now_ms);
        if let Some(gesture) = self.input.gesture.tick(now_ms) {
            self.input.last_gesture = Some(gesture);
        }
        if self.input.scroll.advance((dt / 1000.0) as f32) {
            if let Some(path) = self.input.scroll_target {
                self.store.set_scroll(path, self.input.scroll.offset());
            }
        }
        dt
    }

    /// The input state.
    #[must_use]
    pub fn input(&self) -> &InputState {
        &self.input
    }

    /// The input state, mutably (an app may push a focus scope).
    pub fn input_mut(&mut self) -> &mut InputState {
        &mut self.input
    }

    /// Opens an overlay and remembers the focused control as its opener.
    ///
    /// `Escape` (one level) and a click outside (the whole chain) close it and
    /// return the focus to that control, across windows: the state lives here,
    /// not in the window that hosts the overlay.
    pub fn open_overlay(&mut self, path: u64) {
        let opener = self
            .input
            .focused
            .and_then(|id| self.tree.node(id))
            .map(|node| node.path_hash);
        self.input.push_overlay_from(path, opener);
        // The new overlay is topmost under its own rect; a cached hit from the
        // control underneath must not answer the next pointer query.
        self.index.invalidate();
    }

    /// The shortcut registry.
    #[must_use]
    pub fn shortcuts(&self) -> &ShortcutRegistry<Msg> {
        &self.shortcuts
    }

    /// The shortcut registry, mutably (bind or clear a scope).
    pub fn shortcuts_mut(&mut self) -> &mut ShortcutRegistry<Msg> {
        &mut self.shortcuts
    }

    /// The precomputed focus order.
    #[must_use]
    pub fn focus_order(&self) -> &FocusOrder {
        &self.focus
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
        self.animation_damage.clear();

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
            self.focus.rebuild(&self.tree, self.input.focus_scope());
            self.input.rebind_focus(&self.tree, self.store.focus());
        }

        self.dispatch_pending(&mut frame.messages);

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

        let now = self.input.now_ms;
        let has_caret = self.has_caret();
        let blink_visible = crate::motion::caret_visible(now);
        let blink_toggle = has_caret && self.last_blink_phase != Some(blink_visible);
        let repaint = !self.painted
            || !frame.display_list.damage.is_empty()
            || !frame.messages.is_empty()
            || layout_damage
            || self.visuals.any_animating(now)
            || self.input.scroll.is_moving()
            || self.ring_animating
            || blink_toggle;

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
        self.last_blink_phase = has_caret.then_some(blink_visible);
        self.frames_painted += 1;
        if repaint {
            self.frames_repainted += 1;
        }
    }

    /// Routes the queued input events now, without painting.
    ///
    /// The frame calls this itself before painting. A host that measures or
    /// batches input may call it directly once the layout is valid (after one
    /// [`Self::paint`]), so routing cost is separable from paint cost.
    pub fn dispatch_pending(&mut self, messages: &mut Vec<Msg>) {
        if self.pending.is_empty() {
            return;
        }
        let mut events = std::mem::take(&mut self.pending);
        for event in events.drain(..) {
            let mut context = RouteContext {
                tree: &self.tree,
                index: &mut self.index,
                layout: &self.engine,
                store: &mut self.store,
                input: &mut self.input,
                focus: &mut self.focus,
                shortcuts: &self.shortcuts,
                messages,
            };
            route(&mut context, event);
        }
        self.pending = events;
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
                resolved: self.engine.resolved(node.id).cloned().unwrap_or_else(|| {
                    crate::style::Style::default().resolve(&self.theme, self.viewport.width)
                }),
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

    /// Whether any animation is still running at the injected instant.
    ///
    /// This covers colour transitions, smooth scroll easing, and the animated
    /// focus ring. A host that renders on demand asks this after `paint` and
    /// keeps requesting frames until it is false, so a hover fade, a wheel
    /// glide or a focus ring completes.
    #[must_use]
    pub fn animating(&self) -> bool {
        self.visuals.any_animating(self.input.now_ms)
            || self.input.scroll.is_moving()
            || self.ring_animating
    }

    /// Whether the tree contains a text caret.
    ///
    /// A caret blinks on the injected clock, so an otherwise idle host still
    /// needs one wake per blink phase while this is true.
    #[must_use]
    pub fn has_caret(&self) -> bool {
        self.tree
            .nodes
            .iter()
            .any(|node| node.style.caret.is_some())
    }

    /// The next instant the host should paint if no input arrives.
    ///
    /// While an animation runs this asks for another frame on the next tick;
    /// otherwise a caret asks for the next blink boundary; otherwise the UI is
    /// idle and `None` lets the host sleep. The returned instant is on the
    /// same injected millisecond clock passed to [`Self::set_time_ms`].
    #[must_use]
    pub fn next_wake_ms(&self, now_ms: f64) -> Option<f64> {
        if self.animating() {
            Some(now_ms + 16.0)
        } else if self.has_caret() {
            Some(crate::motion::next_caret_wake_ms(now_ms))
        } else {
            None
        }
    }

    /// The number of `paint` calls since construction (the redraw counter).
    #[must_use]
    pub fn frames_painted(&self) -> u64 {
        self.frames_painted
    }

    /// The number of those `paint` calls that rebuilt the display list.
    #[must_use]
    pub fn frames_repainted(&self) -> u64 {
        self.frames_repainted
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
        self.ring_animating = false;
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
        self.paint_node(root, Interaction::default(), &mut output);
        frame.display_list.commands = output;
        frame
            .display_list
            .damage
            .extend_from_slice(&self.animation_damage);
    }

    fn paint_node(&mut self, id: NodeId, parent: Interaction, output: &mut Vec<DrawCommand>) {
        // An overlay that does not fit belongs to a popup window: the host
        // draws it there, never in the main window (no duplicate pixels).
        if self.engine.overlay_host(id) == Some(crate::overlay::OverlayHost::Popup) {
            return;
        }
        let resolved = self.engine.resolved(id).cloned().unwrap_or_else(|| {
            crate::style::Style::default().resolve(&self.theme, self.viewport.width)
        });
        if resolved.hidden || resolved.opacity <= 0.0 {
            return;
        }
        let (clip, opacity, path_hash, selected, disabled) = {
            let node = &self.tree.nodes[id.index()];
            (
                resolved.clip,
                resolved.opacity,
                node.path_hash,
                node.style.selected,
                node.style.disabled,
            )
        };
        let state = self.store.get(path_hash).copied().unwrap_or_default();
        let interaction = Interaction {
            hovered: parent.hovered || state.hovered,
            pressed: parent.pressed || state.pressed,
            focused: parent.focused || state.focused,
            selected: parent.selected || selected,
            disabled: parent.disabled || disabled,
        };
        let rect = self.engine.rect(id);
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
        self.emit_own(id, rect, &resolved, interaction, state.focused, output);
        let child_count = self.tree.nodes[id.index()].children.len();
        for index in 0..child_count {
            let child = self.tree.nodes[id.index()].children[index];
            self.paint_node(child, interaction, output);
        }
        self.emit_scrollbar(id, rect, &resolved, path_hash, output);
        if opacity < 1.0 {
            output.push(DrawCommand::LayerPop { node: id });
        }
        if clip {
            output.push(DrawCommand::ClipPop { node: id });
        }
    }

    /// Emits the overlay scrollbar thumb for a scroll container, if active.
    ///
    /// The thumb is painted after the children and before the clip pop, so it
    /// sits on top of the content and stays inside the container's clip. A
    /// vertical and a horizontal thumb are independent; when both axes overflow
    /// they overlap at the bottom-right corner, which the rounded ends hide.
    fn emit_scrollbar(
        &self,
        id: NodeId,
        rect: Rect,
        resolved: &crate::style::ResolvedStyle,
        path_hash: u64,
        output: &mut Vec<DrawCommand>,
    ) {
        if !resolved.scroll.clips() || !self.input.scrollbar_active(&self.tree, id, path_hash) {
            return;
        }
        let offset = self.store.scroll(path_hash);
        let content = self.engine.content_size(id);
        let margin = self.theme.spacing(Spacing::S1);
        let thickness = self.theme.spacing(Spacing::S1);
        let min_length = self.theme.spacing(Spacing::S4);
        let color = self.theme.role(ColorRole::TextMuted);
        if matches!(resolved.scroll, ScrollAxes::Vertical | ScrollAxes::Both)
            && content.height > rect.height + 0.5
        {
            let track = (rect.height - margin * 2.0).max(0.0);
            let length = (track * (rect.height / content.height))
                .max(min_length)
                .min(track);
            let travel = (track - length).max(0.0);
            let range = content.height - rect.height;
            let progress = if range > 0.0 {
                (offset.y / range).clamp(0.0, 1.0)
            } else {
                0.0
            };
            let thumb = Rect::new(
                rect.right() - margin - thickness,
                rect.y + margin + travel * progress,
                thickness,
                length,
            );
            output.push(DrawCommand::Rect {
                node: id,
                rect: thumb,
                color,
                radii: corners(thumb, Radius::Full, &self.theme),
            });
        }
        if matches!(resolved.scroll, ScrollAxes::Horizontal | ScrollAxes::Both)
            && content.width > rect.width + 0.5
        {
            let track = (rect.width - margin * 2.0).max(0.0);
            let length = (track * (rect.width / content.width))
                .max(min_length)
                .min(track);
            let travel = (track - length).max(0.0);
            let range = content.width - rect.width;
            let progress = if range > 0.0 {
                (offset.x / range).clamp(0.0, 1.0)
            } else {
                0.0
            };
            let thumb = Rect::new(
                rect.x + margin + travel * progress,
                rect.bottom() - margin - thickness,
                length,
                thickness,
            );
            output.push(DrawCommand::Rect {
                node: id,
                rect: thumb,
                color,
                radii: corners(thumb, Radius::Full, &self.theme),
            });
        }
    }

    fn emit_own(
        &mut self,
        id: NodeId,
        rect: Rect,
        resolved: &crate::style::ResolvedStyle,
        interaction: Interaction,
        self_focused: bool,
        output: &mut Vec<DrawCommand>,
    ) {
        let (
            path_hash,
            content_hash,
            kind,
            image,
            icon_path,
            path_points,
            style_bg,
            style_border,
            style_blur,
            visual,
            has_visual,
        ) = {
            let node = &self.tree.nodes[id.index()];
            (
                node.path_hash,
                node.content_hash,
                node.kind,
                node.image,
                node.icon_path.clone(),
                node.path_points.clone(),
                node.style.bg,
                node.style.border,
                node.style.backdrop_blur,
                node.style.visual,
                node.has_visual,
            )
        };

        // Plain nodes (no state roles and no accent fill) have a constant
        // colour; the flag is computed once when the tree is built.
        let (bg_role, border_role, base_tone) = if has_visual {
            let bg = state_role(style_bg, visual.bg, interaction);
            let border = state_role(style_border, visual.border, interaction);
            let tone = state_tone(resolved.tone, visual.tone, interaction).paired_with(bg);
            (bg, border, tone)
        } else {
            (style_bg, style_border, resolved.tone)
        };
        let elevation_level = if interaction.hovered || interaction.selected {
            visual.elevation_hover.unwrap_or(resolved.elevation_level)
        } else {
            resolved.elevation_level
        };
        let keyboard_ring = self_focused && self.input.keyboard_focus;

        let now = self.input.now_ms;
        let transition = visual.transition;
        // The animator map only holds nodes that actually transition, so a
        // plain node's colour stays a direct theme lookup.
        let (bg, border, foreground, animating) = if has_visual {
            let bg = bg_role.map(|role| {
                let target = self.theme.role(role);
                self.visuals
                    .color(path_hash, BACKGROUND, target, now, transition)
            });
            let border = border_role.map(|role| {
                let target = self.theme.role(role);
                self.visuals
                    .color(path_hash, BORDER, target, now, transition)
            });
            let foreground = self.visuals.color(
                path_hash,
                FOREGROUND,
                base_tone.color(&self.theme),
                now,
                transition,
            );
            let animating = self.visuals.is_animating(path_hash, now);
            if animating {
                self.animation_damage.push(rect);
            }
            (bg, border, foreground, animating)
        } else {
            (
                bg_role.map(|role| self.theme.role(role)),
                border_role.map(|role| self.theme.role(role)),
                base_tone.color(&self.theme),
                false,
            )
        };

        // The focus ring fades in and out as one scalar. Priming the scalar at
        // zero on every visual node means a node that gains keyboard focus
        // eases from 0 instead of snapping on.
        let ring_target = if keyboard_ring { 1.0 } else { 0.0 };
        let ring_alpha = if has_visual || keyboard_ring {
            self.visuals.scalar(
                path_hash,
                FOCUS_RING,
                ring_target,
                now,
                self.theme.duration(Duration::Fast),
            )
        } else {
            0.0
        };
        // Only keyboard focus may show a ring, so a pointer click still paints
        // none. A visual node that just lost focus keeps its fading ring while
        // the keyboard flag remains set.
        let ring_drawn = self.input.keyboard_focus && ring_alpha > 0.01;
        let ring_changing = self.input.keyboard_focus && (ring_alpha - ring_target).abs() > 0.001;
        if has_visual || keyboard_ring {
            self.ring_animating = self.ring_animating || ring_changing;
        }
        let animating = animating || ring_changing;
        let caret = self.tree.nodes[id.index()].style.caret;
        let caret_visible = caret.is_some() && crate::motion::caret_visible(now);

        let mut hash = Fnv::new();
        hash.write_u64(content_hash);
        let state_sensitive = has_visual || ring_drawn || style_bg == Some(ColorRole::AccentFill);
        if state_sensitive {
            hash.write_bool(interaction.hovered);
            hash.write_bool(interaction.pressed);
            hash.write_bool(interaction.focused);
            hash.write_bool(interaction.selected);
            hash.write_bool(interaction.disabled);
        } else {
            for _ in 0..5 {
                hash.write_bool(false);
            }
        }
        hash.write_f32(ring_alpha);
        hash.write_bool(ring_drawn);
        hash.write_bool(caret_visible);
        hash.write_u8(base_tone as u8);
        hash.write_u8(match self.theme.mode {
            crate::theme::ThemeMode::Dark => 0,
            crate::theme::ThemeMode::Light => 1,
        });
        let hash = hash.finish();
        let reused = !animating
            && self.command_hashes.get(id.index()).copied() == Some(hash)
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
        let shape_radii = corners(rect, resolved.radius, &self.theme);
        if let Some(radius) = style_blur {
            commands.push(DrawCommand::BackdropBlur {
                node: id,
                rect,
                radius: self.theme.spacing(radius),
                radii: shape_radii,
            });
        }
        // Elevation shadows sit **under** the surface: emitting them before
        // the background keeps the shadow quad from painting its tinted
        // interior over the card (the grey smears and the banded edges the
        // reviewer saw came from the shadow being composited on top).
        if elevation_level != crate::theme::ElevationLevel::E0 {
            let elevation = self.theme.elevation_at(elevation_level);
            commands.push(DrawCommand::Shadow {
                node: id,
                rect,
                color: elevation.key.color,
                offset: glam::Vec2::new(elevation.key.offset_x, elevation.key.offset_y),
                blur: elevation.key.blur,
                spread: elevation.key.spread,
                radii: shape_radii,
            });
            commands.push(DrawCommand::Shadow {
                node: id,
                rect,
                color: elevation.ambient.color,
                offset: glam::Vec2::new(elevation.ambient.offset_x, elevation.ambient.offset_y),
                blur: elevation.ambient.blur,
                spread: elevation.ambient.spread,
                radii: shape_radii,
            });
        }
        if let Some(color) = bg {
            commands.push(DrawCommand::Rect {
                node: id,
                rect,
                color,
                radii: shape_radii,
            });
        }
        if kind == NodeKind::Divider {
            commands.push(DrawCommand::Rect {
                node: id,
                rect,
                color: self.theme.role(ColorRole::Border),
                radii: shape_radii,
            });
        }
        if let Some(color) = border {
            commands.push(DrawCommand::Border {
                node: id,
                rect,
                color,
                width: resolved.border_width,
                sides: resolved.border_sides,
                radii: shape_radii,
            });
        }
        if let Some(points) = path_points {
            if points.len() >= 2 {
                let width = resolved.stroke_width.unwrap_or(1.0);
                let color = foreground;
                let absolute: Vec<glam::Vec2> = points
                    .iter()
                    .map(|point| {
                        glam::Vec2::new(
                            rect.x + point.x * rect.width,
                            rect.y + point.y * rect.height,
                        )
                    })
                    .collect();
                commands.push(DrawCommand::Path {
                    node: id,
                    points: Arc::from(absolute),
                    width,
                    color,
                });
            }
        }
        if let Some(source) = image {
            commands.push(DrawCommand::Image {
                node: id,
                rect,
                source,
                fit: ImageFit::Cover,
                radii: shape_radii,
            });
        }
        if let Some(path) = icon_path {
            let content = content_box(rect, resolved);
            commands.push(DrawCommand::Icon {
                node: id,
                rect: content,
                path,
                color: foreground,
            });
        }
        if ring_drawn {
            let width = self
                .theme
                .borders
                .resolve(crate::theme::BorderWidth::Regular);
            let ring = Rect::new(
                rect.x - width,
                rect.y - width,
                rect.width + width * 2.0,
                rect.height + width * 2.0,
            );
            let base = self.theme.role(ColorRole::FocusRing);
            commands.push(DrawCommand::Border {
                node: id,
                rect: ring,
                color: base.with_alpha(base.alpha() * ring_alpha),
                width,
                sides: [width; 4],
                radii: corners(ring, resolved.radius, &self.theme),
            });
        }
        if let Some(text) = &self.tree.nodes[id.index()].text {
            if let Some(shape) = self.engine.shape(id) {
                let mut spans = Vec::with_capacity(text.spans.len());
                let mut cursor = 0usize;
                for span in &text.spans {
                    let end = cursor + span.text.len();
                    spans.push(TextSpanStyle {
                        start: cursor,
                        end,
                        color: span.tone.map_or(foreground, |tone| tone.color(&self.theme)),
                        strong: span.strong,
                        mono: span.mono,
                        link: span.link,
                    });
                    cursor = end;
                }
                let text: Arc<str> = Arc::from(text.text());
                let text_len = text.len();
                let content = content_box(rect, resolved);
                let text_rect = Rect::new(
                    content.x,
                    content.y + (content.height - shape.height) * 0.5,
                    content.width,
                    shape.height,
                );
                let line_height = resolved.text.style.line_height.max(1.0);
                // Real selection rects from the shaped glyph positions: one
                // rect per line the range touches, behind the glyphs.
                if let Some((start, end)) = self.tree.nodes[id.index()].style.selection {
                    let (start, end) = (start.min(text.len()), end.min(text.len()));
                    if start < end {
                        let selection = self.theme.role(ColorRole::AccentSubtle);
                        for (line_index, line) in shape.lines.iter().enumerate() {
                            let from = start.max(line.start);
                            let to = end.min(line.end);
                            if from >= to {
                                continue;
                            }
                            let x0 = caret_x(shape, line_index, from);
                            let x1 = caret_x(shape, line_index, to);
                            commands.push(DrawCommand::Rect {
                                node: id,
                                rect: Rect::new(
                                    text_rect.x + x0,
                                    text_rect.y + line_offset(line_index, line_height),
                                    (x1 - x0).max(1.0),
                                    line_height,
                                ),
                                color: selection,
                                radii: [0.0; 4],
                            });
                        }
                    }
                }
                commands.push(DrawCommand::Text {
                    node: id,
                    rect: text_rect,
                    run: TextRun {
                        text,
                        lines: Arc::from(shape.lines.as_slice()),
                        style: resolved.text.style,
                        spans: Arc::from(spans),
                    },
                });
                // A real caret rect at the shaped glyph position, drawn over
                // the glyphs. It blinks on the injected clock.
                if caret_visible {
                    if let Some(caret) = caret {
                        let caret = caret.min(text_len);
                        let line_index = shape
                            .lines
                            .iter()
                            .position(|line| caret >= line.start && caret <= line.end)
                            .unwrap_or(shape.lines.len().saturating_sub(1));
                        let x = caret_x(shape, line_index, caret);
                        commands.push(DrawCommand::Rect {
                            node: id,
                            rect: Rect::new(
                                text_rect.x + x - 0.75,
                                text_rect.y + line_offset(line_index, line_height),
                                1.5,
                                line_height,
                            ),
                            color: self.theme.role(ColorRole::Accent),
                            radii: [0.0; 4],
                        });
                    }
                }
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
}

/// The interaction state a node inherits from its layout ancestors.
#[allow(clippy::struct_excessive_bools)] // reason: the five flags are independent inherited states
#[derive(Clone, Copy, Debug, Default)]
struct Interaction {
    hovered: bool,
    pressed: bool,
    focused: bool,
    selected: bool,
    disabled: bool,
}

/// Picks the colour role for the current interaction state.
fn state_role(
    rest: Option<ColorRole>,
    roles: crate::style::StateRoles,
    interaction: Interaction,
) -> Option<ColorRole> {
    let role = if interaction.disabled {
        roles.disabled.or(rest)
    } else if interaction.selected {
        roles.selected.or(rest)
    } else if interaction.pressed {
        roles.pressed.or(rest)
    } else if interaction.hovered {
        roles.hover.or(rest)
    } else {
        rest
    };
    role.map(|role| match role {
        ColorRole::AccentFill if !interaction.disabled => {
            if interaction.pressed {
                ColorRole::AccentFillPressed
            } else if interaction.hovered {
                ColorRole::AccentFillHover
            } else {
                role
            }
        }
        other => other,
    })
}

/// Picks the text tone for the current interaction state.
fn state_tone(rest: Tone, tones: crate::style::StateTones, interaction: Interaction) -> Tone {
    if interaction.disabled {
        tones.disabled.unwrap_or(rest)
    } else if interaction.selected {
        tones.selected.unwrap_or(rest)
    } else if interaction.pressed {
        tones.pressed.unwrap_or(rest)
    } else if interaction.hovered {
        tones.hover.unwrap_or(rest)
    } else {
        rest
    }
}

fn corners(rect: Rect, radius: crate::theme::Radius, theme: &Theme) -> [f32; 4] {
    let min_side = rect.width.min(rect.height);
    let value = theme.radii.resolve(radius, min_side);
    [value; 4]
}

/// The padded content box of a border-box rectangle.
/// The top of `line_index` inside a text box.
#[allow(clippy::cast_precision_loss)] // reason: a shaped line count is far below f32's exact range
fn line_offset(line_index: usize, line_height: f32) -> f32 {
    line_index as f32 * line_height
}

/// The x offset of `byte` inside `line_index`, from the shaped glyphs.
///
/// The caret sits at the left edge of the glyph that starts at `byte`, or at
/// the current pen position when `byte` is inside a grapheme or past the last
/// glyph. It never guesses from character counts.
fn caret_x(shape: &crate::text::TextShape, line_index: usize, byte: usize) -> f32 {
    let Some(line) = shape.lines.get(line_index) else {
        return 0.0;
    };
    let mut x = 0.0;
    for glyph in shape
        .glyphs
        .iter()
        .take(line.glyph_end)
        .skip(line.glyph_start)
    {
        if glyph.byte_start >= byte {
            return glyph.x;
        }
        x = glyph.x + glyph.advance;
    }
    x.max(0.0).min(line.width.max(0.0))
}

fn content_box(rect: Rect, resolved: &crate::style::ResolvedStyle) -> Rect {
    let padding = resolved.padding;
    Rect::new(
        rect.x + padding.left,
        rect.y + padding.top,
        (rect.width - padding.left - padding.right).max(0.0),
        (rect.height - padding.top - padding.bottom).max(0.0),
    )
}

#[cfg(test)]
mod state_tests {
    use super::{Frame, Ui};
    use crate::display_list::DrawCommand;
    use crate::geom::Size;
    use crate::icons::Icon;
    use crate::input::{InputEvent, PointerButton};
    use crate::prelude::*;
    use crate::theme::Duration;
    use glam::Vec2;

    fn hover_view() -> Element<()> {
        Element::column()
            .width(SizeSpec::Fill)
            .height(SizeSpec::Fill)
            .child(
                Element::row()
                    .key("target")
                    .width(SizeSpec::Fill)
                    .height(SizeSpec::Token(Spacing::S10))
                    .bg(ColorRole::Surface)
                    .hover_bg(ColorRole::SurfaceRaised)
                    .pressed_bg(ColorRole::SurfaceSunken)
                    .hover_tone(Tone::Accent)
                    .radius(Radius::Sm)
                    .interactive()
                    .transition(Duration::Fast)
                    .child(text::<()>("Hover me")),
            )
    }

    #[test]
    fn hover_animates_between_theme_roles_on_the_injected_clock() {
        let theme = Theme::dark();
        let view = hover_view();
        let mut ui: Ui<()> = Ui::new(theme.clone());
        ui.set_viewport(Size::new(400.0, 120.0), 1.0);
        let mut frame = Frame::new();
        let mut text = MetricsTextSystem::new();
        ui.paint(&view, &mut frame, &mut text);
        assert!(!ui.animating());

        let id = ui.tree().find_key("target").expect("target");
        let rect = ui.layout().rect(id);
        let point = Vec2::new(rect.x + 8.0, rect.y + rect.height * 0.5);
        ui.push_event(InputEvent::PointerMoved { position: point });
        ui.set_time_ms(16.0);
        ui.paint(&view, &mut frame, &mut text);
        assert!(ui.animating(), "the hover fade is still running");
        // The frame that starts the fade is still at the rest colour; sample
        // again halfway through the 90 ms transition.
        ui.set_time_ms(61.0);
        ui.paint(&view, &mut frame, &mut text);
        assert!(ui.animating(), "the hover fade is still running");

        let fill = frame
            .display_list
            .commands
            .iter()
            .find_map(|command| match command {
                DrawCommand::Rect { node, color, .. } if *node == id => Some(*color),
                _ => None,
            })
            .expect("the target paints a background");
        let rest_color = theme.role(ColorRole::Surface).to_linear()[0];
        let hover_color = theme.role(ColorRole::SurfaceRaised).to_linear()[0];
        let value = fill.to_linear()[0];
        assert!(
            value > rest_color.min(hover_color) && value < rest_color.max(hover_color),
            "{value} is not between {rest_color} and {hover_color}"
        );

        ui.set_time_ms(500.0);
        ui.paint(&view, &mut frame, &mut text);
        assert!(!ui.animating(), "the fade finished");
    }

    #[test]
    fn keyboard_focus_paints_the_focus_ring_and_a_pointer_click_does_not() {
        let theme = Theme::dark();
        let view = hover_view();
        let mut ui: Ui<()> = Ui::new(theme.clone());
        ui.set_viewport(Size::new(400.0, 120.0), 1.0);
        let mut frame = Frame::new();
        let mut text = MetricsTextSystem::new();
        ui.paint(&view, &mut frame, &mut text);
        let ring = theme.role(ColorRole::FocusRing);

        ui.push_event(InputEvent::KeyDown {
            key: ShortcutKey::Tab,
            repeat: false,
        });
        ui.set_time_ms(16.0);
        ui.paint(&view, &mut frame, &mut text);
        ui.set_time_ms(200.0);
        ui.paint(&view, &mut frame, &mut text);
        assert!(
            frame.display_list.commands.iter().any(|command| matches!(
                command,
                DrawCommand::Border { color, .. } if *color == ring
            )),
            "keyboard focus paints a ring"
        );

        let id = ui.tree().find_key("target").expect("target");
        let rect = ui.layout().rect(id);
        ui.push_event(InputEvent::PointerDown {
            position: Vec2::new(rect.x + 8.0, rect.y + 8.0),
            button: PointerButton::Primary,
        });
        ui.set_time_ms(220.0);
        ui.paint(&view, &mut frame, &mut text);
        assert!(
            !frame.display_list.commands.iter().any(|command| matches!(
                command,
                DrawCommand::Border { color, .. } if *color == ring
            )),
            "a pointer focus does not paint a keyboard ring"
        );
    }

    #[test]
    fn an_icon_node_emits_an_icon_command_in_the_text_tone() {
        let theme = Theme::dark();
        let view: Element<()> = Element::icon(Icon::Search.path())
            .key("icon")
            .width(SizeSpec::Token(Spacing::S4))
            .height(SizeSpec::Token(Spacing::S4))
            .tone(Tone::Accent);
        let mut ui: Ui<()> = Ui::new(theme.clone());
        ui.set_viewport(Size::new(100.0, 100.0), 1.0);
        let mut frame = Frame::new();
        let mut text = MetricsTextSystem::new();
        ui.paint(&view, &mut frame, &mut text);
        let icon = frame
            .display_list
            .commands
            .iter()
            .find_map(|command| match command {
                DrawCommand::Icon { path, color, .. } => Some((path.clone(), *color)),
                _ => None,
            });
        let (path, color) = icon.expect("an icon command");
        assert_eq!(&*path, Icon::Search.path());
        assert_eq!(color, theme.role(ColorRole::Accent));
    }
}
