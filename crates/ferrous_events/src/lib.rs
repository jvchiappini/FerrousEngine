//! `ferrous_events` — Abstracción pura de eventos de entrada para la interfaz de usuario.
//!
//! Este crate separa la lógica de interacción del motor gráfico y del sistema de ventanas (OS).
//! Proporciona un lenguaje común para que los widgets reaccionen a clicks, movimientos,
//! y pulsaciones de teclas sin conocer a `winit` o APIs similares.

use ferrous_ui_core::{NodeId, UiTree, UiEvent, GuiKey, Rect};
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
    pub fn hit_test(&self, tree: &UiTree, pos: Vec2) -> Option<NodeId> {
        if let Some(root_id) = tree.get_root() {
            self.hit_test_recursive(tree, root_id, pos)
        } else {
            None
        }
    }

    fn hit_test_recursive(&self, tree: &UiTree, id: NodeId, pos: Vec2) -> Option<NodeId> {
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
    pub fn dispatch_event(&mut self, tree: &mut UiTree, event: UiEvent) {
        match event {
            UiEvent::MouseMove { pos } => {
                let new_hover = self.hit_test(tree, pos);
                
                if new_hover != self.hovered_node {
                    // Mouse Leave
                    if let Some(old_id) = self.hovered_node {
                        self.send_to_node(tree, old_id, UiEvent::MouseLeave);
                    }
                    // Mouse Enter
                    if let Some(new_id) = new_hover {
                        self.send_to_node(tree, new_id, UiEvent::MouseEnter);
                    }
                    self.hovered_node = new_hover;
                }

                // Propagar el movimiento al nodo hovered
                if let Some(id) = self.hovered_node {
                    self.bubble_event(tree, id, UiEvent::MouseMove { pos });
                }
            }
            UiEvent::MouseDown { button, pos } => {
                let target = self.hit_test(tree, pos);
                if let Some(id) = target {
                    self.focused_node = Some(id);
                    self.bubble_event(tree, id, UiEvent::MouseDown { button, pos });
                } else {
                    self.focused_node = None;
                }
            }
            UiEvent::MouseUp { button, pos } => {
                if let Some(id) = self.hit_test(tree, pos) {
                    self.bubble_event(tree, id, UiEvent::MouseUp { button, pos });
                }
            }
            UiEvent::KeyDown { text, code } => {
                if let Some(id) = self.focused_node {
                    self.bubble_event(tree, id, UiEvent::KeyDown { text, code });
                }
            }
            _ => {}
        }
    }

    fn send_to_node(&mut self, tree: &mut UiTree, id: ferrous_ui_core::NodeId, event: UiEvent) -> ferrous_ui_core::EventResponse {
        let rect = tree.get_node_rect(id).unwrap_or_default();
        let mut ctx = ferrous_ui_core::EventContext { node_id: id, rect };
        
        let response = if let Some(node) = tree.get_node_mut(id) {
            node.widget.on_event(&mut ctx, &event)
        } else {
            ferrous_ui_core::EventResponse::Ignored
        };

        if let ferrous_ui_core::EventResponse::Redraw = response {
            tree.mark_paint_dirty(id);
        }

        response
    }

    fn bubble_event(&mut self, tree: &mut UiTree, id: ferrous_ui_core::NodeId, event: UiEvent) {
        let response = self.send_to_node(tree, id, event.clone());
        
        if let ferrous_ui_core::EventResponse::Ignored = response {
            // Propagar al padre
            let parent = tree.get_node_parent(id);
            if let Some(parent_id) = parent {
                self.bubble_event(tree, parent_id, event);
            }
        }
    }
}

/// Conversión desde códigos de tecla de `winit` al lenguaje interno de Ferrous UI.
/// Esto permite que el motor use `winit` como proveedor de eventos sin acoplar los crates de la UI.
/// Conversión desde códigos de tecla de `winit` al lenguaje interno de Ferrous UI.
/// Esto permite que el motor use `winit` como proveedor de eventos sin acoplar los crates de la UI.
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
        _ => GuiKey::Backspace, // Desvío por defecto para teclas no manejadas
    }
}
