use crate::{NodeId, Rect, UiTree};
use glam::Vec2;

/// Trees at or below this size are cheap enough to scan linearly.
const LINEAR_SCAN_THRESHOLD: usize = 16;
/// Smallest grid cell edge, in logical pixels.
const MIN_CELL_SIZE: f32 = 16.0;
/// Upper bound on grid cells to keep rebuild cost and memory bounded.
const MAX_CELLS: usize = 4096;

/// Un índice espacial 2D para acelerar el Hit-Testing.
///
/// Mantiene una rejilla uniforme construida a partir de la extensión del
/// árbol y consulta sólo las celdas que contienen el punto, en O(1) esperado,
/// en lugar de recorrer todos los nodos. Los árboles pequeños siguen usando
/// un recorrido lineal.
///
/// Sólo entran nodos con un rectángulo visible no vacío: el rectángulo se
/// recorta contra los `Overflow` de los ancestros, de modo que un hijo
/// desplazado fuera de un `ScrollView` deja de ser candidato de hit-test.
#[derive(Default)]
pub struct SpatialIndex {
    /// Nodos con su rectángulo visible, en orden **Front-to-Back**,
    /// es decir, el elemento visual que está más "arriba" (el último pintado)
    /// aparece de primero en la lista.
    pub nodes: Vec<(NodeId, Rect)>,
    grid: Grid,
    #[cfg(test)]
    pub(crate) rebuilds: usize,
}

/// Rejilla uniforme en formato CSR (counts → starts → entries).
#[derive(Default)]
struct Grid {
    origin: Vec2,
    cell_size: f32,
    cols: usize,
    rows: usize,
    starts: Vec<u32>,
    entries: Vec<u32>,
}

impl Grid {
    fn is_empty(&self) -> bool {
        self.cols == 0 || self.rows == 0 || self.cell_size <= 0.0
    }

    fn contains(&self, pos: Vec2) -> bool {
        if self.is_empty() {
            return false;
        }
        let width = self.cols as f32 * self.cell_size;
        let height = self.rows as f32 * self.cell_size;
        pos.x >= self.origin.x
            && pos.y >= self.origin.y
            && pos.x <= self.origin.x + width
            && pos.y <= self.origin.y + height
    }

    fn cell_coord(&self, x: f32, y: f32) -> (usize, usize) {
        let cx = ((x - self.origin.x) / self.cell_size).floor();
        let cy = ((y - self.origin.y) / self.cell_size).floor();
        (clamp_coord(cx, self.cols), clamp_coord(cy, self.rows))
    }
}

/// Where a point query reads its candidate indices from.
enum CandidateSource<'a> {
    /// Every node, front-to-back (small trees).
    All,
    /// The entries of one grid cell, in front-to-back order.
    Cell(&'a [u32]),
}

/// Allocation-free iterator over the nodes that contain one point.
struct Candidates<'a> {
    point: [f32; 2],
    nodes: &'a [(NodeId, Rect)],
    source: CandidateSource<'a>,
    cursor: usize,
}

impl Iterator for Candidates<'_> {
    type Item = NodeId;

    fn next(&mut self) -> Option<NodeId> {
        loop {
            let index = match self.source {
                CandidateSource::All => {
                    if self.cursor >= self.nodes.len() {
                        return None;
                    }
                    self.cursor
                }
                CandidateSource::Cell(entries) => *entries.get(self.cursor)? as usize,
            };
            self.cursor += 1;
            if let Some((id, rect)) = self.nodes.get(index) {
                if rect.contains(self.point) {
                    return Some(*id);
                }
            }
        }
    }
}

fn clamp_coord(value: f32, count: usize) -> usize {
    if !value.is_finite() || count == 0 {
        return 0;
    }
    let max = count.saturating_sub(1) as f32;
    value.max(0.0).min(max) as usize
}

impl SpatialIndex {
    pub fn new() -> Self {
        Self::default()
    }

    /// Reconstruye por completo el índice aplanando el árbol jerárquico.
    /// Operación O(N); sólo debe ejecutarse cuando la geometría, los clips o
    /// la jerarquía cambian (ver [`UiTree::spatial_index_dirty`]).
    pub fn rebuild<App>(&mut self, tree: &UiTree<App>) {
        #[cfg(test)]
        {
            self.rebuilds += 1;
        }

        self.nodes.clear();
        if let Some(root) = tree.get_root() {
            self.collect_recursive(tree, root);
        }

        // Collect visitó Back-to-Front (padre antes que hijos).
        // Hit-testing necesita evaluar el Frente primero.
        self.nodes.reverse();
        self.build_grid();
    }

    fn collect_recursive<App>(&mut self, tree: &UiTree<App>, id: NodeId) {
        let Some(node) = tree.get_node(id) else {
            return;
        };

        if let Some(visible) = tree.visible_rect(id) {
            self.nodes.push((id, visible));
        }

        for &child in &node.children {
            self.collect_recursive(tree, child);
        }
    }

    /// Devuelve un iterador perezoso que evalúa sólo los AABB visibles que
    /// contienen el punto 2D, en orden Front-to-Back.
    ///
    /// The query allocates nothing: a linear tree walks its node list, and a
    /// gridded tree walks the entries of the single cell that contains the
    /// point.  Entries within a cell are stored in front-to-back order, so no
    /// sort or dedup is needed.
    pub fn bounding_candidates_at(&self, pos: Vec2) -> impl Iterator<Item = NodeId> + '_ {
        let source = if self.nodes.len() <= LINEAR_SCAN_THRESHOLD || self.grid.is_empty() {
            CandidateSource::All
        } else if !self.grid.contains(pos) {
            CandidateSource::Cell(&[])
        } else {
            let (cx, cy) = self.grid.cell_coord(pos.x, pos.y);
            let cell = cy * self.grid.cols + cx;
            let start = self.grid.starts[cell] as usize;
            let end = self.grid.starts[cell + 1] as usize;
            CandidateSource::Cell(&self.grid.entries[start..end])
        };
        Candidates {
            point: [pos.x, pos.y],
            nodes: &self.nodes,
            source,
            cursor: 0,
        }
    }

    /// Reconstruye el índice sólo si el árbol reporta un cambio de geometría,
    /// clips o jerarquía. El llamador debe limpiar el flag después (ver
    /// [`UiTree::update_spatial_index`]).
    pub fn update_if_dirty<App>(&mut self, tree: &UiTree<App>) {
        if tree.spatial_index_dirty() {
            self.rebuild(tree);
        }
    }

    fn build_grid(&mut self) {
        let previous_cell_size = self.grid.cell_size;
        self.grid = Grid::default();

        let node_count = self.nodes.len();
        if node_count <= LINEAR_SCAN_THRESHOLD {
            return;
        }

        let mut min = Vec2::splat(f32::INFINITY);
        let mut max = Vec2::splat(f32::NEG_INFINITY);
        for &(_, rect) in &self.nodes {
            min.x = min.x.min(rect.x);
            min.y = min.y.min(rect.y);
            max.x = max.x.max(rect.x + rect.width);
            max.y = max.y.max(rect.y + rect.height);
        }

        let width = (max.x - min.x).max(1.0);
        let height = (max.y - min.y).max(1.0);
        if !width.is_finite() || !height.is_finite() {
            return;
        }

        let target = (width * height / node_count as f32)
            .sqrt()
            .max(MIN_CELL_SIZE);

        // Keep the previous cell size while the extent stays within 2x of the
        // previous density, so small movements do not re-tile the grid.
        let cell_size = if previous_cell_size > 0.0 {
            let ratio = (target * target) / (previous_cell_size * previous_cell_size);
            if (0.5..=2.0).contains(&ratio) {
                previous_cell_size
            } else {
                target
            }
        } else {
            target
        };
        if !cell_size.is_finite() || cell_size <= 0.0 {
            return;
        }

        let mut cell_size = cell_size;
        let mut cols = ((width / cell_size).ceil() as usize).max(1);
        let mut rows = ((height / cell_size).ceil() as usize).max(1);
        while cols.saturating_mul(rows) > MAX_CELLS {
            let scale = (cols as f32 * rows as f32 / MAX_CELLS as f32)
                .sqrt()
                .max(1.01);
            cell_size *= scale;
            cols = ((width / cell_size).ceil() as usize).max(1);
            rows = ((height / cell_size).ceil() as usize).max(1);
        }
        if !cell_size.is_finite() || cell_size <= 0.0 {
            return;
        }

        let cell_count = cols * rows;
        let mut counts = vec![0u32; cell_count];

        self.grid.origin = min;
        self.grid.cell_size = cell_size;
        self.grid.cols = cols;
        self.grid.rows = rows;

        for &(_, rect) in &self.nodes {
            let (x0, y0) = self.grid.cell_coord(rect.x, rect.y);
            let (x1, y1) = self
                .grid
                .cell_coord(rect.x + rect.width, rect.y + rect.height);
            for cy in y0..=y1 {
                let row = cy * cols;
                for cx in x0..=x1 {
                    counts[row + cx] += 1;
                }
            }
        }

        let mut starts = vec![0u32; cell_count + 1];
        let mut total = 0u32;
        for (index, count) in counts.iter().enumerate() {
            starts[index] = total;
            total += *count;
        }
        starts[cell_count] = total;

        let mut cursor = starts.clone();
        let mut entries = vec![0u32; total as usize];
        for (index, &(_, rect)) in self.nodes.iter().enumerate() {
            let (x0, y0) = self.grid.cell_coord(rect.x, rect.y);
            let (x1, y1) = self
                .grid
                .cell_coord(rect.x + rect.width, rect.y + rect.height);
            for cy in y0..=y1 {
                let row = cy * cols;
                for cx in x0..=x1 {
                    let cell = row + cx;
                    let slot = cursor[cell] as usize;
                    entries[slot] = index as u32;
                    cursor[cell] += 1;
                }
            }
        }

        self.grid.starts = starts;
        self.grid.entries = entries;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::widgets::{PlaceholderWidget, ScrollView};
    use crate::{Overflow, Style};

    #[test]
    fn index_rebuilds_only_when_dirty() {
        let mut tree = UiTree::<()>::new();
        let root = tree.add_node(Box::new(PlaceholderWidget), None);
        tree.set_node_rect(root, Rect::new(0.0, 0.0, 100.0, 100.0));

        tree.update_spatial_index();
        let first = tree.spatial_index.rebuilds;

        tree.update_spatial_index();
        assert_eq!(tree.spatial_index.rebuilds, first);

        tree.set_node_rect(root, Rect::new(5.0, 0.0, 100.0, 100.0));
        tree.update_spatial_index();
        assert_eq!(tree.spatial_index.rebuilds, first + 1);
    }

    #[test]
    fn scrolled_out_node_is_not_a_candidate() {
        let mut tree = UiTree::<()>::new();
        let mut scroll_widget = ScrollView::<()>::new();
        // Scrolled down 150px; children positions include that offset.
        scroll_widget.scroll_offset = Vec2::new(0.0, 150.0);
        let scroll = tree.add_node(Box::new(scroll_widget), None);
        let hidden = tree.add_node(Box::new(PlaceholderWidget), Some(scroll));
        let shown = tree.add_node(Box::new(PlaceholderWidget), Some(scroll));

        let mut style = Style::default();
        style.overflow = Overflow::Scroll;
        tree.set_node_style(scroll, style);

        tree.set_node_rect(scroll, Rect::new(0.0, 0.0, 100.0, 100.0));
        tree.set_node_rect(hidden, Rect::new(0.0, -150.0, 100.0, 20.0));
        tree.set_node_rect(shown, Rect::new(10.0, 10.0, 20.0, 20.0));
        tree.update_spatial_index();

        let hidden_hits: Vec<NodeId> = tree
            .spatial_index
            .bounding_candidates_at(Vec2::new(50.0, -145.0))
            .collect();
        assert!(!hidden_hits.contains(&hidden));

        let shown_hits: Vec<NodeId> = tree
            .spatial_index
            .bounding_candidates_at(Vec2::new(15.0, 15.0))
            .collect();
        assert_eq!(shown_hits.first(), Some(&shown));
    }

    #[test]
    fn grid_query_preserves_front_to_back_order() {
        let mut tree = UiTree::<()>::new();
        let root = tree.add_node(Box::new(PlaceholderWidget), None);
        tree.set_node_rect(root, Rect::new(0.0, 0.0, 200.0, 200.0));

        let mut ids = Vec::new();
        for _ in 0..40 {
            let id = tree.add_node(Box::new(PlaceholderWidget), Some(root));
            tree.set_node_rect(id, Rect::new(0.0, 0.0, 200.0, 200.0));
            ids.push(id);
        }
        tree.update_spatial_index();

        let candidates: Vec<NodeId> = tree
            .spatial_index
            .bounding_candidates_at(Vec2::new(100.0, 100.0))
            .collect();

        ids.reverse();
        ids.push(root);
        assert_eq!(candidates, ids);
    }
}
