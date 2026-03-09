//! # `Tabs` — Widget de Pestañas con Contenido Lazy
//!
//! Contenedor de navegación por pestañas. Solo renderiza el contenido de la pestaña activa
//! (lazy rendering), optimizando CPU y memoria al no mantener en el árbol los widgets inactivos.
//!
//! ## Ejemplo de uso
//!
//! ```rust,ignore
//! use ferrous_ui_core::{Tabs, Label, Panel};
//!
//! let tabs = Tabs::<MyApp>::new()
//!     .add_tab("General",  Box::new(Label::new("Configuración general")))
//!     .add_tab("Avanzado", Box::new(Label::new("Opciones avanzadas")));
//!
//! tree.add_node(Box::new(tabs), Some(root_id));
//! ```
//!
//! ## Arquitectura interna
//!
//! | Capa | Descripción |
//! |------|-------------|
//! | Barra de cabecera | Fila de `TabButton` — botones ligeros que emiten click |
//! | Área de contenido | Solo contiene el widget de la pestaña activa (lazy) |
//!
//! Al cambiar de pestaña, `on_event` reemplaza el único hijo del área de contenido
//! con el widget correspondiente al nuevo índice activo.

use crate::{
    Widget, RenderCommand, DrawContext, BuildContext, LayoutContext, EventContext,
    EventResponse, UiEvent, Rect, Vec2, NodeId, Color, StyleBuilder, StyleExt,
};

// ─── TabButton (interna) ─────────────────────────────────────────────────────

/// Botón de cabecera usado internamente por [`Tabs`].
struct TabButton {
    title: String,
    /// Índice lógico de esta pestaña. Usado para discriminar clicks en `Tabs::on_event`.
    pub tab_index: usize,
    /// Estado visual: true si es la pestaña activa.
    pub is_active: bool,
}

impl TabButton {
    fn new(title: String, tab_index: usize, is_active: bool) -> Self {
        Self { title, tab_index, is_active }
    }
}

impl<App> Widget<App> for TabButton {
    fn draw(&self, ctx: &mut DrawContext, cmds: &mut Vec<RenderCommand>) {
        let r = &ctx.rect;
        let theme = &ctx.theme;

        let bg = if self.is_active {
            theme.primary.with_alpha(0.25)
        } else {
            theme.surface_elevated.with_alpha(0.1)
        };

        cmds.push(RenderCommand::Quad {
            rect: *r,
            color: bg.to_array(),
            radii: [theme.border_radius, theme.border_radius, 0.0, 0.0],
            flags: 0,
        });

        // Línea inferior activa
        if self.is_active {
            cmds.push(RenderCommand::Quad {
                rect: Rect::new(r.x + 4.0, r.y + r.height - 2.0, r.width - 8.0, 2.0),
                color: theme.primary.to_array(),
                radii: [1.0; 4],
                flags: 0,
            });
        }

        // Texto
        let text_color = if self.is_active {
            theme.on_surface
        } else {
            theme.on_surface_muted
        };

        cmds.push(RenderCommand::Text {
            rect: Rect::new(r.x + 10.0, r.y + 2.0, r.width - 20.0, r.height - 4.0),
            text: self.title.clone(),
            color: text_color.to_array(),
            font_size: theme.font_size_base,
        });
    }

    fn calculate_size(&self, _ctx: &mut LayoutContext) -> Vec2 {
        glam::vec2(80.0 + self.title.len() as f32 * 6.5, 35.0)
    }
}

// ─── Tabs ────────────────────────────────────────────────────────────────────

/// Widget de navegación que organiza contenido en pestañas.
///
/// Ver [documentación del módulo][self] para más detalles.
pub struct Tabs<App> {
    /// Índice de la pestaña visible actualmente.
    pub active_index: usize,

    // Almacenamiento: (título, widget de contenido).
    // Los widgets de pestañas inactivas permanecen aquí fuera del árbol.
    panels: Vec<(String, Option<Box<dyn Widget<App>>>)>,

    // IDs de nodos en el árbol (se rellenan en `build`).
    header_id: Option<NodeId>,
    content_area_id: Option<NodeId>,
    tab_btn_ids: Vec<NodeId>,
}

impl<App> Tabs<App> {
    /// Construye un `Tabs` vacío.
    pub fn new() -> Self {
        Self {
            active_index: 0,
            panels: Vec::new(),
            header_id: None,
            content_area_id: None,
            tab_btn_ids: Vec::new(),
        }
    }

    /// Añade una pestaña.
    ///
    /// - `title`: texto del botón de cabecera.
    /// - `content`: widget que se renderizará cuando la pestaña esté activa.
    pub fn add_tab(mut self, title: impl Into<String>, content: Box<dyn Widget<App>>) -> Self {
        self.panels.push((title.into(), Some(content)));
        self
    }

    /// Establece el índice inicial de la pestaña activa (por defecto `0`).
    pub fn with_active(mut self, index: usize) -> Self {
        self.active_index = index;
        self
    }
}

impl<App> Default for Tabs<App> {
    fn default() -> Self {
        Self::new()
    }
}

impl<App: 'static + Send + Sync> Widget<App> for Tabs<App> {
    fn build(&mut self, ctx: &mut BuildContext<App>) {
        let my_id = ctx.node_id;

        // Layout raíz: columna [header | content]
        let root_style = StyleBuilder::new().column().fill_width().fill_height().build();
        ctx.tree.set_node_style(my_id, root_style);

        // ── Barra de cabecera ─────────────────────────────────────────────
        let header_style = StyleBuilder::new().row().fill_width().height_px(36.0).build();
        let header_panel = Box::new(crate::widgets::Panel::new()
            .with_color(Color::hex("#1A1A2E"))
            .with_radius(0.0));
        let header_id = ctx.tree.add_node(header_panel, Some(my_id));
        ctx.tree.set_node_style(header_id, header_style);
        self.header_id = Some(header_id);

        // Botones de pestaña
        self.tab_btn_ids.clear();
        for (i, (title, _)) in self.panels.iter().enumerate() {
            let is_active = i == self.active_index;
            let btn = Box::new(TabButton::new(title.clone(), i, is_active));
            let btn_style = StyleBuilder::new().height_px(36.0).build();
            let btn_id = ctx.tree.add_node(btn, Some(header_id));
            ctx.tree.set_node_style(btn_id, btn_style);
            self.tab_btn_ids.push(btn_id);
        }

        // ── Área de contenido ─────────────────────────────────────────────
        let content_style = StyleBuilder::new()
            .column()
            .fill_width()
            .flex(1.0)
            .padding_all(8.0)
            .build();
        let content_bg = Box::new(crate::widgets::Panel::new()
            .with_color(Color::hex("#181825")));
        let content_area_id = ctx.tree.add_node(content_bg, Some(my_id));
        ctx.tree.set_node_style(content_area_id, content_style);
        self.content_area_id = Some(content_area_id);

        // Insertar solo el widget de la pestaña activa (lazy)
        self.insert_active_content(ctx.tree, content_area_id);
    }

    fn draw(&self, _ctx: &mut DrawContext, _cmds: &mut Vec<RenderCommand>) {
        // El Panel de fondo ya cubre el fondo del widget raíz.
    }

    fn calculate_size(&self, _ctx: &mut LayoutContext) -> Vec2 {
        Vec2::ZERO // Tamaño dado por el padre (fill)
    }

    fn on_event(
        &mut self,
        ctx: &mut EventContext<App>,
        event: &UiEvent,
    ) -> EventResponse {
        if let UiEvent::MouseDown { pos, .. } = event {
            for (i, &btn_id) in self.tab_btn_ids.iter().enumerate() {
                if let Some(rect) = ctx.tree.get_node_rect(btn_id) {
                    if !rect.contains([pos.x, pos.y]) || i == self.active_index {
                        continue;
                    }

                    // ── Cambiar a pestaña i ──────────────────────────────
                    let old_active = self.active_index;
                    self.active_index = i;

                    // Actualizar estado visual de los botones (sin downcast)
                    self.update_button_state(ctx.tree, old_active, i);

                    // Reemplazar el contenido activo
                    if let Some(area_id) = self.content_area_id {
                        // Extraer el widget del área de contenido
                        self.swap_active_content(ctx.tree, area_id, old_active);
                    }

                    return EventResponse::Redraw;
                }
            }
        }
        EventResponse::Ignored
    }
}

impl<App: 'static + Send + Sync> Tabs<App> {
    /// Inserta el widget de la pestaña activa como único hijo de `content_area_id`.
    fn insert_active_content(&mut self, tree: &mut crate::UiTree<App>, area_id: NodeId) {
        if let Some((_, widget_opt)) = self.panels.get_mut(self.active_index) {
            if let Some(widget) = widget_opt.take() {
                let child_style = StyleBuilder::new().fill_width().fill_height().build();
                let child_id = tree.add_node(widget, Some(area_id));
                tree.set_node_style(child_id, child_style);
            }
        }
    }

    fn update_button_state(
        &self,
        tree: &mut crate::UiTree<App>,
        old_i: usize,
        new_i: usize,
    ) {
        // Reemplazar los TabButton afectados con versiones actualizadas (evita downcast)
        for (j, &btn_id) in self.tab_btn_ids.iter().enumerate() {
            if j == old_i || j == new_i {
                let title = self.panels.get(j)
                    .map(|(t, _)| t.clone())
                    .unwrap_or_default();
                let is_active = j == new_i;
                let new_btn = Box::new(TabButton::new(title, j, is_active));
                if let Some(node) = tree.get_node_mut(btn_id) {
                    node.widget = new_btn;
                    node.dirty.paint = true;
                    node.dirty.subtree_dirty = true;
                }
            }
        }
    }


    /// Elimina el hijo del área de contenido (devolviendo el widget al pool interno)
    /// e inserta el widget de la nueva pestaña activa.
    fn swap_active_content(
        &mut self,
        tree: &mut crate::UiTree<App>,
        area_id: NodeId,
        old_index: usize,
    ) {
        // Obtener el hijo actual del área de contenido (debe ser exactamente 1)
        let children = tree.get_node_children(area_id)
            .map(|c| c.to_vec())
            .unwrap_or_default();

        // Extraer el widget del hijo actual y devolverlo al pool
        if let Some(&old_child_id) = children.first() {
            // Extraemos el widget con un placeholder
            let placeholder = Box::new(crate::widgets::PlaceholderWidget);
            if let Some(node) = tree.get_node_mut(old_child_id) {
                let recovered = std::mem::replace(&mut node.widget, placeholder);
                // Devolver al slot del panel
                if let Some((_, slot)) = self.panels.get_mut(old_index) {
                    *slot = Some(recovered);
                }
            }
            // Eliminar el nodo del árbol (simplificado: lo marcamos como placeholder)
            // Un árbol real necesitaría un método remove_node. Por ahora lo dejamos como
            // placeholder invisible (tamaño cero): no genera comandos de render.
            if let Some(node) = tree.get_node_mut(old_child_id) {
                node.style.size = (crate::Units::Px(0.0), crate::Units::Px(0.0));
                node.dirty.paint = true;
            }
        }

        // Insertar el nuevo widget activo
        self.insert_active_content(tree, area_id);
        tree.mark_paint_dirty(area_id);
    }
}
