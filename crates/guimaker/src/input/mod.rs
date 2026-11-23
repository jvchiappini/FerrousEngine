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
        self.overlays.push(path);
    }

    /// Pops the topmost overlay.
    pub fn pop_overlay(&mut self) -> Option<u64> {
        self.overlays.pop()
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

impl<Msg: Clone> RouteContext<'_, Msg> {
    fn pick(&mut self, position: Vec2) -> Option<NodeId> {
        // The last-hit cache is keyed by the layout generation (the index
        // rebuild count), not by interaction state: a hover change must not
        // invalidate a hit that is still geometrically valid.
        let generation = self.index.rebuilds();
        self.index.set_generation(generation);
        self.index
            .pick(position, generation)
            .filter(|id| !self.is_disabled(*id))
    }

    fn is_disabled(&self, id: NodeId) -> bool {
        self.tree.node(id).is_some_and(|node| node.style.disabled)
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
                        context.input.pop_overlay();
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
    use crate::input::scroll::ScrollKind;
    use crate::input::shortcut::{Chord, Scope, ShortcutKey, ShortcutRegistry};
    use crate::layout::LayoutEngine;
    use crate::prelude::*;
    use crate::spatial::HitIndex;
    use crate::store::Store;

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
}
