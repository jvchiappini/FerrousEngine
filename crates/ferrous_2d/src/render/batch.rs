use crate::render::types::SpriteInstance;
// BTreeMap: orden deterministico por texture_id + cache-friendly en iteración.
// A diferencia de HashMap, no hay hashing overhead ni reordering entre frames.
use std::collections::BTreeMap;

/// Agrupa sprites por Texture ID: 1 draw call por textura.
/// Usa `BTreeMap` para orden deterministico (reproducible entre frames).
#[derive(Default)]
pub struct SpriteBatcher {
    // texture_id → lista de instancias. BTreeMap mantiene el orden por texture_id.
    /// Instances grouped by texture id; iterated in ascending id order.
    pub batches: BTreeMap<u32, Vec<SpriteInstance>>,
}

impl SpriteBatcher {
    /// Limpia los datos sin liberar las allociones (reutiliza la memoria del frame anterior).
    pub fn clear(&mut self) {
        for instances in self.batches.values_mut() {
            instances.clear();
        }
    }

    /// Append one sprite instance to the bucket for `texture_id`.
    pub fn push_sprite(&mut self, texture_id: u32, instance: SpriteInstance) {
        self.batches
            .entry(texture_id)
            .or_insert_with(Vec::new)
            .push(instance);
    }

    /// Total de instancias en todos los batches.
    #[inline]
    pub fn total_instances(&self) -> usize {
        self.batches.values().map(|v| v.len()).sum()
    }
}

/// Batcher para shapes geométricas sin textura (SDF).
/// Es un Vec plano — máxima cache locality, O(1) push, 1 draw call total.
#[derive(Default)]
pub struct ShapeBatcher {
    /// Flat, insertion-ordered list of shape instances.
    pub instances: Vec<crate::render::types::ShapeInstance>,
}

impl ShapeBatcher {
    /// Crea un batcher con capacidad pre-reservada para evitar reallocaciones.
    pub fn with_capacity(cap: usize) -> Self {
        Self { instances: Vec::with_capacity(cap) }
    }

    /// Limpia sin liberar memoria (hot path: O(1)).
    #[inline]
    pub fn clear(&mut self) {
        self.instances.clear();
    }

    /// Push O(1) amortizado. Sin sort: el orden de inserción es el orden de render.
    /// En Pure2D inmediato el usuario controla el orden.
    #[inline]
    pub fn push_shape(&mut self, instance: crate::render::types::ShapeInstance) {
        self.instances.push(instance);
    }

    /// Ordena por Z (profundidad) en el eje W de la columna 3 de la matrix.
    /// Llamar solo desde el sistema ECS, no en el modo inmediato.
    pub fn sort_by_z(&mut self) {
        self.instances.sort_unstable_by(|a, b| {
            // La traslación Z está en transform_c3[2] (columna 3, componente Z)
            a.transform_c3[2].partial_cmp(&b.transform_c3[2])
                .unwrap_or(std::cmp::Ordering::Equal)
        });
    }
}

/// A single draw command in the unified 2D command stream, sorted by ascending z
/// so shapes and sprites interleave correctly regardless of pipeline.
///
/// Indices refer to the *flat GPU upload layouts* produced each frame by
/// `Renderer2d::prepare_shapes` / `Renderer2d::prepare`:
/// - `Shape(i)` → `ShapeBatcher.instances[i]`.
/// - `Sprite(tex_id, i)` → i-th instance of the flattened sprite upload, which
///   is `SpriteBatcher` iterated in `BTreeMap` (ascending `texture_id`) order.
///
/// The z value used for sorting is read from the instance's own transform
/// (`transform_c3[2]`), which is how `sort_by_z` and the immediate-mode
/// renderer encode depth. (The ECS `prepare_*_system` helpers are deprecated
/// dead code: nothing registers them.)
///
/// `Ord`/`PartialOrd` are derived so the command itself can act as the
/// deterministic tie-break for equal `z` (variant order = insertion order:
/// shapes before sprites, then by shape index / texture id / sprite offset).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum DrawCommand2d {
    /// Draw `ShapeBatcher.instances[index]` with the shape (SDF) pipeline.
    Shape(usize),
    /// Draw the sprite after `SpriteBatcher` flattening: `texture_id` selects
    /// the texture bind group, `index` is the offset in the flattened upload.
    Sprite(u32, usize),
}
