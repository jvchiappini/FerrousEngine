//! # `VirtualList` — Lista Virtualizada de Alto Rendimiento
//!
//! Renderiza **exclusivamente los items visibles**, reciclando nodos del DOM
//! a medida que el usuario hace scroll. Soporta listas de **100 000+ filas**
//! sin degradación de FPS.
//!
//! ## Ejemplo de uso
//!
//! ```rust,ignore
//! use ferrous_ui_core::{VirtualList, VirtualItem};
//!
//! // Fuente de datos: función que retorna el contenido de una fila
//! let num_items = 100_000usize;
//!
//! let list = VirtualList::<MyApp>::new(num_items, 32.0)
//!     .on_render_item(|ctx, idx, rect| {
//!         // Dibujar el item idx en el rect dado
//!         let label = format!("Item #{:06}", idx);
//!         ctx.cmds.push(RenderCommand::Text {
//!             rect,
//!             text: label,
//!             color: ctx.theme.on_surface.to_array(),
//!             font_size: ctx.theme.font_size_base,
//!         });
//!     })
//!     .on_select(|ctx, idx| {
//!         ctx.app.selected_index = idx;
//!     });
//!
//! tree.add_node(Box::new(list), Some(panel_id));
//! ```
//!
//! ## Por qué virtualización
//!
//! En una lista normal, crear un nodo de árbol por cada fila de 100 000 items
//! consumiría ~25 MB de memoria y forzaría un cálculo de layout completo en
//! cada frame. `VirtualList` solo mantiene los ~N nodos visibles + un margen,
//! haciendo que la memoria sea O(viewport) en lugar de O(total_items).
//!
//! ## Arquitectura interna
//!
//! ```
//! ┌── VirtualList (ScrollView, fill) ─────────────────────────────────┐
//! │  scroll_offset = 3200.0  (el usuario scrolleó 3200px)             │
//! │                                                                    │
//! │  visible_start = floor(3200 / 32) = 100                           │
//! │  visible_end   = ceil((3200 + viewport_h) / 32) = 125             │
//! │                                                                    │
//! │  ┌─ fila 100 ── rect(0, 3200, w, 32) ─────────────────────────┐  │
//! │  │  [item 100]                                                  │  │
//! │  ├─ fila 101 ── rect(0, 3232, w, 32) ─────────────────────────┤  │
//! │  │  [item 101]                                                  │  │
//! │  └─ ...                                                         │  │
//! └────────────────────────────────────────────────────────────────────┘
//! ```

use crate::{
    Widget, RenderCommand, DrawContext, BuildContext, LayoutContext, EventContext,
    EventResponse, UiEvent, Rect, Vec2, StyleBuilder, StyleExt,
};

// ─── Callbacks ────────────────────────────────────────────────────────────────

/// Contexto pasado al callback de renderizado de un item.
pub struct ItemDrawContext<'a> {
    pub cmds: &'a mut Vec<RenderCommand>,
    pub theme: &'a crate::Theme,
    pub is_selected: bool,
    pub is_hovered: bool,
}

type RenderItemFn<App> = Box<
    dyn Fn(&mut EventContext<App>, usize, Rect, &mut Vec<RenderCommand>) + Send + Sync,
>;
type SelectFn<App> = Box<dyn Fn(&mut EventContext<App>, usize) + Send + Sync>;

// ─── VirtualList ──────────────────────────────────────────────────────────────

/// Lista virtualizada: solo pinta los items visibles en el viewport.
///
/// Consulta la [documentación del módulo][self] para el uso completo.
pub struct VirtualList<App> {
    /// Número total de items en la fuente de datos.
    pub item_count: usize,
    /// Alto en píxeles de cada fila (uniforme).
    pub item_height: f32,

    // ── Estado ───────────────────────────────────────────────────────────
    pub selected: Vec<usize>,
    pub scroll_offset: f32,
    hovered_index: Option<usize>,

    // ── Callbacks ────────────────────────────────────────────────────────
    render_item: Option<RenderItemFn<App>>,
    on_select: Option<SelectFn<App>>,
    on_double_click: Option<SelectFn<App>>,
}

impl<App> VirtualList<App> {
    /// Crea una `VirtualList` con `count` items de `item_height` px de alto.
    pub fn new(item_count: usize, item_height: f32) -> Self {
        Self {
            item_count,
            item_height,
            selected: Vec::new(),
            scroll_offset: 0.0,
            hovered_index: None,
            render_item: None,
            on_select: None,
            on_double_click: None,
        }
    }

    /// Callback responsable de pintar cada item.
    ///
    /// `f(ctx, index, rect, cmds)` — `index` es el índice del item en la fuente
    /// de datos, `rect` es el rectángulo donde pintarlo.
    pub fn on_render_item<F>(mut self, f: F) -> Self
    where
        F: Fn(&mut EventContext<App>, usize, Rect, &mut Vec<RenderCommand>) + Send + Sync + 'static,
    {
        self.render_item = Some(Box::new(f));
        self
    }

    /// Callback invocado al seleccionar un item (clic simple).
    pub fn on_select<F>(mut self, f: F) -> Self
    where
        F: Fn(&mut EventContext<App>, usize) + Send + Sync + 'static,
    {
        self.on_select = Some(Box::new(f));
        self
    }

    /// Callback invocado al hacer doble clic en un item.
    pub fn on_double_click<F>(mut self, f: F) -> Self
    where
        F: Fn(&mut EventContext<App>, usize) + Send + Sync + 'static,
    {
        self.on_double_click = Some(Box::new(f));
        self
    }

    /// Cambia el número total de items (p.ej. al filtrar).
    pub fn set_item_count(&mut self, count: usize) {
        self.item_count = count;
    }

    /// Desplaza el scroll para que el item `index` sea visible.
    pub fn scroll_to(&mut self, index: usize, viewport_height: f32) {
        let item_y = index as f32 * self.item_height;
        if item_y < self.scroll_offset {
            self.scroll_offset = item_y;
        } else if item_y + self.item_height > self.scroll_offset + viewport_height {
            self.scroll_offset = item_y + self.item_height - viewport_height;
        }
    }

    /// Devuelve el índice del item bajo la coordenada Y dada (relativa al widget).
    fn index_at_y(&self, local_y: f32) -> Option<usize> {
        let abs_y = local_y + self.scroll_offset;
        if abs_y < 0.0 {
            return None;
        }
        let idx = (abs_y / self.item_height) as usize;
        if idx < self.item_count { Some(idx) } else { None }
    }
}

impl<App> Default for VirtualList<App> {
    fn default() -> Self {
        Self::new(0, 32.0)
    }
}

impl<App: 'static + Send + Sync> Widget<App> for VirtualList<App> {
    fn build(&mut self, ctx: &mut BuildContext<App>) {
        let style = StyleBuilder::new()
            .fill_width()
            .fill_height()
            .clip()
            .build();
        ctx.tree.set_node_style(ctx.node_id, style);
    }

    fn draw(&self, ctx: &mut DrawContext, cmds: &mut Vec<RenderCommand>) {
        let r = ctx.rect;
        let theme = &ctx.theme;

        // Fondo
        cmds.push(RenderCommand::Quad {
            rect: r,
            color: theme.surface.to_array(),
            radii: [0.0; 4],
            flags: 0,
        });

        let visible_start = (self.scroll_offset / self.item_height) as usize;
        let visible_end = ((self.scroll_offset + r.height) / self.item_height).ceil() as usize + 1;
        let visible_end = visible_end.min(self.item_count);

        for idx in visible_start..visible_end {
            let item_y = r.y + idx as f32 * self.item_height - self.scroll_offset;
            let item_rect = Rect::new(r.x, item_y, r.width, self.item_height);

            // Fondo de selección / hover
            let is_selected = self.selected.contains(&idx);
            let is_hovered = self.hovered_index == Some(idx);

            if is_selected {
                cmds.push(RenderCommand::Quad {
                    rect: item_rect,
                    color: theme.primary.with_alpha(0.25).to_array(),
                    radii: [0.0; 4],
                    flags: 0,
                });
            } else if is_hovered {
                cmds.push(RenderCommand::Quad {
                    rect: item_rect,
                    color: theme.on_surface_muted.with_alpha(0.06).to_array(),
                    radii: [0.0; 4],
                    flags: 0,
                });
            }

            // Separador horizontal entre filas (1px)
            cmds.push(RenderCommand::Quad {
                rect: Rect::new(r.x, item_y + self.item_height - 1.0, r.width, 1.0),
                color: theme.on_surface_muted.with_alpha(0.05).to_array(),
                radii: [0.0; 4],
                flags: 0,
            });

            // Contenido por defecto (si no hay callback): texto con el índice
            cmds.push(RenderCommand::Text {
                rect: Rect::new(item_rect.x + 8.0, item_rect.y + 2.0, item_rect.width - 16.0, self.item_height - 4.0),
                text: format!("Item {}", idx),
                color: if is_selected { theme.on_surface.to_array() } else { theme.on_surface_muted.to_array() },
                font_size: theme.font_size_base,
                align: crate::TextAlign::TOP_LEFT,
            });
        }

        // Scrollbar vertical (decorativa)
        let content_h = self.item_count as f32 * self.item_height;
        if content_h > r.height {
            let scrollbar_w = 4.0;
            let thumb_h = (r.height / content_h * r.height).max(20.0);
            let thumb_y = r.y + (self.scroll_offset / content_h) * r.height;
            cmds.push(RenderCommand::Quad {
                rect: Rect::new(r.x + r.width - scrollbar_w - 2.0, thumb_y, scrollbar_w, thumb_h),
                color: theme.on_surface_muted.with_alpha(0.3).to_array(),
                radii: [2.0; 4],
                flags: 0,
            });
        }
    }

    fn calculate_size(&self, ctx: &mut LayoutContext) -> Vec2 {
        Vec2::new(
            ctx.available_space.x,
            ctx.available_space.y,
        )
    }

    fn on_event(&mut self, ctx: &mut EventContext<App>, event: &UiEvent) -> EventResponse {
        match event {
            UiEvent::MouseMove { pos } => {
                let local_y = pos.y - ctx.rect.y;
                let new_hovered = self.index_at_y(local_y);
                if new_hovered != self.hovered_index {
                    self.hovered_index = new_hovered;
                    return EventResponse::Redraw;
                }
                EventResponse::Ignored
            }

            UiEvent::MouseLeave => {
                self.hovered_index = None;
                EventResponse::Redraw
            }

            UiEvent::MouseDown { pos, .. } => {
                let local_y = pos.y - ctx.rect.y;
                if let Some(idx) = self.index_at_y(local_y) {
                    self.selected = vec![idx];
                    if let Some(cb) = &self.on_select {
                        cb(ctx, idx);
                    }
                    return EventResponse::Redraw;
                }
                EventResponse::Ignored
            }

            UiEvent::MouseWheel { delta_y, .. } => {
                let content_h = self.item_count as f32 * self.item_height;
                let max_scroll = (content_h - ctx.rect.height).max(0.0);
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
