use crate::render::types::SpriteInstance;
use crate::render::BlendMode2d;
// BTreeMap: orden deterministico por texture_id + cache-friendly en iteración.
// A diferencia de HashMap, no hay hashing overhead ni reordering entre frames.
use std::collections::BTreeMap;

/// One queued sprite: the instance plus its per-draw [`BlendMode2d`].
#[derive(Debug, Clone, Copy)]
pub struct SpriteBatchEntry {
    /// The sprite instance (straight linear RGBA tint, see `SpriteInstance`).
    pub instance: SpriteInstance,
    /// Blend mode used when this sprite is drawn.
    pub blend: BlendMode2d,
}

/// Agrupa sprites por Texture ID: 1 draw call por textura.
/// Usa `BTreeMap` para orden deterministico (reproducible entre frames).
#[derive(Default)]
pub struct SpriteBatcher {
    // texture_id → lista de instancias. BTreeMap mantiene el orden por texture_id.
    /// Instances grouped by texture id; iterated in ascending id order.
    pub batches: BTreeMap<u32, Vec<SpriteBatchEntry>>,
}

impl SpriteBatcher {
    /// Limpia los datos sin liberar las allociones (reutiliza la memoria del frame anterior).
    pub fn clear(&mut self) {
        for instances in self.batches.values_mut() {
            instances.clear();
        }
    }

    /// Append one sprite instance to the bucket for `texture_id` with the
    /// default [`BlendMode2d::Normal`] blend.
    pub fn push_sprite(&mut self, texture_id: u32, instance: SpriteInstance) {
        self.push_sprite_blend(texture_id, instance, BlendMode2d::Normal);
    }

    /// Append one sprite instance with an explicit blend mode.
    pub fn push_sprite_blend(
        &mut self,
        texture_id: u32,
        instance: SpriteInstance,
        blend: BlendMode2d,
    ) {
        self.batches
            .entry(texture_id)
            .or_default()
            .push(SpriteBatchEntry { instance, blend });
    }

    /// Drop the bucket (key included) for a freed texture id.
    ///
    /// `clear` intentionally keeps empty buckets so the per-texture `Vec`
    /// allocations survive to the next frame, but a *freed* texture will
    /// never be pushed again, so its key would otherwise be iterated forever
    /// (the measured "2000 orphan keys" leak).  Returns `true` if a bucket
    /// existed.
    pub fn remove_texture(&mut self, texture_id: u32) -> bool {
        self.batches.remove(&texture_id).is_some()
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
        Self {
            instances: Vec::with_capacity(cap),
        }
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
            a.transform_c3[2]
                .partial_cmp(&b.transform_c3[2])
                .unwrap_or(std::cmp::Ordering::Equal)
        });
    }
}

/// A single draw command in the unified 2D command stream, sorted by ascending z
/// so shapes and sprites interleave correctly regardless of pipeline.
///
/// Indices refer to the *flat GPU upload layouts* produced each frame by
/// `Renderer2d::prepare_shapes` / `Renderer2d::prepare` / the engine's text
/// upload:
/// - `Shape { index, .. }` → `ShapeBatcher.instances[index]`.
/// - `Sprite { texture_id, index, .. }` → i-th instance of the flattened sprite
///   upload, which is `SpriteBatcher` iterated in `BTreeMap` (ascending
///   `texture_id`) order.
/// - `Text { index, .. }` → i-th entry of the engine's text batch.
///
/// Each command carries its [`BlendMode2d`], so a run of commands only
/// coalesces into one draw call when kind, bind group *and* blend mode match.
///
/// The z value used for sorting is read from the instance's own transform
/// (`transform_c3[2]`).  `Ord`/`PartialOrd` are derived so the command itself
/// is the deterministic tie-break for equal `z`.  The documented full order is:
///
/// 1. ascending `z`;
/// 2. for equal `z`: shapes, then sprites, then text (variant order);
/// 3. within a kind: submission order (shape index / texture id + sprite
///    offset / text index), then blend mode.
///
/// Text therefore interleaves with shapes and sprites by `z`, and glyphs of one
/// `draw_text` call stay in submission order, so consecutive glyphs of the same
/// atlas coalesce into one draw call.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum DrawCommand2d {
    /// Draw `ShapeBatcher.instances[index]` with the shape (SDF) pipeline.
    Shape {
        /// Index into the painter-ordered shape upload.
        index: usize,
        /// Per-draw blend mode.
        blend: BlendMode2d,
    },
    /// Draw the sprite after `SpriteBatcher` flattening: `texture_id` selects
    /// the texture bind group, `index` is the offset in the flattened upload.
    Sprite {
        /// Registered sprite texture id.
        texture_id: u32,
        /// Offset in the flattened sprite upload.
        index: usize,
        /// Per-draw blend mode.
        blend: BlendMode2d,
    },
    /// Draw the text-batch entry `index` with the MSDF text pipeline.
    Text {
        /// Index into the text batch.
        index: usize,
        /// Per-draw blend mode.
        blend: BlendMode2d,
    },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn remove_texture_drops_key_but_clear_keeps_it() {
        let mut b = SpriteBatcher::default();
        // `clear` is a per-frame reset that must preserve the `Vec`
        // allocations of live textures; `remove_texture` is the free path that
        // must delete the key entirely to avoid unbounded orphan-key growth.
        b.batches.insert(7, Vec::new());
        b.batches.insert(9, Vec::new());

        assert!(b.remove_texture(7));
        assert!(!b.batches.contains_key(&7));
        assert!(b.batches.contains_key(&9));
        assert!(!b.remove_texture(7), "removing an absent key returns false");

        b.clear();
        assert!(b.batches.contains_key(&9), "clear keeps live buckets");
    }
}
