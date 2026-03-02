/// `RenderObject` list.
///
/// ## Three-phase reconciliation (all O(n))
///
/// 1. **Remove** objects whose ID is no longer present in the world.
/// 2. **Spawn** new `RenderObject`s for entities that don't have one yet.
/// 3. **Update** the transform of all surviving objects.
///
/// Uses a `Vec<Option<RenderObject>>` for guaranteed sequential access.
use ferrous_core::scene::{ElementKind, World};

use crate::geometry::primitives::{cube::cube as create_cube, quad::quad as create_quad};
use crate::scene::RenderObject;

/// Update `objects` so that it mirrors the renderable entities in `world`.
///
/// `shared_cube_mesh` should be a persistent `Option<Mesh>` owned by the
/// caller (typically `Renderer`).  It is initialised on the first spawn and
/// reused thereafter so that every cube `RenderObject` shares the same
/// `Arc<wgpu::Buffer>` pointers — a prerequisite for the instancing grouping
/// in `build_base_packet`.
/// Update `objects` so that it mirrors the renderable entities in `world`.
///
/// `shared_cube_mesh` should be a persistent `Option<Mesh>` owned by the
/// caller (typically `Renderer`).  It is initialised on the first spawn and
/// reused thereafter so that every cube `RenderObject` shares the same
/// `Arc<wgpu::Buffer>` pointers — a prerequisite for the instancing grouping
/// in `build_base_packet`.
///
/// `shared_quad_mesh` is the equivalent cache for the unit quad mesh used by
/// `ElementKind::Quad`.
pub fn sync_world(
    world: &World,
    objects: &mut Vec<Option<RenderObject>>,
    device: &wgpu::Device,
    shared_cube_mesh: &mut Option<crate::geometry::Mesh>,
    shared_quad_mesh: &mut Option<crate::geometry::Mesh>,
) -> bool {
    let mut mutated = false;

    if objects.len() != world.capacity() {
        objects.resize_with(world.capacity(), || None);
        mutated = true;
    }

    // ── Phase 1: remove stale objects ────────────────────────────────────────
    for (id, opt_obj) in objects.iter_mut().enumerate() {
        if opt_obj.is_some() && !world.contains(ferrous_core::scene::Handle(id as u64)) {
            *opt_obj = None;
            mutated = true;
        }
    }

    // ── Phase 2 & 3: spawn or update ─────────────────────────────────────────
    for element in world.iter() {
        let is_renderable = matches!(
            element.kind,
            ElementKind::Cube { .. } | ElementKind::Mesh { .. } | ElementKind::Quad { .. }
        );
        if !is_renderable || !element.visible {
            continue;
        }

        let matrix = element.transform.matrix();
        let idx = element.id as usize;

        if let Some(ref mut obj) = objects[idx] {
            // Phase 3 — O(1) update.
            if obj.matrix != matrix {
                obj.set_matrix(matrix);
                mutated = true;
            }
        } else {
            // Phase 2 — spawn new object.
            let mesh = match &element.kind {
                ElementKind::Cube { .. } | ElementKind::Mesh { .. } => shared_cube_mesh
                    .get_or_insert_with(|| create_cube(device))
                    .clone(),
                ElementKind::Quad { .. } => shared_quad_mesh
                    .get_or_insert_with(|| create_quad(device))
                    .clone(),
                _ => unreachable!(),
            };
            let double_sided =
                matches!(element.kind, ElementKind::Quad { double_sided, .. } if double_sided);
            let mut obj = RenderObject::new(device, element.id, mesh, matrix, 0, double_sided);
            // Override AABB with per-axis half_extents when available.
            match element.kind {
                ElementKind::Cube { half_extents } => {
                    use crate::scene::culling::Aabb;
                    obj.local_aabb = Aabb::new(-half_extents, half_extents);
                    obj.cached_world_aabb = obj.local_aabb.transform(&matrix);
                }
                ElementKind::Quad { width, height, .. } => {
                    use crate::scene::culling::Aabb;
                    let hw = width * 0.5;
                    let hh = height * 0.5;
                    obj.local_aabb =
                        Aabb::new(glam::Vec3::new(-hw, -hh, 0.0), glam::Vec3::new(hw, hh, 0.0));
                    obj.cached_world_aabb = obj.local_aabb.transform(&matrix);
                }
                _ => {}
            }
            objects[idx] = Some(obj);
            mutated = true;
        }
    }

    mutated
}
