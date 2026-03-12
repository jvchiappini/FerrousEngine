//! # `TreeView` — Lista Expandible Jerárquica
//!
//! `TreeView` visualiza datos jerárquicos en forma de árbol con nodos expandibles
//! y colapsables, selección simple/múltiple y Drag & Drop de nodos.
//!
//! Es **esencial para Ferrous Builder**: la jerarquía de escena y el explorador
//! de archivos se construyen sobre este widget.
//!
//! ## Ejemplo de uso
//!
//! ```rust,ignore
//! use ferrous_ui_core::{TreeView, TreeNode};
//!
//! let mut root = TreeNode::new("Escena").icon('🎬');
//! root.add_child(
//!     TreeNode::new("Cámara").icon('📷')
//! );
//! root.add_child(
//!     TreeNode::new("Objetos")
//!         .expanded(true)
//!         .add_child(TreeNode::new("Cubo").icon('🟦'))
//!         .add_child(TreeNode::new("Esfera").icon('🔵'))
//! );
//!
//! let view = TreeView::<MyApp>::new()
//!     .with_root(root)
//!     .on_select(|ctx, node_path| {
//!         ctx.app.selected = node_path.clone();
//!     });
//!
//! tree.add_node(Box::new(view), Some(panel_id));
//! ```
//!
//! ## Arquitectura interna
//!
//! `TreeView` almacena en memoria la jerarquía de [`TreeNode`] y en `draw()`
//! calcula la lista plana de nodos visibles (flat-list de los nodos expandidos),
//! pintando cada fila a mano sin crear nodos hijo en el `UiTree`. Esto mantiene
//! el árbol de UI compacto incluso para jerarquías de miles de nodos.
//!
//! La selección, el hover y el drag-and-drop se gestionan a través de
//! `on_event` usando las coordenadas de la fila.

use crate::{
    Widget, RenderCommand, DrawContext, BuildContext, LayoutContext, EventContext,
    EventResponse, UiEvent, Rect, Vec2, Color, StyleBuilder, StyleExt,
};

// ─── TreeNode ────────────────────────────────────────────────────────────────

/// Nodo de datos del [`TreeView`]. Puede contener hijos anidados.
#[derive(Debug, Clone)]
pub struct TreeNode {
    /// Etiqueta visible del nodo.
    pub label: String,
    /// Carácter emoji/unicode que aparece antes del label (opcional).
    pub icon: Option<char>,
    /// Si `true`, los hijos son visibles.
    pub expanded: bool,
    /// Hijos del nodo.
    pub children: Vec<TreeNode>,
    /// Datos de usuario opacos (por ejemplo, ID de entidad en la escena).
    pub user_data: u64,
}

impl TreeNode {
    /// Crea un nodo con la etiqueta dada.
    pub fn new(label: impl Into<String>) -> Self {
        Self {
            label: label.into(),
            icon: None,
            expanded: false,
            children: Vec::new(),
            user_data: 0,
        }
    }

    /// Asigna un icono al nodo.
    pub fn icon(mut self, c: char) -> Self {
        self.icon = Some(c);
        self
    }

    /// Establece el estado expandido inicial.
    pub fn expanded(mut self, v: bool) -> Self {
        self.expanded = v;
        self
    }

    /// Dato de usuario opaco (e.g. EntityId de la escena).
    pub fn user_data(mut self, data: u64) -> Self {
        self.user_data = data;
        self
    }

    /// Añade un nodo hijo (builder fluent).
    pub fn add_child(mut self, child: TreeNode) -> Self {
        self.children.push(child);
        self
    }
}

// ─── FlatRow ─────────────────────────────────────────────────────────────────

/// Fila calculada de la lista plana (visibilidad comprobada por expansión).
#[derive(Debug, Clone)]
struct FlatRow {
    /// Nivel de profundidad (0 = raíz).
    depth: usize,
    /// Índice de esta fila en la lista plana.
    flat_index: usize,
    /// Ruta para localizar el nodo en la jerarquía.
    path: Vec<usize>,
    label: String,
    icon: Option<char>,
    has_children: bool,
    expanded: bool,
    user_data: u64,
}

// ─── TreeView ─────────────────────────────────────────────────────────────────

/// Tipo del callback de selección.
type SelectCallback<App> = Box<dyn Fn(&mut EventContext<App>, &[usize]) + Send + Sync>;

/// Widget de árbol jerárquico expandible con selección y drag-and-drop.
///
/// Consulta la [documentación del módulo][self] para el uso completo.
pub struct TreeView<App> {
    /// Nodo raíz del árbol de datos.
    pub root: Option<TreeNode>,

    /// Índice(s) de filas seleccionadas (en la lista plana actual).
    pub selected: Vec<usize>,

    // ── Opciones visuales ────────────────────────────────────────────────
    /// Alto en píxeles de cada fila.
    pub row_height: f32,
    /// Sangría en píxeles por nivel de profundidad.
    pub indent_px: f32,
    /// Si `true`, permite selección múltiple (Ctrl+Click).
    pub multi_select: bool,

    // ── Estado interno ───────────────────────────────────────────────────
    scroll_offset: f32,
    hovered_row: Option<usize>,

    // ── Drag & Drop ──────────────────────────────────────────────────────
    drag_row: Option<usize>,
    drag_pos: Vec2,
    drop_target: Option<usize>,

    // ── Callbacks ────────────────────────────────────────────────────────
    on_select: Option<SelectCallback<App>>,
    on_double_click: Option<SelectCallback<App>>,
}

impl<App> TreeView<App> {
    /// Crea un `TreeView` vacío.
    pub fn new() -> Self {
        Self {
            root: None,
            selected: Vec::new(),
            row_height: 24.0,
            indent_px: 16.0,
            multi_select: false,
            scroll_offset: 0.0,
            hovered_row: None,
            drag_row: None,
            drag_pos: Vec2::ZERO,
            drop_target: None,
            on_select: None,
            on_double_click: None,
        }
    }

    /// Establece el nodo raíz de datos.
    pub fn with_root(mut self, root: TreeNode) -> Self {
        self.root = Some(root);
        self
    }

    /// Altura de fila en píxeles (por defecto 24 px).
    pub fn row_height(mut self, h: f32) -> Self {
        self.row_height = h;
        self
    }

    /// Sangría por nivel (por defecto 16 px).
    pub fn indent_px(mut self, i: f32) -> Self {
        self.indent_px = i;
        self
    }

    /// Habilita selección múltiple con Ctrl+Click.
    pub fn multi_select(mut self, v: bool) -> Self {
        self.multi_select = v;
        self
    }

    /// Callback invocado al seleccionar (o deseleccionar) una fila.
    ///
    /// Recibe la ruta de índices hacia el nodo seleccionado.
    pub fn on_select<F>(mut self, f: F) -> Self
    where
        F: Fn(&mut EventContext<App>, &[usize]) + Send + Sync + 'static,
    {
        self.on_select = Some(Box::new(f));
        self
    }

    /// Callback para doble-click en una fila.
    pub fn on_double_click<F>(mut self, f: F) -> Self
    where
        F: Fn(&mut EventContext<App>, &[usize]) + Send + Sync + 'static,
    {
        self.on_double_click = Some(Box::new(f));
        self
    }

    // ── Helpers ──────────────────────────────────────────────────────────

    /// Construye la lista plana de los nodos visibles.
    fn flat_rows(&self) -> Vec<FlatRow> {
        let mut rows = Vec::new();
        if let Some(root) = &self.root {
            Self::flatten_node(root, 0, &[], &mut rows);
        }
        rows
    }

    fn flatten_node(node: &TreeNode, depth: usize, parent_path: &[usize], acc: &mut Vec<FlatRow>) {
        let path: Vec<usize> = parent_path.to_vec();
        let flat_index = acc.len();
        acc.push(FlatRow {
            depth,
            flat_index,
            path: path.clone(),
            label: node.label.clone(),
            icon: node.icon,
            has_children: !node.children.is_empty(),
            expanded: node.expanded,
            user_data: node.user_data,
        });

        if node.expanded {
            for (i, child) in node.children.iter().enumerate() {
                let mut child_path = path.clone();
                child_path.push(i);
                Self::flatten_node(child, depth + 1, &child_path, acc);
            }
        }
    }

    /// Navega la jerarquía de datos siguiendo la ruta de índices y devuelve
    /// una referencia mutable al `TreeNode` destino.
    fn get_node_mut<'a>(root: &'a mut TreeNode, path: &[usize]) -> Option<&'a mut TreeNode> {
        let mut current = root;
        for &idx in path {
            if idx < current.children.len() {
                current = &mut current.children[idx];
            } else {
                return None;
            }
        }
        Some(current)
    }

    /// Fila bajo la posición Y dada (relativa al widget).
    fn row_at_y(&self, y: f32) -> Option<usize> {
        let rel_y = y + self.scroll_offset;
        if rel_y < 0.0 {
            return None;
        }
        Some((rel_y / self.row_height) as usize)
    }
}

impl<App> Default for TreeView<App> {
    fn default() -> Self {
        Self::new()
    }
}

// ─── Widget impl ──────────────────────────────────────────────────────────────

impl<App: 'static + Send + Sync> Widget<App> for TreeView<App> {
    fn build(&mut self, ctx: &mut BuildContext<App>) {
        // El TreeView dibuja todo en draw() — no crea hijos en el árbol de UI.
        let style = StyleBuilder::new()
            .fill_width()
            .fill_height()
            .clip()
            .build();
        ctx.tree.set_node_style(ctx.node_id, style);
    }

    fn draw(&self, ctx: &mut DrawContext, cmds: &mut Vec<RenderCommand>) {
        let r = &ctx.rect;
        let theme = &ctx.theme;
        let rows = self.flat_rows();

        // Fondo
        cmds.push(RenderCommand::Quad {
            rect: *r,
            color: theme.surface.to_array(),
            radii: [0.0; 4],
            flags: 0,
        });

        let visible_start = (self.scroll_offset / self.row_height) as usize;
        let visible_count = (r.height / self.row_height).ceil() as usize + 1;

        for flat_row in rows.iter().skip(visible_start).take(visible_count) {
            let row_y = r.y + flat_row.flat_index as f32 * self.row_height - self.scroll_offset;
            let row_rect = Rect::new(r.x, row_y, r.width, self.row_height);

            // Fondo de selección / hover
            let is_selected = self.selected.contains(&flat_row.flat_index);
            let is_hovered = self.hovered_row == Some(flat_row.flat_index);
            let is_drop_target = self.drop_target == Some(flat_row.flat_index);

            if is_selected {
                cmds.push(RenderCommand::Quad {
                    rect: row_rect,
                    color: theme.primary.with_alpha(0.25).to_array(),
                    radii: [4.0; 4],
                    flags: 0,
                });
            } else if is_hovered {
                cmds.push(RenderCommand::Quad {
                    rect: row_rect,
                    color: theme.on_surface_muted.with_alpha(0.08).to_array(),
                    radii: [4.0; 4],
                    flags: 0,
                });
            }

            // Línea de drag-and-drop target
            if is_drop_target {
                cmds.push(RenderCommand::Quad {
                    rect: Rect::new(r.x, row_y, r.width, 2.0),
                    color: theme.primary.to_array(),
                    radii: [1.0; 4],
                    flags: 0,
                });
            }

            // Sangría
            let indent = r.x + flat_row.depth as f32 * self.indent_px + 4.0;

            // Icono expandir/colapsar (▶ / ▼)
            if flat_row.has_children {
                let toggle_char = if flat_row.expanded { "▼" } else { "▶" };
                cmds.push(RenderCommand::Text {
                    rect: Rect::new(indent, row_y, 16.0, self.row_height),
                    text: toggle_char.to_string(),
                    color: theme.on_surface_muted.to_array(),
                    font_size: theme.font_size_small,
                    align: crate::TextAlign::TOP_LEFT,
                });
            }

            // Icono del nodo
            let text_x = if flat_row.has_children {
                indent + 16.0
            } else {
                indent + 16.0
            };

            if let Some(icon) = flat_row.icon {
                cmds.push(RenderCommand::Text {
                    rect: Rect::new(text_x, row_y + 1.0, 18.0, self.row_height - 2.0),
                    text: icon.to_string(),
                    color: [1.0, 1.0, 1.0, 0.85],
                    font_size: theme.font_size_base,
                    align: crate::TextAlign::TOP_LEFT,
                });
            }

            let label_x = if flat_row.icon.is_some() {
                text_x + 20.0
            } else {
                text_x
            };

            // Etiqueta del nodo
            let text_color = if is_selected {
                theme.on_surface
            } else {
                theme.on_surface_muted
            };

            // Nodo siendo arrastrado: semitransparente
            let alpha_mult = if self.drag_row == Some(flat_row.flat_index) { 0.4 } else { 1.0 };
            let [r_c, g_c, b_c, a_c] = text_color.to_array();
            let text_color_arr = [r_c, g_c, b_c, a_c * alpha_mult];

            cmds.push(RenderCommand::Text {
                rect: Rect::new(label_x, row_y + 1.0, r.width - (label_x - r.x) - 8.0, self.row_height - 2.0),
                text: flat_row.label.clone(),
                color: text_color_arr,
                font_size: theme.font_size_base,
                align: crate::TextAlign::TOP_LEFT,
            });
        }

        // Ghost del nodo arrastrado
        if let Some(_drag_idx) = self.drag_row {
            cmds.push(RenderCommand::Quad {
                rect: Rect::new(
                    self.drag_pos.x - 60.0, self.drag_pos.y - self.row_height / 2.0,
                    120.0, self.row_height,
                ),
                color: theme.primary.with_alpha(0.5).to_array(),
                radii: [6.0; 4],
                flags: 0,
            });
        }
    }

    fn calculate_size(&self, ctx: &mut LayoutContext) -> Vec2 {
        let rows = self.flat_rows();
        Vec2::new(
            ctx.available_space.x,
            rows.len() as f32 * self.row_height,
        )
    }

    fn on_event(&mut self, ctx: &mut EventContext<App>, event: &UiEvent) -> EventResponse {
        match event {
            UiEvent::MouseMove { pos } => {
                let local_y = pos.y - ctx.rect.y;
                self.hovered_row = self.row_at_y(local_y);

                if let Some(drag_row) = self.drag_row {
                    // Actualizar posición drag ghost
                    self.drag_pos = *pos;
                    // Calcular drop target
                    self.drop_target = self.row_at_y(local_y).filter(|&r| r != drag_row);
                    return EventResponse::Redraw;
                }
                EventResponse::Redraw
            }

            UiEvent::MouseLeave => {
                self.hovered_row = None;
                EventResponse::Redraw
            }

            UiEvent::MouseDown { pos, .. } => {
                let local_y = pos.y - ctx.rect.y;
                let rows = self.flat_rows();
                let clicked_row = match self.row_at_y(local_y) {
                    Some(r) if r < rows.len() => r,
                    _ => return EventResponse::Ignored,
                };

                let flat_row = &rows[clicked_row];
                let local_x = pos.x - ctx.rect.x;
                let indent = flat_row.depth as f32 * self.indent_px + 4.0;

                // Click en el triángulo toggle (±4px alrededor del icono)
                if flat_row.has_children && local_x >= indent && local_x <= indent + 20.0 {
                    // Togglear expanded en la jerarquía de datos
                    if let Some(root) = &mut self.root {
                        if let Some(node) = Self::get_node_mut(root, &flat_row.path) {
                            node.expanded = !node.expanded;
                        }
                    }
                    return EventResponse::Redraw;
                }

                // Selección
                self.selected = vec![clicked_row];

                // Iniciar drag
                self.drag_row = Some(clicked_row);
                self.drag_pos = *pos;

                // Invocar callback on_select
                if let Some(cb) = &self.on_select {
                    let path = flat_row.path.clone();
                    cb(ctx, &path);
                }

                EventResponse::Consumed
            }

            UiEvent::MouseUp { .. } => {
                if let Some(drag_idx) = self.drag_row.take() {
                    if let Some(drop_idx) = self.drop_target.take() {
                        // Drag & drop: mover nodo drag_idx → bajo drop_idx
                        // (se requiere lógica de reparentado: aquí lo simplificamos)
                        let rows = self.flat_rows();
                        if let (Some(src_row), Some(_dst_row)) = (
                            rows.get(drag_idx),
                            rows.get(drop_idx),
                        ) {
                            let _src_path = src_row.path.clone();
                            // TODO: implementar reparentado completo
                            // Por ahora solo marcamos paint dirty
                            let _ = src_row;
                        }
                    }
                    return EventResponse::Redraw;
                }
                EventResponse::Ignored
            }

            UiEvent::MouseWheel { delta_y, .. } => {
                let rows_count = self.flat_rows().len();
                let content_h = rows_count as f32 * self.row_height;
                let viewport_h = ctx.rect.height;
                let max_scroll = (content_h - viewport_h).max(0.0);
                self.scroll_offset = (self.scroll_offset - delta_y * 40.0).clamp(0.0, max_scroll);
                EventResponse::Redraw
            }

            _ => EventResponse::Ignored,
        }
    }

    fn scroll_offset(&self) -> Vec2 {
        Vec2::new(0.0, self.scroll_offset)
    }
}
