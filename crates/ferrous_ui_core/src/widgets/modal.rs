//! # `Modal` / `Dialog` — Ventana Flotante Bloqueante
//!
//! `Modal` presenta contenido sobre un backdrop semitransparente que bloquea la
//! interacción con el resto de la UI mientras la ventana está abierta.
//!
//! ## API de uso
//!
//! ```rust,ignore
//! use ferrous_ui_core::{Modal, Label, Button};
//!
//! // Crear el Modal con contenido
//! let mut modal = Modal::<MyApp>::new()
//!     .with_title("Confirmar acción")
//!     .with_content(Box::new(Label::new("¿Deseas eliminar el archivo?")))
//!     .with_width(360.0)
//!     .with_height(180.0);
//!
//! // Añadir al árbol (inactivo por defecto)
//! let modal_id = tree.add_node(Box::new(modal), Some(root_id));
//!
//! // Desde cualquier callback con EventContext:
//! // Para abrir:
//! if let Some(node) = ctx.tree.get_node_mut(modal_id) {
//!     if let Some(m) = node.widget.downcast_mut::<Modal<MyApp>>() {
//!         m.open();
//!     }
//! }
//! ```
//!
//! ## Comportamiento
//!
//! - **Backdrop**: cubre toda la pantalla con `rgba(0,0,0,0.55)`. Consume `MouseDown` para
//!   cerrar el modal al hacer clic fuera (configurable con `.close_on_backdrop(bool)`).
//! - **Panel del diálogo**: centrado en el viewport, con sombra, borde y color de superficie.
//! - **Bloqueo de eventos**: mientras está abierto, el backdrop captura todos los eventos
//!   de mouse antes de que lleguen a nodos detrás.
//!
//! ## Arquitectura interna
//!
//! ```text
//! Modal (root — Position::Absolute, fill viewport)
//! ├── Backdrop Quad         ← rgba(0,0,0,0.55), consume clicks fuera
//! └── Dialog Panel          ← centrado, tamaño configurado
//!     ├── Barra de título   ← texto + botón [×]
//!     └── <content widget>  ← proporcionado por el usuario
//! ```
//!
//! El Modal ocupa todo el viewport con `Position::Absolute` y tamaño 100%.
//! Cuando `is_open = false`, simplemente no emite ningún `RenderCommand` ni consume eventos.

use crate::{
    Widget, RenderCommand, DrawContext, BuildContext, LayoutContext, EventContext,
    EventResponse, UiEvent, Rect, Vec2, Color, NodeId, StyleBuilder, StyleExt,
};

// ─── Modal ────────────────────────────────────────────────────────────────────

/// Diálogo flotante bloqueante con backdrop.
///
/// Se posiciona de forma absoluta cubriendo todo el viewport.
/// El contenido se centra automáticamente.
pub struct Modal<App> {
    /// Título mostrado en la barra superior del diálogo.
    pub title: String,
    /// Si `true`, el diálogo está actualmente visible.
    pub is_open: bool,
    /// Si `true`, hacer clic en el backdrop cierra el modal.
    pub close_on_backdrop: bool,

    /// Ancho del panel de diálogo en píxeles.
    pub dialog_width: f32,
    /// Alto del panel de diálogo en píxeles.
    pub dialog_height: f32,

    /// Color del backdrop (por defecto negro semitransparente).
    pub backdrop_color: Color,

    content: Option<Box<dyn Widget<App>>>,
    content_id: Option<NodeId>,
    close_btn_id: Option<NodeId>,

    on_close_cb: Option<Box<dyn Fn(&mut EventContext<App>) + Send + Sync + 'static>>,
}

impl<App> Modal<App> {
    /// Crea un Modal cerrado y vacío.
    pub fn new() -> Self {
        Self {
            title: String::new(),
            is_open: false,
            close_on_backdrop: true,
            dialog_width: 400.0,
            dialog_height: 240.0,
            backdrop_color: Color::from_rgba8(0, 0, 0, 140),
            content: None,
            content_id: None,
            close_btn_id: None,
            on_close_cb: None,
        }
    }

    /// Título que aparece en la barra superior del diálogo.
    pub fn with_title(mut self, title: impl Into<String>) -> Self {
        self.title = title.into();
        self
    }

    /// Widget de contenido que se muestra dentro del diálogo.
    pub fn with_content(mut self, content: Box<dyn Widget<App>>) -> Self {
        self.content = Some(content);
        self
    }

    /// Ancho del panel de diálogo en píxeles.
    pub fn with_width(mut self, w: f32) -> Self {
        self.dialog_width = w;
        self
    }

    /// Alto del panel de diálogo en píxeles.
    pub fn with_height(mut self, h: f32) -> Self {
        self.dialog_height = h;
        self
    }

    /// Si `true` (por defecto), hacer clic en el backdrop cierra el modal.
    pub fn close_on_backdrop(mut self, v: bool) -> Self {
        self.close_on_backdrop = v;
        self
    }

    /// Color personalizado del fondo backdrop.
    pub fn backdrop_color(mut self, color: Color) -> Self {
        self.backdrop_color = color;
        self
    }

    /// Abre el diálogo.
    pub fn open(&mut self) {
        self.is_open = true;
    }

    /// Cierra el diálogo.
    pub fn close(&mut self) {
        self.is_open = false;
    }

    /// Alterna el estado abierto/cerrado.
    pub fn toggle(&mut self) {
        self.is_open = !self.is_open;
    }

    /// Registra una función que se invoca cuando el usuario cierra el modal
    /// (clic en backdrop, botón [×] o tecla Escape).
    pub fn on_close(mut self, f: impl Fn(&mut EventContext<App>) + Send + Sync + 'static) -> Self {
        self.on_close_cb = Some(Box::new(f));
        self
    }

    /// Calcula el rectángulo centrado del panel de diálogo dado un viewport.
    fn dialog_rect(&self, viewport: Rect) -> Rect {
        let x = viewport.x + (viewport.width - self.dialog_width) * 0.5;
        let y = viewport.y + (viewport.height - self.dialog_height) * 0.5;
        Rect::new(x, y, self.dialog_width, self.dialog_height)
    }
}

impl<App> Default for Modal<App> {
    fn default() -> Self {
        Self::new()
    }
}

impl<App: 'static + Send + Sync> Widget<App> for Modal<App> {
    fn build(&mut self, ctx: &mut BuildContext<App>) {
        // Posición absoluta que cubre todo el viewport
        let root_style = StyleBuilder::new()
            .absolute()
            .top(0.0).left(0.0)
            .fill_width().fill_height()
            .build();
        ctx.tree.set_node_style(ctx.node_id, root_style);

        // Insertar el widget de contenido si existe (siempre en el árbol, invisible cuando cerrado)
        if let Some(content) = self.content.take() {
            let content_style = StyleBuilder::new().fill_width().fill_height().build();
            let content_id = ctx.tree.add_node(content, Some(ctx.node_id));
            ctx.tree.set_node_style(content_id, content_style);
            self.content_id = Some(content_id);
        }
    }

    fn draw(&self, ctx: &mut DrawContext, cmds: &mut Vec<RenderCommand>) {
        if !self.is_open {
            return;
        }

        let theme = &ctx.theme;

        // ── Backdrop ──────────────────────────────────────────────────────
        // Cubrimos el rect del nodo (que debería ser el viewport completo)
        cmds.push(RenderCommand::Quad {
            rect: ctx.rect,
            color: self.backdrop_color.to_array(),
            radii: [0.0; 4],
            flags: 0,
        });

        let dr = self.dialog_rect(ctx.rect);

        // ── Sombra del diálogo ────────────────────────────────────────────
        cmds.push(RenderCommand::Quad {
            rect: Rect::new(dr.x + 6.0, dr.y + 8.0, dr.width, dr.height),
            color: Color::BLACK.with_alpha(0.35).to_array(),
            radii: [theme.border_radius + 2.0; 4],
            flags: 0,
        });

        // ── Panel del diálogo ─────────────────────────────────────────────
        cmds.push(RenderCommand::Quad {
            rect: dr,
            color: theme.surface.to_array(),
            radii: [theme.border_radius; 4],
            flags: 0,
        });

        // Borde superior (accent)
        cmds.push(RenderCommand::Quad {
            rect: Rect::new(dr.x, dr.y, dr.width, 2.0),
            color: theme.primary.to_array(),
            radii: [theme.border_radius, theme.border_radius, 0.0, 0.0],
            flags: 0,
        });

        // ── Barra de título ───────────────────────────────────────────────
        const TITLE_H: f32 = 40.0;
        cmds.push(RenderCommand::Quad {
            rect: Rect::new(dr.x, dr.y, dr.width, TITLE_H),
            color: theme.surface_elevated.to_array(),
            radii: [theme.border_radius, theme.border_radius, 0.0, 0.0],
            flags: 0,
        });

        // Texto del título
        if !self.title.is_empty() {
            cmds.push(RenderCommand::Text {
                rect: Rect::new(dr.x + 16.0, dr.y + 2.0, dr.width - 48.0, TITLE_H),
                text: self.title.clone(),
                color: theme.on_surface.to_array(),
                font_size: theme.font_size_base + 2.0,
                align: crate::TextAlign::TOP_LEFT,
            });
        }

        // Botón de cierre [×]
        let close_x = dr.x + dr.width - 32.0;
        let close_y = dr.y + 4.0;
        cmds.push(RenderCommand::Quad {
            rect: Rect::new(close_x, close_y, 28.0, 28.0),
            color: theme.on_surface_muted.with_alpha(0.1).to_array(),
            radii: [6.0; 4],
            flags: 0,
        });
        cmds.push(RenderCommand::Text {
            rect: Rect::new(close_x, close_y, 28.0, 28.0),
            text: "×".to_string(),
            color: theme.on_surface_muted.to_array(),
            font_size: 18.0,
            align: crate::TextAlign::TOP_LEFT,
        });

        // ── Área de contenido ─────────────────────────────────────────────
        // El contenido se dibuja como hijo normal del árbol (si se configuró).
        // Para contenidos simples de texto (sin widget hijo), lo dibujamos inline:
        if self.content_id.is_none() {
            cmds.push(RenderCommand::Text {
                rect: Rect::new(dr.x + 16.0, dr.y + TITLE_H + 12.0, dr.width - 32.0, dr.height - TITLE_H - 24.0),
                text: "(sin contenido)".to_string(),
                color: theme.on_surface_muted.to_array(),
                font_size: theme.font_size_base,
                align: crate::TextAlign::TOP_LEFT,
            });
        }
    }

    fn calculate_size(&self, _ctx: &mut LayoutContext) -> Vec2 {
        Vec2::ZERO // Absolute, no participa en el flujo
    }

    fn on_event(
        &mut self,
        ctx: &mut EventContext<App>,
        event: &UiEvent,
    ) -> EventResponse {
        if !self.is_open {
            return EventResponse::Ignored;
        }

        match event {
            UiEvent::MouseDown { pos, .. } => {
                let dr = self.dialog_rect(ctx.rect);

                // ¿Clic en el botón de cierre [×]?
                let close_x = dr.x + dr.width - 32.0;
                let close_y = dr.y + 4.0;
                let close_rect = Rect::new(close_x, close_y, 28.0, 28.0);
                if close_rect.contains([pos.x, pos.y]) {
                    self.is_open = false;
                    if let Some(cb) = &self.on_close_cb { cb(ctx); }
                    ctx.tree.mark_paint_dirty(ctx.node_id);
                    return EventResponse::Consumed;
                }

                // ¿Clic dentro del panel de diálogo? → no cerrar
                if dr.contains([pos.x, pos.y]) {
                    return EventResponse::Consumed; // consume para no llegar a nodos debajo
                }

                // ¿Clic en el backdrop?
                if self.close_on_backdrop {
                    self.is_open = false;
                    if let Some(cb) = &self.on_close_cb { cb(ctx); }
                    ctx.tree.mark_paint_dirty(ctx.node_id);
                    return EventResponse::Consumed;
                }

                // Aunque no cierre, consume el evento para bloquear la UI de fondo
                EventResponse::Consumed
            }

            // Bloquear todos los eventos de mouse mientras está abierto
            UiEvent::MouseMove { .. } | UiEvent::MouseWheel { .. } => {
                EventResponse::Consumed
            }

            UiEvent::KeyDown { key, .. } if *key == crate::GuiKey::Escape => {
                self.is_open = false;
                if let Some(cb) = &self.on_close_cb { cb(ctx); }
                ctx.tree.mark_paint_dirty(ctx.node_id);
                EventResponse::Consumed
            }

            _ => EventResponse::Ignored,
        }
    }
}
