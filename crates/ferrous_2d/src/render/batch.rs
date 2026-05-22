use crate::render::types::SpriteInstance;
// BTreeMap: orden deterministico por texture_id + cache-friendly en iteración.
// A diferencia de HashMap, no hay hashing overhead ni reordering entre frames.
use std::collections::BTreeMap;

/// Agrupa sprites por Texture ID: 1 draw call por textura.
/// Usa `BTreeMap` para orden deterministico (reproducible entre frames).
#[derive(Default)]
pub struct SpriteBatcher {
    // texture_id → lista de instancias. BTreeMap mantiene el orden por texture_id.
    pub batches: BTreeMap<u32, Vec<SpriteInstance>>,
}

impl SpriteBatcher {
    /// Limpia los datos sin liberar las allociones (reutiliza la memoria del frame anterior).
    pub fn clear(&mut self) {
        for instances in self.batches.values_mut() {
            instances.clear();
        }
    }

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
