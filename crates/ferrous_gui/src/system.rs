use ferrous_ui_core::{UiTree, Rect, UiEvent, NodeId, Widget};
use ferrous_layout::LayoutEngine;
use ferrous_events::EventManager;
use ferrous_ui_render::{GuiBatch, ToBatches};

/// `UiSystem` es el orquestador maestro del sistema de UI. 
/// 
/// Coordina el estado de los widgets (`UiTree`), el motor de layout (`LayoutEngine`), 
/// la gestión de eventos (`EventManager`) y facilita la generación de comandos de dibujo.
pub struct UiSystem<App> {
    /// Árbol de widgets que contiene la jerarquía y el estado reactivo.
    pub tree: UiTree<App>,
    /// Motor encargado de calcular posiciones y tamaños usando Taffy (Flexbox).
    pub layout_engine: LayoutEngine,
    /// Gestor de interacción, foco y hover.
    pub event_manager: EventManager,
}

impl<App> UiSystem<App> {
    /// Crea un nuevo sistema de UI con configuraciones por defecto.
    pub fn new() -> Self {
        Self {
            tree: UiTree::new(),
            layout_engine: LayoutEngine::new(),
            event_manager: EventManager::new(),
        }
    }

    /// Añade un widget a la raíz del sistema (alias de add_node).
    pub fn add(&mut self, widget: impl Widget<App> + 'static) -> NodeId {
        self.tree.add_node(Box::new(widget), None)
    }

    /// Registra un widget especial de Viewport. 
    /// En esta versión, simplemente lo añade como un nodo más.
    pub fn register_viewport(&mut self, viewport: impl Widget<App> + 'static) -> NodeId {
        self.tree.add_node(Box::new(viewport), None)
    }

    /// Punto de entrada para el ciclo de actualización. 
    /// Procesa la reactividad y recalcula la disposición de los elementos.
    pub fn update(&mut self, dt: f32, viewport_width: f32, viewport_height: f32) {
        // 1. Actualizar la lógica interna de los widgets y el sistema reactivo.
        self.tree.update(dt);

        // 2. Recalcular el layout de todo el árbol.
        // Taffy es eficiente y solo recalcula lo que ha cambiado internamente (Dirty Nodes).
        self.layout_engine.compute_layout(&mut self.tree, viewport_width, viewport_height);
    }

    /// Despacha un evento de entrada al sistema.
    /// Maneja automáticamente el Hit-Testing y la propagación de eventos (Bubbling).
    pub fn dispatch_event(&mut self, app: &mut App, event: UiEvent) {
        self.event_manager.dispatch_event(&mut self.tree, app, event);
    }

    /// Genera un lote de quads listos para ser enviados al backend de renderizado (WGPU).
    /// Realiza culling automático basado en el viewport proporcionado.
    #[cfg(feature = "text")]
    pub fn render(&mut self, viewport: Rect, font: Option<&ferrous_assets::Font>) -> GuiBatch {
        let mut cmds = Vec::new();
        self.tree.collect_commands(&mut cmds, viewport);

        let mut batch = GuiBatch::new();
        for cmd in cmds {
            cmd.to_batches(&mut batch, font);
        }
        batch
    }

    /// Versión de renderizado cuando el soporte de texto está deshabilitado.
    #[cfg(not(feature = "text"))]
    pub fn render(&mut self, viewport: Rect) -> GuiBatch {
        let mut cmds = Vec::new();
        self.tree.collect_commands(&mut cmds, viewport);

        let mut batch = GuiBatch::new();
        for cmd in cmds {
            cmd.to_batches(&mut batch);
        }
        batch
    }
}

impl<App> Default for UiSystem<App> {
    fn default() -> Self {
        Self::new()
    }
}
