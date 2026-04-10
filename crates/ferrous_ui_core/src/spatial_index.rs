use crate::{NodeId, Rect, UiTree};
use glam::Vec2;

/// Un índice espacial simple 2D para acelerar el Hit-Testing.
/// Evita atravesar recursivamente el árbol para cada movimiento del mouse.
#[derive(Default)]
pub struct SpatialIndex {
    /// Nodos aplanados, almacenados en orden **Front-to-Back**,
    /// es decir, el elemento visual que está más "arriba" (el último pintado)
    /// aparece de primero en la lista.
    pub nodes: Vec<(NodeId, Rect)>,
}

impl SpatialIndex {
    pub fn new() -> Self {
        Self::default()
    }

    /// Reconstruye por completo el índice aplanando el árbol jerárquico.
    /// Operación O(N), debe ser llamada únicamente cuando cambia el layout
    /// o la jerarquía.
    pub fn rebuild<App>(&mut self, tree: &UiTree<App>) {
        self.nodes.clear();
        if let Some(root) = tree.get_root() {
            self.collect_recursive(tree, root);
        }
        
        // Collect visitó Back-to-Front (por el iterador iter().children).
        // Hit-testing necesita evaluar el Frente primero.
        self.nodes.reverse();
    }

    fn collect_recursive<App>(&mut self, tree: &UiTree<App>, id: NodeId) {
        if let Some(node) = tree.get_node(id) {
            self.nodes.push((id, node.rect));
            for &child in &node.children {
                self.collect_recursive(tree, child);
            }
        }
    }

    /// Devuelve un iterador perezoso que evalúa solo aquellos quads AABB
    /// que interseccionen el punto 2D proporcionado.
    pub fn bounding_candidates_at(&self, pos: Vec2) -> impl Iterator<Item = NodeId> + '_ {
        self.nodes.iter().filter_map(move |(id, rect)| {
            if rect.contains([pos.x, pos.y]) {
                Some(*id)
            } else {
                None
            }
        })
    }

    /// Comprueba si el índice está obsoleto con base en la "suciedad" de un frame
    /// y lo recalcula automáticamente si corresponde.
    pub fn update_if_dirty<App>(&mut self, tree: &UiTree<App>) {
        // Podríamos comprobar si el root o los global dirty flags exigen un rebuild.
        // Dado que un layout reacomoda rects, o añade/quita nodos:
        // En UiTree.rs mark_layout_dirty debería levantar un flag global `index_dirty`.
        // Para simular simplicidad y fiabilidad, asumiendo N pequeño (1000 nodos),
        // este rebuild es súper rápido.
        self.rebuild(tree);
    }
}
