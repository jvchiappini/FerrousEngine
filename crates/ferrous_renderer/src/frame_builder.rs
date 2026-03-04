//! `FrameBuilder` — construye el `FramePacket` por frame a partir del estado
//! de escena, aplicando frustum culling y agrupando instancias.
//!
//! Extraído de `ferrous_renderer::lib::build_base_packet` (Fase 3 del roadmap).
//! Esto deja `Renderer::do_render` enfocado en **orquestar passes**, no en
//! gestionar lógica de culling/instancing.
//!
//! ## Responsabilidades
//! - Mantener caches de draw commands (reutilización de `Vec` entre frames)
//! - Frustum culling de objetos legacy y world
//! - Agrupación de objetos world por mesh (instancing)
//! - Upload de matrices al `InstanceBuffer` y `ModelBuffer`
//! - Calcular `RenderStats` para el frame

use std::collections::HashMap;
use std::sync::Arc;

#[cfg(not(target_arch = "wasm32"))]
use rayon::prelude::*;

use crate::graph::frame_packet::{
    CameraPacket, DrawCommand, FramePacket, InstancedDrawCommand, Viewport,
};
use crate::render_stats::RenderStats;
use crate::resources::InstanceBuffer;
use crate::scene::{Frustum, RenderObject};

/// All per-frame scratch state that `FrameBuilder` needs to track between calls.
pub struct FrameBuilder {
    // Reusable draw command lists (zeroed each frame, allocated once)
    pub draw_commands_cache: Vec<DrawCommand>,
    pub instanced_commands_cache: Vec<InstancedDrawCommand>,
    pub instance_matrix_scratch: Vec<glam::Mat4>,
    pub shadow_scene_cache: Vec<DrawCommand>,
    pub shadow_instanced_cache: Vec<InstancedDrawCommand>,
    pub shadow_matrix_scratch: Vec<glam::Mat4>,

    /// Last view-proj matrix; used to skip rebuild when neither scene nor
    /// camera changed.
    pub prev_view_proj: Option<glam::Mat4>,
    /// Set to `true` whenever scene geometry or materials change.
    pub scene_dirty: bool,
}

impl Default for FrameBuilder {
    fn default() -> Self {
        FrameBuilder::new()
    }
}

impl FrameBuilder {
    pub fn new() -> Self {
        FrameBuilder {
            draw_commands_cache: Vec::new(),
            instanced_commands_cache: Vec::new(),
            instance_matrix_scratch: Vec::new(),
            shadow_scene_cache: Vec::new(),
            shadow_instanced_cache: Vec::new(),
            shadow_matrix_scratch: Vec::new(),
            prev_view_proj: None,
            scene_dirty: true,
        }
    }

    /// Mark that the scene has changed and the next frame must be rebuilt.
    #[inline]
    pub fn mark_dirty(&mut self) {
        self.scene_dirty = true;
    }

    // -----------------------------------------------------------------------

    /// Build a `FramePacket` for the current frame.
    ///
    /// If neither the scene nor the camera changed since last frame, the
    /// cached command lists are reused (zero CPU work).
    #[allow(clippy::too_many_arguments)]
    pub fn build(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        viewport: Viewport,
        camera_packet: CameraPacket,
        legacy_objects: &HashMap<u64, RenderObject>,
        world_objects: &[Option<RenderObject>],
        instance_buf: &mut InstanceBuffer,
        instance_layout: &wgpu::BindGroupLayout,
        shadow_instance_buf: &mut InstanceBuffer,
        // Callbacks to notify passes of new instance buffer bind groups
        instance_callback: &mut dyn FnMut(Arc<wgpu::BindGroup>, Arc<wgpu::BindGroup>),
    ) -> (FramePacket, RenderStats) {
        // -- Fast path: scene unchanged + camera unchanged --------------------
        if !self.scene_dirty && self.prev_view_proj == Some(camera_packet.view_proj) {
            let mut packet = FramePacket::new(Some(viewport), camera_packet);
            std::mem::swap(&mut packet.scene_objects, &mut self.draw_commands_cache);
            std::mem::swap(
                &mut packet.instanced_objects,
                &mut self.instanced_commands_cache,
            );
            std::mem::swap(
                &mut packet.shadow_scene_objects,
                &mut self.shadow_scene_cache,
            );
            std::mem::swap(
                &mut packet.shadow_instanced_objects,
                &mut self.shadow_instanced_cache,
            );
            // Stats are recalculated for simplicity in this path too.
            let stats = self.compute_stats();
            return (packet, stats);
        }

        // -- Slow path: rebuild -----------------------------------------------
        self.scene_dirty = false;
        self.prev_view_proj = Some(camera_packet.view_proj);
        self.draw_commands_cache.clear();
        self.instanced_commands_cache.clear();
        self.instance_matrix_scratch.clear();
        self.shadow_scene_cache.clear();
        self.shadow_instanced_cache.clear();
        self.shadow_matrix_scratch.clear();

        let frustum = Frustum::from_view_proj(&camera_packet.view_proj);

        // -- Legacy objects ---------------------------------------------------
        // Shadow casters: all legacy objects regardless of camera frustum
        for obj in legacy_objects.values() {
            self.shadow_scene_cache.push(DrawCommand {
                vertex_buffer: obj.mesh.vertex_buffer.clone(),
                index_buffer: obj.mesh.index_buffer.clone(),
                index_count: obj.mesh.index_count,
                vertex_count: obj.mesh.vertex_count,
                index_format: obj.mesh.index_format,
                model_slot: obj.slot,
                double_sided: obj.double_sided,
                material_slot: obj.material_slot,
                distance_sq: 0.0,
            });
        }

        // Camera-visible legacy objects
        for obj in legacy_objects.values() {
            if frustum.intersects_aabb(&obj.world_aabb()) {
                let pos = obj.matrix.w_axis.truncate();
                let diff = pos - camera_packet.eye;
                let dist_sq = diff.length_squared();
                self.draw_commands_cache.push(DrawCommand {
                    vertex_buffer: obj.mesh.vertex_buffer.clone(),
                    index_buffer: obj.mesh.index_buffer.clone(),
                    index_count: obj.mesh.index_count,
                    vertex_count: obj.mesh.vertex_count,
                    index_format: obj.mesh.index_format,
                    model_slot: obj.slot,
                    double_sided: obj.double_sided,
                    material_slot: obj.material_slot,
                    distance_sq: dist_sq,
                });
            }
        }

        // -- World objects (instanced) ----------------------------------------
        type MeshGroupKey = (usize, usize, bool);
        type MeshGroupVal = (crate::geometry::Mesh, usize, Vec<glam::Mat4>);

        #[cfg(not(target_arch = "wasm32"))]
        let visible_mesh_groups: HashMap<MeshGroupKey, MeshGroupVal> = world_objects
            .par_iter()
            .flatten()
            .filter(|obj| frustum.intersects_aabb(&obj.world_aabb()))
            .fold(
                HashMap::new,
                |mut map: HashMap<MeshGroupKey, MeshGroupVal>, obj| {
                    let key = (
                        Arc::as_ptr(&obj.mesh.vertex_buffer) as usize,
                        obj.material_slot,
                        obj.double_sided,
                    );
                    map.entry(key)
                        .or_insert_with(|| (obj.mesh.clone(), obj.material_slot, Vec::new()))
                        .2
                        .push(obj.matrix);
                    map
                },
            )
            .reduce(HashMap::new, |mut a, b| {
                for (k, (mesh, mat, mats)) in b {
                    a.entry(k)
                        .or_insert_with(|| (mesh.clone(), mat, Vec::new()))
                        .2
                        .extend(mats);
                }
                a
            });

        #[cfg(target_arch = "wasm32")]
        let visible_mesh_groups: HashMap<MeshGroupKey, MeshGroupVal> = world_objects
            .iter()
            .flatten()
            .filter(|obj| frustum.intersects_aabb(&obj.world_aabb()))
            .fold(HashMap::new(), |mut map, obj| {
                let key = (
                    Arc::as_ptr(&obj.mesh.vertex_buffer) as usize,
                    obj.material_slot,
                    obj.double_sided,
                );
                map.entry(key)
                    .or_insert_with(|| (obj.mesh.clone(), obj.material_slot, Vec::new()))
                    .2
                    .push(obj.matrix);
                map
            });

        let mut total_visible = 0;
        for (_, (_, _, mats)) in &visible_mesh_groups {
            total_visible += mats.len();
        }

        // Upload visible matrices
        if total_visible > 0 {
            let prev_bg = instance_buf.bind_group.clone();
            instance_buf.reserve(device, instance_layout, total_visible);
            if !Arc::ptr_eq(&prev_bg, &instance_buf.bind_group) {
                instance_callback(
                    instance_buf.bind_group.clone(),
                    shadow_instance_buf.bind_group.clone(),
                );
            }

            let mut offset = 0;
            self.instance_matrix_scratch.clear();
            self.instance_matrix_scratch.reserve(total_visible);

            for ((_key, _mat, double_sided), (mesh, material_slot, mats)) in visible_mesh_groups {
                let count = mats.len() as u32;
                self.instance_matrix_scratch.extend_from_slice(&mats);

                let mut max_dist_sq = 0.0;
                for m in &mats {
                    let pos = m.w_axis.truncate();
                    let diff = pos - camera_packet.eye;
                    let d = diff.length_squared();
                    if d > max_dist_sq {
                        max_dist_sq = d;
                    }
                }

                self.instanced_commands_cache.push(InstancedDrawCommand {
                    vertex_buffer: mesh.vertex_buffer.clone(),
                    index_buffer: mesh.index_buffer.clone(),
                    index_count: mesh.index_count,
                    vertex_count: mesh.vertex_count,
                    index_format: mesh.index_format,
                    first_instance: offset,
                    instance_count: count,
                    double_sided,
                    material_slot,
                    distance_sq: max_dist_sq,
                });

                offset += count;
            }

            instance_buf.write_slice(queue, 0, &self.instance_matrix_scratch);
        }

        // -- Shadow-caster instanced list (World objects, no camera cull) ----
        {
            self.shadow_matrix_scratch.clear();
            let mut shadow_groups: HashMap<MeshGroupKey, MeshGroupVal> = HashMap::new();
            for obj in world_objects.iter().flatten() {
                let key = (
                    Arc::as_ptr(&obj.mesh.vertex_buffer) as usize,
                    obj.material_slot,
                    obj.double_sided,
                );
                shadow_groups
                    .entry(key)
                    .or_insert_with(|| (obj.mesh.clone(), obj.material_slot, Vec::new()))
                    .2
                    .push(obj.matrix);
            }

            let total_shadow = shadow_groups
                .values()
                .map(|(_, _, m)| m.len())
                .sum::<usize>();
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
                for ((_ptr, _mat, double_sided), (mesh, material_slot, mats)) in shadow_groups {
                    let count = mats.len() as u32;
                    self.shadow_matrix_scratch.extend_from_slice(&mats);
                    self.shadow_instanced_cache.push(InstancedDrawCommand {
                        vertex_buffer: mesh.vertex_buffer.clone(),
                        index_buffer: mesh.index_buffer.clone(),
                        index_count: mesh.index_count,
                        vertex_count: mesh.vertex_count,
                        index_format: mesh.index_format,
                        first_instance: offset,
                        instance_count: count,
                        double_sided,
                        material_slot,
                        distance_sq: 0.0,
                    });
                    offset += count;
                }

                shadow_instance_buf.write_slice(queue, 0, &self.shadow_matrix_scratch);
            }
        }

        let stats = self.compute_stats();

        let mut packet = FramePacket::new(Some(viewport), camera_packet);
        std::mem::swap(&mut packet.scene_objects, &mut self.draw_commands_cache);
        std::mem::swap(
            &mut packet.instanced_objects,
            &mut self.instanced_commands_cache,
        );
        std::mem::swap(
            &mut packet.shadow_scene_objects,
            &mut self.shadow_scene_cache,
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
        std::mem::swap(&mut self.draw_commands_cache, &mut packet.scene_objects);
        std::mem::swap(
            &mut self.instanced_commands_cache,
            &mut packet.instanced_objects,
        );
        std::mem::swap(
            &mut self.shadow_scene_cache,
            &mut packet.shadow_scene_objects,
        );
        std::mem::swap(
            &mut self.shadow_instanced_cache,
            &mut packet.shadow_instanced_objects,
        );
    }

    /// Internal helper to calculate statistics based on current caches.
    fn compute_stats(&self) -> RenderStats {
        let mut stats = RenderStats::default();
        for cmd in &self.draw_commands_cache {
            stats.vertex_count += cmd.vertex_count as u64;
            stats.triangle_count += (cmd.index_count / 3) as u64;
            stats.draw_calls += 1;
        }
        for cmd in &self.instanced_commands_cache {
            let inst = cmd.instance_count as u64;
            stats.vertex_count += cmd.vertex_count as u64 * inst;
            stats.triangle_count += (cmd.index_count / 3) as u64 * inst;
            stats.draw_calls += 1;
        }
        stats
    }
}
