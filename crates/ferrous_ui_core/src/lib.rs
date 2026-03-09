//! `ferrous_ui_core` — Núcleo de datos y lógica del sistema de UI de FerrousEngine.
//!
//! Este crate define las estructuras fundamentales para el sistema de UI "retenido" (Retained Mode).
//! A diferencia del modo inmediato, los widgets aquí persisten en un árbol de memoria (`UiTree`),
//! permitir optimizaciones masivas como el cálculo de layout diferido y el cacheo de comandos
//! de dibujo ("Lag Cero").

use slotmap::{new_key_type, SlotMap};
use glam::Vec2;
use serde::{Deserialize, Serialize};

pub mod events;
pub mod widgets;
pub mod reactive;
pub mod style_builder;
pub mod theme;
pub mod reflect;

// Re-export common types
pub use events::*;
pub use widgets::*;
pub use reactive::*;
pub use style_builder::{StyleBuilder, StyleExt};
pub use theme::{Theme, Color};
pub use reflect::*;
pub use ferrous_ui_macros::ui;

/// Espacio rectilíneo definido por su posición de origen (esquina superior izquierda) y sus dimensiones.
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
pub struct Rect {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

impl Rect {
    pub fn new(x: f32, y: f32, width: f32, height: f32) -> Self {
        Self { x, y, width, height }
    }

    /// Calcula la intersección entre dos rectángulos.
    pub fn intersect(&self, other: &Rect) -> Rect {
        let x = self.x.max(other.x);
        let y = self.y.max(other.y);
        let x2 = (self.x + self.width).min(other.x + other.width);
        let y2 = (self.y + self.height).min(other.y + other.height);
        
        Rect {
            x,
            y,
            width: (x2 - x).max(0.0),
            height: (y2 - y).max(0.0),
        }
    }

    /// Verifica si este rectángulo se solapa con otro.
    pub fn intersects(&self, other: &Rect) -> bool {
        let x = self.x.max(other.x);
        let y = self.y.max(other.y);
        let x2 = (self.x + self.width).min(other.x + other.width);
        let y2 = (self.y + self.height).min(other.y + other.height);
        
        x2 > x && y2 > y
    }

    /// Verifica si un punto está dentro del rectángulo.
    pub fn contains(&self, p: [f32; 2]) -> bool {
        p[0] >= self.x && p[0] <= self.x + self.width && p[1] >= self.y && p[1] <= self.y + self.height
    }
}

/// Define desplazamientos (offsets) para los cuatro lados de un rectángulo.
/// Utilizado para márgenes (margin) y rellenos (padding).
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize)]
pub struct RectOffset {
    pub left: f32,
    pub right: f32,
    pub top: f32,
    pub bottom: f32,
}

impl RectOffset {
    /// Crea un desplazamiento uniforme para todos los lados.
    pub fn all(v: f32) -> Self {
        Self { left: v, right: v, top: v, bottom: v }
    }
}

/// Unidades de medida para el sistema de layout.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum Units {
    /// Valor absoluto en píxeles físicos.
    Px(f32),
    /// Valor relativo al tamaño del contenedor padre (0.0 a 100.0).
    Percentage(f32),
    /// Unidad de flexibilidad para repartir el espacio sobrante en layouts Flexbox.
    Flex(f32),
    /// El tamaño se ajusta automáticamente al contenido o al contenedor.
    Auto,
}

impl Default for Units {
    fn default() -> Self {
        Units::Px(0.0)
    }
}

/// Alineación de elementos dentro de su contenedor (similar a CSS Flexbox).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Alignment {
    /// Alineado al inicio del eje.
    Start,
    /// Centrado en el eje.
    Center,
    /// Alineado al final del eje.
    End,
    /// El elemento se expande para llenar todo el espacio disponible.
    Stretch,
}

impl Default for Alignment {
    fn default() -> Self {
        Alignment::Start
    }
}

/// Define cómo se comportan los hijos dentro un nodo y cómo se posiciona el nodo mismo.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DisplayMode {
    /// Comportamiento estándar de bloque (uno encima de otro o posicionamiento absoluto).
    Block,
    /// Dispone a los hijos en una fila horizontal con lógica Flexbox.
    FlexRow,
    /// Dispone a los hijos en una columna vertical con lógica Flexbox.
    FlexColumn,
}

impl Default for DisplayMode {
    fn default() -> Self {
        DisplayMode::Block
    }
}

/// Define cómo se posiciona el nodo respecto a sus hermanos y padre.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Position {
    /// Posicionamiento relativo al flujo normal del layout.
    Relative,
    /// Posicionamiento absoluto, ignorando a los hermanos y basándose en desplazamientos.
    Absolute,
}

impl Default for Position {
    fn default() -> Self {
        Position::Relative
    }
}

/// Define cómo se comporta el contenido de un nodo cuando excede sus dimensiones.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Overflow {
    /// El contenido sobresale del nodo (por defecto).
    Visible,
    /// El contenido se recorta.
    Hidden,
    /// El contenido se recorta y habilita el desplazamiento.
    Scroll,
}

impl Default for Overflow {
    fn default() -> Self {
        Overflow::Visible
    }
}

/// Contenedor de propiedades visuales y de posicionamiento que definen cómo se verá y ubicará un Widget.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Style {
    /// Espacio exterior alrededor del widget.
    pub margin: RectOffset,
    /// Espacio interior entre el borde del widget y sus hijos.
    pub padding: RectOffset,
    /// Dimensiones deseadas (Ancho, Alto).
    pub size: (Units, Units),
    /// Alineación del contenido.
    pub alignment: Alignment,
    /// Modo de visualización de los hijos.
    pub display: DisplayMode,
    /// Tipo de posicionamiento.
    pub position: Position,
    /// Desplazamientos para posicionamiento absoluto.
    pub offsets: RectOffset,
    /// Comportamiento del contenido excedente.
    pub overflow: Overflow,
}

/// Representación simplificada de una operación de dibujo de la UI.
/// Los comandos se generan durante la fase `draw` y se cachean para optimizar el rendimiento (Lag Cero).
/// Un comando es un "Blueprint" que el backend de renderizado traducirá a primitivas de GPU.
#[derive(Debug, Clone)]
pub enum RenderCommand {
    /// Dibuja un rectángulo sólido o con bordes redondeados.
    Quad {
        rect: Rect,
        color: [f32; 4],
        /// Radio de las 4 esquinas.
        radii: [f32; 4],
        /// Flags adicionales (ej. bit de textura o degradado).
        flags: u32,
    },
    /// Dibuja una cadena de texto.
    Text {
        rect: Rect,
        text: String,
        color: [f32; 4],
        font_size: f32,
    },
    /// Dibuja una imagen texturizada.
    /// Esta variante requiere un `Arc` al recurso de textura para garantizar su vida útil durante el renderizado asíncrono.
    #[cfg(feature = "assets")]
    Image {
        rect: Rect,
        texture: std::sync::Arc<ferrous_assets::Texture2d>,
        uv0: [f32; 2],
        uv1: [f32; 2],
        color: [f32; 4],
    },
    /// Variante de imagen de fallback cuando el sistema de assets no está disponible.
    #[cfg(not(feature = "assets"))]
    Image {
        rect: Rect,
        texture_id: u64,
        uv0: [f32; 2],
        uv1: [f32; 2],
        color: [f32; 4],
    },
    /// Inicia una región de recorte (scissor). Todo lo dibujado después quedará limitado a este rectángulo.
    PushClip { rect: Rect },
    /// Finaliza la región de recorte más reciente y restaura la anterior.
    PopClip,
}

new_key_type! {
    /// Identificador único y estable para un nodo dentro del `UiTree`.
    pub struct NodeId;
}

/// Flags que indican qué aspectos del nodo o su subárbol necesitan ser actualizados.
/// Este sistema es la pieza clave para lograr "Lag Cero".
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct DirtyFlags {
    /// Indica que el tamaño o posición del nodo debe recalcularse.
    pub layout: bool,
    /// Indica que visualmente el nodo ha cambiado y debe regenerar sus `RenderCommand`.
    pub paint: bool,
    /// Indica que la jerarquía (hijos) ha cambiado.
    pub hierarchy: bool,
    /// Propagación: true si este nodo o alguno de sus descendientes está sucio.
    /// Permite saltar ramas enteras del árbol durante el recorrido si es false.
    pub subtree_dirty: bool,
}
/// Cola de comandos diferidos para la UI.
/// Permite que los widgets soliciten acciones que deben ocurrir fuera del ciclo de eventos
/// (ej: abrir una ventana, cerrar la app).
pub struct CmdQueue {
    // TODO: Implementar variantes de comandos diferidos
}

impl CmdQueue {
    pub fn new() -> Self {
        Self {}
    }
}

impl DirtyFlags {
    /// Crea un conjunto de flags "limpias".
    pub fn none() -> Self {
        Self::default()
    }

    /// Crea un conjunto de flags donde todo está marcado como sucio.
    pub fn all() -> Self {
        Self {
            layout: true,
            paint: true,
            hierarchy: true,
            subtree_dirty: true,
        }
    }

    /// Verifica si el nodo local tiene alguna necesidad de actualización.
    pub fn is_dirty(&self) -> bool {
        self.layout || self.paint || self.hierarchy
    }
}

/// Interfaz fundamental para cualquier componente de la interfaz de usuario.
pub trait Widget<App>: Send + Sync {
    /// Se invoca cuando el widget se inserta en el árbol. Es el lugar para añadir hijos iniciales.
    fn build(&mut self, _ctx: &mut BuildContext<App>) {}
    
    /// Se invoca en cada frame para actualizar el estado interno (animaciones, timers, etc.).
    fn update(&mut self, _ctx: &mut UpdateContext) {}
    
    /// Define el tamaño ideal que este widget desea ocupar. El sistema de layout lo usará como sugerencia.
    fn calculate_size(&self, _ctx: &mut LayoutContext) -> Vec2 {
        Vec2::ZERO
    }

    /// Genera la lista de comandos de renderizado para representar visualmente el widget.
    /// Estos comandos se cachearán en el `Node` asociado.
    fn draw(&self, _ctx: &mut DrawContext, _cmds: &mut Vec<RenderCommand>) {}

    /// Se invoca cuando ocurre un evento que afecta a este widget.
    fn on_event(&mut self, _ctx: &mut EventContext<App>, _event: &UiEvent) -> EventResponse {
        EventResponse::Ignored
    }

    /// Devuelve el desplazamiento de scroll actual si el widget lo soporta.
    fn scroll_offset(&self) -> Vec2 {
        Vec2::ZERO
    }

    /// Devuelve la interfaz de reflexión para este widget (opcional).
    fn reflect(&self) -> Option<&dyn FerrousWidgetReflect> {
        None
    }

    /// Devuelve la interfaz de reflexión mutable para este widget (opcional).
    fn reflect_mut(&mut self) -> Option<&mut dyn FerrousWidgetReflect> {
        None
    }
}

/// Contexto proporcionado durante la fase de procesamiento de eventos.
/// Incluye acceso al árbol, al estado de la aplicación y a una cola de comandos.
pub struct EventContext<'a, App> {
    pub node_id: NodeId,
    pub rect: Rect,
    pub theme: Theme,
    pub tree: &'a mut UiTree<App>,
    pub app: &'a mut App,
}


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

/// Interfaz para componentes reutilizables que agrupan otros widgets.
/// Inspirado en `@Composable` de Jetpack Compose o componentes de React.
pub trait Component<App> {
    /// Construye la jerarquía del componente usando el contexto proporcionado.
    fn build(self, ctx: &mut BuildContext<App>);
}

/// Contexto proporcionado durante la fase de actualización de lógica.
pub struct UpdateContext {
    pub delta_time: f32,
    pub node_id: NodeId,
    /// Rectángulo actual del nodo.
    pub rect: Rect,
    pub theme: Theme,
}

/// Contexto proporcionado durante la fase de cálculo de layout.
pub struct LayoutContext {
    /// Espacio máximo disponible otorgado por el padre.
    pub available_space: Vec2,
    /// Dimensiones ya conocidas (si las hay).
    pub known_dimensions: (Option<f32>, Option<f32>),
    pub node_id: NodeId,
    pub theme: Theme,
}

/// Contexto proporcionado durante la fase de generación de primitivas visuales.
pub struct DrawContext {
    pub node_id: NodeId,
    /// Rectángulo resuelto por el motor de layout donde debe dibujarse el widget.
    pub rect: Rect,
    pub theme: Theme,
}

/// Unidad mínima de almacenamiento en el sistema reactivo.
/// Contiene un widget y todos los metadatos necesarios para su gestión y renderizado optimizado.
pub struct Node<App> {
    pub widget: Box<dyn Widget<App>>,
    pub parent: Option<NodeId>,
    pub children: Vec<NodeId>,
    pub style: Style,
    pub dirty: DirtyFlags,
    /// Rectángulo final resuelto por el motor de layout en coordenadas locales/globales.
    pub rect: Rect,
    /// Caché de comandos de dibujo generados en el último frame donde el nodo estuvo "sucio".
    pub cached_cmds: Vec<RenderCommand>,
}

/// Gestor principal del árbol de widgets. 
/// Mantiene la jerarquía usando un `SlotMap` para garantizar acceso O(1) y estabilidad de IDs.
pub struct UiTree<App> {
    nodes: SlotMap<NodeId, Node<App>>,
    root: Option<NodeId>,
    /// Mapeo de identificadores de texto a NodeIds para búsquedas rápidas.
    id_map: std::collections::HashMap<String, NodeId>,
    /// Sistema que gestiona las actualizaciones reactivas de los nodos.
    pub reactivity: ReactivitySystem,
    pub theme: Theme,
    /// Cola de comandos diferidos.
    pub commands: CmdQueue,
}

impl<App> UiTree<App> {
    /// Crea un árbol de UI vacío.
    pub fn new() -> Self {
        Self {
            nodes: SlotMap::with_key(),
            root: None,
            id_map: std::collections::HashMap::new(),
            reactivity: ReactivitySystem::new(),
            theme: Theme::default(),
            commands: CmdQueue::new(),
        }
    }

    pub fn get_root(&self) -> Option<NodeId> {
        self.root
    }

    /// Obtiene una referencia mutable a un nodo del árbol.
    pub fn get_node_mut(&mut self, id: NodeId) -> Option<&mut Node<App>> {
        self.nodes.get_mut(id)
    }

    /// Obtiene una referencia inmutable a un nodo del árbol.
    pub fn get_node(&self, id: NodeId) -> Option<&Node<App>> {
        self.nodes.get(id)
    }

    /// Ejecuta la fase de construcción recursiva desde la raíz.
    pub fn build(&mut self) {
        if let Some(root_id) = self.root {
            self.build_node(root_id);
        }
    }

    fn build_node(&mut self, id: NodeId) {
        if let Some(node) = self.nodes.get_mut(id) {
            node.children.clear();
        }

        // Extraemos temporalmente el widget para evitar doble préstamo del tree
        // mientras llamamos a widget.build(&mut ctx).
        // Usamos un placeholder temporal.
        // NOTA: PlaceholderWidget debe ser compatible con <App>.
        // Como es un marcador, su implementación de Widget<App> será genérica.
        let mut widget = if let Some(node) = self.nodes.get_mut(id) {
            std::mem::replace(&mut node.widget, Box::new(crate::widgets::PlaceholderWidget))
        } else {
            return;
        };

        let theme = self.theme;
        let mut ctx = BuildContext { tree: self, node_id: id, theme };
        widget.build(&mut ctx);

        let children = if let Some(node) = self.nodes.get_mut(id) {
            node.widget = widget;
            node.children.clone()
        } else {
            return;
        };

        for child_id in children {
            self.build_node(child_id);
        }
    }

    /// Actualiza la lógica de todos los widgets del árbol.
    pub fn update(&mut self, delta_time: f32) {
        // Extraemos los nodos pendientes antes de mutar el árbol, evitando el
        // borrow doble que ocurriría con `self.reactivity.apply(self)`.
        let dirty_nodes = std::mem::take(&mut self.reactivity.pending_dirty_nodes);
        for id in dirty_nodes {
            self.mark_paint_dirty(id);
        }

        if let Some(root_id) = self.root {
            self.update_node(root_id, delta_time);
        }
    }


    fn update_node(&mut self, id: NodeId, delta_time: f32) {
        let children = if let Some(node) = self.nodes.get(id) {
            node.children.clone()
        } else {
            return;
        };

        for child_id in children {
            self.update_node(child_id, delta_time);
        }

        if let Some(node) = self.nodes.get_mut(id) {
            let theme = self.theme;
            let mut ctx = UpdateContext { 
                delta_time, 
                node_id: id,
                rect: node.rect,
                theme,
            };
            node.widget.update(&mut ctx);
        }
    }

    /// Recolecta los comandos de renderizado de todo el árbol.
    /// Si un nodo no está marcado como `paint_dirty`, se utilizan los comandos cacheados del frame anterior.
    /// Solo se procesan los nodos que intersectan con el `viewport` proporcionado (Culling).
    pub fn collect_commands(&mut self, cmds: &mut Vec<RenderCommand>, viewport: Rect) {
        if let Some(root_id) = self.root {
            self.collect_node_commands(root_id, cmds, viewport);
        }
    }

    fn collect_node_commands(&mut self, id: NodeId, cmds: &mut Vec<RenderCommand>, viewport: Rect) {
        let (is_dirty, is_subtree_dirty, node_rect) = if let Some(node) = self.nodes.get(id) {
            (node.dirty.is_dirty(), node.dirty.subtree_dirty, node.rect)
        } else {
            return;
        };

        // Culling: Si el nodo está completamente fuera del viewport, lo ignoramos.
        // Asumimos que los hijos están contenidos en el padre (modelo de UI estándar).
        if !node_rect.intersects(&viewport) {
            return;
        }

        if is_dirty {
            if let Some(node) = self.nodes.get_mut(id) {
                node.cached_cmds.clear();
                let theme = self.theme;
                let mut ctx = DrawContext { 
                    node_id: id,
                    rect: node.rect,
                    theme,
                };
                node.widget.draw(&mut ctx, &mut node.cached_cmds);
                node.dirty.paint = false;
                node.dirty.layout = false;
                node.dirty.hierarchy = false;
            }
        }

        // Añadir los comandos (ya sean nuevos o cacheados) a la lista global.
        if let Some(node) = self.nodes.get(id) {
            // Si el nodo tiene recorte (Hidden o Scroll), iniciamos un clipping
            let overflow_clip = node.style.overflow != Overflow::Visible;
            if overflow_clip {
                cmds.push(RenderCommand::PushClip { rect: node.rect });
            }

            cmds.extend(node.cached_cmds.iter().cloned());
            
            if is_subtree_dirty {
                let children = node.children.clone();
                for child_id in children {
                    self.collect_node_commands(child_id, cmds, viewport);
                }
                if let Some(node) = self.nodes.get_mut(id) {
                    node.dirty.subtree_dirty = false;
                }
            }

            if overflow_clip {
                cmds.push(RenderCommand::PopClip);
            }
        }
    }

    /// Marca un nodo como sucio para layout y propaga la flag hacia los padres.
    pub fn mark_layout_dirty(&mut self, id: NodeId) {
        if let Some(node) = self.nodes.get_mut(id) {
            node.dirty.subtree_dirty = true;
            if !node.dirty.layout {
                node.dirty.layout = true;
                if let Some(parent_id) = node.parent {
                    self.mark_layout_dirty(parent_id);
                }
            }
        }
    }

    /// Marca un nodo como sucio para repintado.
    pub fn mark_paint_dirty(&mut self, id: NodeId) {
        if let Some(node) = self.nodes.get_mut(id) {
            node.dirty.subtree_dirty = true;
            node.dirty.paint = true;
            if let Some(parent_id) = node.parent {
                self.mark_subtree_dirty_up(parent_id);
            }
        }
    }

    fn mark_subtree_dirty_up(&mut self, id: NodeId) {
        if let Some(node) = self.nodes.get_mut(id) {
            if !node.dirty.subtree_dirty {
                node.dirty.subtree_dirty = true;
                if let Some(parent_id) = node.parent {
                    self.mark_subtree_dirty_up(parent_id);
                }
            }
        }
    }

    /// Inserta un nuevo nodo en el árbol.
    pub fn add_node(&mut self, widget: Box<dyn Widget<App>>, parent: Option<NodeId>) -> NodeId {
        self.add_node_with_id(widget, parent, None)
    }

    /// Inserta un nuevo nodo en el árbol con un identificador opcional.
    pub fn add_node_with_id(&mut self, widget: Box<dyn Widget<App>>, parent: Option<NodeId>, id_str: Option<String>) -> NodeId {
        let id = self.nodes.insert(Node {
            widget,
            parent,
            children: Vec::new(),
            style: Style::default(),
            dirty: DirtyFlags::all(),
            rect: Rect::default(),
            cached_cmds: Vec::new(),
        });

        if let Some(s) = id_str {
            self.id_map.insert(s, id);
        }

        if let Some(parent_id) = parent {
            if let Some(parent_node) = self.nodes.get_mut(parent_id) {
                parent_node.children.push(id);
                parent_node.dirty.hierarchy = true;
                self.mark_layout_dirty(parent_id);
            }
        } else if self.root.is_none() {
            self.root = Some(id);
        }

        id
    }

    /// Obtiene los hijos de un nodo.
    pub fn get_node_children(&self, id: NodeId) -> Option<&[NodeId]> {
        self.nodes.get(id).map(|n| n.children.as_slice())
    }

    /// Obtiene el estilo de un nodo.
    pub fn get_node_style(&self, id: NodeId) -> Option<&Style> {
        self.nodes.get(id).map(|n| &n.style)
    }

    /// Establece el estilo de un nodo y lo marca como sucio para layout.
    pub fn set_node_style(&mut self, id: NodeId, style: Style) {
        if let Some(node) = self.nodes.get_mut(id) {
            node.style = style;
            self.mark_layout_dirty(id);
        }
    }

    /// Obtiene el rectángulo resuelto de un nodo.
    pub fn get_node_rect(&self, id: NodeId) -> Option<Rect> {
        self.nodes.get(id).map(|n| n.rect)
    }

    /// Obtiene el padre de un nodo.
    pub fn get_node_parent(&self, id: NodeId) -> Option<NodeId> {
        self.nodes.get(id).and_then(|n| n.parent)
    }

    /// Establece el rectángulo de un nodo y lo marca como sucio para repintado.
    pub fn set_node_rect(&mut self, id: NodeId, rect: Rect) {
        if let Some(node) = self.nodes.get_mut(id) {
            node.rect = rect;
            node.dirty.paint = true;
            // No llamamos a mark_layout_dirty aquí porque esto suele ser el RESULTADO del layout.
        }
    }
    pub fn get_node_mut(&mut self, id: NodeId) -> Option<&mut Node<App>> {
        self.nodes.get_mut(id)
    }

    pub fn get_node(&self, id: NodeId) -> Option<&Node<App>> {
        self.nodes.get(id)
    }

    /// Busca un nodo por su identificador de texto.
    pub fn get_node_by_id(&self, id_str: &str) -> Option<NodeId> {
        self.id_map.get(id_str).copied()
    }
}
