//! `ferrous_layout` — Motor de cálculo de posiciones y dimensiones para la UI.
//!
//! Se encarga de procesar el árbol de nodos de `ferrous_ui_core` y resolver
//! las restricciones de tamaño (`Units`, `Alignment`, `DisplayMode`) para
//! asignar coordenadas físicas (`Rect`) a cada elemento.
//!
//! Utiliza `Taffy` (una implementación de Rust para Flexbox y CSS Grid) como 
//! motor subyacente de resolución de restricciones.

use taffy::TaffyTree;

/// Motor de layout que sincroniza el `UiTree` con un grafo de Taffy de alto rendimiento.
pub struct LayoutEngine {
    /// Árbol interno de Taffy donde se realizan los cálculos pesados.
    pub taffy: TaffyTree<()>,
}

impl LayoutEngine {
    /// Crea una nueva instancia del motor de layout.
    pub fn new() -> Self {
        Self {
            taffy: TaffyTree::new(),
        }
    }
}
