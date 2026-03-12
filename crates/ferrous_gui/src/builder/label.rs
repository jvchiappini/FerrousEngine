//! [`LabelBuilder`] — builder fluent para etiquetas de texto.

use ferrous_ui_core::{Color, HAlign, Label, NodeId, TextAlign, VAlign};

use crate::UiSystem;
use super::base::{BuilderBase, impl_builder_base};

// =========================================================================
// LabelBuilder
// =========================================================================

/// Builder fluent para `Label`.
///
/// ```rust,ignore
/// ui.label("Hola Mundo")
///     .at(20.0, 20.0)
///     .color(Color::WHITE)
///     .font_size(18.0)
///     .spawn(&mut ui);
/// ```
pub struct LabelBuilder<App: 'static> {
    pub(super) inner: Label,
    pub(super) base: BuilderBase,
    pub(super) _app: std::marker::PhantomData<fn() -> App>,
}

impl_builder_base!(LabelBuilder<App>);

impl<App: 'static> LabelBuilder<App> {
    pub(crate) fn new(text: impl Into<String>) -> Self {
        Self {
            inner: Label::new(text),
            base: BuilderBase::default(),
            _app: std::marker::PhantomData,
        }
    }

    /// Color del texto.
    pub fn color(mut self, color: Color) -> Self {
        self.inner = self.inner.with_color(color);
        self
    }

    /// Tamaño de fuente en píxeles.
    pub fn font_size(mut self, size: f32) -> Self {
        self.inner = self.inner.with_size(size);
        self
    }

    /// Establece la alineación completa del texto.
    pub fn text_align(mut self, align: TextAlign) -> Self {
        self.inner = self.inner.with_text_align(align);
        self
    }

    /// Alineación horizontal: `HAlign::Left`, `Center`, `Right`, o `Custom`.
    pub fn h_align(mut self, h: HAlign) -> Self {
        self.inner = self.inner.with_h_align(h);
        self
    }

    /// Alineación vertical: `VAlign::Top`, `Center`, `Bottom`, o `Custom`.
    pub fn v_align(mut self, v: VAlign) -> Self {
        self.inner = self.inner.with_v_align(v);
        self
    }

    /// Instancia el label en el `UiSystem` y devuelve su `NodeId`.
    pub fn spawn(self, ui: &mut UiSystem<App>) -> NodeId {
        let (style, explicit_parent, id_str) = self.base.into_style();
        let parent = explicit_parent.or_else(|| ui.current_parent());
        let id = ui.tree.add_node_with_id(Box::new(self.inner), parent, id_str);
        ui.tree.set_node_style(id, style);
        id
    }
}
