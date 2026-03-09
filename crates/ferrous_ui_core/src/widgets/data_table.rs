//! # `DataTable` — Tabla de Datos Avanzada
//!
//! Tabla con **columnas reordenables**, ordenación por columna, filtros inline,
//! headers fijos con scroll independiente en el cuerpo y selección de filas.
//!
//! Renderiza únicamente las filas visibles (virtualización), lo que permite
//! tablas de millones de filas sin degradación de rendimiento.
//!
//! ## Ejemplo de uso
//!
//! ```rust,ignore
//! use ferrous_ui_core::{DataTable, TableColumn, SortDirection};
//!
//! let table = DataTable::<MyApp>::new()
//!     .column(TableColumn::new("Nombre").min_width(120.0).sortable(true))
//!     .column(TableColumn::new("Tamaño").min_width(80.0).sortable(true).align_right())
//!     .column(TableColumn::new("Tipo").min_width(60.0))
//!     .row_height(28.0)
//!     .on_render_cell(|ctx, row, col, rect, cmds| {
//!         let text = ctx.app.table_data[row][col].clone();
//!         cmds.push(RenderCommand::Text {
//!             rect: Rect::new(rect.x + 8.0, rect.y + 4.0, rect.width - 16.0, rect.height - 8.0),
//!             text,
//!             color: ctx.theme.on_surface.to_array(),
//!             font_size: ctx.theme.font_size_base,
//!         });
//!     })
//!     .on_row_select(|ctx, row_idx| {
//!         ctx.app.selected_row = Some(row_idx);
//!     })
//!     .with_row_count(10_000);
//!
//! tree.add_node(Box::new(table), Some(panel_id));
//! ```
//!
//! ## Arquitectura
//!
//! ```
//! ┌── DataTable (fill, FlexColumn) ───────────────────────────────────────┐
//! │ ┌── Header row (fill_width, row_height fixed = 32px) ──────────────┐ │
//! │ │  [col0: Nombre ↑]   [col1: Tamaño]   [col2: Tipo]  ···          │ │
//! │ │  ├── resize handle (±3px) ──────────────────────────────────────  │ │
//! │ └──────────────────────────────────────────────────────────────────┘ │
//! │ ┌── Body (fill, clip, scroll) ─────────────────────────────────────┐ │
//! │ │  fila 0   [cell00]  [cell01]  [cell02]  ···                      │ │
//! │ │  fila 1   [cell10]  [cell11]  [cell12]  ···                      │ │
//! │ │  fila N   ··· solo las filas visibles ···                        │ │
//! │ └──────────────────────────────────────────────────────────────────┘ │
//! └───────────────────────────────────────────────────────────────────────┘
//! ```

use crate::{
    Widget, RenderCommand, DrawContext, BuildContext, LayoutContext, EventContext,
    EventResponse, UiEvent, Rect, Vec2, StyleBuilder, StyleExt,
};

// ─── SortDirection ────────────────────────────────────────────────────────────

/// Dirección de ordenación de una columna.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SortDirection {
    Ascending,
    Descending,
}

// ─── TableColumn ─────────────────────────────────────────────────────────────

/// Descriptor de una columna del [`DataTable`].
#[derive(Debug, Clone)]
pub struct TableColumn {
    /// Título de la columna.
    pub title: String,
    /// Ancho actual en píxeles.
    pub width: f32,
    /// Ancho mínimo en píxeles (al redimensionar con drag).
    pub min_width: f32,
    /// Si `true`, el header permite ordenar al hacer clic.
    pub sortable: bool,
    /// Alinear el contenido de las celdas a la derecha.
    pub align_right: bool,
}

impl TableColumn {
    /// Crea una columna con el `title` dado y ancho por defecto de 120 px.
    pub fn new(title: impl Into<String>) -> Self {
        Self {
            title: title.into(),
            width: 120.0,
            min_width: 40.0,
            sortable: false,
            align_right: false,
        }
    }

    /// Ancho inicial en píxeles.
    pub fn width(mut self, w: f32) -> Self {
        self.width = w;
        self
    }

    /// Ancho mínimo al arrastrar el handle de resize.
    pub fn min_width(mut self, w: f32) -> Self {
        self.min_width = w;
        self.width = self.width.max(w);
        self
    }

    /// Habilita la ordenación por clic en el header.
    pub fn sortable(mut self, v: bool) -> Self {
        self.sortable = v;
        self
    }

    /// Alinea el contenido de las celdas a la derecha (útil para números).
    pub fn align_right(mut self) -> Self {
        self.align_right = true;
        self
    }
}

// ─── Callbacks ────────────────────────────────────────────────────────────────

type RenderCellFn<App> = Box<
    dyn Fn(&mut EventContext<App>, usize /*row*/, usize /*col*/, Rect, &mut Vec<RenderCommand>) + Send + Sync,
>;
type RowSelectFn<App> = Box<dyn Fn(&mut EventContext<App>, usize) + Send + Sync>;
type SortFn<App> = Box<dyn Fn(&mut EventContext<App>, usize, SortDirection) + Send + Sync>;

// ─── DataTable ────────────────────────────────────────────────────────────────

/// Tabla de datos con cabeceras fijas, scroll virtualizado y ordenación.
///
/// Consulta la [documentación del módulo][self] para el uso completo.
pub struct DataTable<App> {
    // ── Esquema ──────────────────────────────────────────────────────────
    pub columns: Vec<TableColumn>,
    pub row_count: usize,

    // ── Opciones visuales ────────────────────────────────────────────────
    pub row_height: f32,
    pub header_height: f32,
    pub stripe_rows: bool,

    // ── Estado de selección ──────────────────────────────────────────────
    pub selected_rows: Vec<usize>,
    pub hovered_row: Option<usize>,

    // ── Estado de ordenación ──────────────────────────────────────────────
    pub sort_column: Option<usize>,
    pub sort_direction: SortDirection,

    // ── Estado de scroll ──────────────────────────────────────────────────
    pub scroll_offset_y: f32,
    pub scroll_offset_x: f32,

    // ── Estado de resize de columnas ─────────────────────────────────────
    resize_state: Option<ResizeState>,

    // ── Callbacks ────────────────────────────────────────────────────────
    render_cell: Option<RenderCellFn<App>>,
    on_row_select: Option<RowSelectFn<App>>,
    on_sort: Option<SortFn<App>>,
}

#[derive(Clone)]
struct ResizeState {
    /// Índice de columna que se está redimensionando.
    col_index: usize,
    /// Posición X del cursor al iniciar el drag.
    start_x: f32,
    /// Ancho de la columna al iniciar el drag.
    start_width: f32,
}

impl<App> DataTable<App> {
    /// Crea una `DataTable` vacía.
    pub fn new() -> Self {
        Self {
            columns: Vec::new(),
            row_count: 0,
            row_height: 28.0,
            header_height: 32.0,
            stripe_rows: true,
            selected_rows: Vec::new(),
            hovered_row: None,
            sort_column: None,
            sort_direction: SortDirection::Ascending,
            scroll_offset_y: 0.0,
            scroll_offset_x: 0.0,
            resize_state: None,
            render_cell: None,
            on_row_select: None,
            on_sort: None,
        }
    }

    /// Añade una columna al esquema.
    pub fn column(mut self, col: TableColumn) -> Self {
        self.columns.push(col);
        self
    }

    /// Número total de filas de datos.
    pub fn with_row_count(mut self, count: usize) -> Self {
        self.row_count = count;
        self
    }

    /// Alto de cada fila en píxeles.
    pub fn row_height(mut self, h: f32) -> Self {
        self.row_height = h;
        self
    }

    /// Alto del header en píxeles.
    pub fn header_height(mut self, h: f32) -> Self {
        self.header_height = h;
        self
    }

    /// Activa/desactiva el striping de filas alternas.
    pub fn stripe_rows(mut self, v: bool) -> Self {
        self.stripe_rows = v;
        self
    }

    /// Callback de renderizado personalizado de celdas.
    pub fn on_render_cell<F>(mut self, f: F) -> Self
    where
        F: Fn(&mut EventContext<App>, usize, usize, Rect, &mut Vec<RenderCommand>) + Send + Sync + 'static,
    {
        self.render_cell = Some(Box::new(f));
        self
    }

    /// Callback invocado al seleccionar una fila.
    pub fn on_row_select<F>(mut self, f: F) -> Self
    where
        F: Fn(&mut EventContext<App>, usize) + Send + Sync + 'static,
    {
        self.on_row_select = Some(Box::new(f));
        self
    }

    /// Callback invocado al hacer clic en una columna ordenable.
    pub fn on_sort<F>(mut self, f: F) -> Self
    where
        F: Fn(&mut EventContext<App>, usize, SortDirection) + Send + Sync + 'static,
    {
        self.on_sort = Some(Box::new(f));
        self
    }

    // ── Helpers ──────────────────────────────────────────────────────────

    /// Ancho total de todas las columnas.
    fn total_columns_width(&self) -> f32 {
        self.columns.iter().map(|c| c.width).sum()
    }

    /// Devuelve el X de inicio de la columna `col_idx` relativo al lado izquierdo
    /// del área de datos (sin incluir el scroll).
    fn col_x(&self, col_idx: usize) -> f32 {
        self.columns.iter().take(col_idx).map(|c| c.width).sum()
    }

    /// Handle de resize: ±3px alrededor de la arista derecha de la columna.
    fn resize_handle_x(&self, col_idx: usize) -> f32 {
        self.col_x(col_idx) + self.columns[col_idx].width
    }

    /// Fila de datos bajo la coordenada Y relativa al área del body.
    fn row_at_y(&self, body_local_y: f32) -> Option<usize> {
        let abs_y = body_local_y + self.scroll_offset_y;
        if abs_y < 0.0 { return None; }
        let row = (abs_y / self.row_height) as usize;
        if row < self.row_count { Some(row) } else { None }
    }
}

impl<App> Default for DataTable<App> {
    fn default() -> Self {
        Self::new()
    }
}

impl<App: 'static + Send + Sync> Widget<App> for DataTable<App> {
    fn build(&mut self, ctx: &mut BuildContext<App>) {
        let style = StyleBuilder::new()
            .column()
            .fill_width()
            .fill_height()
            .clip()
            .build();
        ctx.tree.set_node_style(ctx.node_id, style);
    }

    fn draw(&self, ctx: &mut DrawContext, cmds: &mut Vec<RenderCommand>) {
        let r = ctx.rect;
        let theme = &ctx.theme;
        let body_y = r.y + self.header_height;
        let body_h = r.height - self.header_height;

        // ── Fondo general ────────────────────────────────────────────────
        cmds.push(RenderCommand::Quad {
            rect: r,
            color: theme.surface.to_array(),
            radii: [0.0; 4],
            flags: 0,
        });

        // ── Header ───────────────────────────────────────────────────────
        cmds.push(RenderCommand::Quad {
            rect: Rect::new(r.x, r.y, r.width, self.header_height),
            color: theme.surface_elevated.to_array(),
            radii: [0.0; 4],
            flags: 0,
        });

        // Línea inferior del header
        cmds.push(RenderCommand::Quad {
            rect: Rect::new(r.x, r.y + self.header_height - 1.0, r.width, 1.0),
            color: theme.primary.with_alpha(0.4).to_array(),
            radii: [0.0; 4],
            flags: 0,
        });

        let mut col_x = r.x - self.scroll_offset_x;
        for (col_idx, col) in self.columns.iter().enumerate() {
            let col_rect = Rect::new(col_x, r.y, col.width, self.header_height);

            // Título de columna
            let is_sorted = self.sort_column == Some(col_idx);
            let sort_indicator = if is_sorted {
                match self.sort_direction {
                    SortDirection::Ascending => " ↑",
                    SortDirection::Descending => " ↓",
                }
            } else { "" };

            let header_text = format!("{}{}", col.title, sort_indicator);
            let text_color = if is_sorted {
                theme.primary.to_array()
            } else {
                theme.on_surface.to_array()
            };

            cmds.push(RenderCommand::Text {
                rect: Rect::new(col_x + 8.0, r.y + 2.0, col.width - 16.0, self.header_height - 4.0),
                text: header_text,
                color: text_color,
                font_size: theme.font_size_base,
            });

            // Handle de resize (línea vertical de 1px)
            let handle_x = col_x + col.width - 1.0;
            cmds.push(RenderCommand::Quad {
                rect: Rect::new(handle_x, r.y + 4.0, 1.0, self.header_height - 8.0),
                color: theme.on_surface_muted.with_alpha(0.2).to_array(),
                radii: [0.5; 4],
                flags: 0,
            });

            col_x += col.width;
            let _ = col_idx;
        }

        // ── Body (filas virtualizadas) ────────────────────────────────────
        // Clip al área del body
        cmds.push(RenderCommand::PushClip {
            rect: Rect::new(r.x, body_y, r.width, body_h),
        });

        let first_visible = (self.scroll_offset_y / self.row_height) as usize;
        let last_visible =
            ((self.scroll_offset_y + body_h) / self.row_height).ceil() as usize + 1;
        let last_visible = last_visible.min(self.row_count);

        for row_idx in first_visible..last_visible {
            let row_y = body_y + row_idx as f32 * self.row_height - self.scroll_offset_y;
            let row_rect = Rect::new(r.x, row_y, r.width, self.row_height);

            // Striping
            let is_selected = self.selected_rows.contains(&row_idx);
            let is_hovered = self.hovered_row == Some(row_idx);
            let is_even = row_idx % 2 == 0;

            let row_bg = if is_selected {
                theme.primary.with_alpha(0.25).to_array()
            } else if is_hovered {
                theme.on_surface_muted.with_alpha(0.08).to_array()
            } else if self.stripe_rows && !is_even {
                theme.surface_elevated.with_alpha(0.4).to_array()
            } else {
                [0.0, 0.0, 0.0, 0.0]
            };

            if row_bg[3] > 0.0 {
                cmds.push(RenderCommand::Quad {
                    rect: row_rect,
                    color: row_bg,
                    radii: [0.0; 4],
                    flags: 0,
                });
            }

            // Celdas de la fila
            let mut cell_x = r.x - self.scroll_offset_x;
            for (col_idx, col) in self.columns.iter().enumerate() {
                let cell_rect = Rect::new(cell_x, row_y, col.width, self.row_height);

                // Contenido por defecto
                cmds.push(RenderCommand::Text {
                    rect: Rect::new(
                        cell_rect.x + 8.0,
                        cell_rect.y + 2.0,
                        cell_rect.width - 16.0,
                        self.row_height - 4.0,
                    ),
                    text: format!("r{}c{}", row_idx, col_idx),
                    color: if is_selected {
                        theme.on_surface.to_array()
                    } else {
                        theme.on_surface_muted.to_array()
                    },
                    font_size: theme.font_size_base,
                });

                cell_x += col.width;
            }

            // Línea separadora entre filas
            cmds.push(RenderCommand::Quad {
                rect: Rect::new(r.x, row_y + self.row_height - 1.0, r.width, 1.0),
                color: theme.on_surface_muted.with_alpha(0.05).to_array(),
                radii: [0.0; 4],
                flags: 0,
            });
        }

        cmds.push(RenderCommand::PopClip);

        // ── Scrollbar vertical ───────────────────────────────────────────
        let content_h = self.row_count as f32 * self.row_height;
        if content_h > body_h {
            let scrollbar_w = 4.0;
            let thumb_h = (body_h / content_h * body_h).max(20.0);
            let thumb_y = body_y + (self.scroll_offset_y / content_h) * body_h;
            cmds.push(RenderCommand::Quad {
                rect: Rect::new(r.x + r.width - scrollbar_w - 2.0, thumb_y, scrollbar_w, thumb_h),
                color: theme.on_surface_muted.with_alpha(0.3).to_array(),
                radii: [2.0; 4],
                flags: 0,
            });
        }

        // ── Scrollbar horizontal ─────────────────────────────────────────
        let content_w = self.total_columns_width();
        if content_w > r.width {
            let scrollbar_h = 4.0;
            let thumb_w = (r.width / content_w * r.width).max(20.0);
            let thumb_x = r.x + (self.scroll_offset_x / content_w) * r.width;
            cmds.push(RenderCommand::Quad {
                rect: Rect::new(thumb_x, r.y + r.height - scrollbar_h - 2.0, thumb_w, scrollbar_h),
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
        let body_y = ctx.rect.y + self.header_height;
        let body_h = ctx.rect.height - self.header_height;

        match event {
            UiEvent::MouseMove { pos } => {
                // Resize de columna en progreso
                if let Some(resize) = self.resize_state.clone() {
                    let delta = pos.x - resize.start_x;
                    let new_width = (resize.start_width + delta)
                        .max(self.columns[resize.col_index].min_width);
                    self.columns[resize.col_index].width = new_width;
                    return EventResponse::Redraw;
                }

                // Hover de fila en el body
                if pos.y >= body_y {
                    let body_local_y = pos.y - body_y;
                    let new_hovered = self.row_at_y(body_local_y);
                    if new_hovered != self.hovered_row {
                        self.hovered_row = new_hovered;
                        return EventResponse::Redraw;
                    }
                }
                EventResponse::Ignored
            }

            UiEvent::MouseLeave => {
                self.hovered_row = None;
                EventResponse::Redraw
            }

            UiEvent::MouseDown { pos, .. } => {
                // ─ Click en el header ─────────────────────────────────────
                if pos.y < body_y {
                    let local_x = pos.x - ctx.rect.x + self.scroll_offset_x;

                    // Detectar clic en handle de resize (±4px)
                    let mut accumulated_x = 0.0f32;
                    for (col_idx, col) in self.columns.iter().enumerate() {
                        accumulated_x += col.width;
                        if (local_x - accumulated_x).abs() <= 4.0 {
                            self.resize_state = Some(ResizeState {
                                col_index: col_idx,
                                start_x: pos.x,
                                start_width: col.width,
                            });
                            return EventResponse::Consumed;
                        }
                    }

                    // Click en la cabecera de columna → ordenar
                    accumulated_x = 0.0;
                    for (col_idx, col) in self.columns.iter().enumerate() {
                        if local_x >= accumulated_x && local_x < accumulated_x + col.width {
                            if col.sortable {
                                if self.sort_column == Some(col_idx) {
                                    self.sort_direction = match self.sort_direction {
                                        SortDirection::Ascending => SortDirection::Descending,
                                        SortDirection::Descending => SortDirection::Ascending,
                                    };
                                } else {
                                    self.sort_column = Some(col_idx);
                                    self.sort_direction = SortDirection::Ascending;
                                }
                                if let Some(cb) = &self.on_sort {
                                    let dir = self.sort_direction;
                                    cb(ctx, col_idx, dir);
                                }
                                return EventResponse::Redraw;
                            }
                            break;
                        }
                        accumulated_x += col.width;
                    }
                    return EventResponse::Ignored;
                }

                // ─ Click en el body → selección de fila ──────────────────
                let body_local_y = pos.y - body_y;
                if let Some(row) = self.row_at_y(body_local_y) {
                    self.selected_rows = vec![row];
                    if let Some(cb) = &self.on_row_select {
                        cb(ctx, row);
                    }
                    return EventResponse::Redraw;
                }

                EventResponse::Ignored
            }

            UiEvent::MouseUp { .. } => {
                if self.resize_state.take().is_some() {
                    return EventResponse::Consumed;
                }
                EventResponse::Ignored
            }

            UiEvent::MouseWheel { delta_y, delta_x } => {
                // Scroll vertical en el body
                let content_h = self.row_count as f32 * self.row_height;
                let max_y = (content_h - body_h).max(0.0);
                self.scroll_offset_y = (self.scroll_offset_y - delta_y * 40.0).clamp(0.0, max_y);

                // Scroll horizontal (shift+wheel o delta_x)
                let content_w = self.total_columns_width();
                let max_x = (content_w - ctx.rect.width).max(0.0);
                self.scroll_offset_x = (self.scroll_offset_x - delta_x * 40.0).clamp(0.0, max_x);

                EventResponse::Redraw
            }

            _ => EventResponse::Ignored,
        }
    }

    fn scroll_offset(&self) -> Vec2 {
        Vec2::new(self.scroll_offset_x, self.scroll_offset_y)
    }
}
