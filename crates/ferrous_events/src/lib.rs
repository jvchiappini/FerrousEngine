//! `ferrous_events` — Event manager and hit-testing for the UI system.
//!
//! Separates interaction logic from the graphics engine and windowing system.
//! Provides 2-level hit-testing:
//!
//! 1. **CPU AABB** — Instantly discards 99% of nodes.
//! 2. **`Widget::hit_test()`** — Allows precise hit-testing for custom shapes.

pub use ferrous_ui_core::{
    NodeId, UiTree, UiEvent, Rect, EventResponse, EventContext, GuiKey, MouseButton
};
use glam::Vec2;

/// Manages focus and hover states for the event system.
pub struct EventManager {
    /// Node currently under the cursor. Used for visual effects and MouseMove routing.
    pub hovered_node: Option<NodeId>,
    /// Node with keyboard focus. All KeyDown events are directed here first.
    pub focused_node: Option<NodeId>,
    /// Node that initiated a MouseDown (ensures MouseUp goes to the same node).
    pub pressed_node: Option<NodeId>,
    /// Current cursor position (updated on every MouseMove).
    pub cursor_pos: Vec2,
    /// Result from GPU hit-test (if performed this frame).
    pub gpu_hit_test_result: Option<NodeId>,
    
    // --- Native Drag & Drop State ---
    pub dragging_data: Option<String>,
    pub dragging_source: Option<NodeId>,
    pub last_drag_target: Option<NodeId>,
}


impl EventManager {
    pub fn new() -> Self {
        Self {
            hovered_node: None,
            focused_node: None,
            pressed_node: None,
            cursor_pos: Vec2::ZERO,
            gpu_hit_test_result: None,
            dragging_data: None,
            dragging_source: None,
            last_drag_target: None,
        }
    }


    // ─── Hit-testing ──────────────────────────────────────────────────────────

    /// 2-level hit-test: Fast AABB + precise `Widget::hit_test()`.
    ///
    /// Iterates through the tree front-to-back (last child = highest visual priority).
    pub fn hit_test<App>(&self, tree: &UiTree<App>, pos: Vec2) -> Option<NodeId> {
        let candidates = tree.spatial_index.bounding_candidates_at(pos);
        
        for candidate_id in candidates {
            let node = match tree.get_node(candidate_id) {
                Some(n) => n,
                None => continue,
            };

            // For widgets with `needs_gpu_hit_test() == true`, GPU will handle it
            if node.widget.needs_gpu_hit_test() {
                if let Some(gpu_id) = self.gpu_hit_test_result {
                    if gpu_id == candidate_id {
                        return Some(candidate_id);
                    }
                }
                continue;
            }

            let rect = node.rect;
            let local_pos = Vec2::new(pos.x - rect.x, pos.y - rect.y);
            let size = Vec2::new(rect.width, rect.height);
            
            if node.widget.hit_test(local_pos, size) {
                return Some(candidate_id);
            }
        }
        None
    }

    // ─── Dispatch ─────────────────────────────────────────────────────────────

    pub fn dispatch_event<App>(
        &mut self,
        tree: &mut UiTree<App>,
        app: &mut App,
        event: UiEvent,
    ) {
        match &event {
            UiEvent::MouseMove { pos } => {
                let pos = *pos;
                self.cursor_pos = pos;
                let target = self.hit_test(tree, pos);

                // --- Handle Dragging State ---
                if let Some(data) = self.dragging_data.clone() {
                    if target != self.last_drag_target {
                        if let Some(old_id) = self.last_drag_target {
                            self.send_to_node(tree, app, old_id, UiEvent::DragLeave, None);
                        }
                        self.last_drag_target = target;
                    }

                    if let Some(id) = target {
                        // Current DragOver
                        self.bubble_event(tree, app, id, UiEvent::DragOver { pos, data }, Some(pos));
                    }
                } else {
                    // --- Normal Hover Logic ---
                    if target != self.hovered_node {
                        if let Some(old_id) = self.hovered_node {
                            self.send_to_node(tree, app, old_id, UiEvent::MouseLeave, None);
                        }
                        if let Some(new_id) = target {
                            self.send_to_node(tree, app, new_id, UiEvent::MouseEnter, Some(pos));
                        }
                        self.hovered_node = target;
                    }

                    if let Some(id) = self.hovered_node {
                        self.bubble_event(tree, app, id, UiEvent::MouseMove { pos }, Some(pos));
                    }
                }
            }

            UiEvent::MouseDown { button, pos } => {
                let pos = *pos;
                let button = *button;
                self.cursor_pos = pos;
                let target = self.hit_test(tree, pos);
                if let Some(id) = target {
                    self.focused_node = Some(id);
                    self.pressed_node = Some(id);
                    self.bubble_event(tree, app, id, UiEvent::MouseDown { button, pos }, Some(pos));
                } else {
                    self.focused_node = None;
                    self.pressed_node = None;
                }
            }

            UiEvent::MouseUp { button, pos } => {
                let pos = *pos;
                let button = *button;
                self.cursor_pos = pos;

                // --- Handle Drop ---
                if let Some(data) = self.dragging_data.clone() {
                    let target = self.hit_test(tree, pos);
                    if let Some(id) = target {
                        self.bubble_event(tree, app, id, UiEvent::Drop { pos, data }, Some(pos));
                    }
                    self.dragging_data = None;
                    self.dragging_source = None;
                    self.last_drag_target = None;
                    // Note: We consume the MouseUp here to prevent normal click logic if it was a drag
                } else {
                    let target = self.pressed_node
                        .or_else(|| self.hit_test(tree, pos));

                    if let Some(id) = target {
                        self.bubble_event(tree, app, id, UiEvent::MouseUp { button, pos }, Some(pos));
                    }
                }
                self.pressed_node = None;
            }


            UiEvent::KeyDown { key } => {
                let key = *key;
                if let Some(id) = self.focused_node {
                    self.bubble_event(tree, app, id, UiEvent::KeyDown { key }, None);
                }
            }

            UiEvent::KeyUp { key } => {
                let key = *key;
                if let Some(id) = self.focused_node {
                    self.bubble_event(tree, app, id, UiEvent::KeyUp { key }, None);
                }
            }

            UiEvent::Char { c } => {
                let c = *c;
                if let Some(id) = self.focused_node {
                    self.bubble_event(tree, app, id, UiEvent::Char { c }, None);
                }
            }

            UiEvent::MouseWheel { delta_x, delta_y } => {
                let (dx, dy) = (*delta_x, *delta_y);
                if let Some(id) = self.hovered_node {
                    self.bubble_event(
                        tree, app, id,
                        UiEvent::MouseWheel { delta_x: dx, delta_y: dy },
                        Some(self.cursor_pos),
                    );
                }
            }

            _ => {}
        }
    }

    // ─── Private Helpers ──────────────────────────────────────────────────────

    fn send_to_node<App>(
        &mut self,
        tree: &mut UiTree<App>,
        app: &mut App,
        id: NodeId,
        event: UiEvent,
        mouse_pos: Option<Vec2>,
    ) -> EventResponse {
        let (rect, theme) = if let Some(node) = tree.get_node(id) {
            (node.rect, tree.theme)
        } else {
            return EventResponse::Ignored;
        };

        let local_mouse_pos = mouse_pos.map(|p| Vec2::new(p.x - rect.x, p.y - rect.y));

        let mut widget = if let Some(n) = tree.get_node_mut(id) {
            std::mem::replace(
                &mut n.widget,
                Box::new(ferrous_ui_core::widgets::PlaceholderWidget),
            )
        } else {
            return EventResponse::Ignored;
        };

        let response = {
            let mut ctx = EventContext {
                node_id: id,
                rect,
                theme,
                tree,
                app,
                mouse_pos,
                local_mouse_pos,
            };
            widget.on_event(&mut ctx, &event)
        };

        if let Some(n) = tree.get_node_mut(id) {
            n.widget = widget;
        }

        if matches!(response, EventResponse::Redraw) {
            tree.mark_paint_dirty(id);
        }

        if let EventResponse::StartDrag { data } = &response {
            self.dragging_data = Some(data.clone());
            self.dragging_source = Some(id);
            self.last_drag_target = Some(id);
            // Inform the widget that drag has started
            self.send_to_node(tree, app, id, UiEvent::DragStart { pos: self.cursor_pos, data: data.clone() }, Some(self.cursor_pos));
        }

        response
    }


    fn bubble_event<App>(
        &mut self,
        tree: &mut UiTree<App>,
        app: &mut App,
        id: NodeId,
        event: UiEvent,
        mouse_pos: Option<Vec2>,
    ) {
        let response = self.send_to_node(tree, app, id, event.clone(), mouse_pos);

        if matches!(response, EventResponse::Ignored) {
            if let Some(parent_id) = tree.get_node_parent(id) {
                self.bubble_event(tree, app, parent_id, event, mouse_pos);
            }
        }
    }
}

impl Default for EventManager {
    fn default() -> Self {
        Self::new()
    }
}

// ─── Winit Converters ────────────────────────────────────────────────────────

pub fn winit_to_guikey(k: winit::keyboard::KeyCode) -> GuiKey {
    match k {
        winit::keyboard::KeyCode::Backspace  => GuiKey::Backspace,
        winit::keyboard::KeyCode::Delete     => GuiKey::Delete,
        winit::keyboard::KeyCode::ArrowLeft  => GuiKey::ArrowLeft,
        winit::keyboard::KeyCode::ArrowRight => GuiKey::ArrowRight,
        winit::keyboard::KeyCode::ArrowUp    => GuiKey::ArrowUp,
        winit::keyboard::KeyCode::ArrowDown  => GuiKey::ArrowDown,
        winit::keyboard::KeyCode::Home       => GuiKey::Home,
        winit::keyboard::KeyCode::End        => GuiKey::End,
        winit::keyboard::KeyCode::Enter | winit::keyboard::KeyCode::NumpadEnter => GuiKey::Enter,
        winit::keyboard::KeyCode::Escape     => GuiKey::Escape,
        winit::keyboard::KeyCode::Tab        => GuiKey::Tab,
        _                                    => GuiKey::Backspace,
    }
}

pub fn winit_to_mousebutton(b: winit::event::MouseButton) -> ferrous_ui_core::MouseButton {
    match b {
        winit::event::MouseButton::Left   => ferrous_ui_core::MouseButton::Left,
        winit::event::MouseButton::Right  => ferrous_ui_core::MouseButton::Right,
        winit::event::MouseButton::Middle => ferrous_ui_core::MouseButton::Middle,
        _                                 => ferrous_ui_core::MouseButton::Left,
    }
}
