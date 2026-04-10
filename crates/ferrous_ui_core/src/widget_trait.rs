//! Widget trait — define el ciclo de vida y comportamiento de todo widget de UI.

use std::cell::RefCell;
use std::rc::Rc;

use glam::Vec2;

use crate::BuildContext;
use crate::DrawContext;
use crate::EventContext;
use crate::EventResponse;
use crate::FerrousWidgetReflect;
use crate::LayoutContext;
use crate::RenderCommand;
use crate::UpdateContext;
use crate::UiEvent;

/// Trait central que define el ciclo de vida de un widget.
///
/// # Ciclo de vida por frame
///
/// 1. **`build`** — Se llama una sola vez al insertar el nodo. Añade hijos iniciales.
/// 2. **`update`** — Cada frame: animaciones, timers, estado interno.
/// 3. **`calculate_size`** — El engine de layout consulta el tamaño preferido.
/// 4. **`draw`** — Genera `RenderCommand`s. Solo se llama si el nodo está `paint_dirty`.
/// 5. **`on_event`** — Recibe eventos de input ya enrutados por `EventManager`.
///
/// # Hit-testing automático
///
/// El sistema despacha eventos automáticamente. El usuario **no** necesita escribir
/// lógica de detección de mouse. Simplemente registra callbacks:
///
/// ```rust,ignore
/// ui.button("OK")
///     .on_click(|ctx| ctx.app.save())
///     .size(120.0, 36.0)
///     .spawn(&mut ui);
/// ```
///
/// Para formas no rectangulares, sobreescribe `hit_test` para dar precisión pixel-perfect:
///
/// ```rust,ignore
/// fn hit_test(&self, local_pos: Vec2, size: Vec2) -> bool {
///     // Ejemplo: botón circular
///     let center = size * 0.5;
///     local_pos.distance(center) <= center.x
/// }
/// ```
pub trait Widget<App> {
    // ── Ciclo de vida ─────────────────────────────────────────────────────────

    /// Se invoca cuando el widget se inserta en el árbol. Es el lugar para añadir hijos.
    fn build(&mut self, _ctx: &mut BuildContext<App>) {}

    /// Se invoca en cada frame para actualizar el estado interno (animaciones, timers, etc.).
    fn update(&mut self, _ctx: &mut UpdateContext) {}

    /// Define el tamaño ideal que este widget desea ocupar.
    /// El engine de layout lo usará como sugerencia pero puede ignorarlo si el padre es Flex.
    fn calculate_size(&self, _ctx: &mut LayoutContext) -> Vec2 {
        Vec2::ZERO
    }

    /// Genera la lista de comandos de renderizado para representar visualmente el widget.
    /// Este método solo se llama cuando `node.dirty.paint == true`.
    /// Los comandos se cachean en `Node::cached_cmds` y se reutilizan en frames limpios.
    fn draw(&self, _ctx: &mut DrawContext, _cmds: &mut Vec<RenderCommand>) {}

    /// Se invoca cuando ocurre un evento que afecta a este widget (ya enrutado por hit-test).
    ///
    /// Retorna `EventResponse::Ignored` para propagar el evento al padre (bubbling).
    /// Retorna `EventResponse::Consumed` para absorberlo sin necesitar redibujado.
    /// Retorna `EventResponse::Redraw` para absorberlo y marcar el nodo como paint-dirty.
    fn on_event(&mut self, _ctx: &mut EventContext<App>, _event: &UiEvent) -> EventResponse {
        EventResponse::Ignored
    }

    // ── Hit-testing ───────────────────────────────────────────────────────────

    /// Comprueba si un punto en **coordenadas locales** (relativas al origen del widget)
    /// está dentro del área interactiva del widget.
    ///
    /// Por defecto: AABB simple (siempre true si el punto está dentro del rect).
    /// Sobreescribir para formas especiales: círculos, elipses, polígonos, etc.
    ///
    /// `local_pos`: posición del cursor relativa a `(rect.x, rect.y)` del widget.
    /// `size`: tamaño del widget en píxeles `(rect.width, rect.height)`.
    fn hit_test(&self, local_pos: Vec2, size: Vec2) -> bool {
        local_pos.x >= 0.0 && local_pos.y >= 0.0 && local_pos.x <= size.x && local_pos.y <= size.y
    }
    
    /// Cuando es `true`, el `EventManager` usará el GPU ID Buffer para máxima precisión.
    fn needs_gpu_hit_test(&self) -> bool {
        false
    }

    // ── Scroll ────────────────────────────────────────────────────────────────

    /// Devuelve el desplazamiento de scroll actual si el widget lo soporta.
    fn scroll_offset(&self) -> Vec2 {
        Vec2::ZERO
    }

    // ── Reflexión ─────────────────────────────────────────────────────────────

    /// Devuelve la interfaz de reflexión para este widget (para el editor).
    fn reflect(&self) -> Option<&dyn FerrousWidgetReflect> {
        None
    }

    /// Devuelve la interfaz de reflexión mutable para este widget.
    fn reflect_mut(&mut self) -> Option<&mut dyn FerrousWidgetReflect> {
        None
    }
}

// ─── Impl para Rc<RefCell<W>> ──────────────────────────────────────────────────

impl<App, W: Widget<App>> Widget<App> for Rc<RefCell<W>> {
    fn build(&mut self, ctx: &mut BuildContext<App>) {
        self.borrow_mut().build(ctx)
    }

    fn update(&mut self, ctx: &mut UpdateContext) {
        self.borrow_mut().update(ctx)
    }

    fn calculate_size(&self, ctx: &mut LayoutContext) -> Vec2 {
        self.borrow().calculate_size(ctx)
    }

    fn draw(&self, ctx: &mut DrawContext, cmds: &mut Vec<RenderCommand>) {
        self.borrow().draw(ctx, cmds)
    }

    fn on_event(&mut self, ctx: &mut EventContext<App>, event: &UiEvent) -> EventResponse {
        self.borrow_mut().on_event(ctx, event)
    }

    fn hit_test(&self, local_pos: Vec2, size: Vec2) -> bool {
        self.borrow().hit_test(local_pos, size)
    }

    fn needs_gpu_hit_test(&self) -> bool {
        self.borrow().needs_gpu_hit_test()
    }
}