//! `FrameBuilder` — constructs the `FramePacket` per frame from ECS state,
//! applying frustum culling and grouping instances.
//!
//! ## Responsibilities
//! - Maintain caches of draw commands (reuse `Vec` between frames)
//! - Frustum culling of world (ECS) objects
//! - Group world objects by mesh (instancing)
//! - Upload matrices to `InstanceBuffer`
//! - Calculate `RenderStats` for the frame

use std::collections::HashMap;
use std::sync::Arc;

use ferrous_core::scene::world::{Element, ElementKind, MaterialComponent};
use ferrous_core::transform::Transform;

use crate::geometry::primitives::{
    cube::cube as create_cube, quad::quad as create_quad, sphere::sphere as create_sphere,
};
use crate::graph::frame_packet::{
    CameraPacket, FramePacket, InstancedDrawCommand, Viewport,
};
use crate::render_stats::RenderStats;
use crate::resources::InstanceBuffer;
use crate::scene::Frustum;

/// All per-frame scratch state that `FrameBuilder` needs to track between calls.
pub struct FrameBuilder {
    // Reusable draw command lists (zeroed each frame, allocated once)
    pub instanced_commands_cache: Vec<InstancedDrawCommand>,
    pub instance_matrix_scratch: Vec<glam::Mat4>,
    pub shadow_instanced_cache: Vec<InstancedDrawCommand>,
    pub shadow_matrix_scratch: Vec<glam::Mat4>,

    /// Last view-proj matrix; used to skip rebuild when neither scene nor
    /// camera changed.
    pub prev_view_proj: Option<glam::Mat4>,
    /// Set to `true` whenever scene geometry or materials change.
    pub scene_dirty: bool,

    // ── Phase 8: shared mesh caches (moved from Renderer) ───────────────────
    /// Shared cube mesh — lazily created on first ECS query spawn.
    pub shared_cube_mesh: Option<crate::geometry::Mesh>,
    /// Shared quad mesh.
    pub shared_quad_mesh: Option<crate::geometry::Mesh>,
    /// Shared sphere mesh + (latitudes, longitudes) key.
    pub shared_sphere_mesh: Option<(crate::geometry::Mesh, u32, u32)>,
    /// Cache of arbitrary meshes keyed by asset string. Only available when
    /// the `assets` feature is enabled; otherwise the field is omitted and
    /// related logic is compiled away.
    #[cfg(feature = "assets")]
    pub mesh_cache: HashMap<String, crate::geometry::Mesh>,

    // ── Phase 8: ECS-derived world draw commands (replaces world_objects) ───
    /// Instanced draw commands built from the ECS world query.
    /// Populated by `build_world_commands`; consumed by `build`.
    pub world_instanced: Vec<InstancedDrawCommand>,
    /// Shadow-caster instanced commands from the ECS query.
    world_shadow_instanced: Vec<InstancedDrawCommand>,
    /// Scratch matrices for world instancing (written to `InstanceBuffer`).
    pub world_instance_matrices: Vec<glam::Mat4>,
    /// Scratch matrices for shadow instancing.
    world_shadow_matrices: Vec<glam::Mat4>,
}

impl Default for FrameBuilder {
    fn default() -> Self {
        FrameBuilder::new()
    }
}

impl FrameBuilder {
    pub fn new() -> Self {
        FrameBuilder {
            instanced_commands_cache: Vec::new(),
            instance_matrix_scratch: Vec::new(),
            shadow_instanced_cache: Vec::new(),
            shadow_matrix_scratch: Vec::new(),
            prev_view_proj: None,
            scene_dirty: true,
            shared_cube_mesh: None,
            shared_quad_mesh: None,
            shared_sphere_mesh: None,
            #[cfg(feature = "assets")]
            mesh_cache: HashMap::new(),
            world_instanced: Vec::new(),
            world_shadow_instanced: Vec::new(),
            world_instance_matrices: Vec::new(),
            world_shadow_matrices: Vec::new(),
        }
    }

    /// Mark that the scene has changed and the next frame must be rebuilt.
    #[inline]
    pub fn mark_dirty(&mut self) {
        self.scene_dirty = true;
    }

    // -----------------------------------------------------------------------
    // Phase 8: ECS world command builder
    // -----------------------------------------------------------------------

    /// Query the ECS world and rebuild the world instanced draw command caches.
    ///
    /// Called by `Renderer::sync_world` whenever the scene changes.  Replaces
    /// the old `sync_world → world_objects Vec` indirection.
    ///
    /// Frustum culling is deferred to `build()` so that a camera change still
    /// re-culls even when the scene is otherwise static.
    pub fn build_world_commands(
        &mut self,
        world: &ferrous_core::scene::World,
        device: &wgpu::Device,
        frustum: &Frustum,
        camera_eye: glam::Vec3,
        instance_buf: &mut InstanceBuffer,
        instance_layout: &wgpu::BindGroupLayout,
        shadow_instance_buf: &mut InstanceBuffer,
        instance_callback: &mut dyn FnMut(Arc<wgpu::BindGroup>, Arc<wgpu::BindGroup>),
        queue: &wgpu::Queue,
    ) {
        // prune mesh cache of any keys that are no longer referenced by the world
        #[cfg(feature = "assets")]
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
            self.mesh_cache
                .retain(|k, _| live_keys.contains(k.as_str()));
        }

        type MeshGroupKey = (usize, usize, bool);
        type MeshGroupVal = (crate::geometry::Mesh, usize, Vec<glam::Mat4>);

        // Visible (camera-culled) groups for main draw pass
        let mut visible_groups: HashMap<MeshGroupKey, MeshGroupVal> = HashMap::new();
        // All-objects groups for shadow pass (no frustum culling)
        let mut shadow_groups: HashMap<MeshGroupKey, MeshGroupVal> = HashMap::new();

        for (_entity, element, transform, material) in
            world.ecs.query3::<Element, Transform, MaterialComponent>()
        {
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

            let is_double_sided = if let ElementKind::Quad { double_sided, .. } = element.kind {
                double_sided
            } else {
                false
            };

            let mesh = match &element.kind {
                ElementKind::Cube { .. } => self
                    .shared_cube_mesh
                    .get_or_insert_with(|| create_cube(device))
                    .clone(),
                ElementKind::Mesh { asset_key } => {
                    #[cfg(feature = "assets")]
                    {
                        if let Some(m) = self.mesh_cache.get(asset_key.as_str()) {
                            m.clone()
                        } else {
                            self.shared_cube_mesh
                                .get_or_insert_with(|| create_cube(device))
                                .clone()
                        }
                    }
                    #[cfg(not(feature = "assets"))]
                    {
                        // without asset support we just fall back to a cube mesh
                        self.shared_cube_mesh
                            .get_or_insert_with(|| create_cube(device))
                            .clone()
                    }
                }
                ElementKind::Quad { .. } => self
                    .shared_quad_mesh
                    .get_or_insert_with(|| create_quad(device))
                    .clone(),
                ElementKind::Sphere {
                    latitudes,
                    longitudes,
                    ..
                } => {
                    let lat = latitudes;
                    let lon = longitudes;
                    let use_mesh = if let Some((m, l, o)) = &self.shared_sphere_mesh {
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
                        self.shared_sphere_mesh = Some((new.clone(), *lat, *lon));
                        new
                    }
                }
                _ => continue,
            };

            let matrix = transform.matrix();
            let material_slot = material.handle.0 as usize;

            // Compute a quick AABB from the matrix for frustum culling
            let local_aabb = crate::scene::culling::Aabb::unit_cube();
            let world_aabb = local_aabb.transform(&matrix);

            let key = (
                Arc::as_ptr(&mesh.vertex_buffer) as usize,
                material_slot,
                is_double_sided,
            );

            // Shadow pass — all objects
            shadow_groups
                .entry(key)
                .or_insert_with(|| (mesh.clone(), material_slot, Vec::new()))
                .2
                .push(matrix);

            // Main pass — frustum culled
            if frustum.intersects_aabb(&world_aabb) {
                visible_groups
                    .entry(key)
                    .or_insert_with(|| (mesh.clone(), material_slot, Vec::new()))
                    .2
                    .push(matrix);
            }
        }

        // -- Build visible instanced commands --------------------------------
        self.world_instanced.clear();
        self.world_instance_matrices.clear();

        let total_visible: usize = visible_groups.values().map(|(_, _, m)| m.len()).sum();
        if total_visible > 0 {
            let prev_bg = instance_buf.bind_group.clone();
            instance_buf.reserve(device, instance_layout, total_visible);
            if !Arc::ptr_eq(&prev_bg, &instance_buf.bind_group) {
                instance_callback(
                    instance_buf.bind_group.clone(),
                    shadow_instance_buf.bind_group.clone(),
                );
            }

            let mut offset = 0u32;
            for ((_ptr, _mat_s, double_sided), (mesh, material_slot, mats)) in &visible_groups {
                let count = mats.len() as u32;
                self.world_instance_matrices.extend_from_slice(mats);

                let mut max_dist_sq = 0.0f32;
                for m in mats {
                    let pos = m.w_axis.truncate();
                    let d = (pos - camera_eye).length_squared();
                    if d > max_dist_sq {
                        max_dist_sq = d;
                    }
                }

                self.world_instanced.push(InstancedDrawCommand {
                    vertex_buffer: mesh.vertex_buffer.clone(),
                    index_buffer: mesh.index_buffer.clone(),
                    index_count: mesh.index_count,
                    vertex_count: mesh.vertex_count,
                    index_format: mesh.index_format,
                    first_instance: offset,
                    instance_count: count,
                    double_sided: *double_sided,
                    material_slot: *material_slot,
                    distance_sq: max_dist_sq,
                });
                offset += count;
            }
            instance_buf.write_slice(queue, 0, &self.world_instance_matrices);
        } else {
            // Ensure instance buffer has space even when empty
            let prev_bg = instance_buf.bind_group.clone();
            instance_buf.reserve(device, instance_layout, 1);
            if !Arc::ptr_eq(&prev_bg, &instance_buf.bind_group) {
                instance_callback(
                    instance_buf.bind_group.clone(),
                    shadow_instance_buf.bind_group.clone(),
                );
            }
        }

        // -- Build shadow instanced commands ---------------------------------
        self.world_shadow_instanced.clear();
        self.world_shadow_matrices.clear();

        let total_shadow: usize = shadow_groups.values().map(|(_, _, m)| m.len()).sum();
        if total_shadow > 0 {
            let prev_bg = shadow_instance_buf.bind_group.clone();
            shadow_instance_buf.reserve(device, instance_layout, total_shadow);
            if !Arc::ptr_eq(&prev_bg, &shadow_instance_buf.bind_group) {
                instance_callback(
                    instance_buf.bind_group.clone(),
                    shadow_instance_buf.bind_group.clone(),
                );
            }

            let mut offset = 0u32;
            for ((_ptr, _mat_s, double_sided), (mesh, material_slot, mats)) in &shadow_groups {
                let count = mats.len() as u32;
                self.world_shadow_matrices.extend_from_slice(mats);
                self.world_shadow_instanced.push(InstancedDrawCommand {
                    vertex_buffer: mesh.vertex_buffer.clone(),
                    index_buffer: mesh.index_buffer.clone(),
                    index_count: mesh.index_count,
                    vertex_count: mesh.vertex_count,
                    index_format: mesh.index_format,
                    first_instance: offset,
                    instance_count: count,
                    double_sided: *double_sided,
                    material_slot: *material_slot,
                    distance_sq: 0.0,
                });
                offset += count;
            }
            shadow_instance_buf.write_slice(queue, 0, &self.world_shadow_matrices);
        }
    }

    // -----------------------------------------------------------------------

    /// Build a `FramePacket` for the current frame.
    ///
    /// If neither the scene nor the camera changed since last frame, the
    /// cached command lists are reused (zero CPU work).
    ///
    /// World-object draw commands (ECS-derived) are taken from the caches
    /// populated by `build_world_commands()`.
    pub fn build(
        &mut self,
        viewport: Viewport,
        camera_packet: CameraPacket,
    ) -> (FramePacket, RenderStats) {
        // -- Fast path: scene unchanged + camera unchanged --------------------
        if !self.scene_dirty && self.prev_view_proj == Some(camera_packet.view_proj) {
            let mut packet = FramePacket::new(Some(viewport), camera_packet);
            std::mem::swap(
                &mut packet.instanced_objects,
                &mut self.instanced_commands_cache,
            );
            std::mem::swap(
                &mut packet.shadow_instanced_objects,
                &mut self.shadow_instanced_cache,
            );
            let stats = self.compute_stats();
            return (packet, stats);
        }

        // -- Slow path: rebuild -----------------------------------------------
        self.scene_dirty = false;
        self.prev_view_proj = Some(camera_packet.view_proj);
        self.instanced_commands_cache.clear();
        self.instance_matrix_scratch.clear();
        self.shadow_instanced_cache.clear();
        self.shadow_matrix_scratch.clear();

        // World-object instanced commands are already populated by
        // `build_world_commands()` (called from `Renderer::sync_world`).
        self.instanced_commands_cache
            .extend_from_slice(&self.world_instanced);
        self.shadow_instanced_cache
            .extend_from_slice(&self.world_shadow_instanced);

        let stats = self.compute_stats();

        let mut packet = FramePacket::new(Some(viewport), camera_packet);
        std::mem::swap(
            &mut packet.instanced_objects,
            &mut self.instanced_commands_cache,
        );
        std::mem::swap(
            &mut packet.shadow_instanced_objects,
            &mut self.shadow_instanced_cache,
        );

        (packet, stats)
    }

    /// Reclaim the internal caches from a used `FramePacket`.
    #[inline]
    pub fn reclaim(&mut self, mut packet: FramePacket) {
        std::mem::swap(
            &mut self.instanced_commands_cache,
            &mut packet.instanced_objects,
        );
        std::mem::swap(
            &mut self.shadow_instanced_cache,
            &mut packet.shadow_instanced_objects,
        );
    }

    /// Internal helper to calculate statistics based on current caches.
    fn compute_stats(&self) -> RenderStats {
        let mut stats = RenderStats::default();
        for cmd in &self.instanced_commands_cache {
            let inst = cmd.instance_count as u64;
            stats.vertex_count += cmd.vertex_count as u64 * inst;
            stats.triangle_count += (cmd.index_count / 3) as u64 * inst;
            stats.draw_calls += 1;
        }
        stats
    }
}
