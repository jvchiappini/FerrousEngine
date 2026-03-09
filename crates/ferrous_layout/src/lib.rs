//! `ferrous_layout` — Motor de cálculo de posiciones y dimensiones para la UI.
//!
//! Se encarga de procesar el árbol de nodos de `ferrous_ui_core` y resolver
//! las restricciones de tamaño (`Units`, `Alignment`, `DisplayMode`) para
//! asignar coordenadas físicas (`Rect`) a cada elemento.

use taffy::prelude::*;
use ferrous_ui_core::{UiTree, NodeId, Rect, Units, DisplayMode, Alignment};
use std::collections::HashMap;

/// Motor de layout que sincroniza el `UiTree` con un grafo de Taffy de alto rendimiento.
pub struct LayoutEngine {
    /// Árbol interno de Taffy donde se realizan los cálculos pesados.
    pub taffy: TaffyTree<NodeId>,
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
    pub fn compute_layout<App>(&mut self, tree: &mut UiTree<App>, available_width: f32, available_height: f32) {
        if let Some(root_id) = tree.get_root() {
            // 1. Sincronización recursiva (crear/actualizar nodos en Taffy)
            let taff_root = self.sync_node(root_id, tree);
            
            // 2. Ejecutar el motor de Taffy con función de medida personalizada
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
                        let theme = tree.theme; // Copiamos el Theme (es Copy)
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
                }
            );

            // 3. Aplicar los resultados de vuelta al UiTree
            self.apply_layout(root_id, tree, 0.0, 0.0);
        }
    }

    fn sync_node<App>(&mut self, id: NodeId, tree: &UiTree<App>) -> taffy::NodeId {
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

        let _ = self.taffy.set_node_context(taffy_id, Some(id));
        taffy_id
    }

    fn apply_layout<App>(&self, id: NodeId, tree: &mut UiTree<App>, parent_x: f32, parent_y: f32) {
        if let Some(&taffy_id) = self.node_map.get(&id) {
            let layout = self.taffy.layout(taffy_id).unwrap();
            
            let x = parent_x + layout.location.x;
            let y = parent_y + layout.location.y;

            tree.set_node_rect(id, Rect {
                x,
                y,
                width: layout.size.width,
                height: layout.size.height,
            });

            let scroll = if let Some(node) = tree.get_node(id) {
                node.widget.scroll_offset()
            } else {
                glam::Vec2::ZERO
            };

            let children = tree.get_node_children(id).unwrap_or(&[]).to_vec();
            for child_id in children {
                self.apply_layout(child_id, tree, x - scroll.x, y - scroll.y);
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

        let overflow_val = match style.overflow {
            ferrous_ui_core::Overflow::Visible => taffy::Overflow::Visible,
            ferrous_ui_core::Overflow::Hidden => taffy::Overflow::Hidden,
            ferrous_ui_core::Overflow::Scroll => taffy::Overflow::Scroll,
        };
        t_style.overflow = taffy::Point { x: overflow_val, y: overflow_val };

        t_style
    }

    fn to_dimension(&self, unit: Units) -> taffy::Dimension {
        match unit {
            Units::Px(val) => taffy::Dimension::Length(val),
            Units::Percentage(val) => taffy::Dimension::Percent(val / 100.0),
            Units::Flex(_) => taffy::Dimension::Auto,
            Units::Auto => taffy::Dimension::Auto,
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
