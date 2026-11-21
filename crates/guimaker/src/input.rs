//! Input routing through the laid-out tree.
//!
//! Phase 2 only routes the pointer: hit test, hover, press, pointer capture
//! and the click message. The full gesture state machine, focus order,
//! shortcuts, text editing and scroll controllers arrive in phase 4; they build
//! on the same [`InputState`] and the keyed store.

use glam::Vec2;

use crate::store::Store;
use crate::view::{NodeId, Tree};

/// A pointer button.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PointerButton {
    /// The primary (left) button.
    Primary,
    /// The secondary (right) button.
    Secondary,
    /// The middle button.
    Middle,
}

/// A raw input event the host feeds the library.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum InputEvent {
    /// The pointer moved.
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
}

/// Pointer state that survives between events.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct InputState {
    /// The last pointer position.
    pub pointer: Vec2,
    /// The node currently under the pointer.
    pub hovered: Option<NodeId>,
    /// The node that initiated the press.
    pub pressed: Option<NodeId>,
    /// The node that captured the pointer.
    pub captured: Option<NodeId>,
    /// Whether the pointer has ever been seen.
    pub pointer_seen: bool,
}

impl InputState {
    /// Builds an empty input state.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Forgets the hover and capture after a layout change.
    pub fn invalidate(&mut self) {
        self.hovered = None;
        self.pressed = None;
        self.captured = None;
    }
}

/// Routes one event, pushing any emitted message.
#[allow(clippy::too_many_arguments)] // reason: routing needs the tree, index, layout, store and buffers
pub fn route<Msg: Clone>(
    event: InputEvent,
    tree: &Tree<Msg>,

    index: &mut crate::spatial::HitIndex,
    store: &mut Store,
    input: &mut InputState,
    messages: &mut Vec<Msg>,
) {
    let generation = store.generation();
    index.set_generation(generation);
    match event {
        InputEvent::PointerMoved { position } => {
            input.pointer = position;
            input.pointer_seen = true;
            let hit = index
                .pick(position, generation)
                .filter(|id| !is_disabled(tree, *id));
            if hit != input.hovered {
                if let Some(previous) = input.hovered {
                    if let Some(node) = tree.node(previous) {
                        store.set_hovered(node.path_hash, false);
                        store.mark_repaint(
                            node.path_hash,
                            crate::store::RepaintReason::HoverChanged,
                        );
                    }
                }
                if let Some(next) = hit {
                    if let Some(node) = tree.node(next) {
                        store.set_hovered(node.path_hash, true);
                    }
                }
                input.hovered = hit;
            }
        }
        InputEvent::PointerDown { position, .. } => {
            input.pointer = position;
            input.pointer_seen = true;
            let hit = index
                .pick(position, generation)
                .filter(|id| !is_disabled(tree, *id));
            input.pressed = hit;
            input.captured = hit;
            if let Some(node) = hit.and_then(|id| tree.node(id)) {
                store.set_pressed(node.path_hash, true);
                store.set_focus(Some(node.path_hash));
            }
        }
        InputEvent::PointerUp { position, .. } => {
            input.pointer = position;
            let released = input.captured.take();
            let pressed = input.pressed.take();
            if let Some(node_id) = pressed {
                if let Some(node) = tree.node(node_id) {
                    store.set_pressed(node.path_hash, false);
                }
            }
            let hit = index.pick(position, generation);
            if let Some(node_id) = released {
                if hit == Some(node_id) {
                    if let Some(node) = tree.node(node_id) {
                        if let Some(action) = &node.action {
                            messages.push(action.clone());
                        }
                    }
                }
            }
        }
        InputEvent::PointerLeft => {
            if let Some(previous) = input.hovered.take() {
                if let Some(node) = tree.node(previous) {
                    store.set_hovered(node.path_hash, false);
                }
            }
            input.pointer_seen = false;
        }
        InputEvent::FocusLost => {
            if let Some(pressed) = input.pressed.take() {
                if let Some(node) = tree.node(pressed) {
                    store.set_pressed(node.path_hash, false);
                }
            }
            input.captured = None;
            store.set_focus(None);
        }
    }
}

fn is_disabled<Msg>(tree: &Tree<Msg>, id: NodeId) -> bool {
    tree.node(id).is_some_and(|node| node.style.disabled)
}
