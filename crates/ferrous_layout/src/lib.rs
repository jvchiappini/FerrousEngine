//! `ferrous_layout` — Motor de cálculo de posiciones y dimensiones para la UI.
//!
//! Se encarga de procesar el árbol de nodos de `ferrous_ui_core` y resolver
//! las restricciones de tamaño (`Units`, `Alignment`, `DisplayMode`) para
//! asignar coordenadas físicas (`Rect`) a cada elemento.
//!
//! Utiliza `Taffy` (una implementación de Rust para Flexbox y CSS Grid) como 
//! motor subyacente de resolución de restricciones.

use taffy::prelude::*;
use ferrous_ui_core::{UiTree, NodeId, Rect, Units, DisplayMode, Alignment};
use std::collections::HashMap;

/// Motor de layout que sincroniza el `UiTree` con un grafo de Taffy de alto rendimiento.
pub struct LayoutEngine {
    /// Árbol interno de Taffy donde se realizan los cálculos pesados.
    pub taffy: TaffyTree<()>,
    /// Mapeo entre los IDs de Ferrous UI y los IDs de Taffy.
    node_map: HashMap<NodeId, taffy::NodeId>,
}

impl LayoutEngine {
    /// Crea una nueva instancia del motor de layout.
    pub fn new() -> Self {
        Self {
            taffy: TaffyTree::new(),
            node_map: HashMap::new(),
        }
    }

    /// Recorre el `UiTree`, sincroniza su estructura con Taffy y calcula las posiciones finales.
    pub fn compute_layout(&mut self, tree: &mut UiTree, available_width: f32, available_height: f32) {
        if let Some(root_id) = tree.get_root() {
            // 1. Sincronización recursiva (crear/actualizar nodos en Taffy)
            let taff_root = self.sync_node(root_id, tree);
            
            // 2. Ejecutar el motor de Taffy
            let size = Size {
                width: AvailableSpace::Definite(available_width),
                height: AvailableSpace::Definite(available_height),
            };
            let _ = self.taffy.compute_layout(taff_root, size);

            // 3. Aplicar los resultados de vuelta al UiTree
            self.apply_layout(root_id, tree, 0.0, 0.0);
        }
    }

    fn sync_node(&mut self, id: NodeId, tree: &UiTree) -> taffy::NodeId {
        // Obtenemos el nodo del árbol de UI
        // Nota: en un sistema real, usaríamos DirtyFlags para solo actualizar lo necesario.
        // Por simplicidad en este MVP, sincronizamos todo.

        // Convertimos el estilo de Ferrous a Taffy
        // Buscamos si ya existe el nodo en Taffy para reutilizarlo o creamos uno nuevo
        // Pero TaffyTree::new_with_children es más fácil para sincronización completa.

        // En un motor real, mantendríamos el nodo vivo. Aquí lo recreamos para asegurar
        // que la jerarquía es idéntica (MVP approach).
        
        let children_ids = tree.get_node_children(id).unwrap_or(&[]).to_vec();
        let mut taffy_children = Vec::new();
        for child_id in children_ids {
            taffy_children.push(self.sync_node(child_id, tree));
        }

        let style = tree.get_node_style(id).cloned().unwrap_or_default();
        let taffy_style = self.convert_style(&style);

        let taffy_id = if let Some(&existing) = self.node_map.get(&id) {
            let _ = self.taffy.set_style(existing, taffy_style);
            let _ = self.taffy.set_children(existing, &taffy_children);
            existing
        } else {
            let n = self.taffy.new_with_children(taffy_style, &taffy_children).unwrap();
            self.node_map.insert(id, n);
            n
        };

        taffy_id
    }

    fn apply_layout(&self, id: NodeId, tree: &mut UiTree, parent_x: f32, parent_y: f32) {
        if let Some(&taffy_id) = self.node_map.get(&id) {
            let layout = self.taffy.layout(taffy_id).unwrap();
            
            // Coordenadas absolutas
            let x = parent_x + layout.location.x;
            let y = parent_y + layout.location.y;

            tree.set_node_rect(id, Rect {
                x,
                y,
                width: layout.size.width,
                height: layout.size.height,
            });

            let children = tree.get_node_children(id).unwrap_or(&[]).to_vec();
            for child_id in children {
                self.apply_layout(child_id, tree, x, y);
            }
        }
    }

    fn convert_style(&self, style: &ferrous_ui_core::Style) -> taffy::Style {
        let mut t_style = taffy::Style::default();

        t_style.display = match style.display {
            DisplayMode::Block => taffy::Display::Block,
            DisplayMode::FlexRow => taffy::Display::Flex,
            DisplayMode::FlexColumn => taffy::Display::Flex,
        };

        if style.display == DisplayMode::FlexColumn {
            t_style.flex_direction = taffy::FlexDirection::Column;
        } else {
            t_style.flex_direction = taffy::FlexDirection::Row;
        }

        t_style.size = Size {
            width: self.to_dimension(style.size.0),
            height: self.to_dimension(style.size.1),
        };

        // En Taffy, Flex(x) se maneja usualmente mediante flex_grow.
        if let Units::Flex(val) = style.size.0 {
            t_style.flex_grow = val;
        }
        if let Units::Flex(val) = style.size.1 {
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

        t_style
    }

    fn to_dimension(&self, unit: Units) -> taffy::Dimension {
        match unit {
            Units::Px(val) => taffy::Dimension::Length(val),
            Units::Percentage(val) => taffy::Dimension::Percent(val / 100.0),
            Units::Flex(_) => taffy::Dimension::Auto,
        }
    }

    fn to_lp(&self, unit: Units) -> taffy::LengthPercentage {
        match unit {
            Units::Px(val) => taffy::LengthPercentage::Length(val),
            Units::Percentage(val) => taffy::LengthPercentage::Percent(val / 100.0),
            Units::Flex(_) => taffy::LengthPercentage::Length(0.0), // No aplica a padding generalmente
        }
    }

    fn to_lp_auto(&self, unit: Units) -> taffy::LengthPercentageAuto {
        match unit {
            Units::Px(val) => taffy::LengthPercentageAuto::Length(val),
            Units::Percentage(val) => taffy::LengthPercentageAuto::Percent(val / 100.0),
            Units::Flex(_) => taffy::LengthPercentageAuto::Auto,
        }
    }
}
