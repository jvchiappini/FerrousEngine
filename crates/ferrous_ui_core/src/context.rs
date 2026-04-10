//! Context types for the Ferrous UI system.
//!
//! Contiene los tipos de contexto que los métodos del trait `Widget` reciben
//! en cada fase del ciclo de vida: build, update, layout, draw y eventos.

use glam::Vec2;
use crate::primitives::Rect;
use crate::theme::Theme;
use crate::NodeId;
use crate::UiTree;
use crate::Widget;

// ─── EventContext ──────────────────────────────────────────────────────────────

/// Contexto proporcionado durante el procesamiento de un evento.
///
/// Incluye acceso al árbol (para mutar hijos), al estado de la aplicación,
/// y a las coordenadas de mouse para hit-testing personalizado.
pub struct EventContext<'a, App> {
    pub node_id: NodeId,
    /// Rectángulo absoluto del widget en coordenadas de ventana.
    pub rect: Rect,
    pub theme: Theme,
    pub tree: &'a mut UiTree<App>,
    pub app: &'a mut App,
    /// Posición absoluta del cursor en coordenadas de ventana (px).
    /// `None` si el evento no tiene posición de mouse (ej: KeyDown).
    pub mouse_pos: Option<Vec2>,
    /// Posición del cursor relativa al origen del widget (`rect.x`, `rect.y`).
    /// `None` si el evento no tiene posición de mouse.
    /// Permite saber en qué parte interna del widget ocurrió el evento.
    pub local_mouse_pos: Option<Vec2>,
}

impl<'a, App> EventContext<'a, App> {
    /// Comprueba si el cursor está dentro del rect del widget (AABB).
    #[inline]
    pub fn is_cursor_inside(&self) -> bool {
        if let Some(pos) = self.mouse_pos {
            self.rect.contains([pos.x, pos.y])
        } else {
            false
        }
    }

    /// Devuelve la posición local normalizada [0,1]×[0,1] del cursor dentro del widget.
    /// `None` si el evento no tiene posición de mouse o si el widget tiene tamaño cero.
    pub fn local_uv(&self) -> Option<Vec2> {
        let lp = self.local_mouse_pos?;
        if self.rect.width <= 0.0 || self.rect.height <= 0.0 {
            return None;
        }
        Some(Vec2::new(
            (lp.x / self.rect.width).clamp(0.0, 1.0),
            (lp.y / self.rect.height).clamp(0.0, 1.0),
        ))
    }

    /// Marca el widget actual como paint-dirty (necesita redibujarse).
    pub fn request_redraw(&mut self) {
        self.tree.mark_paint_dirty(self.node_id);
    }
}

// ─── BuildContext ──────────────────────────────────────────────────────────────

/// Contexto proporcionado durante la fase de construcción de la jerarquía.
pub struct BuildContext<'a, App> {
    pub tree: &'a mut UiTree<App>,
    pub node_id: NodeId,
    pub theme: Theme,
}

impl<'a, App> BuildContext<'a, App> {
    /// Añade un widget hijo al nodo actual.
    pub fn add_child(&mut self, widget: Box<dyn Widget<App>>) -> NodeId {
        self.tree.add_node(widget, Some(self.node_id))
    }

    /// Obtiene el ID del nodo actual.
    pub fn node_id(&self) -> NodeId {
        self.node_id
    }

    /// Añade un componente reutilizable a la jerarquía actual.
    pub fn add_component<C: Component<App>>(&mut self, component: C) {
        component.build(self);
    }
}

// ─── Component ────────────────────────────────────────────────────────────────

/// Interfaz para componentes reutilizables que agrupan otros widgets.
/// Inspirado en `@Composable` de Jetpack Compose o componentes de React.
pub trait Component<App> {
    /// Construye la jerarquía del componente usando el contexto proporcionado.
    fn build(self, ctx: &mut BuildContext<App>);
}

// ─── UpdateContext ─────────────────────────────────────────────────────────────

/// Contexto proporcionado durante la fase de actualización lógica (cada frame).
pub struct UpdateContext {
    pub delta_time: f32,
    pub node_id: NodeId,
    /// Rectángulo actual del nodo tal como lo resolvió el engine de layout.
    pub rect: Rect,
    /// Tamaño total ocupado por los hijos (bounding box de hijos).
    /// Permite al widget saber si necesita scrollbars o limitar el desplazamiento.
    pub content_size: Vec2,
    pub theme: Theme,
    /// Si el widget lo pone a `true`, el nodo se marcará como paint-dirty al final del frame.
    /// Util para animaciones internas (cursor parpadeante, transiciones).
    pub needs_redraw: bool,
}


// ─── LayoutContext ─────────────────────────────────────────────────────────────

/// Contexto proporcionado durante el cálculo de tamaño preferido del widget.
pub struct LayoutContext {
    /// Espacio máximo disponible otorgado por el padre.
    pub available_space: Vec2,
    /// Dimensiones ya conocidas (si las hay) — permiten al widget adaptarse.
    pub known_dimensions: (Option<f32>, Option<f32>),
    pub node_id: NodeId,
    pub theme: Theme,
}

// ─── DrawContext ───────────────────────────────────────────────────────────────

/// Contexto proporcionado durante la generación de primitivas visuales.
pub struct DrawContext {
    pub node_id: NodeId,
    /// Rectángulo resuelto por el motor de layout donde debe dibujarse el widget.
    pub rect: Rect,
    pub theme: Theme,
}