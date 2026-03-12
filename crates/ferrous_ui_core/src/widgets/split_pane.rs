//! # `SplitPane` — Dos Paneles con Divisor Arrastrable
//!
//! `SplitPane` divide una región en dos sub-paneles separados por un divisor
//! interactivo que el usuario puede arrastrar para cambiar la proporción.
//! Soporta orientación horizontal y vertical así como persistencia del ratio.
//!
//! ## Ejemplo de uso
//!
//! ```rust,ignore
//! use ferrous_ui_core::{SplitPane, SplitOrientation, Label};
//!
//! let split = SplitPane::<MyApp>::new(SplitOrientation::Horizontal)
//!     .with_first(Box::new(Label::new("Panel Izquierdo")))
//!     .with_second(Box::new(Label::new("Panel Derecho")))
//!     .with_ratio(0.35)   // 35% para el primero, 65% para el segundo
//!     .divider_size(6.0);
//!
//! tree.add_node(Box::new(split), Some(root_id));
//! ```
//!
//! ## Diseño interno
//!
//! ```text
//! ┌────────────┬──┬──────────────────┐
//! │  Primer    │ ↔│   Segundo        │  ← SplitOrientation::Horizontal
//! │  panel     │  │   panel          │
//! └────────────┴──┴──────────────────┘
//!               ↑
//!         Divisor (6px), arrastrable
//! ```
//!
//! El widget calcula las anchuras/alturas de los dos paneles a partir de `ratio` y
//! el rectángulo resuelto por el layout padre. El divisor tiene su propio `NodeId` y
//! captura eventos `MouseDown`/`MouseMove`/`MouseUp` para actualizar `ratio`.

use crate::{
    Widget, RenderCommand, DrawContext, BuildContext, LayoutContext, EventContext,
    EventResponse, UiEvent, Rect, Vec2, NodeId, Color, StyleBuilder, StyleExt,
    Units,
};

// ─── SplitOrientation ────────────────────────────────────────────────────────

/// Define la dirección en que se dividen los dos paneles del `SplitPane`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SplitOrientation {
    /// El divisor es vertical: primer panel a la izquierda, segundo a la derecha.
    Horizontal,
    /// El divisor es horizontal: primer panel arriba, segundo abajo.
    Vertical,
}

// ─── SplitPane ───────────────────────────────────────────────────────────────

/// Contenedor dividido en dos paneles con divisor arrastrable.
///
/// El `ratio` determina qué fracción del espacio total ocupa el primer panel.
/// Un ratio de `0.5` divide el espacio por la mitad.
pub struct SplitPane<App> {
    /// Orientación del divisor.
    pub orientation: SplitOrientation,
    /// Proporción del primer panel (0.0–1.0). Por defecto `0.5`.
    pub ratio: f32,
    /// Tamaño del divisor en píxeles.
    pub divider_size: f32,
    /// Rango permitido para el ratio (min, max).
    pub ratio_range: (f32, f32),

    first: Option<Box<dyn Widget<App>>>,
    second: Option<Box<dyn Widget<App>>>,

    // Estado de arrastre
    is_dragging: bool,
    drag_start_pos: f32,
    drag_start_ratio: f32,

    // IDs de nodos
    first_id: Option<NodeId>,
    second_id: Option<NodeId>,
    divider_id: Option<NodeId>,

    on_ratio_change_cb: Option<Box<dyn Fn(&mut EventContext<App>, f32) + Send + Sync + 'static>>,
}

impl<App> SplitPane<App> {
    /// Crea un `SplitPane` con orientación dada y ratio inicial `0.5`.
    pub fn new(orientation: SplitOrientation) -> Self {
        Self {
            orientation,
            ratio: 0.5,
            divider_size: 6.0,
            ratio_range: (0.1, 0.9),
            first: None,
            second: None,
            is_dragging: false,
            drag_start_pos: 0.0,
            drag_start_ratio: 0.5,
            first_id: None,
            second_id: None,
            divider_id: None,
            on_ratio_change_cb: None,
        }
    }

    /// Establece el widget del primer panel (izquierdo o superior).
    pub fn with_first(mut self, widget: Box<dyn Widget<App>>) -> Self {
        self.first = Some(widget);
        self
    }

    /// Establece el widget del segundo panel (derecho o inferior).
    pub fn with_second(mut self, widget: Box<dyn Widget<App>>) -> Self {
        self.second = Some(widget);
        self
    }

    /// Establece el ratio inicial. Debe estar en el rango (0, 1).
    ///
    /// - `0.0` → primer panel colapsado
    /// - `0.5` → mitad/mitad (por defecto)
    /// - `1.0` → segundo panel colapsado
    pub fn with_ratio(mut self, ratio: f32) -> Self {
        self.ratio = ratio.clamp(0.05, 0.95);
        self
    }

    /// Tamaño del divisor en píxeles (por defecto `6.0`).
    pub fn divider_size(mut self, size: f32) -> Self {
        self.divider_size = size;
        self
    }

    /// Rango admisible para el ratio. Útil para evitar que un panel quede invisible.
    pub fn ratio_range(mut self, min: f32, max: f32) -> Self {
        self.ratio_range = (min.clamp(0.0, 1.0), max.clamp(0.0, 1.0));
        self
    }

    /// Registra una función que se invoca mientras el usuario arrastra el divisor.
    ///
    /// El parámetro es el nuevo ratio del primer panel (0.0–1.0).
    pub fn on_ratio_change(mut self, f: impl Fn(&mut EventContext<App>, f32) + Send + Sync + 'static) -> Self {
        self.on_ratio_change_cb = Some(Box::new(f));
        self
    }
}

impl<App> Default for SplitPane<App> {
    fn default() -> Self {
        Self::new(SplitOrientation::Horizontal)
    }
}

// ─── Divisor interno ─────────────────────────────────────────────────────────

struct Divider {
    orientation: SplitOrientation,
    is_hovered: bool,
}

impl Divider {
    fn new(orientation: SplitOrientation) -> Self {
        Self { orientation, is_hovered: false }
    }
}

impl<App> Widget<App> for Divider {
    fn draw(&self, ctx: &mut DrawContext, cmds: &mut Vec<RenderCommand>) {
        let r = &ctx.rect;
        let theme = &ctx.theme;

        let color = if self.is_hovered {
            theme.primary.with_alpha(0.6)
        } else {
            theme.on_surface_muted.with_alpha(0.2)
        };

        cmds.push(RenderCommand::Quad {
            rect: *r,
            color: color.to_array(),
            radii: [0.0; 4],
            flags: 0,
        });

        // Línea de agarres (grip dots) en el centro del divisor
        let (dots_x, dots_y) = match self.orientation {
            SplitOrientation::Horizontal => (r.x + r.width * 0.5 - 1.0, r.y + r.height * 0.5 - 6.0),
            SplitOrientation::Vertical   => (r.x + r.width * 0.5 - 6.0, r.y + r.height * 0.5 - 1.0),
        };

        for i in 0..3 {
            let (dx, dy) = match self.orientation {
                SplitOrientation::Horizontal => (0.0, i as f32 * 5.0),
                SplitOrientation::Vertical   => (i as f32 * 5.0, 0.0),
            };
            cmds.push(RenderCommand::Quad {
                rect: Rect::new(dots_x + dx, dots_y + dy, 2.0, 2.0),
                color: theme.on_surface_muted.with_alpha(0.6).to_array(),
                radii: [1.0; 4],
                flags: 0,
            });
        }
    }

    fn calculate_size(&self, _ctx: &mut LayoutContext) -> Vec2 {
        Vec2::ZERO
    }

    fn on_event(&mut self, _ctx: &mut EventContext<App>, event: &UiEvent) -> EventResponse {
        match event {
            UiEvent::MouseMove { .. } => {
                self.is_hovered = true;
                EventResponse::Redraw
            }
            _ => EventResponse::Ignored,
        }
    }
}

// ─── Widget<App> for SplitPane ───────────────────────────────────────────────

impl<App: 'static + Send + Sync> Widget<App> for SplitPane<App> {
    fn build(&mut self, ctx: &mut BuildContext<App>) {
        let my_id = ctx.node_id;

        // Orientación del layout raíz
        let root_style = match self.orientation {
            SplitOrientation::Horizontal => StyleBuilder::new().row().fill_width().fill_height().build(),
            SplitOrientation::Vertical   => StyleBuilder::new().column().fill_width().fill_height().build(),
        };
        ctx.tree.set_node_style(my_id, root_style);

        // ── Primer panel ──────────────────────────────────────────────────
        let first_widget = self.first.take()
            .unwrap_or_else(|| Box::new(crate::widgets::PlaceholderWidget));
        let first_style = self.panel_style(self.ratio);
        let first_id = ctx.tree.add_node(first_widget, Some(my_id));
        ctx.tree.set_node_style(first_id, first_style);
        self.first_id = Some(first_id);

        // ── Divisor ───────────────────────────────────────────────────────
        let divider_widget = Box::new(Divider::new(self.orientation));
        let divider_style = self.divider_style();
        let div_id = ctx.tree.add_node(divider_widget, Some(my_id));
        ctx.tree.set_node_style(div_id, divider_style);
        self.divider_id = Some(div_id);

        // ── Segundo panel ─────────────────────────────────────────────────
        let second_widget = self.second.take()
            .unwrap_or_else(|| Box::new(crate::widgets::PlaceholderWidget));
        let second_ratio = 1.0 - self.ratio;
        let second_style = self.panel_style(second_ratio);
        let second_id = ctx.tree.add_node(second_widget, Some(my_id));
        ctx.tree.set_node_style(second_id, second_style);
        self.second_id = Some(second_id);
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
        let div_id = match self.divider_id {
            Some(id) => id,
            None => return EventResponse::Ignored,
        };

        match event {
            UiEvent::MouseDown { pos, .. } => {
                if let Some(div_rect) = ctx.tree.get_node_rect(div_id) {
                    if div_rect.contains([pos.x, pos.y]) {
                        self.is_dragging = true;
                        self.drag_start_pos = match self.orientation {
                            SplitOrientation::Horizontal => pos.x,
                            SplitOrientation::Vertical   => pos.y,
                        };
                        self.drag_start_ratio = self.ratio;
                        return EventResponse::Consumed;
                    }
                }
                EventResponse::Ignored
            }
            UiEvent::MouseMove { pos } if self.is_dragging => {
                let current_pos = match self.orientation {
                    SplitOrientation::Horizontal => pos.x,
                    SplitOrientation::Vertical   => pos.y,
                };
                let total_size = match self.orientation {
                    SplitOrientation::Horizontal => ctx.rect.width,
                    SplitOrientation::Vertical   => ctx.rect.height,
                };

                if total_size > 0.0 {
                    let delta_ratio = (current_pos - self.drag_start_pos) / total_size;
                    let new_ratio = (self.drag_start_ratio + delta_ratio)
                        .clamp(self.ratio_range.0, self.ratio_range.1);

                    if (new_ratio - self.ratio).abs() > 0.001 {
                        self.ratio = new_ratio;
                        // Actualizar estilos de los dos paneles
                        self.apply_ratio(ctx.tree);
                        // Notificar al usuario del nuevo ratio
                        if let Some(cb) = &self.on_ratio_change_cb {
                            cb(ctx, new_ratio);
                        }
                        return EventResponse::Redraw;
                    }
                }
                EventResponse::Ignored
            }
            UiEvent::MouseUp { .. } if self.is_dragging => {
                self.is_dragging = false;
                EventResponse::Consumed
            }
            _ => EventResponse::Ignored,
        }
    }
}

impl<App: 'static + Send + Sync> SplitPane<App> {
    fn panel_style(&self, ratio: f32) -> crate::Style {
        let pct = ratio * 100.0;
        match self.orientation {
            SplitOrientation::Horizontal => StyleBuilder::new()
                .width_pct(pct)
                .fill_height()
                .clip()
                .build(),
            SplitOrientation::Vertical => StyleBuilder::new()
                .fill_width()
                .height_pct(pct)
                .clip()
                .build(),
        }
    }

    fn divider_style(&self) -> crate::Style {
        match self.orientation {
            SplitOrientation::Horizontal => StyleBuilder::new()
                .width_px(self.divider_size)
                .fill_height()
                .build(),
            SplitOrientation::Vertical => StyleBuilder::new()
                .fill_width()
                .height_px(self.divider_size)
                .build(),
        }
    }

    fn apply_ratio(&mut self, tree: &mut crate::UiTree<App>) {
        if let Some(first_id) = self.first_id {
            let s = self.panel_style(self.ratio);
            tree.set_node_style(first_id, s);
        }
        if let Some(second_id) = self.second_id {
            let s = self.panel_style(1.0 - self.ratio);
            tree.set_node_style(second_id, s);
        }
    }
}
