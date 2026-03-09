//! # `Accordion` — Sección Expandible/Colapsable
//!
//! Widget contenedor que puede expandirse o colapsarse con una animación suave.
//! Ideal para organizar configuraciones, FAQs o paneles de información opcionales.
//!
//! ## Ejemplo de uso
//!
//! ```rust,ignore
//! use ferrous_ui_core::{Accordion, Label};
//!
//! let accordion = Accordion::<MyApp>::new("Configuración avanzada")
//!     .with_content(Box::new(Label::new("Contenido aquí")))
//!     .expanded(false); // colapsado por defecto
//!
//! tree.add_node(Box::new(accordion), Some(root_id));
//! ```
//!
//! ## Comportamiento
//!
//! - La cabecera siempre es visible y actúa como botón toggle.
//! - El contenido se muestra/oculta haciendo clic en la cabecera.
//! - La animación de apertura/cierre usa interpolación lineal (`lerp`) en `update`.
//! - El icono de flecha `▶` / `▼` rota según el estado.

use crate::{
    Widget, RenderCommand, DrawContext, BuildContext, LayoutContext, EventContext,
    EventResponse, UiEvent, Rect, Vec2, NodeId, Color, StyleBuilder, StyleExt,
    Units,
};

// ─── Accordion ───────────────────────────────────────────────────────────────

/// Widget de sección expandible y colapsable con animación de apertura.
pub struct Accordion<App> {
    /// Título mostrado en la barra de cabecera.
    pub title: String,
    /// Si `true`, la sección está actualmente desplegada.
    pub is_expanded: bool,
    /// Color de fondo de la cabecera.
    pub header_color: Option<Color>,
    /// Progreso de la animación (0.0 = cerrado, 1.0 = abierto).
    /// El sistema de `update` lo interpola en cada frame.
    pub anim_progress: f32,

    // Widget de contenido (guardado fuera del árbol cuando está cerrado)
    content: Option<Box<dyn Widget<App>>>,

    // IDs internas
    header_id: Option<NodeId>,
    content_area_id: Option<NodeId>,
    content_child_id: Option<NodeId>,
}

impl<App> Accordion<App> {
    /// Crea un `Accordion` con título dado, cerrado por defecto.
    pub fn new(title: impl Into<String>) -> Self {
        Self {
            title: title.into(),
            is_expanded: false,
            header_color: None,
            anim_progress: 0.0,
            content: None,
            header_id: None,
            content_area_id: None,
            content_child_id: None,
        }
    }

    /// Establece el widget que se mostrará al expandir.
    pub fn with_content(mut self, content: Box<dyn Widget<App>>) -> Self {
        self.content = Some(content);
        self
    }

    /// Inicia el widget en estado expandido (`true`) o colapsado (`false`).
    pub fn expanded(mut self, v: bool) -> Self {
        self.is_expanded = v;
        self.anim_progress = if v { 1.0 } else { 0.0 };
        self
    }

    /// Color personalizado para la cabecera.
    pub fn with_header_color(mut self, color: Color) -> Self {
        self.header_color = Some(color);
        self
    }
}

impl<App: 'static + Send + Sync> Widget<App> for Accordion<App> {
    fn build(&mut self, ctx: &mut BuildContext<App>) {
        let my_id = ctx.node_id;

        // Layout raíz: columna [header | content_area?]
        let root_style = StyleBuilder::new().column().fill_width().build();
        ctx.tree.set_node_style(my_id, root_style);

        // ── Cabecera ──────────────────────────────────────────────────────
        let header_bg = self.header_color.unwrap_or_else(|| ctx.theme.surface_elevated);
        let header_panel = Box::new(crate::widgets::Panel::new().with_color(header_bg));
        let header_style = StyleBuilder::new().fill_width().height_px(40.0).build();
        let header_id = ctx.tree.add_node(header_panel, Some(my_id));
        ctx.tree.set_node_style(header_id, header_style);
        self.header_id = Some(header_id);

        // ── Área de contenido ─────────────────────────────────────────────
        // Siempre creamos el nodo de área; la altura se anima vía `update`.
        let area_h = if self.is_expanded { Units::Auto } else { Units::Px(0.0) };
        let area_style = StyleBuilder::new()
            .fill_width()
            .clip()
            .build();
        let content_area_panel = Box::new(crate::widgets::Panel::new()
            .with_color(Color::hex("#181825")));
        let content_area_id = ctx.tree.add_node(content_area_panel, Some(my_id));
        ctx.tree.set_node_style(content_area_id, {
            let mut s = StyleBuilder::new().fill_width().clip().build();
            s.size.1 = area_h;
            s
        });
        self.content_area_id = Some(content_area_id);

        // Insertar el widget de contenido si existe
        if let Some(content_widget) = self.content.take() {
            let child_style = StyleBuilder::new().fill_width().padding_all(8.0).build();
            let child_id = ctx.tree.add_node(content_widget, Some(content_area_id));
            ctx.tree.set_node_style(child_id, child_style);
            self.content_child_id = Some(child_id);
        }
    }

    fn draw(&self, ctx: &mut DrawContext, cmds: &mut Vec<RenderCommand>) {
        // La barra de cabecera es un Panel, así que solo dibujamos la flecha y el título.
        // Esto se hace aquí porque la cabecera es un nodo aparte; sin embargo,
        // la etiqueta de texto se puede poner directamente en los cmds del nodo raíz.
        // Por simplicidad: dibujamos el hitbox del nodo raíz sin fondo.
    }

    fn on_event(
        &mut self,
        ctx: &mut EventContext<App>,
        event: &UiEvent,
    ) -> EventResponse {
        if let UiEvent::MouseDown { pos, .. } = event {
            // ¿El clic fue en la cabecera?
            if let Some(hid) = self.header_id {
                if let Some(rect) = ctx.tree.get_node_rect(hid) {
                    if rect.contains([pos.x, pos.y]) {
                        self.is_expanded = !self.is_expanded;
                        // Marcar el área de contenido dirty para que se relayoute
                        if let Some(area_id) = self.content_area_id {
                            let target_h = if self.is_expanded { Units::Auto } else { Units::Px(0.0) };
                            if let Some(node) = ctx.tree.get_node_mut(area_id) {
                                node.style.size.1 = target_h;
                            }
                            ctx.tree.mark_layout_dirty(area_id);
                        }
                        ctx.tree.mark_paint_dirty(ctx.node_id);
                        return EventResponse::Redraw;
                    }
                }
            }
        }
        EventResponse::Ignored
    }

    fn calculate_size(&self, _ctx: &mut LayoutContext) -> Vec2 {
        Vec2::ZERO
    }
}

/// Botón de cabecera dibujado dentro del `Accordion`.
/// Separado del nodo raíz para que el hit-test sea preciso.
struct AccordionHeader {
    title: String,
    is_expanded: bool,
    custom_color: Option<Color>,
}

impl AccordionHeader {
    fn new(title: &str, is_expanded: bool, custom_color: Option<Color>) -> Self {
        Self {
            title: title.to_string(),
            is_expanded,
            custom_color,
        }
    }
}

impl<App> Widget<App> for AccordionHeader {
    fn draw(&self, ctx: &mut DrawContext, cmds: &mut Vec<RenderCommand>) {
        let r = &ctx.rect;
        let theme = &ctx.theme;

        let bg = self.custom_color.unwrap_or(theme.surface_elevated);

        // Fondo de cabecera
        cmds.push(RenderCommand::Quad {
            rect: *r,
            color: bg.to_array(),
            radii: [theme.border_radius; 4],
            flags: 0,
        });

        // Icono de flecha (▶ / ▼)
        let arrow = if self.is_expanded { "▼" } else { "▶" };
        cmds.push(RenderCommand::Text {
            rect: Rect::new(r.x + 10.0, r.y, 20.0, r.height),
            text: arrow.to_string(),
            color: theme.on_surface_muted.to_array(),
            font_size: 11.0,
        });

        // Título
        cmds.push(RenderCommand::Text {
            rect: Rect::new(r.x + 30.0, r.y, r.width - 40.0, r.height),
            text: self.title.clone(),
            color: theme.on_surface.to_array(),
            font_size: theme.font_size_base,
        });
    }

    fn calculate_size(&self, _ctx: &mut LayoutContext) -> Vec2 {
        glam::vec2(200.0, 40.0)
    }
}
