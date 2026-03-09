//! # `DockLayout` — Sistema de Paneles Anclables
//!
//! `DockLayout` organiza contenido en zonas de anclaje fijas (`Left`, `Right`,
//! `Top`, `Bottom`, `Center`), con divisores arrastrables entre zonas y soporte
//! para mostrar/ocultar paneles individualmente.
//!
//! Es el widget base de la interfaz de **Ferrous Builder** y cualquier aplicación
//! tipo IDE (editor de código, panel de propiedades, visor de escena…).
//!
//! ## Ejemplo de uso
//!
//! ```rust,ignore
//! use ferrous_ui_core::{DockLayout, DockZone, Label};
//!
//! let layout = DockLayout::<MyApp>::new()
//!     .dock(DockZone::Left,   320.0, Box::new(scene_hierarchy))
//!     .dock(DockZone::Right,  280.0, Box::new(properties_panel))
//!     .dock(DockZone::Bottom, 200.0, Box::new(console_panel))
//!     .dock(DockZone::Center,   0.0, Box::new(viewport_widget));
//!
//! let id = tree.add_node(Box::new(layout), Some(root_id));
//! tree.set_node_style(id, StyleBuilder::new().fill().build());
//! ```
//!
//! ## Arquitectura
//!
//! ```
//! DockLayout (root — fill, FlexColumn)
//! ├── [top panel]   — height: top_size px, width: 100%
//! ├── Middle row (FlexRow, flex=1)
//! │   ├── [left panel]    — width: left_size px
//! │   ├── Divider V       — 4px
//! │   ├── [center widget] — flex=1
//! │   ├── Divider V       — 4px
//! │   └── [right panel]   — width: right_size px
//! └── [bottom panel] — height: bottom_size px, width: 100%
//! ```
//!
//! Cada zona es un `Panel` de fondo `surface`, con un `DockDivider` separando
//! las zonas adyacentes. Los divisores son arrastrables exactamente como en `SplitPane`.

use crate::{
    Widget, RenderCommand, DrawContext, BuildContext, LayoutContext, EventContext,
    EventResponse, UiEvent, Rect, Vec2, NodeId, Color, StyleBuilder, StyleExt, Units,
};

// ─── DockZone ────────────────────────────────────────────────────────────────

/// Zona de anclaje disponible en un [`DockLayout`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DockZone {
    /// Panel lateral izquierdo (ancho fijo en píxeles).
    Left,
    /// Panel lateral derecho (ancho fijo en píxeles).
    Right,
    /// Banda superior (alto fijo en píxeles).
    Top,
    /// Banda inferior (alto fijo en píxeles).
    Bottom,
    /// Área central, ocupa todo el espacio restante (`flex=1`).
    Center,
}

// ─── DockEntry ───────────────────────────────────────────────────────────────

struct DockEntry<App> {
    zone: DockZone,
    /// Tamaño en píxeles (ancho para Left/Right, alto para Top/Bottom).
    /// Ignorado para `Center`.
    size: f32,
    /// Tamaño mínimo al arrastrar el divisor.
    min_size: f32,
    /// Tamaño máximo al arrastrar el divisor.
    max_size: f32,
    /// Si `false`, el panel está colapsado (tamaño 0).
    visible: bool,
    widget: Option<Box<dyn Widget<App>>>,
    node_id: Option<NodeId>,
}

impl<App> DockEntry<App> {
    fn new(zone: DockZone, size: f32, widget: Box<dyn Widget<App>>) -> Self {
        Self {
            zone,
            size,
            min_size: 60.0,
            max_size: 2000.0,
            visible: true,
            widget: Some(widget),
            node_id: None,
        }
    }
}

// ─── DockDivider (interna) ───────────────────────────────────────────────────

/// Divisor arrastrable entre dos zonas del DockLayout.
struct DockDivider {
    /// Si `true`, divisor vertical (entre paneles horizontales).
    _vertical: bool,
    is_hovered: bool,
}

impl DockDivider {
    fn new(vertical: bool) -> Self {
        Self { _vertical: vertical, is_hovered: false }
    }
}

impl<App> Widget<App> for DockDivider {
    fn draw(&self, ctx: &mut DrawContext, cmds: &mut Vec<RenderCommand>) {
        let r = &ctx.rect;
        let theme = &ctx.theme;

        let color = if self.is_hovered {
            theme.primary.with_alpha(0.5)
        } else {
            theme.on_surface_muted.with_alpha(0.12)
        };

        cmds.push(RenderCommand::Quad {
            rect: *r,
            color: color.to_array(),
            radii: [0.0; 4],
            flags: 0,
        });
    }

    fn calculate_size(&self, _ctx: &mut LayoutContext) -> Vec2 {
        Vec2::ZERO
    }

    fn on_event(&mut self, _ctx: &mut EventContext<App>, event: &UiEvent) -> EventResponse {
        match event {
            UiEvent::MouseEnter => { self.is_hovered = true;  EventResponse::Redraw }
            UiEvent::MouseLeave => { self.is_hovered = false; EventResponse::Redraw }
            _ => EventResponse::Ignored,
        }
    }
}

// ─── DockLayout ──────────────────────────────────────────────────────────────

const DIVIDER_SIZE: f32 = 4.0;

/// Sistema de paneles anclables tipo IDE.
///
/// Consulta la [documentación del módulo][self] para el uso completo.
pub struct DockLayout<App> {
    entries: Vec<DockEntry<App>>,

    // IDs de nodos para actualizar tamaños en tiempo de ejecución
    middle_row_id: Option<NodeId>,
    // Divisores: uno entre left/center y otro entre center/right
    div_lc_id: Option<NodeId>, // left-center divider
    div_cr_id: Option<NodeId>, // center-right divider
    div_tc_id: Option<NodeId>, // top divider
    div_cb_id: Option<NodeId>, // bottom divider

    // Estado de arrastre activo
    dragging: Option<DragState>,
}

#[derive(Clone)]
struct DragState {
    /// Qué entrada se está redimensionando.
    entry_index: usize,
    /// Posición del cursor al inicio del drag.
    start_pos: f32,
    /// Tamaño de la entrada al inicio del drag.
    start_size: f32,
    /// Si el resize es horizontal (`true`) o vertical (`false`).
    horizontal: bool,
}

impl<App> DockLayout<App> {
    /// Crea un `DockLayout` sin paneles.
    pub fn new() -> Self {
        Self {
            entries: Vec::new(),
            middle_row_id: None,
            div_lc_id: None,
            div_cr_id: None,
            div_tc_id: None,
            div_cb_id: None,
            dragging: None,
        }
    }

    /// Ancla un widget a una zona del layout.
    ///
    /// - `zone`: dónde se colocará (`Left`, `Right`, `Top`, `Bottom`, `Center`).
    /// - `size`: ancho (para Left/Right) o alto (para Top/Bottom) inicial en px.
    ///   Se ignora para `Center`.
    /// - `widget`: contenido del panel.
    pub fn dock(mut self, zone: DockZone, size: f32, widget: Box<dyn Widget<App>>) -> Self {
        self.entries.push(DockEntry::new(zone, size, widget));
        self
    }

    /// Muestra u oculta un panel por índice de registro.
    pub fn set_visible(&mut self, index: usize, visible: bool) {
        if let Some(e) = self.entries.get_mut(index) {
            e.visible = visible;
        }
    }

    /// Número de entradas registradas.
    pub fn entry_count(&self) -> usize {
        self.entries.len()
    }

    // Índice de la entrada con determinada zona (primera coincidencia)
    fn find(&self, zone: DockZone) -> Option<usize> {
        self.entries.iter().position(|e| e.zone == zone)
    }

    fn entry_style(&self, zone: DockZone, size: f32, visible: bool) -> crate::Style {
        if !visible {
            let mut s = StyleBuilder::new().build();
            s.size = (Units::Px(0.0), Units::Px(0.0));
            return s;
        }
        match zone {
            DockZone::Left | DockZone::Right => StyleBuilder::new()
                .width_px(size)
                .fill_height()
                .clip()
                .build(),
            DockZone::Top | DockZone::Bottom => StyleBuilder::new()
                .fill_width()
                .height_px(size)
                .clip()
                .build(),
            DockZone::Center => StyleBuilder::new()
                .flex(1.0)
                .fill_height()
                .clip()
                .build(),
        }
    }
}

impl<App> Default for DockLayout<App> {
    fn default() -> Self {
        Self::new()
    }
}

impl<App: 'static + Send + Sync> Widget<App> for DockLayout<App> {
    fn build(&mut self, ctx: &mut BuildContext<App>) {
        let my_id = ctx.node_id;

        // Raíz: columna vertical [top | middle_row | bottom]
        let root_style = StyleBuilder::new().column().fill_width().fill_height().build();
        ctx.tree.set_node_style(my_id, root_style);

        // ── Top panel ─────────────────────────────────────────────────────
        if let Some(i) = self.find(DockZone::Top) {
            let size = self.entries[i].size;
            let vis = self.entries[i].visible;
            let widget = self.entries[i].widget.take()
                .unwrap_or_else(|| Box::new(crate::widgets::PlaceholderWidget));
            let style = self.entry_style(DockZone::Top, size, vis);
            let nid = ctx.tree.add_node(widget, Some(my_id));
            ctx.tree.set_node_style(nid, style);
            self.entries[i].node_id = Some(nid);

            // Divisor top-middle
            let div = Box::new(DockDivider::new(false));
            let ds = StyleBuilder::new().fill_width().height_px(DIVIDER_SIZE).build();
            let div_id = ctx.tree.add_node(div, Some(my_id));
            ctx.tree.set_node_style(div_id, ds);
            self.div_tc_id = Some(div_id);
        }

        // ── Fila central (left | center | right) ──────────────────────────
        let mid_style = StyleBuilder::new().row().fill_width().flex(1.0).build();
        let mid_id = ctx.tree.add_node(Box::new(crate::widgets::Panel::new()), Some(my_id));
        ctx.tree.set_node_style(mid_id, mid_style);
        self.middle_row_id = Some(mid_id);

        // Left
        if let Some(i) = self.find(DockZone::Left) {
            let size = self.entries[i].size;
            let vis = self.entries[i].visible;
            let widget = self.entries[i].widget.take()
                .unwrap_or_else(|| Box::new(crate::widgets::PlaceholderWidget));
            let style = self.entry_style(DockZone::Left, size, vis);
            let nid = ctx.tree.add_node(widget, Some(mid_id));
            ctx.tree.set_node_style(nid, style);
            self.entries[i].node_id = Some(nid);

            // Divisor left-center
            let div = Box::new(DockDivider::new(true));
            let ds = StyleBuilder::new().width_px(DIVIDER_SIZE).fill_height().build();
            let div_id = ctx.tree.add_node(div, Some(mid_id));
            ctx.tree.set_node_style(div_id, ds);
            self.div_lc_id = Some(div_id);
        }

        // Center
        if let Some(i) = self.find(DockZone::Center) {
            let vis = self.entries[i].visible;
            let widget = self.entries[i].widget.take()
                .unwrap_or_else(|| Box::new(crate::widgets::PlaceholderWidget));
            let style = self.entry_style(DockZone::Center, 0.0, vis);
            let nid = ctx.tree.add_node(widget, Some(mid_id));
            ctx.tree.set_node_style(nid, style);
            self.entries[i].node_id = Some(nid);
        }

        // Divisor center-right
        if self.find(DockZone::Right).is_some() {
            let div = Box::new(DockDivider::new(true));
            let ds = StyleBuilder::new().width_px(DIVIDER_SIZE).fill_height().build();
            let div_id = ctx.tree.add_node(div, Some(mid_id));
            ctx.tree.set_node_style(div_id, ds);
            self.div_cr_id = Some(div_id);
        }

        // Right
        if let Some(i) = self.find(DockZone::Right) {
            let size = self.entries[i].size;
            let vis = self.entries[i].visible;
            let widget = self.entries[i].widget.take()
                .unwrap_or_else(|| Box::new(crate::widgets::PlaceholderWidget));
            let style = self.entry_style(DockZone::Right, size, vis);
            let nid = ctx.tree.add_node(widget, Some(mid_id));
            ctx.tree.set_node_style(nid, style);
            self.entries[i].node_id = Some(nid);
        }

        // ── Divisor bottom + Bottom panel ─────────────────────────────────
        if let Some(i) = self.find(DockZone::Bottom) {
            let div = Box::new(DockDivider::new(false));
            let ds = StyleBuilder::new().fill_width().height_px(DIVIDER_SIZE).build();
            let div_id = ctx.tree.add_node(div, Some(my_id));
            ctx.tree.set_node_style(div_id, ds);
            self.div_cb_id = Some(div_id);

            let size = self.entries[i].size;
            let vis = self.entries[i].visible;
            let widget = self.entries[i].widget.take()
                .unwrap_or_else(|| Box::new(crate::widgets::PlaceholderWidget));
            let style = self.entry_style(DockZone::Bottom, size, vis);
            let nid = ctx.tree.add_node(widget, Some(my_id));
            ctx.tree.set_node_style(nid, style);
            self.entries[i].node_id = Some(nid);
        }
    }

    fn draw(&self, _ctx: &mut DrawContext, _cmds: &mut Vec<RenderCommand>) {}

    fn calculate_size(&self, _ctx: &mut LayoutContext) -> Vec2 {
        Vec2::ZERO
    }

    fn on_event(
        &mut self,
        ctx: &mut EventContext<App>,
        event: &UiEvent,
    ) -> EventResponse {
        match event {
            UiEvent::MouseDown { pos, .. } => {
                // Detectar qué divisor se está pulsando y comenzar drag
                let dividers: &[(Option<NodeId>, usize, bool)] = &[
                    (self.div_lc_id, self.find(DockZone::Left).unwrap_or(usize::MAX), true),
                    (self.div_cr_id, self.find(DockZone::Right).unwrap_or(usize::MAX), true),
                    (self.div_tc_id, self.find(DockZone::Top).unwrap_or(usize::MAX), false),
                    (self.div_cb_id, self.find(DockZone::Bottom).unwrap_or(usize::MAX), false),
                ];

                for &(div_id_opt, entry_i, horizontal) in dividers {
                    if entry_i == usize::MAX { continue; }
                    if let Some(div_id) = div_id_opt {
                        if let Some(div_rect) = ctx.tree.get_node_rect(div_id) {
                            // Ampliar hitbox del divisor a 6px para usabilidad
                            let hit = Rect::new(
                                div_rect.x - 2.0, div_rect.y - 2.0,
                                div_rect.width + 4.0, div_rect.height + 4.0,
                            );
                            if hit.contains([pos.x, pos.y]) {
                                let start_pos = if horizontal { pos.x } else { pos.y };
                                let start_size = self.entries[entry_i].size;
                                self.dragging = Some(DragState {
                                    entry_index: entry_i,
                                    start_pos,
                                    start_size,
                                    horizontal,
                                });
                                return EventResponse::Consumed;
                            }
                        }
                    }
                }
                EventResponse::Ignored
            }

            UiEvent::MouseMove { pos } => {
                // Extraemos una copia del estado de drag para evitar borrow conflict
                let drag = match self.dragging.clone() {
                    Some(d) => d,
                    None => return EventResponse::Ignored,
                };

                let current = if drag.horizontal { pos.x } else { pos.y };
                let i = drag.entry_index;

                // Leer campos necesarios de la entry antes de mutarla
                let zone = self.entries[i].zone;
                let min_size = self.entries[i].min_size;
                let max_size = self.entries[i].max_size;
                let old_size = self.entries[i].size;
                let visible = self.entries[i].visible;
                let node_id = self.entries[i].node_id;

                let sign = match zone {
                    DockZone::Left | DockZone::Top    =>  1.0,
                    DockZone::Right | DockZone::Bottom => -1.0,
                    DockZone::Center => 0.0,
                };

                let new_size = (drag.start_size + (current - drag.start_pos) * sign)
                    .clamp(min_size, max_size);

                if (new_size - old_size).abs() > 0.5 {
                    self.entries[i].size = new_size;
                    if let Some(nid) = node_id {
                        let style = self.entry_style(zone, new_size, visible);
                        ctx.tree.set_node_style(nid, style);
                        ctx.tree.mark_layout_dirty(nid);
                    }
                    return EventResponse::Redraw;
                }
                EventResponse::Ignored
            }

            UiEvent::MouseUp { .. } => {
                if self.dragging.take().is_some() {
                    return EventResponse::Consumed;
                }
                EventResponse::Ignored
            }

            _ => EventResponse::Ignored,
        }
    }
}
