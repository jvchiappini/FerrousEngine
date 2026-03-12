use ferrous_events::EventManager;
use ferrous_layout::LayoutEngine;
use ferrous_ui_core::{NodeId, Rect, UiEvent, UiTree, Widget};
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
    /// Pila de padres implícitos para `spawn_with`.
    /// Cuando no está vacía, los `.spawn()` insertan el widget como hijo del tope.
    pub(crate) parent_stack: Vec<NodeId>,
}

impl<App> UiSystem<App> {
    /// Crea un nuevo sistema de UI con configuraciones por defecto.
    pub fn new() -> Self {
        Self {
            tree: UiTree::new(),
            layout_engine: LayoutEngine::new(),
            event_manager: EventManager::new(),
            parent_stack: Vec::new(),
        }
    }

    /// Empuja un padre implícito. Usado por `PanelBuilder::spawn_with`.
    pub fn push_parent(&mut self, id: NodeId) {
        self.parent_stack.push(id);
    }

    /// Saca el último padre implícito.
    pub fn pop_parent(&mut self) {
        self.parent_stack.pop();
    }

    /// Devuelve el padre implícito actual, si existe.
    pub fn current_parent(&self) -> Option<NodeId> {
        self.parent_stack.last().copied()
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
        self.layout_engine
            .compute_layout(&mut self.tree, viewport_width, viewport_height);
    }

    /// Despacha un evento de entrada al sistema.
    /// Maneja automáticamente el Hit-Testing y la propagación de eventos (Bubbling).
    pub fn dispatch_event(&mut self, app: &mut App, event: UiEvent) {
        self.event_manager
            .dispatch_event(&mut self.tree, app, event);
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

    // =========================================================================
    // API fluent — conveniencia para crear widgets sin tocar UiTree directamente
    // =========================================================================

    /// Inicia la construccion de un boton con texto.
    ///
    /// ```rust,ignore
    /// ui.button("Guardar")
    ///     .at(10.0, 10.0)
    ///     .size(120.0, 36.0)
    ///     .on_click(|_| println!("guardado"))
    ///     .spawn(&mut ui);
    /// ```
    pub fn button(&mut self, label: impl Into<String>) -> crate::builder::ButtonBuilder<App> {
        crate::builder::ButtonBuilder::new(label)
    }

    /// Inicia la construccion de un label (texto estatico o dinamico).
    ///
    /// ```rust,ignore
    /// ui.label("Hola")
    ///     .at(20.0, 60.0)
    ///     .font_size(18.0)
    ///     .spawn(&mut ui);
    /// ```
    pub fn label(&mut self, text: impl Into<String>) -> crate::builder::LabelBuilder<App> {
        crate::builder::LabelBuilder::new(text)
    }

    /// Inicia la construccion de un panel (contenedor con fondo).
    ///
    /// ```rust,ignore
    /// ui.panel()
    ///     .at(50.0, 50.0)
    ///     .size(300.0, 200.0)
    ///     .spawn_with(&mut ui, |ui, p| {
    ///         ui.button("OK").child_of(p).at(8.0, 8.0).size(80.0, 32.0).spawn(ui);
    ///     });
    /// ```
    pub fn panel(&mut self) -> crate::builder::PanelBuilder<App> {
        crate::builder::PanelBuilder::new()
    }

    /// Inicia la construccion de cualquier widget personalizado.
    ///
    /// ```rust,ignore
    /// ui.widget(MyCustomWidget::new())
    ///     .at(0.0, 0.0)
    ///     .size(200.0, 100.0)
    ///     .spawn(&mut ui);
    /// ```
    pub fn widget(
        &mut self,
        widget: impl Widget<App> + 'static,
    ) -> crate::builder::WidgetBuilder<App> {
        crate::builder::WidgetBuilder::new(widget)
    }
}

impl<App> Default for UiSystem<App> {
    fn default() -> Self {
        Self::new()
    }
}
