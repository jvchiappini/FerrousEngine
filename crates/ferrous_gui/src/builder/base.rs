//! Datos y lógica compartida entre todos los builders.

use ferrous_ui_core::{NodeId, Position, RectOffset, Style, Units};

// =========================================================================
// BuilderBase
// =========================================================================

/// Datos de posición, tamaño y jerarquía compartidos entre todos los builders.
#[derive(Default)]
pub(super) struct BuilderBase {
    pub x: f32,
    pub y: f32,
    pub width: Option<f32>,
    pub height: Option<f32>,
    pub parent: Option<NodeId>,
    pub id_str: Option<String>,
    pub absolute: bool,
    /// Si true, usa `Units::Flex(1.0)` en ambos ejes en lugar de tamaño fijo.
    pub flex: bool,
}

impl BuilderBase {
    /// Convierte los datos base en un `Style` + metadatos de jerarquía.
    pub fn into_style(self) -> (Style, Option<NodeId>, Option<String>) {
        let mut style = Style::default();

        style.size = if self.flex {
            (Units::Flex(1.0), Units::Flex(1.0))
        } else {
            (
                self.width.map_or(Units::Auto, Units::Px),
                self.height.map_or(Units::Auto, Units::Px),
            )
        };

        if self.absolute {
            style.position = Position::Absolute;
            style.offsets =
                RectOffset { left: self.x, top: self.y, right: 0.0, bottom: 0.0 };
        }

        (style, self.parent, self.id_str)
    }
}

// =========================================================================
// Macro: métodos comunes de posición/tamaño/jerarquía
// =========================================================================

/// Genera los métodos comunes (`at`, `size`, `width`, `height`, `fill`,
/// `child_of`, `id`) para un builder que tenga un campo `base: BuilderBase`.
macro_rules! impl_builder_base {
    ($T:ty) => {
        impl<App: 'static> $T {
            /// Posición absoluta en el canvas.
            pub fn at(mut self, x: f32, y: f32) -> Self {
                self.base.x = x;
                self.base.y = y;
                self.base.absolute = true;
                self
            }

            /// Tamaño fijo en píxeles.
            pub fn size(mut self, w: f32, h: f32) -> Self {
                self.base.width = Some(w);
                self.base.height = Some(h);
                self
            }

            /// Solo ancho.
            pub fn width(mut self, w: f32) -> Self {
                self.base.width = Some(w);
                self
            }

            /// Solo alto.
            pub fn height(mut self, h: f32) -> Self {
                self.base.height = Some(h);
                self
            }

            /// Ocupa todo el espacio disponible del padre (`Flex(1.0)` en ambos ejes).
            pub fn fill(mut self) -> Self {
                self.base.flex = true;
                self
            }

            /// Hace este widget hijo de `parent`.
            pub fn child_of(mut self, parent: NodeId) -> Self {
                self.base.parent = Some(parent);
                self
            }

            /// ID de texto para buscarlo con `ui.tree.get_node_by_id`.
            pub fn id(mut self, s: impl Into<String>) -> Self {
                self.base.id_str = Some(s.into());
                self
            }
        }
    };
}

pub(super) use impl_builder_base;
