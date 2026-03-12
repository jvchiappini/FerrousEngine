//! [`WidgetBuilder`] — builder genérico para cualquier widget.

use ferrous_ui_core::{NodeId, Widget};

use crate::UiSystem;
use super::base::{BuilderBase, impl_builder_base};

// =========================================================================
// WidgetBuilder
// =========================================================================

/// Builder fluent para cualquier widget que implemente `Widget<App>`.
///
/// Úsalo cuando ninguno de los builders específicos encaje.
///
/// ```rust,ignore
/// ui.widget(MyCustomWidget::new())
///     .at(10.0, 10.0)
///     .size(200.0, 40.0)
///     .spawn(&mut ui);
/// ```
pub struct WidgetBuilder<App: 'static> {
    pub(super) widget: Box<dyn Widget<App>>,
    pub(super) base: BuilderBase,
}

impl_builder_base!(WidgetBuilder<App>);

impl<App: 'static> WidgetBuilder<App> {
    /// Envuelve cualquier widget en el builder.
    pub fn new(widget: impl Widget<App> + 'static) -> Self {
        Self { widget: Box::new(widget), base: BuilderBase::default() }
    }

    /// Instancia el widget en el `UiSystem` y devuelve su `NodeId`.
    pub fn spawn(self, ui: &mut UiSystem<App>) -> NodeId {
        let (style, explicit_parent, id_str) = self.base.into_style();
        let parent = explicit_parent.or_else(|| ui.current_parent());
        let id = ui.tree.add_node_with_id(self.widget, parent, id_str);
        ui.tree.set_node_style(id, style);
        id
    }

    /// Instancia el widget y luego llama al closure para agregar hijos.
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
