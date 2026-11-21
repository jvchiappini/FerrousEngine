//! `ferrous_layout` — Motor de cálculo de posiciones y dimensiones para la UI.
//!
//! Se encarga de procesar el árbol de nodos de `ferrous_ui_core` y resolver
//! las restricciones de tamaño (`Units`, `Alignment`, `DisplayMode`) para
//! asignar coordenadas físicas (`Rect`) a cada elemento.
//!
//! Optimizaciones activas:
//!   - El `taffy::NodeId` se guarda en `Node::taffy_id` (como u64) en lugar de un HashMap separado,
//!     eliminando un hash-lookup por nodo en cada frame.
//!   - `sync_node` reutiliza el nodo de Taffy existente y vuelca estilo e hijos en cada pase;
//!     el control incremental (saltar subárboles limpios) vive ahora en GUIMaker.
//!   - `apply_layout` y `sync_node` evitan allocations intermedias usando slices directos.

pub use ferrous_ui_core::{Alignment, DisplayMode, NodeId, Rect, UiTree, Units};
use taffy::prelude::*;

/// Convierte un `taffy::NodeId` a `u64` para almacenarlo en `Node::taffy_id`.
#[inline(always)]
fn taffy_to_u64(id: taffy::NodeId) -> u64 {
    id.into()
}

/// Recupera un `taffy::NodeId` desde el `u64` guardado en `Node::taffy_id`.
#[inline(always)]
fn u64_to_taffy(v: u64) -> taffy::NodeId {
    taffy::NodeId::from(v)
}

/// Motor de layout que sincroniza el `UiTree` con un grafo de Taffy de alto rendimiento.
pub struct LayoutEngine {
    /// Árbol interno de Taffy donde se realizan los cálculos pesados.
    pub taffy: TaffyTree<NodeId>,
}

impl Default for LayoutEngine {
    fn default() -> Self {
        Self::new()
    }
}

impl LayoutEngine {
    /// Crea una nueva instancia del motor de layout.
    pub fn new() -> Self {
        Self {
            taffy: TaffyTree::new(),
        }
    }

    /// Recorre el `UiTree`, sincroniza todos los nodos con Taffy y calcula las
    /// posiciones finales.
    ///
    /// El pase siempre se ejecuta completo: el controlador incremental que
    /// decidía si había que correr (y qué subárboles saltar) vive ahora en
    /// GUIMaker, no en el motor.
    ///
    /// 1. `sync_node` recorre el árbol completo y vuelca el estilo y los hijos
    ///    de cada nodo, reutilizando su `taffy_id` cuando ya existe.
    /// 2. Taffy calcula el layout.
    /// 3. `apply_layout` recorre el árbol completo para volcar los rectángulos;
    ///    los rectángulos idénticos no generan paint dirty (`UiTree::set_node_rect`).
    pub fn compute_layout<App>(
        &mut self,
        tree: &mut UiTree<App>,
        available_width: f32,
        available_height: f32,
    ) {
        if let Some(root_id) = tree.get_root() {
            // 1. Sincronizar el árbol completo con Taffy.
            let Some(taff_root) = self.sync_node(root_id, tree) else {
                // Taffy no pudo crear el nodo raíz; reintentar en el próximo frame.
                return;
            };

            // 2. Ejecutar Taffy con función de medida personalizada
            let size = Size {
                width: AvailableSpace::Definite(available_width),
                height: AvailableSpace::Definite(available_height),
            };

            let _ = self.taffy.compute_layout_with_measure(
                taff_root,
                size,
                |known, available, _taffy_id, node_context, _style| {
                    if let Some(ferrous_id) = node_context {
                        let ferrous_id = *ferrous_id;
                        let theme = tree.theme;
                        if let Some(node) = tree.get_node_mut(ferrous_id) {
                            let mut ctx = ferrous_ui_core::LayoutContext {
                                available_space: glam::vec2(
                                    match available.width {
                                        AvailableSpace::Definite(v) => v,
                                        _ => available_width,
                                    },
                                    match available.height {
                                        AvailableSpace::Definite(v) => v,
                                        _ => available_height,
                                    },
                                ),
                                known_dimensions: (known.width, known.height),
                                node_id: ferrous_id,
                                theme,
                            };
                            let size = node.widget.calculate_size(&mut ctx);
                            return taffy::geometry::Size {
                                width: size.x,
                                height: size.y,
                            };
                        }
                    }
                    taffy::geometry::Size::ZERO
                },
            );

            // 3. Aplicar resultados de vuelta al UiTree
            self.apply_layout(root_id, tree, 0.0, 0.0);
        }
    }

    /// Sincroniza recursivamente el nodo con Taffy.
    /// - Si el nodo no existe en Taffy aún, lo crea.
    /// - Si ya existe, reutiliza su id y actualiza siempre el estilo y los hijos.
    ///
    /// Retorna `None` si Taffy no pudo crear el nodo, para reintentar sin panic.
    fn sync_node<App>(&mut self, id: NodeId, tree: &mut UiTree<App>) -> Option<taffy::NodeId> {
        let existing_taffy = tree.get_node(id)?.taffy_id;

        // Sincronizar hijos primero, iterando por índice para no clonar la lista.
        let child_count = tree.get_node(id).map_or(0, |n| n.children.len());
        let mut taffy_children: Vec<taffy::NodeId> = Vec::with_capacity(child_count);
        for index in 0..child_count {
            let child_id = tree
                .get_node(id)
                .and_then(|n| n.children.get(index).copied())?;
            let child_taffy = self.sync_node(child_id, tree)?;
            taffy_children.push(child_taffy);
        }

        let taffy_id = if let Some(existing) = existing_taffy {
            let existing = u64_to_taffy(existing);
            let style = tree.get_node_style(id).cloned().unwrap_or_default();
            let taffy_style = self.convert_style(&style);
            let _ = self.taffy.set_style(existing, taffy_style);
            let _ = self.taffy.set_children(existing, &taffy_children);
            existing
        } else {
            // Nodo nuevo: crear en Taffy
            let style = tree.get_node_style(id).cloned().unwrap_or_default();
            let taffy_style = self.convert_style(&style);
            self.taffy
                .new_with_children(taffy_style, &taffy_children)
                .ok()?
        };

        let _ = self.taffy.set_node_context(taffy_id, Some(id));

        // Limpiar el flag de jerarquía (paint se limpia en el render).
        if let Some(node) = tree.get_node_mut(id) {
            node.taffy_id = Some(taffy_to_u64(taffy_id));
            node.dirty.hierarchy = false;
        }

        Some(taffy_id)
    }

    fn apply_layout<App>(&self, id: NodeId, tree: &mut UiTree<App>, parent_x: f32, parent_y: f32) {
        let Some(taffy_id) = tree.get_node(id).and_then(|n| n.taffy_id.map(u64_to_taffy)) else {
            return;
        };

        let layout = match self.taffy.layout(taffy_id) {
            Ok(l) => *l,
            Err(_) => return,
        };

        let x = parent_x + layout.location.x;
        let y = parent_y + layout.location.y;

        tree.set_node_rect(
            id,
            Rect {
                x,
                y,
                width: layout.size.width,
                height: layout.size.height,
            },
        );

        let scroll = tree
            .get_node(id)
            .map(|n| n.widget.scroll_offset())
            .unwrap_or(glam::Vec2::ZERO);

        // Iterar hijos por índice sin clonar la lista.
        let child_count = tree.get_node(id).map_or(0, |n| n.children.len());
        for index in 0..child_count {
            let child_id = tree
                .get_node(id)
                .and_then(|n| n.children.get(index).copied());
            if let Some(child_id) = child_id {
                self.apply_layout(child_id, tree, x - scroll.x, y - scroll.y);
            }
        }
    }

    #[allow(clippy::field_reassign_with_default)] // reason: one field-by-field taffy style conversion
    fn convert_style(&self, style: &ferrous_ui_core::Style) -> taffy::Style {
        let mut t_style = taffy::Style::default();

        t_style.display = match style.display {
            DisplayMode::Block => taffy::Display::Block,
            DisplayMode::FlexRow | DisplayMode::FlexColumn => taffy::Display::Flex,
        };

        t_style.flex_direction = if style.display == DisplayMode::FlexColumn {
            taffy::FlexDirection::Column
        } else {
            taffy::FlexDirection::Row
        };

        t_style.position = match style.position {
            ferrous_ui_core::Position::Relative => taffy::Position::Relative,
            ferrous_ui_core::Position::Absolute => taffy::Position::Absolute,
        };

        t_style.inset = taffy::Rect {
            left: self.to_lp_auto(Units::Px(style.offsets.left)),
            right: self.to_lp_auto(Units::Px(style.offsets.right)),
            top: self.to_lp_auto(Units::Px(style.offsets.top)),
            bottom: self.to_lp_auto(Units::Px(style.offsets.bottom)),
        };

        t_style.size = Size {
            width: self.to_dimension(style.size.0),
            height: self.to_dimension(style.size.1),
        };

        // Flex grow desde Units::Flex
        if let Units::Flex(val) = style.size.0 {
            t_style.flex_grow = val;
        } else if let Units::Flex(val) = style.size.1 {
            t_style.flex_grow = val;
        }

        t_style.margin = taffy::Rect {
            left: self.to_lp_auto(Units::Px(style.margin.left)),
            right: self.to_lp_auto(Units::Px(style.margin.right)),
            top: self.to_lp_auto(Units::Px(style.margin.top)),
            bottom: self.to_lp_auto(Units::Px(style.margin.bottom)),
        };

        t_style.padding = taffy::Rect {
            left: self.to_lp(Units::Px(style.padding.left)),
            right: self.to_lp(Units::Px(style.padding.right)),
            top: self.to_lp(Units::Px(style.padding.top)),
            bottom: self.to_lp(Units::Px(style.padding.bottom)),
        };

        match style.alignment {
            Alignment::Start => {
                t_style.align_items = Some(taffy::AlignItems::Start);
                t_style.justify_content = Some(taffy::JustifyContent::Start);
            }
            Alignment::Center => {
                t_style.align_items = Some(taffy::AlignItems::Center);
                t_style.justify_content = Some(taffy::JustifyContent::Center);
            }
            Alignment::End => {
                t_style.align_items = Some(taffy::AlignItems::End);
                t_style.justify_content = Some(taffy::JustifyContent::End);
            }
            Alignment::Stretch => {
                t_style.align_items = Some(taffy::AlignItems::Stretch);
            }
        }

        // Separación entre hijos en layouts flex
        if style.gap > 0.0 {
            t_style.gap = taffy::Size {
                width: taffy::LengthPercentage::Length(style.gap),
                height: taffy::LengthPercentage::Length(style.gap),
            };
        }

        let overflow_val = match style.overflow {
            ferrous_ui_core::Overflow::Visible => taffy::Overflow::Visible,
            ferrous_ui_core::Overflow::Hidden => taffy::Overflow::Hidden,
            ferrous_ui_core::Overflow::Scroll => taffy::Overflow::Scroll,
        };
        t_style.overflow = taffy::Point {
            x: overflow_val,
            y: overflow_val,
        };

        t_style
    }

    fn to_dimension(&self, unit: Units) -> taffy::Dimension {
        match unit {
            Units::Px(val) => taffy::Dimension::Length(val),
            Units::Percentage(val) => taffy::Dimension::Percent(val / 100.0),
            Units::Flex(_) | Units::Auto => taffy::Dimension::Auto,
        }
    }

    fn to_lp(&self, unit: Units) -> taffy::LengthPercentage {
        match unit {
            Units::Px(val) => taffy::LengthPercentage::Length(val),
            Units::Percentage(val) => taffy::LengthPercentage::Percent(val / 100.0),
            Units::Flex(_) | Units::Auto => taffy::LengthPercentage::Length(0.0),
        }
    }

    fn to_lp_auto(&self, unit: Units) -> taffy::LengthPercentageAuto {
        match unit {
            Units::Px(val) => taffy::LengthPercentageAuto::Length(val),
            Units::Percentage(val) => taffy::LengthPercentageAuto::Percent(val / 100.0),
            Units::Flex(_) | Units::Auto => taffy::LengthPercentageAuto::Auto,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ferrous_ui_core::{PlaceholderWidget, Style};

    fn build_tree() -> (UiTree<()>, NodeId) {
        let mut tree = UiTree::<()>::new();
        let root = tree.add_node(Box::new(PlaceholderWidget), None);
        let child = tree.add_node(Box::new(PlaceholderWidget), Some(root));

        let root_style = Style {
            display: DisplayMode::FlexColumn,
            size: (Units::Px(200.0), Units::Px(100.0)),
            ..Style::default()
        };
        tree.set_node_style(root, root_style);

        let child_style = Style {
            size: (Units::Px(40.0), Units::Px(20.0)),
            ..Style::default()
        };
        tree.set_node_style(child, child_style);

        (tree, child)
    }

    #[test]
    fn clean_tree_still_recomputes_layout() {
        let (mut tree, child) = build_tree();
        let mut engine = LayoutEngine::new();
        engine.compute_layout(&mut tree, 800.0, 600.0);

        let laid_out = tree.get_node_rect(child).expect("child rect");
        let sentinel = Rect::new(7.0, 9.0, 11.0, 13.0);
        if let Some(node) = tree.get_node_mut(child) {
            node.rect = sentinel;
        }

        // There is no layout-dirty flag: every pass must recompute and restore
        // the real rect over the sentinel.
        engine.compute_layout(&mut tree, 800.0, 600.0);

        assert_eq!(tree.get_node_rect(child), Some(laid_out));
    }

    #[test]
    fn viewport_resize_relayouts_clean_tree() {
        let mut tree = UiTree::<()>::new();
        let root = tree.add_node(Box::new(PlaceholderWidget), None);

        let mut style = Style::default();
        style.display = DisplayMode::FlexRow;
        style.size = (Units::Percentage(100.0), Units::Px(100.0));
        tree.set_node_style(root, style);

        let mut engine = LayoutEngine::new();
        engine.compute_layout(&mut tree, 800.0, 600.0);
        assert_eq!(tree.get_node_rect(root).map(|r| r.width), Some(800.0));

        // Every pass recomputes, so the new available size is applied even
        // though no node is individually dirty.
        engine.compute_layout(&mut tree, 400.0, 600.0);
        assert_eq!(tree.get_node_rect(root).map(|r| r.width), Some(400.0));
    }

    #[test]
    fn dirty_node_relayouts() {
        let (mut tree, child) = build_tree();
        let mut engine = LayoutEngine::new();
        engine.compute_layout(&mut tree, 800.0, 600.0);

        let sentinel = Rect::new(7.0, 9.0, 11.0, 13.0);
        tree.set_node_rect(child, sentinel);

        engine.compute_layout(&mut tree, 800.0, 600.0);

        let rect = tree.get_node_rect(child).expect("child rect");
        assert_ne!(rect, sentinel);
        assert_eq!(rect.width, 40.0);
        assert_eq!(rect.height, 20.0);
    }
}
