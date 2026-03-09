//! # `VirtualGrid` — Cuadrícula Virtualizada de Alto Rendimiento
//!
//! Equivalente 2D de [`VirtualList`]. Renderiza solamente las celdas visibles
//! en el viewport, permitiendo galerías y grids de **miles de items** sin
//! degradación de rendimiento.
//!
//! Casos de uso típicos: galería de sprites, selector de texturas, galería
//! de assets en un editor de juegos, spritesheet picker.
//!
//! ## Ejemplo de uso
//!
//! ```rust,ignore
//! use ferrous_ui_core::VirtualGrid;
//!
//! let gallery = VirtualGrid::<MyApp>::new(total_assets, 128.0, 128.0)
//!     .columns(4)                      // cuatro columnas fijas
//!     .gap(8.0)                        // separación entre celdas
//!     .on_render_cell(|ctx, col, row, rect, cmds| {
//!         let idx = row * 4 + col;
//!         // Dibujar la miniatura del asset
//!         cmds.push(RenderCommand::Text {
//!             rect,
//!             text: format!("asset_{:04}", idx),
//!             color: ctx.theme.on_surface.to_array(),
//!             font_size: ctx.theme.font_size_small,
//!         });
//!     })
//!     .on_select(|ctx, idx| {
//!         ctx.app.selected_asset = idx;
//!     });
//!
//! tree.add_node(Box::new(gallery), Some(panel_id));
//! ```
//!
//! ## Arquitectura interna
//!
//! ```
//! VirtualGrid (columns=4, cell=128×128, gap=8)
//! ─────────────────────────────────────────────────────
//!  row=12, visible given scroll_offset = 3200px
//!
//!  visible_row_start = floor(3200 / (128+8)) = 22
//!  visible_row_end   = ceil((3200 + vp_h) / 136) = 27
//!
//!  Para cada (col, row) visible: rect calculado en tiempo real.
//!  O(viewport_rows × columns) celdas pintadas, nunca O(total_items).
//! ```

use crate::{
    Widget, RenderCommand, DrawContext, BuildContext, LayoutContext, EventContext,
    EventResponse, UiEvent, Rect, Vec2, StyleBuilder, StyleExt,
};

// ─── Callbacks ────────────────────────────────────────────────────────────────

type RenderCellFn<App> = Box<
    dyn Fn(&mut EventContext<App>, usize /*col*/, usize /*row*/, Rect, &mut Vec<RenderCommand>) + Send + Sync,
>;
type CellSelectFn<App> = Box<dyn Fn(&mut EventContext<App>, usize /*flat_index*/) + Send + Sync>;

// ─── VirtualGrid ──────────────────────────────────────────────────────────────

/// Cuadrícula virtualizada: solo pinta las celdas visibles en el viewport.
///
/// Consulta la [documentación del módulo][self] para el uso completo.
pub struct VirtualGrid<App> {
    /// Total de items en la fuente de datos.
    pub item_count: usize,
    /// Ancho de cada celda en píxeles.
    pub cell_width: f32,
    /// Alto de cada celda en píxeles.
    pub cell_height: f32,
    /// Número de columnas fijas (`0` = calculado automáticamente para llenar el ancho).
    pub columns: usize,
    /// Separación entre celdas en píxeles.
    pub gap: f32,
    /// Padding interno del grid (alrededor del borde).
    pub padding: f32,

    // ── Estado ───────────────────────────────────────────────────────────
    pub selected: Vec<usize>,
    pub scroll_offset: f32,
    hovered_index: Option<usize>,

    // ── Callbacks ────────────────────────────────────────────────────────
    render_cell: Option<RenderCellFn<App>>,
    on_select: Option<CellSelectFn<App>>,
}

impl<App> VirtualGrid<App> {
    /// Crea un `VirtualGrid` con `item_count` items de dimensiones `cell_w × cell_h`.
    pub fn new(item_count: usize, cell_width: f32, cell_height: f32) -> Self {
        Self {
            item_count,
            cell_width,
            cell_height,
            columns: 0, // auto
            gap: 4.0,
            padding: 8.0,
            selected: Vec::new(),
            scroll_offset: 0.0,
            hovered_index: None,
            render_cell: None,
            on_select: None,
        }
    }

    /// Define el número de columnas. `0` → calcula automáticamente.
    pub fn columns(mut self, c: usize) -> Self {
        self.columns = c;
        self
    }

    /// Separación entre celdas (por defecto 4 px).
    pub fn gap(mut self, g: f32) -> Self {
        self.gap = g;
        self
    }

    /// Padding exterior del grid (por defecto 8 px).
    pub fn padding(mut self, p: f32) -> Self {
        self.padding = p;
        self
    }

    /// Callback responsable de pintar el contenido de cada celda.
    pub fn on_render_cell<F>(mut self, f: F) -> Self
    where
        F: Fn(&mut EventContext<App>, usize, usize, Rect, &mut Vec<RenderCommand>) + Send + Sync + 'static,
    {
        self.render_cell = Some(Box::new(f));
        self
    }

    /// Callback invocado al hacer clic en una celda.
    pub fn on_select<F>(mut self, f: F) -> Self
    where
        F: Fn(&mut EventContext<App>, usize) + Send + Sync + 'static,
    {
        self.on_select = Some(Box::new(f));
        self
    }

    // ── Helpers ──────────────────────────────────────────────────────────

    /// Columnas efectivas dado el ancho disponible.
    fn effective_columns(&self, available_width: f32) -> usize {
        if self.columns > 0 {
            return self.columns;
        }
        let cols = ((available_width - self.padding * 2.0 + self.gap)
            / (self.cell_width + self.gap))
            .floor() as usize;
        cols.max(1)
    }

    /// Alto total del contenido (todos los ítems).
    fn content_height(&self, cols: usize) -> f32 {
        if cols == 0 { return 0.0; }
        let rows = (self.item_count + cols - 1) / cols;
        self.padding * 2.0 + rows as f32 * (self.cell_height + self.gap) - self.gap
    }

    /// Índice plano a partir de la posición de cursor (local al widget).
    fn index_at_pos(&self, local_x: f32, local_y: f32, cols: usize) -> Option<usize> {
        let inner_x = local_x - self.padding;
        let inner_y = local_y + self.scroll_offset - self.padding;
        if inner_x < 0.0 || inner_y < 0.0 {
            return None;
        }
        let col = (inner_x / (self.cell_width + self.gap)) as usize;
        let row = (inner_y / (self.cell_height + self.gap)) as usize;
        // Verificar que el cursor está dentro de la celda (no en el gap)
        let cell_local_x = inner_x % (self.cell_width + self.gap);
        let cell_local_y = inner_y % (self.cell_height + self.gap);
        if cell_local_x > self.cell_width || cell_local_y > self.cell_height {
            return None;
        }
        if col >= cols {
            return None;
        }
        let idx = row * cols + col;
        if idx < self.item_count { Some(idx) } else { None }
    }
}

impl<App> Default for VirtualGrid<App> {
    fn default() -> Self {
        Self::new(0, 128.0, 128.0)
    }
}

impl<App: 'static + Send + Sync> Widget<App> for VirtualGrid<App> {
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
        let cols = self.effective_columns(r.width);

        // Fondo
        cmds.push(RenderCommand::Quad {
            rect: r,
            color: theme.surface.to_array(),
            radii: [0.0; 4],
            flags: 0,
        });

        let cell_stride_y = self.cell_height + self.gap;
        let cell_stride_x = self.cell_width + self.gap;

        let first_visible_row = (self.scroll_offset / cell_stride_y) as usize;
        let last_visible_row =
            ((self.scroll_offset + r.height - self.padding) / cell_stride_y).ceil() as usize + 1;

        for row in first_visible_row..last_visible_row {
            for col in 0..cols {
                let flat_idx = row * cols + col;
                if flat_idx >= self.item_count {
                    break;
                }

                let cell_x = r.x + self.padding + col as f32 * cell_stride_x;
                let cell_y = r.y + self.padding + row as f32 * cell_stride_y - self.scroll_offset;
                let cell_rect = Rect::new(cell_x, cell_y, self.cell_width, self.cell_height);

                // Fondo de celda
                let is_selected = self.selected.contains(&flat_idx);
                let is_hovered = self.hovered_index == Some(flat_idx);

                let bg_color = if is_selected {
                    theme.primary.with_alpha(0.35).to_array()
                } else if is_hovered {
                    theme.surface_elevated.to_array()
                } else {
                    theme.surface_elevated.with_alpha(0.7).to_array()
                };

                cmds.push(RenderCommand::Quad {
                    rect: cell_rect,
                    color: bg_color,
                    radii: [theme.border_radius; 4],
                    flags: 0,
                });

                // Borde de selección
                if is_selected {
                    // Borde interior ~2px
                    cmds.push(RenderCommand::Quad {
                        rect: Rect::new(cell_rect.x + 1.0, cell_rect.y + 1.0, cell_rect.width - 2.0, 2.0),
                        color: theme.primary.to_array(),
                        radii: [theme.border_radius; 4],
                        flags: 0,
                    });
                }

                // Contenido por defecto: índice centrado
                cmds.push(RenderCommand::Text {
                    rect: Rect::new(
                        cell_rect.x + 4.0,
                        cell_rect.y + cell_rect.height / 2.0 - 8.0,
                        cell_rect.width - 8.0,
                        16.0,
                    ),
                    text: format!("{}", flat_idx),
                    color: if is_selected {
                        theme.on_surface.to_array()
                    } else {
                        theme.on_surface_muted.to_array()
                    },
                    font_size: theme.font_size_small,
                });
            }
        }

        // Scrollbar vertical
        let content_h = self.content_height(cols);
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
        Vec2::new(ctx.available_space.x, ctx.available_space.y)
    }

    fn on_event(&mut self, ctx: &mut EventContext<App>, event: &UiEvent) -> EventResponse {
        let cols = self.effective_columns(ctx.rect.width);

        match event {
            UiEvent::MouseMove { pos } => {
                let local_x = pos.x - ctx.rect.x;
                let local_y = pos.y - ctx.rect.y;
                let new_hovered = self.index_at_pos(local_x, local_y, cols);
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
                let local_x = pos.x - ctx.rect.x;
                let local_y = pos.y - ctx.rect.y;
                if let Some(idx) = self.index_at_pos(local_x, local_y, cols) {
                    self.selected = vec![idx];
                    if let Some(cb) = &self.on_select {
                        cb(ctx, idx);
                    }
                    return EventResponse::Redraw;
                }
                EventResponse::Ignored
            }

            UiEvent::MouseWheel { delta_y, .. } => {
                let content_h = self.content_height(cols);
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
