/// Synchronises a `ferrous_core::scene::World` with the renderer's internal
/// `RenderObject` list.
///
/// The function performs a three-phase reconciliation:
///
/// 1. **Remove** objects whose ID is no longer present in the world.
/// 2. **Spawn** new `RenderObject`s for entities that don't have one yet.
/// 3. **Update** the transform of all surviving objects.
///
/// Only entities with a `Cube` or `Mesh` kind are rendered; `PointLight` and
/// `Empty` kinds are silently skipped (they may be consumed by other systems).
use ferrous_core::scene::{ElementKind, World};

use crate::geometry::primitives::cube::cube as create_cube;
use crate::scene::RenderObject;

/// Update `objects` so that it mirrors the renderable entities in `world`.
pub fn sync_world(
    world:        &World,
    objects:      &mut Vec<RenderObject>,
    device:       &wgpu::Device,
    queue:        &wgpu::Queue,
    model_layout: &wgpu::BindGroupLayout,
) {
    // ── Phase 1: remove stale objects ──────────────────────────────────────
    objects.retain(|obj| world.contains(ferrous_core::scene::Handle(obj.id)));

    // ── Phase 2 & 3: spawn or update ──────────────────────────────────────
    for element in world.iter() {
        // Skip non-renderable kinds
        let is_renderable = matches!(
            element.kind,
            ElementKind::Cube { .. } | ElementKind::Mesh { .. }
        );
        if !is_renderable || !element.visible {
            continue;
        }

        let matrix = element.transform.matrix();

        if let Some(obj) = objects.iter_mut().find(|o| o.id == element.id) {
            // Phase 3 — update
            obj.update_transform(queue, matrix);
        } else {
            // Phase 2 — spawn
            let mesh = match &element.kind {
                ElementKind::Cube { .. } => create_cube(device),
                // Future: load mesh by asset_key
                ElementKind::Mesh { .. } => create_cube(device),
                _ => unreachable!(),
            };
            objects.push(RenderObject::new(device, element.id, mesh, matrix, model_layout));
        }
    }
}
