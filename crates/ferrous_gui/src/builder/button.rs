//! [`ButtonBuilder`] — builder fluent para botones con eventos encadenados.

use ferrous_ui_core::{Background, Button, EventContext, HAlign, NodeId, TextAlign, VAlign};

use super::base::{impl_builder_base, BuilderBase};
use crate::UiSystem;

/// Builder fluent para `Button` con soporte completo de eventos y estilos visuales.
///
/// ```rust,ignore
/// ui.button("Guardar")
///     .size(120.0, 36.0)
///     .border_radius(8.0)
///     .background(Background::linear(
///         [0.42, 0.38, 1.0, 1.0], [0.26, 0.24, 0.8, 1.0], 90.0
///     ))
///     .on_click(|ctx| ctx.app.save())
///     .disabled(ctx.app.is_saving)
///     .spawn(&mut ui);
/// ```
pub struct ButtonBuilder<App: 'static> {
    pub(super) inner: Button<App>,
    pub(super) base: BuilderBase,
}

impl_builder_base!(ButtonBuilder<App>);

impl<App: 'static> ButtonBuilder<App> {
    pub(crate) fn new(label: impl Into<String>) -> Self {
        Self {
            inner: Button::new(label),
            base: BuilderBase::default(),
        }
    }

    // ── Estilo ────────────────────────────────────────────────────────────────

    /// Radio uniforme de las 4 esquinas del botón (píxeles).
    pub fn border_radius(mut self, r: f32) -> Self {
        self.inner = self.inner.with_border_radius(r);
        self
    }

    /// Radios individuales por esquina: [top-left, top-right, bottom-right, bottom-left].
    pub fn radii(mut self, tl: f32, tr: f32, br: f32, bl: f32) -> Self {
        self.inner = self.inner.with_radii(tl, tr, br, bl);
        self
    }

    /// Alineación completa del texto del label.
    pub fn text_align(mut self, align: TextAlign) -> Self {
        self.inner = self.inner.with_text_align(align);
        self
    }

    /// Alineación horizontal del label.
    pub fn h_align(mut self, h: HAlign) -> Self {
        self.inner = self.inner.with_h_align(h);
        self
    }

    /// Alineación vertical del label.
    pub fn v_align(mut self, v: VAlign) -> Self {
        self.inner = self.inner.with_v_align(v);
        self
    }

    /// Fondo personalizado del botón (degradado, sólido, procedural...).
    pub fn background(mut self, bg: Background) -> Self {
        self.inner = self.inner.with_background(bg);
        self
    }

    /// Desactiva el botón. Un botón desactivado no responde a eventos y se muestra atenuado.
    pub fn disabled(mut self, disabled: bool) -> Self {
        self.inner = self.inner.with_disabled(disabled);
        self
    }

    /// Activa/desactiva el chrome visual (sombra y borde de hover).
    pub fn chrome(mut self, chrome: bool) -> Self {
        self.inner = self.inner.with_chrome(chrome);
        self
    }

    // ── Eventos ───────────────────────────────────────────────────────────────

    /// Callback ejecutado al hacer clic izquierdo.
    ///
    /// El click se dispara en `MouseUp` **dentro del área del botón** —
    /// mover el cursor fuera antes de soltar cancela el click automáticamente.
    pub fn on_click(mut self, f: impl Fn(&mut EventContext<App>) + Send + Sync + 'static) -> Self {
        self.inner = self.inner.on_click(f);
        self
    }

    /// Callback ejecutado al hacer clic derecho.
    pub fn on_right_click(
        mut self,
        f: impl Fn(&mut EventContext<App>) + Send + Sync + 'static,
    ) -> Self {
        self.inner = self.inner.on_right_click(f);
        self
    }

    /// Callback ejecutado cuando el cursor entra al botón (hover start).
    pub fn on_hover(mut self, f: impl Fn(&mut EventContext<App>) + Send + Sync + 'static) -> Self {
        self.inner = self.inner.on_hover(f);
        self
    }

    /// Callback ejecutado cuando el cursor sale del botón (hover end).
    pub fn on_hover_end(
        mut self,
        f: impl Fn(&mut EventContext<App>) + Send + Sync + 'static,
    ) -> Self {
        self.inner = self.inner.on_hover_end(f);
        self
    }

    // ── Spawn ─────────────────────────────────────────────────────────────────

    /// Instancia el botón en el `UiSystem` y devuelve su `NodeId`.
    pub fn spawn(self, ui: &mut UiSystem<App>) -> NodeId {
        let (style, explicit_parent, id_str) = self.base.into_style();
        let parent = explicit_parent.or_else(|| ui.current_parent());
        let id = ui.tree.add_node_with_id(Box::new(self.inner), parent, id_str);
        ui.tree.set_node_style(id, style);
        id
    }

    /// Instancia el botón y ejecuta un closure para agregar hijos.
    pub fn spawn_with(
        self,
        ui: &mut UiSystem<App>,
        f: impl FnOnce(&mut UiSystem<App>, NodeId),
    ) -> NodeId {
        let id = self.spawn(ui);
        f(ui, id);
        id
    }
}
