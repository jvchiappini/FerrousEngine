use crate::canvas::Canvas;
use crate::GuiKey;
use crate::Widget;
use crate::layout::{RenderCommand, ToBatches};

/// Objeto de alto nivel que gestiona la interfaz de usuario completa para una aplicación.
/// 
/// `Ui` actúa como el punto de orquestación donde convergen el sistema de widgets
/// heredado ([`Canvas`]), el nuevo sistema de modo retenido ([`UiTree`]), el sistema de eventos
/// y la integración con el motor de layout.
/// 
/// Se recomienda tener una única instancia de `Ui` por ventana de la aplicación.
pub struct Ui {
    /// Contenedor para widgets que aún funcionan en el sistema antiguo de modo inmediato.
    canvas: Canvas,
    /// El nuevo motor de UI retenido que permite optimizaciones extremas (Lag Cero).
    tree: ferrous_ui_core::UiTree,
    /// Referencia opcional a un widget de viewport para aplicaciones 3D/Video.
    viewport: Option<std::rc::Rc<std::cell::RefCell<crate::viewport_widget::ViewportWidget>>>,
}

impl Ui {
    /// Crea una nueva instancia de `Ui` con sistemas de canvas y árbol vacíos.
    pub fn new() -> Self {
        Ui {
            canvas: Canvas::new(),
            tree: ferrous_ui_core::UiTree::new(),
            viewport: None,
        }
    }

    /// Añade un widget al sistema de `Canvas` heredado.
    pub fn add(&mut self, widget: impl Widget + 'static) {
        self.canvas.add(widget);
    }

    /// Añade un builder declarativo (estilo DSL) al nuevo `UiTree`. 
    /// 
    /// Este método es fundamental para la migración gradual hacia la arquitectura de
    /// alto rendimiento, permitiendo usar definiciones de nodos modernas dentro de la
    /// lógica existente.
    pub fn add_builder<T: Into<crate::layout::Node>>(&mut self, builder: T) -> ferrous_ui_core::NodeId {
        let node = builder.into();
        self.tree.add_node(Box::new(crate::layout::LegacyNodeWidget(node)), None)
    }

    /// Registra un widget especial de viewport.
    /// 
    /// El viewport se añade al canvas y además se guarda una referencia interna
    /// para facilitar el ajuste automático de resolución mediante [`set_viewport_rect`].
    pub fn register_viewport(
        &mut self,
        vp: std::rc::Rc<std::cell::RefCell<crate::viewport_widget::ViewportWidget>>,
    ) {
        self.viewport = Some(vp.clone());
        self.add(vp);
    }

    /// Actualiza las dimensiones del viewport registrado. 
    /// Útil para propagar eventos de redimensionado de ventana sin necesidad de
    /// manipular el widget manualmente.
    pub fn set_viewport_rect(&mut self, x: f32, y: f32, w: f32, h: f32) {
        if let Some(vp) = &self.viewport {
            vp.borrow_mut().rect = [x, y, w, h];
        }
    }

    /// Indica si el viewport registrado tiene el foco del usuario.
    /// Útil para alternar entre controles de UI y controles de cámara 3D.
    pub fn viewport_focused(&self) -> bool {
        self.viewport
            .as_ref()
            .map(|vp| vp.borrow().focused)
            .unwrap_or(false)
    }

    /// Propaga el movimiento del ratón a todos los widgets activos.
    pub fn mouse_move(&mut self, mx: f64, my: f64) {
        self.canvas.mouse_move(mx, my);
    }

    /// Propaga la interacción de botones del ratón.
    pub fn mouse_input(&mut self, mx: f64, my: f64, pressed: bool) {
        self.canvas.mouse_input(mx, my, pressed);
    }

    /// Entrega eventos de teclado al sistema de UI.
    pub fn keyboard_input(&mut self, text: Option<&str>, key: Option<GuiKey>, pressed: bool) {
        self.canvas.keyboard_input(text, key, pressed);
    }

    /// Recolecta los comandos de dibujo tanto del sistema antiguo como del nuevo.
    /// 
    /// Esta función es el corazón del pipeline de renderizado:
    /// 1. Recorre el `Canvas` recolectando comandos inmediatos.
    /// 2. Recorre el `UiTree` obteniendo comandos retenidos (usando caché si están limpios).
    /// 3. Traduce los `RenderCommand` abstractos en lotes optimizados de `GuiBatch` y `TextBatch`.
    #[cfg(feature = "text")]
    pub fn draw(
        &mut self,
        quad_batch: &mut crate::renderer::GuiBatch,
        text_batch: &mut crate::renderer::TextBatch,
        font: Option<&ferrous_assets::Font>,
    ) {
        let mut cmds: Vec<RenderCommand> = Vec::new();
        self.canvas.collect(&mut cmds);
        self.tree.collect_commands(&mut cmds);

        for cmd in &cmds {
            cmd.to_batches(quad_batch, text_batch, font);
        }
    }

    /// Versión de [`draw`] optimizada para cuando no se requiere soporte de texto.
    #[cfg(not(feature = "text"))]
    pub fn draw(
        &mut self,
        quad_batch: &mut crate::renderer::GuiBatch,
        text_batch: &mut crate::renderer::TextBatch,
    ) {
        let mut cmds: Vec<RenderCommand> = Vec::new();
        self.canvas.collect(&mut cmds);
        self.tree.collect_commands(&mut cmds);

        for cmd in &cmds {
            cmd.to_batches(quad_batch, text_batch);
        }
    }

    /// Acceso mutable al canvas heredado.
    pub fn canvas_mut(&mut self) -> &mut Canvas {
        &mut self.canvas
    }

    /// Resuelve las restricciones de layout para todos los widgets del canvas.
    /// 
    /// Este método debe llamarse en cada frame antes de [`draw`], especialmente 
    /// si la resolución de la ventana ha cambiado.
    pub fn resolve_constraints(&mut self, window_w: f32, window_h: f32) {
        for child in self.canvas.children_mut() {
            child.apply_constraint(window_w, window_h);
        }
    }
}

