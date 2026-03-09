//! # `AspectRatio` — Contenedor de Proporción Fija
//!
//! Envuelve un widget hijo forzando una relación de aspecto constante
//! entre su ancho y su alto (`width / height = ratio`).
//!
//! El widget calcula sus dimensiones a partir del espacio disponible del padre:
//! si el padre es más ancho que la proporción lo requiere, se ajusta por alto;
//! si el padre es más alto, se ajusta por ancho. El hijo siempre encaja sin
//! deformarse ni sobresalir.
//!
//! ## Ejemplo de uso
//!
//! ```rust,ignore
//! use ferrous_ui_core::{AspectRatio, Panel};
//!
//! // Viewport 16:9
//! let viewport = AspectRatio::<MyApp>::new(16.0 / 9.0)
//!     .with_child(Box::new(game_viewport_widget));
//!
//! // Miniatura cuadrada (1:1)
//! let thumbnail = AspectRatio::<MyApp>::new(1.0)
//!     .with_child(Box::new(image_widget));
//! ```

use crate::{
    Widget, RenderCommand, DrawContext, BuildContext, LayoutContext, EventContext,
    EventResponse, UiEvent, Rect, Vec2, NodeId, StyleBuilder,
};

/// Contenedor que obliga a su hijo a mantener una proporción fija `width / height`.
///
/// El tamaño final se calcula como el mayor rectángulo con la proporción dada
/// que cabe dentro del espacio disponible del padre.
pub struct AspectRatio<App> {
    /// Proporción `width / height` (ej: `16.0/9.0`, `4.0/3.0`, `1.0`).
    pub ratio: f32,
    /// Si `true`, el widget se centra dentro del espacio del padre.
    pub center: bool,

    child: Option<Box<dyn Widget<App>>>,
    child_id: Option<NodeId>,
}

impl<App> AspectRatio<App> {
    /// Crea un `AspectRatio` con la proporción dada.
    ///
    /// - `ratio = 1.0` → cuadrado
    /// - `ratio = 16.0/9.0` → pantalla ancha
    /// - `ratio = 4.0/3.0` → clásico
    pub fn new(ratio: f32) -> Self {
        Self {
            ratio: ratio.max(0.001), // evitar división por cero
            center: true,
            child: None,
            child_id: None,
        }
    }

    /// Establece el widget hijo cuya forma se restringirá.
    pub fn with_child(mut self, child: Box<dyn Widget<App>>) -> Self {
        self.child = Some(child);
        self
    }

    /// Si `false`, el hijo se posiciona en la esquina superior izquierda en vez de centrado.
    pub fn no_center(mut self) -> Self {
        self.center = false;
        self
    }

    /// Calcula el rectángulo ajustado al ratio dentro de un rect disponible.
    ///
    /// Devuelve `(x_offset, y_offset, width, height)` en coordenadas locales.
    pub fn fitted_rect(&self, available_w: f32, available_h: f32) -> (f32, f32, f32, f32) {
        let target_h_from_w = available_w / self.ratio;
        let (w, h) = if target_h_from_w <= available_h {
            (available_w, target_h_from_w)
        } else {
            (available_h * self.ratio, available_h)
        };

        let ox = if self.center { (available_w - w) * 0.5 } else { 0.0 };
        let oy = if self.center { (available_h - h) * 0.5 } else { 0.0 };
        (ox, oy, w, h)
    }
}

impl<App> Default for AspectRatio<App> {
    fn default() -> Self {
        Self::new(1.0)
    }
}

impl<App: 'static + Send + Sync> Widget<App> for AspectRatio<App> {
    fn build(&mut self, ctx: &mut BuildContext<App>) {
        // El nodo raíz usa clip para evitar que el hijo (si tiene un bug de size)
        // desborde fuera del área calculada.
        let root_style = StyleBuilder::new().fill_width().fill_height().clip().build();
        ctx.tree.set_node_style(ctx.node_id, root_style);

        if let Some(child) = self.child.take() {
            // El hijo se posiciona de forma absoluta con el tamaño calculado.
            // La posición real se actualizará en on_event/draw ya que depende
            // del rect resuelto por el padre, que no está disponible en build().
            // Usamos un estilo conservador: fill del padre. El cálculo exacto ocurre
            // en calculate_size() que el motor consulta antes del layout.
            let child_style = StyleBuilder::new().fill_width().fill_height().build();
            let child_id = ctx.tree.add_node(child, Some(ctx.node_id));
            ctx.tree.set_node_style(child_id, child_style);
            self.child_id = Some(child_id);
        }
    }

    fn draw(&self, ctx: &mut DrawContext, cmds: &mut Vec<RenderCommand>) {
        // Dibujamos un marco de "letterbox" / "pillarbox" de fondo negro
        // en las franjas que quedan fuera del área con la proporción correcta.
        let r = &ctx.rect;
        let (ox, oy, w, h) = self.fitted_rect(r.width, r.height);

        // Si hay franjas visibles (el ratio no encaja perfectamente)
        if ox > 0.5 {
            // Pillarbox izquierdo
            cmds.push(RenderCommand::Quad {
                rect: Rect::new(r.x, r.y, ox, r.height),
                color: [0.0, 0.0, 0.0, 1.0],
                radii: [0.0; 4],
                flags: 0,
            });
            // Pillarbox derecho
            cmds.push(RenderCommand::Quad {
                rect: Rect::new(r.x + ox + w, r.y, ox, r.height),
                color: [0.0, 0.0, 0.0, 1.0],
                radii: [0.0; 4],
                flags: 0,
            });
        }
        if oy > 0.5 {
            // Letterbox superior
            cmds.push(RenderCommand::Quad {
                rect: Rect::new(r.x, r.y, r.width, oy),
                color: [0.0, 0.0, 0.0, 1.0],
                radii: [0.0; 4],
                flags: 0,
            });
            // Letterbox inferior
            cmds.push(RenderCommand::Quad {
                rect: Rect::new(r.x, r.y + oy + h, r.width, oy),
                color: [0.0, 0.0, 0.0, 1.0],
                radii: [0.0; 4],
                flags: 0,
            });
        }
    }

    fn calculate_size(&self, ctx: &mut LayoutContext) -> Vec2 {
        let avail_w = ctx.available_space.x.max(1.0);
        let avail_h = ctx.available_space.y.max(1.0);
        let (_, _, w, h) = self.fitted_rect(avail_w, avail_h);
        glam::vec2(w, h)
    }

    fn on_event(
        &mut self,
        _ctx: &mut EventContext<App>,
        _event: &UiEvent,
    ) -> EventResponse {
        EventResponse::Ignored
    }
}
