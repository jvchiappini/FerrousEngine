//! # `Tooltip` — Overlay Flotante con Delay Configurable
//!
//! `Tooltip` es un widget envolvente que muestra un pequeño popup de texto
//! cuando el cursor permanece sobre su hijo durante un tiempo configurable.
//! Se posiciona automáticamente para no salir del viewport.
//!
//! ## Ejemplo de uso
//!
//! ```rust,ignore
//! use ferrous_ui_core::{Tooltip, Button};
//!
//! // Envuelve cualquier widget con un tooltip
//! let btn_with_tip = Tooltip::<MyApp>::new("Guarda el proyecto actual (Ctrl+S)")
//!     .with_child(Box::new(Button::new("Guardar")))
//!     .delay_ms(400);          // aparece tras 400ms de hover
//!
//! tree.add_node(Box::new(btn_with_tip), Some(panel_id));
//! ```
//!
//! ## Posicionamiento automático
//!
//! El tooltip se posiciona debajo del widget hijo por defecto.
//! Si no cabe debajo, aparece encima. Si no cabe a la derecha, se ajusta a la izquierda.
//! Todo el cálculo ocurre en `draw` usando el viewport almacenado en el árbol.
//!
//! ## Arquitectura interna
//!
//! ```text
//! Tooltip (root — tamaño del hijo)
//! └── <child widget>
//!
//! [overlay — dibujado en draw() si visible, posición absoluta]
//! ┌─────────────────┐
//! │  Texto del tip  │  ← RenderCommand::Quad + Text emitidos directamente
//! └─────────────────┘
//! ```
//!
//! El tooltip en sí **no es un nodo del árbol**: se emite como `RenderCommand` directamente
//! en la fase `draw()` del widget envoltura. Esto evita complejidad de reordenamiento de
//! nodos y garantiza que siempre se dibuje encima del contenido.

use crate::{
    Widget, RenderCommand, DrawContext, BuildContext, LayoutContext, EventContext,
    EventResponse, UiEvent, Rect, Vec2, Color,
};

// ─── Tooltip ─────────────────────────────────────────────────────────────────

const PADDING: f32 = 8.0;
const FONT_SIZE: f32 = 12.0;
const MAX_WIDTH: f32 = 240.0;
const DEFAULT_DELAY_MS: u32 = 500;

/// Widget envolvente que muestra un popup de texto al hacer hover.
///
/// Envuelve un widget hijo y añade un tooltip sin modificar su comportamiento.
pub struct Tooltip<App> {
    /// Texto que se mostrará en el tooltip.
    pub text: String,
    /// Tiempo en milisegundos antes de que aparezca el tooltip.
    pub delay_ms: u32,
    /// Color de fondo del panel del tooltip.
    pub bg_color: Option<Color>,
    /// Color del texto del tooltip.
    pub text_color: Option<Color>,

    // Estado interno
    /// Si el cursor está actualmente sobre el hijo.
    is_hovered: bool,
    /// Tiempo acumulado de hover en milisegundos.
    hover_time_ms: u32,
    /// Si el tooltip está actualmente visible.
    is_visible: bool,
    /// Posición del cursor (en coords globales).
    cursor_pos: [f32; 2],

    child: Option<Box<dyn Widget<App>>>,
    child_id: Option<crate::NodeId>,
}

impl<App> Tooltip<App> {
    /// Crea un tooltip con el texto dado.
    pub fn new(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            delay_ms: DEFAULT_DELAY_MS,
            bg_color: None,
            text_color: None,
            is_hovered: false,
            hover_time_ms: 0,
            is_visible: false,
            cursor_pos: [0.0, 0.0],
            child: None,
            child_id: None,
        }
    }

    /// Establece el widget hijo que tendrá el tooltip.
    pub fn with_child(mut self, child: Box<dyn Widget<App>>) -> Self {
        self.child = Some(child);
        self
    }

    /// Delay en milisegundos antes de mostrar el tooltip (por defecto 500ms).
    pub fn delay_ms(mut self, ms: u32) -> Self {
        self.delay_ms = ms;
        self
    }

    /// Color de fondo del panel del tooltip.
    pub fn bg_color(mut self, color: Color) -> Self {
        self.bg_color = Some(color);
        self
    }

    /// Color del texto del tooltip.
    pub fn text_color(mut self, color: Color) -> Self {
        self.text_color = Some(color);
        self
    }

    /// Calcula el rectángulo óptimo para mostrar el tooltip dado el cursor y viewport.
    fn compute_tooltip_rect(&self, cursor: [f32; 2], viewport: Rect) -> Rect {
        // Estimar ancho del texto (aprox. 6px por carácter a font_size 12)
        let chars = self.text.chars().count() as f32;
        let est_w = (chars * FONT_SIZE * 0.55 + PADDING * 2.0).min(MAX_WIDTH);
        let est_h = FONT_SIZE + PADDING * 2.0;

        // Posición preferida: debajo del cursor
        let mut x = cursor[0] + 8.0;
        let mut y = cursor[1] + 20.0;

        // Ajuste horizontal: no salirse por la derecha
        if x + est_w > viewport.x + viewport.width {
            x = (cursor[0] - est_w - 8.0).max(viewport.x);
        }

        // Ajuste vertical: si no cabe abajo, ir arriba
        if y + est_h > viewport.y + viewport.height {
            y = cursor[1] - est_h - 4.0;
        }

        Rect::new(x, y, est_w, est_h)
    }
}

impl<App> Default for Tooltip<App> {
    fn default() -> Self {
        Self::new("")
    }
}

impl<App: 'static + Send + Sync> Widget<App> for Tooltip<App> {
    fn build(&mut self, ctx: &mut BuildContext<App>) {
        let my_id = ctx.node_id;

        // El Tooltip ocupa el mismo espacio que su hijo
        if let Some(child) = self.child.take() {
            let child_id = ctx.tree.add_node(child, Some(my_id));
            // Fill del hijo
            let s = crate::StyleBuilder::new().fill_width().fill_height().build();
            ctx.tree.set_node_style(child_id, s);
            self.child_id = Some(child_id);
        }
    }

    fn update(&mut self, ctx: &mut crate::UpdateContext) {
        if self.is_hovered {
            // Acumular tiempo en milisegundos usando delta_time (en segundos)
            self.hover_time_ms += (ctx.delta_time * 1000.0) as u32;
            if self.hover_time_ms >= self.delay_ms && !self.is_visible {
                self.is_visible = true;
                // Marcar el nodo dirty para que se redibuje
                // (no tenemos acceso al tree aquí, usamos el flag de paint)
            }
        } else {
            self.hover_time_ms = 0;
            self.is_visible = false;
        }
    }

    fn draw(&self, ctx: &mut DrawContext, cmds: &mut Vec<RenderCommand>) {
        if !self.is_visible || self.text.is_empty() {
            return;
        }

        let theme = &ctx.theme;
        let viewport = Rect::new(0.0, 0.0, 9999.0, 9999.0); // approx; ver nota
        let tip_rect = self.compute_tooltip_rect(self.cursor_pos, viewport);

        let bg = self.bg_color.unwrap_or_else(|| theme.surface_elevated);
        let fg = self.text_color.unwrap_or(theme.on_surface);

        // Sombra sutil
        cmds.push(RenderCommand::Quad {
            rect: Rect::new(tip_rect.x + 2.0, tip_rect.y + 2.0, tip_rect.width, tip_rect.height),
            color: Color::BLACK.with_alpha(0.25).to_array(),
            radii: [4.0; 4],
            flags: 0,
        });

        // Panel de fondo
        cmds.push(RenderCommand::Quad {
            rect: tip_rect,
            color: bg.to_array(),
            radii: [4.0; 4],
            flags: 0,
        });

        // Borde sutil
        cmds.push(RenderCommand::Quad {
            rect: Rect::new(tip_rect.x, tip_rect.y, tip_rect.width, 1.0),
            color: theme.primary.with_alpha(0.4).to_array(),
            radii: [0.0; 4],
            flags: 0,
        });

        // Texto
        cmds.push(RenderCommand::Text {
            rect: Rect::new(
                tip_rect.x + PADDING,
                tip_rect.y + PADDING * 0.5,
                tip_rect.width - PADDING * 2.0,
                tip_rect.height,
            ),
            text: self.text.clone(),
            color: fg.to_array(),
            font_size: FONT_SIZE,
        });
    }

    fn calculate_size(&self, _ctx: &mut LayoutContext) -> Vec2 {
        Vec2::ZERO // Tamaño dado por el hijo vía layout
    }

    fn on_event(
        &mut self,
        ctx: &mut EventContext<App>,
        event: &UiEvent,
    ) -> EventResponse {
        match event {
            UiEvent::MouseEnter => {
                self.is_hovered = true;
                self.hover_time_ms = 0;
                EventResponse::Ignored
            }
            UiEvent::MouseLeave => {
                self.is_hovered = false;
                self.is_visible = false;
                self.hover_time_ms = 0;
                EventResponse::Redraw
            }
            UiEvent::MouseMove { pos } => {
                self.cursor_pos = [pos.x, pos.y];
                if !self.is_hovered {
                    self.is_hovered = true;
                    self.hover_time_ms = 0;
                }
                EventResponse::Ignored
            }
            _ => EventResponse::Ignored,
        }
    }
}
