//! [`PanelBuilder`] — builder fluent para el contenedor Panel (Flexbox con fondo).

use ferrous_ui_core::{Alignment, Color, DisplayMode, NodeId, Panel};

use crate::UiSystem;
use super::base::{BuilderBase, impl_builder_base};

// =========================================================================
// PanelBuilder
// =========================================================================

/// Builder fluent para `Panel` (contenedor Flexbox con fondo).
///
/// ## Comportamiento por defecto
/// - `FlexColumn` — apila hijos verticalmente.
/// - `Alignment::Stretch` — los hijos llenan el ancho del panel.
///
/// ## Ejemplo
/// ```rust,ignore
/// // Panel columna con padding y gap entre hijos
/// ui.panel()
///     .at(50.0, 50.0)
///     .size(300.0, 200.0)
///     .padding(8.0)
///     .gap(4.0)
///     .spawn_with(&mut ui, |ui, _| {
///         ui.label("Título").spawn(ui);
///         ui.button("Aceptar").size(100.0, 32.0).spawn(ui);
///     });
///
/// // Panel fila que llena el padre
/// ui.panel()
///     .fill()
///     .row()
///     .gap(8.0)
///     .spawn_with(&mut ui, |ui, _| {
///         ui.button("Izq").fill().spawn(ui);
///         ui.button("Der").fill().spawn(ui);
///     });
/// ```
pub struct PanelBuilder<App: 'static> {
    pub(super) inner: Panel,
    pub(super) base: BuilderBase,
    pub(super) _app: std::marker::PhantomData<fn() -> App>,
}

impl_builder_base!(PanelBuilder<App>);

impl<App: 'static> PanelBuilder<App> {
    pub(crate) fn new() -> Self {
        Self {
            inner: Panel::new(),
            base: BuilderBase::default(),
            _app: std::marker::PhantomData,
        }
    }

    /// Color de fondo del panel.
    pub fn color(mut self, color: Color) -> Self {
        self.inner = self.inner.with_color(color);
        self
    }

    /// Radio de borde del panel.
    pub fn radius(mut self, r: f32) -> Self {
        self.inner = self.inner.with_radius(r);
        self
    }

    /// Dispone los hijos en columna vertical (por defecto).
    pub fn column(mut self) -> Self {
        self.inner = self.inner.with_display(DisplayMode::FlexColumn);
        self
    }

    /// Dispone los hijos en fila horizontal.
    pub fn row(mut self) -> Self {
        self.inner = self.inner.with_display(DisplayMode::FlexRow);
        self
    }

    /// Alineación de los hijos dentro del panel.
    pub fn align(mut self, a: Alignment) -> Self {
        self.inner = self.inner.with_alignment(a);
        self
    }

    /// Padding interno uniforme en píxeles.
    pub fn padding(mut self, pad: f32) -> Self {
        self.inner = self.inner.with_padding(pad);
        self
    }

    /// Separación entre hijos en píxeles.
    pub fn gap(mut self, gap: f32) -> Self {
        self.inner = self.inner.with_gap(gap);
        self
    }

    /// Instancia el panel en el `UiSystem` y devuelve su `NodeId`.
    pub fn spawn(self, ui: &mut UiSystem<App>) -> NodeId {
        let (style, explicit_parent, id_str) = self.base.into_style();
        let parent = explicit_parent.or_else(|| ui.current_parent());
        let id = ui.tree.add_node_with_id(Box::new(self.inner), parent, id_str);
        ui.tree.set_node_style(id, style);
        id
    }

    /// Instancia el panel y añade hijos dentro del closure.
    ///
    /// Los hijos creados con `.spawn(ui)` dentro del closure se insertan
    /// automáticamente como hijos del panel — **no necesitan `.child_of()`**.
    ///
    /// ```rust,ignore
    /// ui.panel()
    ///     .size(300.0, 200.0).padding(8.0).gap(4.0)
    ///     .spawn_with(&mut ui, |ui, _| {
    ///         ui.label("Hola").spawn(ui);
    ///         ui.button("OK").size(80.0, 32.0).spawn(ui);
    ///     });
    /// ```
    pub fn spawn_with(
        self,
        ui: &mut UiSystem<App>,
        f: impl FnOnce(&mut UiSystem<App>, NodeId),
    ) -> NodeId {
        let panel_id = self.spawn(ui);
        ui.push_parent(panel_id);
        f(ui, panel_id);
        ui.pop_parent();
        panel_id
    }
}
