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
    capsule::capsule as create_capsule,
    circle::{circle as create_circle, ring as create_ring},
    cube::cube as create_cube,
    cylinder::cylinder as create_cylinder,
    plane::plane as create_plane,
    quad::quad as create_quad,
    shapes2d::{circle_2d, rect_2d, line_2d, path_to_mesh},
    sphere::sphere as create_sphere,
    torus::torus as create_torus,
};
use crate::graph::frame_packet::{CameraPacket, FramePacket, InstancedDrawCommand, Viewport};
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
    /// Cylinder/cone meshes keyed by (radius_top_bits, radius_bottom_bits, height_bits, segs, rings, open).
    pub cylinder_cache: HashMap<(u32, u32, u32, u32, u32, u8), crate::geometry::Mesh>,
    /// Torus meshes keyed by (radius_bits, tube_bits, radial_segs, tubular_segs).
    pub torus_cache: HashMap<(u32, u32, u32, u32), crate::geometry::Mesh>,
    /// Plane meshes keyed by (width_bits, height_bits, w_segs, h_segs).
    pub plane_cache: HashMap<(u32, u32, u32, u32), crate::geometry::Mesh>,
    /// Capsule meshes keyed by (radius_bits, height_bits, radial_segs, cap_segs).
    pub capsule_cache: HashMap<(u32, u32, u32, u32), crate::geometry::Mesh>,
    /// Circle/Ring meshes keyed by (inner_bits, outer_bits, segs, rings).
    pub disc_cache: HashMap<(u32, u32, u32, u32), crate::geometry::Mesh>,
    /// Text3D meshes keyed by (text, depth_bits, bevel_enabled, bevel_thickness_bits, bevel_size_bits, quality).
    pub text3d_cache: HashMap<(String, u32, bool, u32, u32, u8), crate::geometry::Mesh>,
    ///   Circle2D  → (tag=0, radius, resolution, do_fill, stroke_thickness, 0)
    ///   Rect2D    → (tag=1, width, height, do_fill, stroke_thickness, 0)
    ///   Line2D    → (tag=2, x0, y0, x1, y1, thickness)
    ///   Path      → (tag=3, path_hash, do_fill, stroke_thickness, 0, 0)
    pub shapes2d_cache: HashMap<(u8, u32, u32, u32, u32, u32), crate::geometry::Mesh>,
    /// Cache of procedurally-generated meshes registered via `register_mesh`.
    /// Always available (no feature gate) so that WASM procedural terrain
    /// and other runtime-generated geometry works without the `assets` feature.
    pub procedural_mesh_cache: HashMap<String, crate::geometry::Mesh>,
    /// Cache of arbitrary asset-loaded meshes keyed by asset string.
    /// Only available when the `assets` feature is enabled.
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
            cylinder_cache: HashMap::new(),
            torus_cache: HashMap::new(),
            plane_cache: HashMap::new(),
            capsule_cache: HashMap::new(),
            disc_cache: HashMap::new(),
            text3d_cache: HashMap::new(),
            shapes2d_cache: HashMap::new(),
            procedural_mesh_cache: HashMap::new(),
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
        camera_vp: glam::Mat4,
        viewport: crate::Viewport,
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
        // Note: procedural_mesh_cache is NOT pruned automatically — caller
        // must call `free_procedural_mesh` explicitly when geometry is freed.

        type MeshGroupKey = (usize, usize, bool, i32); // v15: Added Z-Index to grouping key
        type MeshGroupVal = (crate::geometry::Mesh, usize, Vec<glam::Mat4>);

        // Visible (camera-culled) groups for main draw pass
        let mut visible_groups: HashMap<MeshGroupKey, MeshGroupVal> = HashMap::new();
        // All-objects groups for shadow pass (no frustum culling)
        let mut shadow_groups: HashMap<MeshGroupKey, MeshGroupVal> = HashMap::new();
        
        let mut entities_to_render = Vec::new();
        let query = ferrous_ecs::query::Query::<(
                &Element,
                &Transform,
                &MaterialComponent,
                Option<&ferrous_core::scene::ShadowCaster>,
                Option<&ferrous_core::scene::Billboard>,
                Option<&ferrous_core::scene::ScreenSpace>,
                Option<&ferrous_core::scene::world::types::ZIndex>,
            )>::new(&world.ecs);
            
        for (entity, (element, transform, material, shadow_caster, billboard, screen_space, z_index)) in query.iter()
        {
            entities_to_render.push((entity, element, transform, material, shadow_caster, billboard, screen_space, z_index));
        }

        // v13: Algorithm of the Painter (Z-Index sorting)
        // Sort by Z-Index (None = 0)
        entities_to_render.sort_by_key(|(_, _, _, _, _, _, _, zi)| zi.map(|z| z.0).unwrap_or(0));

        for (_entity, element, transform, material, shadow_caster, billboard, screen_space, z_index) in entities_to_render {
            // ... (keep the same match block for mesh selection)
            let is_renderable = matches!(
                element.kind,
                ElementKind::Cube { .. }
                    | ElementKind::Mesh { .. }
                    | ElementKind::Quad { .. }
                    | ElementKind::Sphere { .. }
                    | ElementKind::Cylinder { .. }
                    | ElementKind::Torus { .. }
                    | ElementKind::Plane { .. }
                    | ElementKind::Capsule { .. }
                    | ElementKind::Circle { .. }
                    | ElementKind::Ring { .. }
                    | ElementKind::Text3D { .. }
                    | ElementKind::Circle2D { .. }
                    | ElementKind::Rect2D { .. }
                    | ElementKind::Line2D { .. }
                    | ElementKind::Path
            );
            if !is_renderable || !element.visible {
                continue;
            }

            // The material descriptor dictates if this mesh should be rendered double-sided.
            // (e.g. 2D shapes, ScreenSpace UIs with inverted Y projections, etc.)
            let is_double_sided = material.descriptor.double_sided
                || element.screen_space
                || screen_space.is_some()
                || matches!(
                    element.kind,
                    ElementKind::Circle2D { .. }
                        | ElementKind::Rect2D { .. }
                        | ElementKind::Line2D { .. }
                        | ElementKind::Path
                )
                || if let ElementKind::Quad { double_sided, .. } = element.kind {
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
                    if let Some(m) = self.procedural_mesh_cache.get(asset_key.as_str()) {
                        m.clone()
                    } else {
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
                            continue;
                        }
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
                    let use_mesh = if let Some((m, l, o)) = &self.shared_sphere_mesh {
                        if l == latitudes && o == longitudes {
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
                        let new = create_sphere(device, 1.0, *latitudes, *longitudes);
                        self.shared_sphere_mesh = Some((new.clone(), *latitudes, *longitudes));
                        new
                    }
                }
                ElementKind::Cylinder {
                    radius_top,
                    radius_bottom,
                    height,
                    radial_segments,
                    height_segments,
                    open_ended,
                } => {
                    let key = (
                        radius_top.to_bits(), radius_bottom.to_bits(), height.to_bits(),
                        *radial_segments, *height_segments, *open_ended as u8,
                    );
                    self.cylinder_cache
                        .entry(key)
                        .or_insert_with(|| create_cylinder(
                            device, *radius_top, *radius_bottom, *height,
                            *radial_segments, *height_segments, *open_ended,
                        ))
                        .clone()
                }
                ElementKind::Torus { radius, tube, radial_segments, tubular_segments } => {
                    let key = (
                        radius.to_bits(), tube.to_bits(),
                        *radial_segments, *tubular_segments,
                    );
                    self.torus_cache
                        .entry(key)
                        .or_insert_with(|| create_torus(
                            device, *radius, *tube,
                            *radial_segments, *tubular_segments,
                            std::f32::consts::TAU,
                        ))
                        .clone()
                }
                ElementKind::Plane { width, height, width_segments, height_segments } => {
                    let key = (
                        width.to_bits(), height.to_bits(),
                        *width_segments, *height_segments,
                    );
                    self.plane_cache
                        .entry(key)
                        .or_insert_with(|| create_plane(
                            device, *width, *height,
                            *width_segments, *height_segments,
                        ))
                        .clone()
                }
                ElementKind::Capsule { radius, height, radial_segments, cap_segments } => {
                    let key = (
                        radius.to_bits(), height.to_bits(),
                        *radial_segments, *cap_segments,
                    );
                    self.capsule_cache
                        .entry(key)
                        .or_insert_with(|| create_capsule(
                            device, *radius, *height,
                            *radial_segments, *cap_segments,
                        ))
                        .clone()
                }
                ElementKind::Circle { radius, segments } => {
                    let key = (0u32, radius.to_bits(), *segments, 1u32);
                    self.disc_cache
                        .entry(key)
                        .or_insert_with(|| create_circle(device, *radius, *segments))
                        .clone()
                }
                ElementKind::Ring { inner_radius, outer_radius, segments, rings } => {
                    let key = (
                        inner_radius.to_bits(), outer_radius.to_bits(),
                        *segments, *rings,
                    );
                    self.disc_cache
                        .entry(key)
                        .or_insert_with(|| create_ring(
                            device, *inner_radius, *outer_radius, *segments, *rings,
                        ))
                        .clone()
                }
                ElementKind::Text3D { text, font_data, depth, bevel_enabled, bevel_thickness, bevel_size, quality } => {
                    let key = (
                        text.clone(),
                        depth.to_bits(),
                        *bevel_enabled,
                        bevel_thickness.to_bits(),
                        bevel_size.to_bits(),
                        *quality,
                    );
                    self.text3d_cache
                        .entry(key)
                        .or_insert_with(|| {
                            let builder = crate::geometry::primitives::Text3dBuilder::new(text, font_data)
                                .depth(*depth)
                                .quality(*quality)
                                .bevel(*bevel_enabled, *bevel_thickness, *bevel_size);
                            
                            builder.build(device).unwrap_or_else(|_| create_cube(device))
                        })
                        .clone()
                }
                ElementKind::Circle2D { radius, resolution } => {
                    let do_fill = element.fill_color.is_some();
                    let f_c = element.fill_color.unwrap_or([1.0, 1.0, 1.0, 1.0]);
                    let s_c = element.stroke_color.unwrap_or([1.0, 1.0, 1.0, 1.0]);
                    let f_c_bits = f_c[0].to_bits() ^ f_c[1].to_bits() ^ f_c[2].to_bits() ^ f_c[3].to_bits();
                    let s_c_bits = s_c[0].to_bits() ^ s_c[1].to_bits() ^ s_c[2].to_bits() ^ s_c[3].to_bits();
                    let key = (0u8, radius.to_bits(), *resolution, do_fill as u32, element.stroke_thickness.to_bits(), f_c_bits ^ s_c_bits);
                    self.shapes2d_cache
                        .entry(key)
                        .or_insert_with(|| circle_2d(device, *radius, *resolution, do_fill, element.stroke_thickness, f_c, s_c))
                        .clone()
                }
                ElementKind::Rect2D { width, height } => {
                    let do_fill = element.fill_color.is_some();
                    let f_c = element.fill_color.unwrap_or([1.0, 1.0, 1.0, 1.0]);
                    let s_c = element.stroke_color.unwrap_or([1.0, 1.0, 1.0, 1.0]);
                    let f_c_bits = f_c[0].to_bits() ^ f_c[1].to_bits() ^ f_c[2].to_bits() ^ f_c[3].to_bits();
                    let s_c_bits = s_c[0].to_bits() ^ s_c[1].to_bits() ^ s_c[2].to_bits() ^ s_c[3].to_bits();
                    let key = (1u8, width.to_bits(), height.to_bits(), do_fill as u32, element.stroke_thickness.to_bits(), f_c_bits ^ s_c_bits);
                    self.shapes2d_cache
                        .entry(key)
                        .or_insert_with(|| rect_2d(device, *width, *height, do_fill, element.stroke_thickness, f_c, s_c))
                        .clone()
                }
                ElementKind::Line2D { x0, y0, x1, y1, .. } => {
                    let s_c = element.stroke_color.unwrap_or([1.0, 1.0, 1.0, 1.0]);
                    let s_c_bits = s_c[0].to_bits() ^ s_c[1].to_bits() ^ s_c[2].to_bits() ^ s_c[3].to_bits();
                    let t = element.stroke_thickness;
                    let key = (2u8, x0.to_bits(), y0.to_bits(), x1.to_bits(), y1.to_bits(), t.to_bits() ^ s_c_bits);
                    self.shapes2d_cache
                        .entry(key)
                        .or_insert_with(|| line_2d(device, *x0, *y0, *x1, *y1, t, s_c))
                        .clone()
                }
                ElementKind::Path => {
                    if let Some(path_data) = world.ecs.get::<ferrous_core::scene::world::types::PathData>(_entity) {
                        let fill_c = element.fill_color.unwrap_or([1.0, 1.0, 1.0, 1.0]);
                        let stroke_c = element.stroke_color.unwrap_or([1.0, 1.0, 1.0, 1.0]);
                        path_to_mesh(device, path_data, element.fill_color.is_some(), element.stroke_thickness, fill_c, stroke_c)
                    } else {
                        continue;
                    }
                }
                _ => continue,
            };

            let mut matrix = transform.matrix();
            // v15: Apply Z-Index offset to the world matrix. 
            // This ensures that even if objects are in the same frame and overlap, 
            // the depth buffer will correctly resolve their ordering based on world-space Z.
            let zi = z_index.map(|z| z.0).unwrap_or(0);
            // v16: Añadimos un minúsculo offset basado en el orden de creación (id) 
            // paramitigar el Z-fighting cuando todas las formas tienen z_index=0 por defecto,
            // forzando el Painter's Algorithm automático de Manim.
            let sub_offset = (element.id as f32) * 0.00001;
            matrix.w_axis.z += zi as f32 * 0.001 + sub_offset;

            if let Some(bb) = billboard {
                // ... (billboard logic remains same)
                use ferrous_core::scene::BillboardMode;
                let rot = match bb.mode {
                    BillboardMode::Spherical => {
                        let forward = (camera_eye - transform.position).normalize_or_zero();
                        if forward.length_squared() < 1e-10 {
                            glam::Quat::IDENTITY
                        } else {
                            let world_up = if forward.dot(glam::Vec3::Y).abs() > 0.999 { glam::Vec3::Z } else { glam::Vec3::Y };
                            let right = world_up.cross(forward).normalize();
                            let up    = forward.cross(right);
                            glam::Quat::from_mat3(&glam::Mat3::from_cols(right, up, forward))
                        }
                    }
                    BillboardMode::Cylindrical => {
                        let mut cam_flat = camera_eye;
                        cam_flat.y = transform.position.y;
                        let forward_xz = (cam_flat - transform.position).normalize_or_zero();
                        if forward_xz.length_squared() < 1e-10 {
                            glam::Quat::IDENTITY
                        } else {
                            let right = glam::Vec3::Y.cross(forward_xz).normalize();
                            let up    = forward_xz.cross(right);
                            glam::Quat::from_mat3(&glam::Mat3::from_cols(right, up, forward_xz))
                        }
                    }
                };
                matrix = glam::Mat4::from_scale_rotation_translation(transform.scale, rot, transform.position + glam::Vec3::new(0.0, 0.0, zi as f32 * 0.001 + sub_offset));
            }
            let material_slot = material.handle.0 as usize;

            if element.screen_space || screen_space.is_some() {
                let wf = viewport.width as f32;
                let hf = viewport.height as f32;
                let ortho = glam::Mat4::orthographic_rh(0.0, wf, hf, 0.0, -1.0, 1.0);
                let screen_matrix = crate::resources::camera::OPENGL_TO_WGPU_MATRIX * ortho * matrix;
                let mut col0 = screen_matrix.col(0);
                let mut col1 = screen_matrix.col(1);
                let mut col2 = screen_matrix.col(2);
                let mut col3 = screen_matrix.col(3);
                col0.z = 0.0; col1.z = 0.0; col2.z = 0.0; col3.z = 0.0;
                col2.w = 55.5; col3.w = 1.0;
                matrix = glam::Mat4::from_cols(col0, col1, col2, col3);
                
                let key = (Arc::as_ptr(&mesh.vertex_buffer) as usize, material_slot, is_double_sided, zi);
                visible_groups.entry(key).or_insert_with(|| (mesh.clone(), material_slot, Vec::new())).2.push(matrix);
                continue;
            }

            let world_aabb = mesh.aabb.transform(&matrix);
            let key = (Arc::as_ptr(&mesh.vertex_buffer) as usize, material_slot, is_double_sided, zi);

            if shadow_caster.is_some() {
                shadow_groups.entry(key).or_insert_with(|| (mesh.clone(), material_slot, Vec::new())).2.push(matrix);
            }

            // v15: For mathematical paths, wait for fill_color directly
            let is_filled = element.fill_color.is_some();
            let effective_thick = if element.stroke_thickness > 0.0 { element.stroke_thickness } else { 0.05 };
            
            let is_path = matches!(element.kind, ElementKind::Path | ElementKind::Line2D { .. } | ElementKind::Circle2D { .. } | ElementKind::Rect2D { .. });
            if is_path || true {
                visible_groups.entry(key).or_insert_with(|| (mesh.clone(), material_slot, Vec::new())).2.push(matrix);
            }
        }

        // -- Build visible instanced commands --------------------------------
        self.world_instanced.clear();
        self.world_instance_matrices.clear();

        let mut sorted_group_keys: Vec<_> = visible_groups.keys().cloned().collect();
        // v15: Sort groups by Z-Index to respect Painter's Algorithm even across meshes.
        sorted_group_keys.sort_by_key(|k| k.3);

        let total_visible: usize = visible_groups.values().map(|(_, _, m)| m.len()).sum();
        if total_visible > 0 {
            let prev_bg = instance_buf.bind_group.clone();
            instance_buf.reserve(device, instance_layout, total_visible);
            if !Arc::ptr_eq(&prev_bg, &instance_buf.bind_group) {
                instance_callback(instance_buf.bind_group.clone(), shadow_instance_buf.bind_group.clone());
            }

            let mut offset = 0u32;
            for key in sorted_group_keys {
                let (mesh, material_slot, mats) = &visible_groups[&key];
                let double_sided = key.2;
                let count = mats.len() as u32;
                self.world_instance_matrices.extend_from_slice(mats);

                self.world_instanced.push(InstancedDrawCommand {
                    vertex_buffer: mesh.vertex_buffer.clone(),
                    index_buffer: mesh.index_buffer.clone(),
                    index_count: mesh.index_count,
                    vertex_count: mesh.vertex_count,
                    index_format: mesh.index_format,
                    first_instance: offset,
                    instance_count: count,
                    double_sided,
                    material_slot: *material_slot,
                    distance_sq: 0.0, // Sort by Z-Index replaces distance sorting for 2D
                });
                offset += count;
            }
            instance_buf.write_slice(queue, 0, &self.world_instance_matrices);
        } else {
            let prev_bg = instance_buf.bind_group.clone();
            instance_buf.reserve(device, instance_layout, 1);
            if !Arc::ptr_eq(&prev_bg, &instance_buf.bind_group) {
                instance_callback(instance_buf.bind_group.clone(), shadow_instance_buf.bind_group.clone());
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
            for ((_ptr, _mat_s, double_sided, _zi), (mesh, material_slot, mats)) in &shadow_groups {
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
