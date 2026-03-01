/// Synchronises a `ferrous_core::scene::World` with the renderer's internal
/// `RenderObject` map.
///
/// ## Three-phase reconciliation (all O(n))
///
/// 1. **Remove** objects whose ID is no longer present in the world.
/// 2. **Spawn** new `RenderObject`s for entities that don't have one yet.
/// 3. **Update** the transform of all surviving objects.
///
/// Uses a `HashMap<u64, RenderObject>` for O(1) lookup instead of the
/// previous linear scan which was O(n²).
use std::collections::HashMap;

use ferrous_core::scene::{ElementKind, World};

use crate::geometry::primitives::cube::cube as create_cube;
use crate::scene::RenderObject;

/// Update `objects` so that it mirrors the renderable entities in `world`.
pub fn sync_world(
    world:        &World,
    objects:      &mut HashMap<u64, RenderObject>,
    device:       &wgpu::Device,
    queue:        &wgpu::Queue,
    model_layout: &wgpu::BindGroupLayout,
) {
    // ── Phase 1: remove stale objects ──────────────────────────────────────
    objects.retain(|id, _| world.contains(ferrous_core::scene::Handle(*id)));

    // ── Phase 2 & 3: spawn or update ──────────────────────────────────────
    for element in world.iter() {
        let is_renderable = matches!(
            element.kind,
            ElementKind::Cube { .. } | ElementKind::Mesh { .. }
        );
        if !is_renderable || !element.visible {
            continue;
        }

        let matrix = element.transform.matrix();

        if let Some(obj) = objects.get_mut(&element.id) {
            // Phase 3 — O(1) update
            obj.update_transform(queue, matrix);
        } else {
            // Phase 2 — spawn new object
            let mesh = match &element.kind {
                ElementKind::Cube { .. } => create_cube(device),
                // Future: resolve mesh from asset registry by key
                ElementKind::Mesh { .. } => create_cube(device),
                _ => unreachable!(),
            };
            let obj = RenderObject::new(device, element.id, mesh, matrix, model_layout);
            objects.insert(element.id, obj);
        }
    }
}
