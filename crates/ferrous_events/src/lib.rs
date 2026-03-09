//! `ferrous_events` — Abstracción pura de eventos de entrada para la interfaz de usuario.
//!
//! Este crate separa la lógica de interacción del motor gráfico y del sistema de ventanas (OS).
//! Proporciona un lenguaje común para que los widgets reaccionen a clicks, movimientos,
//! y pulsaciones de teclas sin conocer a `winit` o APIs similares.

use ferrous_ui_core::{NodeId, UiTree, UiEvent, Rect, EventResponse, EventContext, GuiKey};
use glam::Vec2;

/// Gestor del estado de foco y hover del sistema de eventos.
/// Mantiene rastro de qué nodos del `UiTree` están interactuando con el usuario.
pub struct EventManager {
    /// Nodo que actualmente tiene el cursor encima. Utilizado para efectos visuales (Highlight).
    pub hovered_node: Option<NodeId>,
    /// Nodo que posee el foco del teclado. Todos los `KeyDown` se dirigirán aquí primero.
    pub focused_node: Option<NodeId>,
}

impl EventManager {
    /// Inicializa un gestor de eventos sin estado activo.
    pub fn new() -> Self {
        Self {
            hovered_node: None,
            focused_node: None,
        }
    }

    /// Realiza un hit-test recursivo para encontrar el nodo más profundo que contiene el punto dado.
    pub fn hit_test<App>(&self, tree: &UiTree<App>, pos: Vec2) -> Option<NodeId> {
        if let Some(root_id) = tree.get_root() {
            self.hit_test_recursive(tree, root_id, pos)
        } else {
            None
        }
    }

    fn hit_test_recursive<App>(&self, tree: &UiTree<App>, id: NodeId, pos: Vec2) -> Option<NodeId> {
        let rect = tree.get_node_rect(id)?;
        
        // Si el punto no está en el nodo, abortamos esta rama
        if !self.point_in_rect(pos, rect) {
            return None;
        }

        // Revisamos hijos de atrás hacia adelante (los últimos suelen estar "encima")
        if let Some(children) = tree.get_node_children(id) {
            for &child_id in children.iter().rev() {
                if let Some(hit) = self.hit_test_recursive(tree, child_id, pos) {
                    return Some(hit);
                }
            }
        }

        // Si ningún hijo cazó el click pero el nodo actual sí lo contiene
        Some(id)
    }

    fn point_in_rect(&self, pos: Vec2, rect: Rect) -> bool {
        pos.x >= rect.x && pos.x <= rect.x + rect.width &&
        pos.y >= rect.y && pos.y <= rect.y + rect.height
    }

    /// Despacha un evento al árbol de UI, manejando la propagación (bubbling) y el estado de foco/hover.
    pub fn dispatch_event<App>(&mut self, tree: &mut UiTree<App>, app: &mut App, event: UiEvent) {
        match event {
            UiEvent::MouseMove { pos } => {
                let new_hover = self.hit_test(tree, pos);
                
                if new_hover != self.hovered_node {
                    // Mouse Leave
                    if let Some(old_id) = self.hovered_node {
                        self.send_to_node(tree, app, old_id, UiEvent::MouseLeave);
                    }
                    // Mouse Enter
                    if let Some(new_id) = new_hover {
                        self.send_to_node(tree, app, new_id, UiEvent::MouseEnter);
                    }
                    self.hovered_node = new_hover;
                }

                // Propagar el movimiento al nodo hovered
                if let Some(id) = self.hovered_node {
                    self.bubble_event(tree, app, id, UiEvent::MouseMove { pos });
                }
            }
            UiEvent::MouseDown { button, pos } => {
                let target = self.hit_test(tree, pos);
                if let Some(id) = target {
                    self.focused_node = Some(id);
                    self.bubble_event(tree, app, id, UiEvent::MouseDown { button, pos });
                } else {
                    self.focused_node = None;
                }
            }
            UiEvent::MouseUp { button, pos } => {
                if let Some(id) = self.hit_test(tree, pos) {
                    self.bubble_event(tree, app, id, UiEvent::MouseUp { button, pos });
                }
            }
            UiEvent::KeyDown { key } => {
                if let Some(id) = self.focused_node {
                    self.bubble_event(tree, app, id, UiEvent::KeyDown { key });
                }
            }
            UiEvent::KeyUp { key } => {
                if let Some(id) = self.focused_node {
                    self.bubble_event(tree, app, id, UiEvent::KeyUp { key });
                }
            }
            UiEvent::Char { c } => {
                if let Some(id) = self.focused_node {
                    self.bubble_event(tree, app, id, UiEvent::Char { c });
                }
            }
            UiEvent::MouseWheel { delta_x, delta_y } => {
                if let Some(id) = self.hovered_node {
                    self.bubble_event(tree, app, id, UiEvent::MouseWheel { delta_x, delta_y });
                }
            }
            _ => {}
        }
    }

    fn send_to_node<App>(&mut self, tree: &mut UiTree<App>, app: &mut App, id: NodeId, event: UiEvent) -> EventResponse {
        let (rect, theme) = if let Some(node) = tree.get_node(id) {
            (node.rect, tree.theme)
        } else {
            return EventResponse::Ignored;
        };

        // Extraemos el widget temporalmente para evitar doble préstamo mutable del árbol
        // durante la creación del EventContext que requiere &mut UiTree.
        let mut widget = if let Some(n) = tree.get_node_mut(id) {
            std::mem::replace(&mut n.widget, Box::new(ferrous_ui_core::widgets::PlaceholderWidget))
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
            };
            widget.on_event(&mut ctx, &event)
        };

        // Devolvemos el widget a su nodo
        if let Some(n) = tree.get_node_mut(id) {
            n.widget = widget;
        }

        if let EventResponse::Redraw = response {
            tree.mark_paint_dirty(id);
        }

        response
    }

    fn bubble_event<App>(&mut self, tree: &mut UiTree<App>, app: &mut App, id: NodeId, event: UiEvent) {
        let response = self.send_to_node(tree, app, id, event.clone());
        
        if let EventResponse::Ignored = response {
            // Propagar al padre
            if let Some(parent_id) = tree.get_node_parent(id) {
                self.bubble_event(tree, app, parent_id, event);
            }
        }
    }
}

pub fn winit_to_guikey(k: winit::keyboard::KeyCode) -> GuiKey {
    match k {
        winit::keyboard::KeyCode::Backspace => GuiKey::Backspace,
        winit::keyboard::KeyCode::Delete => GuiKey::Delete,
        winit::keyboard::KeyCode::ArrowLeft => GuiKey::ArrowLeft,
        winit::keyboard::KeyCode::ArrowRight => GuiKey::ArrowRight,
        winit::keyboard::KeyCode::ArrowUp => GuiKey::ArrowUp,
        winit::keyboard::KeyCode::ArrowDown => GuiKey::ArrowDown,
        winit::keyboard::KeyCode::Home => GuiKey::Home,
        winit::keyboard::KeyCode::End => GuiKey::End,
        winit::keyboard::KeyCode::Enter | winit::keyboard::KeyCode::NumpadEnter => GuiKey::Enter,
        winit::keyboard::KeyCode::Escape => GuiKey::Escape,
        winit::keyboard::KeyCode::Tab => GuiKey::Tab,
        _ => GuiKey::Backspace, 
    }
}

pub fn winit_to_mousebutton(b: winit::event::MouseButton) -> ferrous_ui_core::MouseButton {
    match b {
        winit::event::MouseButton::Left => ferrous_ui_core::MouseButton::Left,
        winit::event::MouseButton::Right => ferrous_ui_core::MouseButton::Right,
        winit::event::MouseButton::Middle => ferrous_ui_core::MouseButton::Middle,
        _ => ferrous_ui_core::MouseButton::Left,
    }
}
