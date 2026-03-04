/// `RenderObject` list.
///
/// ## Three-phase reconciliation (all O(n))
///
/// 1. **Remove** objects whose ID is no longer present in the world.
/// 2. **Spawn** new `RenderObject`s for entities that don't have one yet.
/// 3. **Update** the transform of all surviving objects.
///
/// Uses a `Vec<Option<RenderObject>>` for guaranteed sequential access.
use ferrous_core::scene::{ElementKind, World, Element};
use ferrous_core::scene::world::MaterialComponent;
use ferrous_core::transform::Transform;

use crate::geometry::primitives::{
    cube::cube as create_cube, quad::quad as create_quad, sphere::sphere as create_sphere,
};
use crate::scene::RenderObject;
use crate::scene::object::RenderObject as Object; // just in case for disambiguation if needed

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
    // cache a sphere mesh along with the subdivisions used to create it.
    // this allows us to reuse the same mesh for multiple spheres with the
    // same latitude/longitude counts.
    shared_sphere_mesh: &mut Option<(crate::geometry::Mesh, u32, u32)>,
    // optional cache for arbitrary meshes keyed by asset string.  callers
    // (typically `Renderer`) are responsible for populating entries via
    // `Renderer::register_mesh` prior to the first sync.
    mesh_cache: &mut std::collections::HashMap<String, crate::geometry::Mesh>,
) -> bool {
    let mut mutated = false;

    if objects.len() != world.capacity() {
        objects.resize_with(world.capacity(), || None);
        mutated = true;
    }

    // prune mesh cache of any keys that are no longer referenced by the
    // world; this keeps the cache from growing unbounded as meshes are
    // spawned/destroyed.  we compute the set lazily to avoid another hash
    // lookup per element.
    {
        let live_keys: std::collections::HashSet<&str> = world
            .iter()
            .filter_map(|e| {
                if let ElementKind::Mesh { asset_key } = &e.kind {
                    Some(asset_key.as_str())
                } else {
                    None
                }
            })
            .collect();
        mesh_cache.retain(|k, _| live_keys.contains(k.as_str()));
    }

    // ── Phase 1: remove stale objects ────────────────────────────────────────
    for (id, opt_obj) in objects.iter_mut().enumerate() {
        if opt_obj.is_some() && !world.contains(ferrous_core::scene::Handle(id as u64)) {
            *opt_obj = None;
            mutated = true;
        }
    }

    // ── Phase 2 & 3: spawn or update ─────────────────────────────────────────
    for (_entity, element, transform, material) in world.ecs.query3::<Element, Transform, MaterialComponent>() {
        let is_renderable = matches!(
            element.kind,
            ElementKind::Cube { .. }
                | ElementKind::Mesh { .. }
                | ElementKind::Quad { .. }
                | ElementKind::Sphere { .. }
        );
        if !is_renderable || !element.visible {
            continue;
        }

        let matrix = transform.matrix();
        let idx = element.id as usize;

        if let Some(ref mut obj) = objects[idx] {
            // Phase 3 — O(1) update.
            if obj.matrix != matrix {
                obj.set_matrix(matrix);
                mutated = true;
            }
            // update material slot if the world descriptor changed
            let slot = material.handle.0 as usize;
            if obj.material_slot != slot {
                obj.material_slot = slot;
                mutated = true;
            }
        } else {
            // Phase 2 — spawn new object.
            let is_double_sided = if let ElementKind::Quad { double_sided, .. } = element.kind {
                double_sided
            } else {
                false
            };

            let mesh = match &element.kind {
                ElementKind::Cube { .. } => shared_cube_mesh
                    .get_or_insert_with(|| create_cube(device))
                    .clone(),
                ElementKind::Mesh { asset_key } => {
                    if let Some(m) = mesh_cache.get(asset_key.as_str()) {
                        m.clone()
                    } else {
                        shared_cube_mesh
                            .get_or_insert_with(|| create_cube(device))
                            .clone()
                    }
                }
                ElementKind::Quad { .. } => shared_quad_mesh
                    .get_or_insert_with(|| create_quad(device))
                    .clone(),
                ElementKind::Sphere {
                    latitudes,
                    longitudes,
                    ..
                } => {
                    let lat = latitudes;
                    let lon = longitudes;
                    
                    let use_mesh = if let Some((m, l, o)) = shared_sphere_mesh {
                        if l == lat && o == lon {
                            Some(m.clone())
                        } else {
                            None
                        }
                    } else {
                        None
                    };

                    if let Some(m) = use_mesh {
                        m
                    } else {
                        let new = create_sphere(device, 1.0, *lat, *lon);
                        *shared_sphere_mesh = Some((new.clone(), *lat, *lon));
                        new
                    }
                }
                _ => continue,
            };
            
            let obj = RenderObject::new(
                device,
                element.id,
                mesh,
                matrix,
                idx, // Using idx as slot for now
                is_double_sided,
                material.handle.0 as usize,
            );
            objects[idx] = Some(obj);
            mutated = true;
        }
    }

    mutated
}
