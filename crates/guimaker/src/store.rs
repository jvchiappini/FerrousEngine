//! Persistent node state keyed by a stable path hash.
//!
//! Hover, press, focus and scroll offsets must survive a view rebuild (and a
//! list reorder must not transfer a hover to a different row), so they live
//! here, keyed by the node's `path_hash`, not in the element tree. The store is
//! also where a component reads its interaction state when it builds itself.

use std::collections::HashMap;

use glam::Vec2;

use crate::geom::Rect;

/// Why a node will be painted this frame.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum RepaintReason {
    /// Nothing changed; the cached commands are replayed.
    #[default]
    None,
    /// The first frame.
    FirstFrame,
    /// The view content (style or text) changed.
    ViewChanged,
    /// The node's rectangle changed.
    LayoutChanged,
    /// The pointer entered or left the node.
    HoverChanged,
    /// The node was pressed or released.
    PressChanged,
    /// The node gained or lost focus.
    FocusChanged,
    /// A scroll container moved.
    Scrolled,
}

impl RepaintReason {
    /// Whether this reason needs the node's commands regenerated.
    #[must_use]
    pub const fn repaints(self) -> bool {
        !matches!(self, Self::None)
    }

    /// A stable name for debug output.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::FirstFrame => "first_frame",
            Self::ViewChanged => "view_changed",
            Self::LayoutChanged => "layout_changed",
            Self::HoverChanged => "hover_changed",
            Self::PressChanged => "press_changed",
            Self::FocusChanged => "focus_changed",
            Self::Scrolled => "scrolled",
        }
    }
}

/// One node's persistent interaction state.
#[allow(clippy::struct_excessive_bools)] // reason: hover, press, focus and disabled are independent state
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct NodeState {
    /// Whether the pointer is over the node.
    pub hovered: bool,
    /// Whether the node is held down.
    pub pressed: bool,
    /// Whether the node has keyboard focus.
    pub focused: bool,
    /// Scroll offset of a scroll container.
    pub scroll: Vec2,
    /// The last laid-out rectangle.
    pub rect: Rect,
    /// Whether the node is disabled.
    pub disabled: bool,
    /// Why the node repainted this frame.
    pub repaint: RepaintReason,
}

/// The keyed state store.
#[derive(Clone, Debug, Default)]
pub struct Store {
    states: HashMap<u64, NodeState>,
    focus: Option<u64>,
    generation: u64,
    layout_generation: u64,
    frame: u64,
}

impl Store {
    /// Builds an empty store.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// The state for a path, if any.
    #[must_use]
    pub fn get(&self, path: u64) -> Option<&NodeState> {
        self.states.get(&path)
    }

    /// The state for a path, creating the default on demand.
    pub fn entry(&mut self, path: u64) -> &mut NodeState {
        self.generation = self.generation.wrapping_add(1);
        self.states.entry(path).or_default()
    }

    /// The scroll offset of a scroll container.
    #[must_use]
    pub fn scroll(&self, path: u64) -> Vec2 {
        self.states
            .get(&path)
            .map_or(Vec2::ZERO, |state| state.scroll)
    }

    /// Sets a scroll offset and returns whether it changed.
    pub fn set_scroll(&mut self, path: u64, offset: Vec2) -> bool {
        let state = self.states.entry(path).or_default();
        if state.scroll == offset {
            return false;
        }
        state.scroll = offset;
        state.repaint = RepaintReason::Scrolled;
        self.generation = self.generation.wrapping_add(1);
        self.layout_generation = self.layout_generation.wrapping_add(1);
        true
    }

    /// The focused path.
    #[must_use]
    pub fn focus(&self) -> Option<u64> {
        self.focus
    }

    /// Moves focus, clearing the previous node's flag.
    pub fn set_focus(&mut self, path: Option<u64>) {
        if self.focus == path {
            return;
        }
        if let Some(previous) = self.focus {
            if let Some(state) = self.states.get_mut(&previous) {
                state.focused = false;
                state.repaint = RepaintReason::FocusChanged;
            }
        }
        if let Some(next) = path {
            let state = self.states.entry(next).or_default();
            state.focused = true;
            state.repaint = RepaintReason::FocusChanged;
        }
        self.focus = path;
        self.generation = self.generation.wrapping_add(1);
    }

    /// Marks a node as hovered or not; returns whether it changed.
    pub fn set_hovered(&mut self, path: u64, hovered: bool) -> bool {
        let state = self.states.entry(path).or_default();
        if state.hovered == hovered {
            return false;
        }
        state.hovered = hovered;
        state.repaint = RepaintReason::HoverChanged;
        self.generation = self.generation.wrapping_add(1);
        true
    }

    /// Marks a node as pressed or not; returns whether it changed.
    pub fn set_pressed(&mut self, path: u64, pressed: bool) -> bool {
        let state = self.states.entry(path).or_default();
        if state.pressed == pressed {
            return false;
        }
        state.pressed = pressed;
        state.repaint = RepaintReason::PressChanged;
        self.generation = self.generation.wrapping_add(1);
        true
    }

    /// Records the laid-out rectangle and marks a layout change.
    pub fn set_rect(&mut self, path: u64, rect: Rect) -> bool {
        let state = self.states.entry(path).or_default();
        if state.rect == rect {
            return false;
        }
        state.rect = rect;
        state.repaint = RepaintReason::LayoutChanged;
        true
    }

    /// Sets the repaint reason, keeping a stronger reason from the same frame.
    pub fn mark_repaint(&mut self, path: u64, reason: RepaintReason) {
        let state = self.states.entry(path).or_default();
        if state.repaint == RepaintReason::None {
            state.repaint = reason;
        }
    }

    /// The repaint reason of a path.
    #[must_use]
    pub fn repaint(&self, path: u64) -> RepaintReason {
        self.states
            .get(&path)
            .map_or(RepaintReason::None, |state| state.repaint)
    }

    /// Clears the per-frame repaint reasons at the start of a frame.
    pub fn begin_frame(&mut self) {
        for state in self.states.values_mut() {
            state.repaint = RepaintReason::None;
        }
    }

    /// Iterates the stored states (debug and damage collection).
    pub fn iter(&self) -> impl Iterator<Item = (&u64, &NodeState)> {
        self.states.iter()
    }

    /// Clears the per-frame repaint reasons, keeping persistent state.
    pub fn end_frame(&mut self) {
        for state in self.states.values_mut() {
            state.repaint = RepaintReason::None;
        }
        self.frame = self.frame.wrapping_add(1);
    }

    /// A generation that changes whenever persistent or interaction state
    /// changes. A frame whose generation is unchanged can reuse its output.
    #[must_use]
    pub fn generation(&self) -> u64 {
        self.generation
    }

    /// A generation that changes only when geometry-affecting state (scroll)
    /// changes. Layout can be skipped while it is unchanged.
    #[must_use]
    pub fn layout_generation(&self) -> u64 {
        self.layout_generation
    }

    /// The frame counter.
    #[must_use]
    pub fn frame(&self) -> u64 {
        self.frame
    }

    /// Removes states whose paths no longer appear in the tree.
    pub fn retain(&mut self, mut keep: impl FnMut(u64) -> bool) {
        self.states.retain(|path, _| keep(*path));
        if self.focus.is_some_and(|focus| !keep(focus)) {
            self.focus = None;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{RepaintReason, Store};
    use crate::geom::Rect;
    use glam::Vec2;

    #[test]
    fn hover_and_press_change_once() {
        let mut store = Store::new();
        assert!(store.set_hovered(1, true));
        assert!(!store.set_hovered(1, true));
        assert!(store.set_pressed(1, true));
        assert_eq!(store.get(1).map(|s| s.hovered), Some(true));
        assert_eq!(
            store.get(1).map(|s| s.repaint),
            Some(RepaintReason::PressChanged)
        );
    }

    #[test]
    fn scroll_and_focus_persist() {
        let mut store = Store::new();
        assert!(store.set_scroll(2, Vec2::new(0.0, 40.0)));
        assert_eq!(store.scroll(2), Vec2::new(0.0, 40.0));
        store.set_focus(Some(2));
        assert_eq!(store.focus(), Some(2));
        assert!(store.get(2).is_some_and(|state| state.focused));
        store.set_focus(None);
        assert_eq!(store.focus(), None);
    }

    #[test]
    fn frames_clear_repaint_reasons() {
        let mut store = Store::new();
        store.set_rect(3, Rect::new(0.0, 0.0, 10.0, 10.0));
        assert_eq!(store.repaint(3), RepaintReason::LayoutChanged);
        store.end_frame();
        assert_eq!(store.repaint(3), RepaintReason::None);
        assert_eq!(store.frame(), 1);
    }
}
