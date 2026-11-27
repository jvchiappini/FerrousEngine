//! Input: raw events in, UI state out.
//!
//! The router is the only place that walks the laid-out tree for input. It
//! consults the spatial index for a hit (`O(cell)`, never `O(nodes)`), the
//! precomputed focus order for Tab, the shortcut registry for chords and the
//! scroll controller for wheel momentum. Gestures come from
//! [`gesture::GestureRecognizer`]; text editing from
//! [`text_editor::TextEditor`]; a scroll container's offset is mirrored into
//! the keyed store so layout, paint and hit testing agree.
//!
//! Nothing here reads a clock: the host calls [`InputState::set_time_ms`] with
//! the injected instant, and every timed decision (double click, long press,
//! hover intent, momentum, caret blink) uses it.

#![allow(
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::cast_possible_wrap
)] // reason: row, column and pixel values are bounded UI quantities

pub mod focus;
pub mod gesture;
pub mod grapheme;
pub mod scroll;
pub mod shortcut;
pub mod text_editor;
pub mod tree;
pub mod virtual_list;

use glam::Vec2;

use crate::layout::LayoutEngine;
use crate::spatial::HitIndex;
use crate::store::{RepaintReason, Store};
use crate::style::{Cursor, ScrollAxes};
use crate::view::{NodeId, Tree};
use focus::{FocusOrder, FocusScope};
use gesture::{Gesture, GestureConfig, GestureRecognizer};
pub use scroll::{ScrollController, ScrollKind};
use shortcut::{Chord, ShortcutKey, ShortcutRegistry};

/// A pointer button.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PointerButton {
    /// The primary (left) button.
    Primary,
    /// The secondary (right) button.
    Secondary,
    /// The middle (wheel click) button.
    Middle,
}

#[allow(clippy::struct_excessive_bools)] // reason: the four platform modifiers are independent
/// The modifier keys held.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Modifiers {
    /// Shift.
    pub shift: bool,
    /// Control.
    pub control: bool,
    /// Alt.
    pub alt: bool,
    /// The platform key.
    pub super_key: bool,
}

impl Modifiers {
    /// No modifier.
    #[must_use]
    pub const fn none() -> Self {
        Self {
            shift: false,
            control: false,
            alt: false,
            super_key: false,
        }
    }
}

/// A raw input event the host feeds the library.
#[derive(Clone, Debug, PartialEq)]
pub enum InputEvent {
    /// The pointer moved. A run of moves coalesces to the latest.
    PointerMoved {
        /// Position in logical pixels.
        position: Vec2,
    },
    /// A button went down.
    PointerDown {
        /// Position in logical pixels.
        position: Vec2,
        /// The button.
        button: PointerButton,
    },
    /// A button went up.
    PointerUp {
        /// Position in logical pixels.
        position: Vec2,
        /// The button.
        button: PointerButton,
    },
    /// The pointer left the window.
    PointerLeft,
    /// The host lost focus.
    FocusLost,
    /// The wheel or touchpad scrolled.
    Wheel {
        /// Position in logical pixels.
        position: Vec2,
        /// The delta; positive `y` scrolls up.
        delta: Vec2,
        /// Wheel lines or touchpad pixels.
        kind: ScrollKind,
    },
    /// A key went down.
    KeyDown {
        /// The key.
        key: ShortcutKey,
        /// Whether this is an auto-repeat.
        repeat: bool,
    },
    /// A key went up.
    KeyUp {
        /// The key.
        key: ShortcutKey,
    },
    /// Committed text (typed, pasted or an IME commit).
    Text(String),
    /// The modifier state changed.
    Modifiers(Modifiers),
}

/// Persistent input and interaction state.
#[derive(Clone, Debug)]
pub struct InputState {
    /// The last pointer position.
    pub pointer: Vec2,
    /// The node currently under the pointer.
    pub hovered: Option<NodeId>,
    /// The node with keyboard focus (the id for this tree generation).
    pub focused: Option<NodeId>,
    /// The node that initiated the press.
    pub pressed: Option<NodeId>,
    /// The node that captured the pointer.
    pub captured: Option<NodeId>,
    /// Whether the pointer has ever been seen.
    pub pointer_seen: bool,
    /// The injected clock instant, in milliseconds.
    pub now_ms: f64,
    /// The last instant the pointer moved.
    pub pointer_moved_at: f64,
    /// The last instant a wheel actually moved a scroll target or offset.
    pub last_scroll_ms: f64,
    /// The modifier state.
    pub modifiers: Modifiers,
    /// The gesture recognizer.
    pub gesture: GestureRecognizer,
    /// The click count of the last completed click.
    pub click_count: u8,
    /// The last gesture (debug and cursor handling).
    pub last_gesture: Option<Gesture>,
    /// The instant the hovered node changed.
    pub hover_since_ms: f64,
    /// The scroll controller of the active scroll container.
    pub scroll: ScrollController,
    /// The path hash of the container `scroll` drives.
    pub scroll_target: Option<u64>,
    /// Whether a drag is in progress.
    pub dragging: bool,
    /// Total drag travel since the press.
    pub drag_total: Vec2,
    /// The focus scope stack, innermost last.
    pub focus_scopes: Vec<FocusScope>,
    /// The overlay stack (path hashes), innermost last.
    pub overlays: Vec<u64>,
    /// The focus path that opened each overlay, innermost last.
    pub overlay_openers: Vec<Option<u64>>,
    /// A pointer press that moved past the drag threshold.
    pub drag_button: Option<PointerButton>,
    /// Whether the last focus change came from the keyboard.
    ///
    /// The focus ring is drawn only while this is true, so a pointer click
    /// focuses a control without painting a keyboard ring.
    pub keyboard_focus: bool,
}

impl Default for InputState {
    fn default() -> Self {
        Self::new()
    }
}

impl InputState {
    /// Builds an empty input state.
    #[must_use]
    pub fn new() -> Self {
        Self {
            pointer: Vec2::ZERO,
            hovered: None,
            focused: None,
            pressed: None,
            captured: None,
            pointer_seen: false,
            now_ms: 0.0,
            pointer_moved_at: 0.0,
            last_scroll_ms: 0.0,
            modifiers: Modifiers::none(),
            gesture: GestureRecognizer::new(GestureConfig::default()),
            click_count: 0,
            last_gesture: None,
            hover_since_ms: 0.0,
            scroll: ScrollController::new(),
            scroll_target: None,
            dragging: false,
            drag_total: Vec2::ZERO,
            focus_scopes: Vec::new(),
            overlays: Vec::new(),
            overlay_openers: Vec::new(),
            drag_button: None,
            keyboard_focus: false,
        }
    }

    /// Sets the injected clock instant and returns the elapsed milliseconds.
    pub fn set_time_ms(&mut self, now_ms: f64) -> f64 {
        let dt = if self.now_ms > 0.0 && now_ms > self.now_ms {
            now_ms - self.now_ms
        } else {
            0.0
        };
        self.now_ms = now_ms;
        dt
    }

    /// The innermost focus scope, if any.
    #[must_use]
    pub fn focus_scope(&self) -> Option<FocusScope> {
        self.focus_scopes.last().copied()
    }

    /// Pushes a focus scope.
    pub fn push_focus_scope(&mut self, scope: FocusScope) {
        self.focus_scopes.push(scope);
    }

    /// Pops the innermost focus scope.
    pub fn pop_focus_scope(&mut self) -> Option<FocusScope> {
        self.focus_scopes.pop()
    }

    /// Pushes an overlay path; shortcuts in the `Overlay` scope become active
    /// and `Escape` closes it.
    pub fn push_overlay(&mut self, path: u64) {
        self.push_overlay_from(path, None);
    }

    /// Pushes an overlay path and remembers the focus path that opened it, so
    /// dismissal can return the focus to the opener.
    pub fn push_overlay_from(&mut self, path: u64, opener: Option<u64>) {
        self.overlays.push(path);
        self.overlay_openers.push(opener);
    }

    /// Pops the topmost overlay and the focus it opened from.
    pub fn pop_overlay(&mut self) -> Option<u64> {
        self.overlay_openers.pop();
        self.overlays.pop()
    }

    /// Closes one overlay level: the path, the focus scope it trapped (when it
    /// is the top scope) and restores focus to the opener.
    ///
    /// Returns whether an overlay was closed.
    pub fn dismiss_overlay_level<Msg>(&mut self, tree: &Tree<Msg>) -> bool {
        let Some(path) = self.overlays.pop() else {
            return false;
        };
        let opener = self.overlay_openers.pop().flatten();
        let root = tree
            .nodes
            .iter()
            .find(|node| node.path_hash == path)
            .map(|node| node.id);
        if root.is_some() && self.focus_scopes.last().map(|scope| scope.root) == root {
            self.focus_scopes.pop();
        }
        if opener.is_some() {
            self.rebind_focus(tree, opener);
        }
        true
    }

    /// Closes the whole overlay chain (an outside click).
    pub fn dismiss_overlay_chain<Msg>(&mut self, tree: &Tree<Msg>) -> bool {
        let mut closed = false;
        while self.dismiss_overlay_level(tree) {
            closed = true;
        }
        closed
    }

    /// Closes the dismissable overlays (menus, selects, popovers) when the app
    /// loses activation, leaving modal dialogs open.
    ///
    /// A dialog pushes a trapping focus scope rooted at its overlay node; the
    /// walk stops there. Returns whether anything closed.
    pub fn dismiss_dismissable_overlays<Msg>(&mut self, tree: &Tree<Msg>) -> bool {
        let mut closed = false;
        while let Some(&path) = self.overlays.last() {
            let modal = self.focus_scopes.last().is_some_and(|scope| {
                scope.trap
                    && tree
                        .nodes
                        .iter()
                        .any(|node| node.path_hash == path && node.id == scope.root)
            });
            if modal || !self.dismiss_overlay_level(tree) {
                break;
            }
            closed = true;
        }
        closed
    }

    /// Whether `hit` is inside the topmost overlay's subtree; with no overlay
    /// open every hit counts as inside.
    #[must_use]
    pub fn pointer_in_top_overlay<Msg>(&self, tree: &Tree<Msg>, hit: Option<NodeId>) -> bool {
        let Some(path) = self.overlays.last() else {
            return true;
        };
        let Some(root) = tree
            .nodes
            .iter()
            .find(|node| node.path_hash == *path)
            .map(|node| node.id)
        else {
            return false;
        };
        let Some(hit) = hit else {
            return false;
        };
        let mut current = Some(hit);
        while let Some(candidate) = current {
            if candidate == root {
                return true;
            }
            current = tree.node(candidate).and_then(|node| node.parent);
        }
        false
    }

    /// Whether an overlay is open.
    #[must_use]
    pub fn overlay_open(&self) -> bool {
        !self.overlays.is_empty()
    }

    /// Whether the pointer has stayed over the hovered node for `delay_ms`.
    #[must_use]
    pub fn hover_elapsed_ms(&self) -> f64 {
        self.now_ms - self.hover_since_ms
    }

    /// The cursor the hovered node asks for.
    #[must_use]
    pub fn cursor<Msg>(&self, tree: &Tree<Msg>) -> Cursor {
        self.hovered
            .and_then(|id| tree.node(id))
            .map_or(Cursor::Default, |node| node.style.cursor)
    }

    /// Whether the overlay scrollbar of `id` should be visible right now.
    ///
    /// A scrollbar is active while the pointer is over the container or one of
    /// its descendants, or for 600 ms after a wheel last changed this
    /// container's scroll target or offset.
    #[must_use]
    pub fn scrollbar_active<Msg>(&self, tree: &Tree<Msg>, id: NodeId, path: u64) -> bool {
        if self.scroll_target == Some(path) && self.now_ms - self.last_scroll_ms < 600.0 {
            return true;
        }
        let mut current = self.hovered;
        while let Some(node_id) = current {
            if node_id == id {
                return true;
            }
            current = tree.node(node_id).and_then(|node| node.parent);
        }
        false
    }

    /// Re-resolves the focused node id after a tree rebuild (one scan, only
    /// when the structure changed).
    pub fn rebind_focus<Msg>(&mut self, tree: &Tree<Msg>, path: Option<u64>) {
        self.focused = path.and_then(|path| {
            tree.nodes
                .iter()
                .find(|node| node.path_hash == path)
                .map(|node| node.id)
        });
    }

    /// Forgets the hover, press and capture after a layout change.
    pub fn invalidate(&mut self) {
        self.hovered = None;
        self.pressed = None;
        self.captured = None;
    }
}

/// Everything the router touches, borrowed from [`crate::frame::Ui`].
pub struct RouteContext<'a, Msg> {
    /// The built tree.
    pub tree: &'a Tree<Msg>,
    /// The spatial index.
    pub index: &'a mut HitIndex,
    /// The laid-out geometry (for scroll bounds and resolved axes).
    pub layout: &'a LayoutEngine,
    /// The keyed store.
    pub store: &'a mut Store,
    /// Persistent input state.
    pub input: &'a mut InputState,
    /// The focus order.
    pub focus: &'a mut FocusOrder,
    /// The shortcut registry.
    pub shortcuts: &'a ShortcutRegistry<Msg>,
    /// This frame's message buffer.
    pub messages: &'a mut Vec<Msg>,
}

/// The interactive node under `point`: the spatial index's topmost hit with
/// disabled nodes filtered out.
///
/// This is the single hit-test decision. Both the input router
/// ([`RouteContext::pick`]) and `Ui::pointer_move_changes` call it, so a host
/// that asks whether a move changes anything and the router that delivers it
/// can never disagree about the target (prompt 15 Phase 9D finding 2).
///
/// The last-hit cache is keyed by the layout generation (the index rebuild
/// count), not by interaction state: a hover change must not invalidate a hit
/// that is still geometrically valid.
pub(crate) fn pick_interactive<Msg>(
    index: &mut HitIndex,
    tree: &Tree<Msg>,
    point: Vec2,
) -> Option<NodeId> {
    let generation = index.generation();
    index.set_generation(generation);
    index
        .pick(point, generation)
        .filter(|id| !tree.node(*id).is_some_and(|node| node.style.disabled))
}

impl<Msg: Clone> RouteContext<'_, Msg> {
    fn pick(&mut self, position: Vec2) -> Option<NodeId> {
        pick_interactive(self.index, self.tree, position)
    }

    fn set_hovered(&mut self, hit: Option<NodeId>) {
        if hit == self.input.hovered {
            return;
        }
        if let Some(previous) = self.input.hovered.take() {
            if let Some(node) = self.tree.node(previous) {
                self.store.set_hovered(node.path_hash, false);
                self.store
                    .mark_repaint(node.path_hash, RepaintReason::HoverChanged);
            }
        }
        if let Some(next) = hit {
            if let Some(node) = self.tree.node(next) {
                self.store.set_hovered(node.path_hash, true);
            }
        }
        self.input.hovered = hit;
        self.input.hover_since_ms = self.input.now_ms;
        self.input.gesture.reset_clicks();
    }

    fn focus(&mut self, id: Option<NodeId>) {
        let path = id
            .and_then(|id| self.tree.node(id))
            .map(|node| node.path_hash);
        if let Some(previous) = self.store.focus() {
            if Some(previous) == path {
                self.input.focused = id;
                return;
            }
            self.store
                .mark_repaint(previous, RepaintReason::FocusChanged);
        }
        self.store.set_focus(path);
        self.input.focused = id;
        self.input.gesture.reset_clicks();
    }

    fn scroll_at(&mut self, position: Vec2, delta: Vec2, kind: ScrollKind) -> bool {
        let Some(hit) = self.pick(position) else {
            return false;
        };
        let mut current = Some(hit);
        let mut target = None;
        while let Some(id) = current {
            let axes = self
                .layout
                .resolved(id)
                .map_or(ScrollAxes::None, |style| style.scroll);
            if axes.clips() {
                target = Some((id, axes));
                break;
            }
            current = self.tree.node(id).and_then(|node| node.parent);
        }
        let Some((id, axes)) = target else {
            return false;
        };
        let Some(node) = self.tree.node(id) else {
            return false;
        };
        let path = node.path_hash;
        if self.input.scroll_target != Some(path) {
            self.input.scroll_target = Some(path);
            self.input.scroll.stop();
            self.input.scroll.set_offset(self.store.scroll(path));
        }
        let rect = self.layout.rect(id);
        let content = self.layout.content_size(id);
        self.input.scroll.set_bounds(
            Vec2::new(rect.width, rect.height),
            Vec2::new(content.width, content.height),
        );
        let delta = Vec2::new(
            if matches!(axes, ScrollAxes::Horizontal | ScrollAxes::Both) {
                delta.x
            } else {
                0.0
            },
            if matches!(axes, ScrollAxes::Vertical | ScrollAxes::Both) {
                delta.y
            } else {
                0.0
            },
        );
        let moved = self
            .input
            .scroll
            .wheel(delta, kind, self.input.modifiers.shift);
        if moved {
            self.input.last_scroll_ms = self.input.now_ms;
            self.store.set_scroll(path, self.input.scroll.offset());
        }
        moved
    }

    fn activate(&mut self, id: NodeId) {
        if let Some(node) = self.tree.node(id) {
            if let Some(action) = &node.action {
                self.messages.push(action.clone());
            }
        }
    }
}

/// Routes one raw event through the tree.
#[allow(
    clippy::too_many_lines,
    clippy::needless_pass_by_value,
    clippy::match_same_arms,
    clippy::collapsible_if
)] // reason: one match arm per raw event keeps the routing table readable
pub fn route<Msg: Clone>(context: &mut RouteContext<'_, Msg>, event: InputEvent) {
    match event {
        InputEvent::PointerMoved { position } => {
            context.input.pointer = position;
            context.input.pointer_seen = true;
            context.input.pointer_moved_at = context.input.now_ms;
            if let Some(gesture) = context
                .input
                .gesture
                .pointer_moved(position, context.input.now_ms)
            {
                context.input.last_gesture = Some(gesture);
                if let Gesture::DragMove { delta, total, .. } = gesture {
                    context.input.drag_total = total;
                    let _ = delta;
                }
            }
            let hit = context.pick(position);
            context.set_hovered(hit);
        }
        InputEvent::PointerDown { position, button } => {
            context.input.keyboard_focus = false;
            context.input.pointer = position;
            context.input.pointer_seen = true;
            let hit = context.pick(position);
            // A press outside the topmost overlay closes the whole chain; the
            // press is swallowed so a menu never leaks a click to the control
            // underneath it.
            if context.input.overlay_open()
                && !context.input.pointer_in_top_overlay(context.tree, hit)
            {
                context.input.dismiss_overlay_chain(context.tree);
                context.index.invalidate();
                context.input.pressed = None;
                context.input.captured = None;
                context.input.drag_button = None;
                return;
            }
            context.input.pressed = hit;
            context.input.captured = hit;
            context.input.drag_button = Some(button);
            context.input.dragging = false;
            context.input.drag_total = Vec2::ZERO;
            context
                .input
                .gesture
                .pointer_down(position, button, context.input.now_ms);
            if let Some(id) = hit {
                if let Some(node) = context.tree.node(id) {
                    context.store.set_pressed(node.path_hash, true);
                    context.focus(Some(id));
                }
            } else {
                context.focus(None);
            }
        }
        InputEvent::PointerUp { position, button } => {
            context.input.pointer = position;
            context.input.scroll.stop();
            let released = context.input.captured.take();
            let pressed = context.input.pressed.take();
            context.input.drag_button = None;
            if let Some(id) = pressed {
                if let Some(node) = context.tree.node(id) {
                    context.store.set_pressed(node.path_hash, false);
                }
            }
            let gesture = context
                .input
                .gesture
                .pointer_up(position, button, context.input.now_ms);
            context.input.last_gesture = gesture;
            match gesture {
                Some(Gesture::Click { count, .. }) => {
                    context.input.click_count = count;
                    context.input.dragging = false;
                    if let Some(id) = released {
                        if context.pick(position) == Some(id) {
                            context.activate(id);
                        }
                    }
                }
                Some(Gesture::DragEnd { .. }) => {
                    context.input.dragging = false;
                }
                _ => {}
            }
        }
        InputEvent::PointerLeft => {
            context.set_hovered(None);
            context.input.pointer_seen = false;
            context.input.gesture.pointer_left();
            context.input.dragging = false;
            if let Some(pressed) = context.input.pressed.take() {
                if let Some(node) = context.tree.node(pressed) {
                    context.store.set_pressed(node.path_hash, false);
                }
            }
            context.input.captured = None;
        }
        InputEvent::FocusLost => {
            context.set_hovered(None);
            if let Some(pressed) = context.input.pressed.take() {
                if let Some(node) = context.tree.node(pressed) {
                    context.store.set_pressed(node.path_hash, false);
                }
            }
            context.input.captured = None;
            context.input.dragging = false;
            context.focus(None);
            // Switching to another application dismisses menus, selects and
            // popovers; a modal dialog stays open.
            if context.input.dismiss_dismissable_overlays(context.tree) {
                context.index.invalidate();
            }
        }
        InputEvent::Wheel {
            position,
            delta,
            kind,
        } => {
            context.input.pointer = position;
            context.scroll_at(position, delta, kind);
        }
        InputEvent::KeyDown { key, repeat } => {
            context.input.keyboard_focus = true;
            let chord = chord_for(key, context.input.modifiers);
            let focused = context.store.focus().is_some();
            let overlay = context.input.overlay_open();
            if let Some(message) = context.shortcuts.lookup(chord, focused, overlay) {
                context.messages.push(message.clone());
                return;
            }
            if repeat {
                return;
            }
            match key {
                ShortcutKey::Tab => {
                    let forward = !context.input.modifiers.shift;
                    if let Some(next) = context.focus.step(context.input.focused, forward) {
                        context.focus(Some(next));
                    }
                }
                ShortcutKey::Enter | ShortcutKey::Space => {
                    if let Some(id) = context.input.focused {
                        context.activate(id);
                    }
                }
                ShortcutKey::Escape => {
                    if context.input.overlay_open() {
                        context.input.dismiss_overlay_level(context.tree);
                        context.index.invalidate();
                    } else {
                        context.focus(None);
                    }
                }
                _ => {}
            }
        }
        InputEvent::KeyUp { .. } => {}
        InputEvent::Text(_) => {
            // The host routes text to the focused field's `TextEditor`.
        }
        InputEvent::Modifiers(modifiers) => {
            context.input.modifiers = modifiers;
        }
    }
}

fn chord_for(key: ShortcutKey, modifiers: Modifiers) -> Chord {
    Chord {
        key,
        control: modifiers.control || modifiers.super_key,
        shift: modifiers.shift,
        alt: modifiers.alt,
        super_key: false,
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::float_cmp)] // reason: exact token arithmetic in tests
    use glam::Vec2;

    use super::{route, InputEvent, InputState, Modifiers, PointerButton, RouteContext};
    use crate::input::focus::FocusOrder;
    use crate::input::focus::FocusScope;
    use crate::input::scroll::ScrollKind;
    use crate::input::shortcut::{Chord, Scope, ShortcutKey, ShortcutRegistry};
    use crate::layout::LayoutEngine;
    use crate::prelude::*;
    use crate::spatial::HitIndex;
    use crate::store::Store;
    use crate::theme::ColorRole;
    use crate::view::NodeId;

    #[derive(Clone, Debug, PartialEq)]
    enum Msg {
        Clicked(&'static str),
        Save,
    }

    struct Harness {
        tree: Tree<Msg>,
        index: HitIndex,
        layout: LayoutEngine,
        store: Store,
        input: InputState,
        focus: FocusOrder,
        shortcuts: ShortcutRegistry<Msg>,
        messages: Vec<Msg>,
    }

    impl Harness {
        fn new(view: &Element<Msg>) -> Self {
            let tree = Tree::build(view);
            let mut layout = LayoutEngine::new();
            layout.set_viewport(Size::new(400.0, 300.0), 1.0);
            let mut text = crate::text::MetricsTextSystem::new();
            let store = Store::new();
            let mut harness = Self {
                tree,
                index: HitIndex::new(),
                layout,
                store,
                input: InputState::new(),
                focus: FocusOrder::new(),
                shortcuts: ShortcutRegistry::new(crate::input::shortcut::Platform::Windows),
                messages: Vec::new(),
            };
            harness.relayout(&mut text);
            harness
        }

        fn relayout(&mut self, text: &mut crate::text::MetricsTextSystem) {
            self.layout
                .run(&self.tree, &Theme::dark(), text, &self.store);
            self.index.rebuild(&self.tree, self.layout.rects());
            self.focus.rebuild(&self.tree, self.input.focus_scope());
            for node in &self.tree.nodes {
                self.store
                    .set_rect(node.path_hash, self.layout.rect(node.id));
            }
        }

        fn send(&mut self, event: InputEvent) {
            let mut context = RouteContext {
                tree: &self.tree,
                index: &mut self.index,
                layout: &self.layout,
                store: &mut self.store,
                input: &mut self.input,
                focus: &mut self.focus,
                shortcuts: &self.shortcuts,
                messages: &mut self.messages,
            };
            route(&mut context, event);
        }

        fn tick(&mut self, now_ms: f64) {
            let dt = self.input.set_time_ms(now_ms);
            if self.input.scroll.advance((dt / 1000.0) as f32) {
                if let Some(path) = self.input.scroll_target {
                    self.store.set_scroll(path, self.input.scroll.offset());
                }
            }
        }
    }

    fn view() -> Element<Msg> {
        Element::<Msg>::column()
            .key("root")
            .width(SizeSpec::Fill)
            .height(SizeSpec::Fill)
            .child(
                Element::row()
                    .key("a")
                    .width(SizeSpec::Fill)
                    .height(SizeSpec::Token(Spacing::S8))
                    .bg(ColorRole::AccentFill)
                    .on_click(Msg::Clicked("a")),
            )
            .child(
                Element::row()
                    .key("b")
                    .width(SizeSpec::Fill)
                    .height(SizeSpec::Token(Spacing::S8))
                    .bg(ColorRole::Surface)
                    .on_click(Msg::Clicked("b")),
            )
    }

    fn center(harness: &Harness, key: &str) -> Vec2 {
        let id = harness.tree.find_key(key).expect("key");
        let rect = harness.layout.rect(id);
        Vec2::new(rect.x + rect.width / 2.0, rect.y + rect.height / 2.0)
    }

    #[test]
    fn click_emits_the_action_and_counts_double_clicks() {
        let mut harness = Harness::new(&view());
        let point = center(&harness, "a");
        harness.send(InputEvent::PointerDown {
            position: point,
            button: PointerButton::Primary,
        });
        harness.send(InputEvent::PointerUp {
            position: point,
            button: PointerButton::Primary,
        });
        assert_eq!(harness.messages, vec![Msg::Clicked("a")]);
        assert_eq!(harness.input.click_count, 1);
        harness.tick(100.0);
        harness.send(InputEvent::PointerDown {
            position: point,
            button: PointerButton::Primary,
        });
        harness.send(InputEvent::PointerUp {
            position: point,
            button: PointerButton::Primary,
        });
        assert_eq!(harness.input.click_count, 2);
        assert_eq!(harness.messages.len(), 2);
    }

    #[test]
    fn hover_moves_only_when_the_target_changes() {
        let mut harness = Harness::new(&view());
        let a = center(&harness, "a");
        harness.send(InputEvent::PointerMoved { position: a });
        let first = harness.input.hovered;
        assert!(first.is_some());
        let queries = harness.index.queries();
        harness.send(InputEvent::PointerMoved {
            position: a + Vec2::new(2.0, 1.0),
        });
        assert_eq!(harness.input.hovered, first);
        assert_eq!(harness.index.queries(), queries);
        let b = center(&harness, "b");
        harness.send(InputEvent::PointerMoved { position: b });
        assert_ne!(harness.input.hovered, first);
    }

    #[test]
    fn drag_does_not_click() {
        let mut harness = Harness::new(&view());
        let point = center(&harness, "a");
        harness.send(InputEvent::PointerDown {
            position: point,
            button: PointerButton::Primary,
        });
        harness.send(InputEvent::PointerMoved {
            position: point + Vec2::new(40.0, 0.0),
        });
        assert!(harness.input.dragging || harness.input.gesture.is_dragging());
        harness.send(InputEvent::PointerUp {
            position: point + Vec2::new(40.0, 0.0),
            button: PointerButton::Primary,
        });
        assert!(harness.messages.is_empty());
    }

    #[test]
    fn tab_walks_the_focus_order_and_shift_tab_reverses() {
        let mut harness = Harness::new(&view());
        harness.send(InputEvent::KeyDown {
            key: ShortcutKey::Tab,
            repeat: false,
        });
        let first = harness.store.focus();
        assert!(first.is_some());
        harness.send(InputEvent::KeyDown {
            key: ShortcutKey::Tab,
            repeat: false,
        });
        let second = harness.store.focus();
        assert_ne!(first, second);
        harness.send(InputEvent::Modifiers(Modifiers {
            shift: true,
            ..Modifiers::none()
        }));
        harness.send(InputEvent::KeyDown {
            key: ShortcutKey::Tab,
            repeat: false,
        });
        assert_eq!(harness.store.focus(), first);
    }

    #[test]
    fn enter_activates_the_focused_node() {
        let mut harness = Harness::new(&view());
        harness.send(InputEvent::KeyDown {
            key: ShortcutKey::Tab,
            repeat: false,
        });
        harness.send(InputEvent::KeyDown {
            key: ShortcutKey::Enter,
            repeat: false,
        });
        assert!(matches!(harness.messages.first(), Some(Msg::Clicked(_))));
    }

    #[test]
    fn shortcuts_fire_in_their_scopes() {
        let mut harness = Harness::new(&view());
        harness.shortcuts.bind(
            Chord::new(ShortcutKey::Letter(b'S')).control(),
            Scope::Global,
            Msg::Save,
        );
        harness.send(InputEvent::Modifiers(Modifiers {
            control: true,
            ..Modifiers::none()
        }));
        harness.send(InputEvent::KeyDown {
            key: ShortcutKey::Letter(b'S'),
            repeat: false,
        });
        assert_eq!(harness.messages, vec![Msg::Save]);
    }

    #[test]
    fn wheel_scrolls_the_container_and_momentum_advances() {
        let view: Element<Msg> = Element::<Msg>::column()
            .width(SizeSpec::Fill)
            .height(SizeSpec::Fill)
            .child(
                Element::scroll(ScrollAxes::Vertical)
                    .key("scroll")
                    .width(SizeSpec::Fill)
                    .height(SizeSpec::Fill)
                    .child(Element::column().height(SizeSpec::Px(2000.0)).shrink(0.0)),
            );
        let mut harness = Harness::new(&view);
        let scroll_id = harness.tree.find_key("scroll").expect("scroll");
        assert!(harness.index.nodes.iter().any(|(id, _)| *id == scroll_id));
        assert_eq!(
            harness.layout.resolved(scroll_id).map(|style| style.scroll),
            Some(ScrollAxes::Vertical)
        );
        assert_eq!(harness.layout.content_size(scroll_id).height, 2000.0);
        assert!(harness.layout.rect(scroll_id).height > 0.0);
        let point = Vec2::new(50.0, 50.0);
        harness.send(InputEvent::Wheel {
            position: point,
            delta: Vec2::new(0.0, -3.0),
            kind: ScrollKind::Line,
        });
        let path = harness
            .tree
            .find_key("scroll")
            .map(|id| harness.tree.node(id).unwrap().path_hash);
        let offset = harness.store.scroll(path.unwrap());
        assert!(offset.y > 0.0);
        harness.send(InputEvent::Wheel {
            position: point,
            delta: Vec2::new(0.0, -3.0),
            kind: ScrollKind::Pixel,
        });
        let before = harness.store.scroll(path.unwrap()).y;
        harness.tick(16.0);
        assert!(harness.store.scroll(path.unwrap()).y >= before);
    }

    fn overlay_view() -> Element<Msg> {
        Element::<Msg>::stack()
            .key("root")
            .width(SizeSpec::Fill)
            .height(SizeSpec::Fill)
            .child(
                Element::row()
                    .key("opener")
                    .width(SizeSpec::Fill)
                    .height(SizeSpec::Fill)
                    .bg(ColorRole::AccentFill)
                    .on_click(Msg::Clicked("a")),
            )
            .child(
                Element::column()
                    .key("popup")
                    .width(SizeSpec::Token(Spacing::S12))
                    .height(SizeSpec::Token(Spacing::S12))
                    .bg(ColorRole::SurfaceRaised)
                    .on_click(Msg::Clicked("b")),
            )
    }

    fn seed(harness: &mut Harness) -> (NodeId, NodeId) {
        let opener = harness.tree.find_key("opener").expect("opener");
        let popup = harness.tree.find_key("popup").expect("popup");
        let opener_path = harness.tree.node(opener).expect("node").path_hash;
        let popup_path = harness.tree.node(popup).expect("node").path_hash;
        harness.store.set_focus(Some(opener_path));
        harness.input.focused = Some(opener);
        harness.input.push_focus_scope(FocusScope {
            root: popup,
            trap: true,
        });
        harness
            .input
            .push_overlay_from(popup_path, Some(opener_path));
        (opener, popup)
    }

    #[test]
    fn escape_closes_one_level_and_returns_the_focus() {
        let mut harness = Harness::new(&overlay_view());
        let (opener, _popup) = seed(&mut harness);
        harness.send(InputEvent::KeyDown {
            key: ShortcutKey::Escape,
            repeat: false,
        });

        assert!(!harness.input.overlay_open(), "Escape closes one level");
        assert!(harness.input.focus_scopes.is_empty());
        assert_eq!(
            harness.store.focus(),
            harness.tree.node(opener).map(|node| node.path_hash),
            "focus returns to the opener"
        );
    }

    #[test]
    fn a_click_outside_closes_the_chain_without_activating() {
        let mut harness = Harness::new(&overlay_view());
        let (opener, popup) = seed(&mut harness);
        let popup_path = harness.tree.node(popup).expect("node").path_hash;

        harness.send(InputEvent::PointerDown {
            position: Vec2::new(200.0, 200.0),
            button: PointerButton::Primary,
        });
        assert!(
            !harness.input.overlay_open(),
            "outside click closes the chain"
        );
        assert!(harness.input.focus_scopes.is_empty());
        assert_eq!(
            harness.store.focus(),
            harness.tree.node(opener).map(|node| node.path_hash)
        );
        assert!(harness.messages.is_empty(), "the press is swallowed");

        // A click inside the popup is routed normally.
        harness.input.push_overlay(popup_path);
        harness.send(InputEvent::PointerDown {
            position: Vec2::new(4.0, 4.0),
            button: PointerButton::Primary,
        });
        harness.send(InputEvent::PointerUp {
            position: Vec2::new(4.0, 4.0),
            button: PointerButton::Primary,
        });
        assert!(harness.input.overlay_open(), "a click inside keeps it open");
        assert_eq!(harness.messages, vec![Msg::Clicked("b")]);
    }
}
