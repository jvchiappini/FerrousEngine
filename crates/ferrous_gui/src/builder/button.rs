//! [`ButtonBuilder`] — builder fluent para botones con eventos encadenados.

use ferrous_ui_core::{Button, EventContext, HAlign, NodeId, TextAlign, VAlign};

use super::base::{impl_builder_base, BuilderBase};
use crate::UiSystem;

// =========================================================================
// ButtonBuilder
// =========================================================================

/// Builder fluent para `Button` con soporte de eventos encadenados.
///
/// ```rust,ignore
/// ui.button("Guardar")
///     .at(100.0, 200.0)
///     .size(120.0, 36.0)
///     .on_click(|_ctx| println!("Guardado!"))
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

    /// Establece el mismo radio de borde para las 4 esquinas (en píxeles).
    pub fn border_radius(mut self, r: f32) -> Self {
        self.inner = self.inner.with_border_radius(r);
        self
    }

    /// Establece radios individuales por esquina:
    /// `[top-left, top-right, bottom-right, bottom-left]` en píxeles.
    pub fn radii(mut self, tl: f32, tr: f32, br: f32, bl: f32) -> Self {
        self.inner = self.inner.with_radii(tl, tr, br, bl);
        self
    }

    /// Establece la alineación completa del texto del label.
    pub fn text_align(mut self, align: TextAlign) -> Self {
        self.inner = self.inner.with_text_align(align);
        self
    }

    /// Alineación horizontal del label: `HAlign::Left`, `Center`, `Right`, o `Custom`.
    pub fn h_align(mut self, h: HAlign) -> Self {
        self.inner = self.inner.with_h_align(h);
        self
    }

    /// Alineación vertical del label: `VAlign::Top`, `Center`, `Bottom`, o `Custom`.
    pub fn v_align(mut self, v: VAlign) -> Self {
        self.inner = self.inner.with_v_align(v);
        self
    }

    /// Callback ejecutado al hacer clic.
    pub fn on_click(mut self, f: impl Fn(&mut EventContext<App>) + Send + Sync + 'static) -> Self {
        self.inner = self.inner.on_click(f);
        self
    }

    /// Callback ejecutado cuando el puntero entra al botón.
    pub fn on_hover(mut self, f: impl Fn(&mut EventContext<App>) + Send + Sync + 'static) -> Self {
        self.inner = self.inner.on_hover(f);
        self
    }

    /// Callback ejecutado cuando el puntero sale del botón.
    pub fn on_hover_end(
        mut self,
        f: impl Fn(&mut EventContext<App>) + Send + Sync + 'static,
    ) -> Self {
        self.inner = self.inner.on_hover_end(f);
        self
    }

    /// Instancia el botón en el `UiSystem` y devuelve su `NodeId`.
    pub fn spawn(self, ui: &mut UiSystem<App>) -> NodeId {
        let (style, explicit_parent, id_str) = self.base.into_style();
        let parent = explicit_parent.or_else(|| ui.current_parent());
        let id = ui
            .tree
            .add_node_with_id(Box::new(self.inner), parent, id_str);
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
