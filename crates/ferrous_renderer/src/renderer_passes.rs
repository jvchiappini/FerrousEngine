//! Renderer pass coordination and execution pipeline
//!
//! This module contains the rendering logic for the ferrous_renderer crate,
//! including the main render pipeline, scene synchronization, and GPU-driven features.

use std::collections::HashMap;
use std::sync::Arc;

use wgpu::{CommandEncoder, TextureView};

use ferrous_core::scene::{MaterialDescriptor, World};
use crate::scene::{SceneData, Frustum, GizmoDraw};
use crate::frame_builder::FrameBuilder;
use crate::gizmo_system::GizmoSystem;
use crate::graph::frame_packet::CameraPacket;
use crate::graph::RenderPass;
use crate::render_target::RenderTarget;
use crate::render_stats::RenderStats;
use crate::resources::InstanceBuffer;
use crate::{CameraSystem, RenderStyle, RendererMode, Viewport};
use ferrous_core::context::EngineContext as core_context;

// Conditional imports for GUI feature
#[cfg(feature = "gui")]
use ferrous_ui_render::GuiBatch;

// Conditional imports for gpu-driven feature
#[cfg(feature = "gpu-driven")]
use crate::passes::CullPass;

// -- RenderDest ---------------------------------------------------------------

enum RenderDest<'a> {
    Target,
    View(&'a TextureView),
}

// -- RendererPasses ----------------------------------------------------------

/// Container for render pass coordination and execution logic.
///
/// This struct holds references to the renderer's core components and provides
/// methods for executing the full rendering pipeline, managing scene state,
/// and handling GPU-driven features.
pub struct RendererPasses {
    // Core renderer components
    pub context: core_context,
    pub render_target: RenderTarget,
    pub viewport: Viewport,
    pub mode: RendererMode,
    
    // Camera system
    pub camera_system: CameraSystem,
    
    // Frame building and statistics
    pub frame_builder: FrameBuilder,
    pub render_stats: RenderStats,
    
    // Built-in passes (references)
    pub world_pass: crate::passes::WorldPass,
    pub post_process_pass: crate::passes::PostProcessPass,
    #[cfg(feature = "gui")]
    pub ui_pass: crate::passes::UiPass,
    
    // Render style passes
    pub cel_pass: Option<crate::passes::CelShadedPass>,
    pub outline_pass: Option<crate::passes::OutlinePass>,
    pub flat_pass: Option<crate::passes::FlatShadedPass>,
    
    // Prepass and SSAO
    pub prepass: crate::passes::PrePass,
    pub ssao_pass: crate::passes::SsaoPass,
    pub ssao_blur_pass: crate::passes::SsaoBlurPass,
    pub ssao_resources: crate::resources::SsaoResources,
    pub ssao_enabled: bool,
    
    // Render style
    pub render_style: RenderStyle,
    
    // Gizmo system
    pub gizmo_system: GizmoSystem,
    
    // Scene state
    pub world_material_descs: HashMap<u64, MaterialDescriptor>,
    pub instance_buf: InstanceBuffer,
    pub instance_layout: Arc<wgpu::BindGroupLayout>,
    pub shadow_instance_buf: InstanceBuffer,
    
    // Material registry reference
    pub material_registry: crate::materials::MaterialRegistry,
    
    // Current directional light
    pub current_dir_light: crate::resources::DirectionalLightUniform,
    
    // Extra passes
    pub extra_passes: Vec<Box<dyn RenderPass>>,
    
    // Conditional features
    #[cfg(feature = "gpu-driven")]
    pub gpu_culling_enabled: bool,
    #[cfg(feature = "gpu-driven")]
    pub cull_pass: Option<CullPass>,
}

impl RendererPasses {
    /// Execute the main rendering pipeline.
    ///
    /// This method orchestrates all render passes in the correct order:
    /// 1. Prepass (depth-normal)
    /// 2. SSAO passes (if enabled)
    /// 3. GPU culling (if enabled)
    /// 4. World pass
    /// 5. Render style passes
    /// 6. Gizmo pass
    /// 7. Post-process pass
    /// 8. UI pass
    /// 9. Extra passes
    #[cfg(feature = "gui")]
    pub fn do_render(
        &mut self,
        encoder: &mut CommandEncoder,
        dest: RenderDest<'_>,
        ui_batch: Option<GuiBatch>,
    ) {
        self.camera_system.sync_gpu(&self.context.queue);

        let camera_packet = CameraPacket {
            view_proj: self.camera_system.camera.build_view_projection_matrix(),
            eye: self.camera_system.camera.eye,
        };
        let (mut packet, stats) = self.frame_builder.build(self.viewport, camera_packet);
        // Propagate the (possibly-reallocated) instance buffer to style passes.
        self.sync_style_instance_buffer(self.instance_buf.bind_group.clone());
        self.render_stats = stats;

        if let Some(b) = ui_batch {
            packet.insert(b);
        }

        // ── Flat2D fast path ───────────────────────────────────────────────
        // In GUI-only mode skip the world pass, render-style passes, gizmos,
        // and post-process entirely.  The UI pass already holds a clear_color
        // set by `set_mode` so it will clear the surface before drawing.
        if self.mode == RendererMode::Flat2D {
            let target_view = match dest {
                RenderDest::Target => &self.render_target.color.view,
                RenderDest::View(v) => v,
            };
            self.ui_pass
                .prepare(&self.context.device, &self.context.queue, &packet);
            self.ui_pass.execute(
                &self.context.device,
                &self.context.queue,
                encoder,
                target_view,
                None,
                None,
                &packet,
            );
            for pass in &mut self.extra_passes {
                pass.prepare(&self.context.device, &self.context.queue, &packet);
                pass.execute(
                    &self.context.device,
                    &self.context.queue,
                    encoder,
                    target_view,
                    None,
                    None,
                    &packet,
                );
            }
            self.frame_builder.reclaim(packet);
            return;
        }

        let dummy_view = self
            .render_target
            .color
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());

        // -- 1. Depth-Normal Prepass (required by SSAO) ------------------------
        log::debug!("[WGPU-Render] Phase 1: Prepass");
        {
            let view = self.camera_system.view_matrix();
            let proj = self.camera_system.proj_matrix();
            self.prepass
                .update_camera(&self.context.queue, view, proj, self.camera_system.eye());
            self.prepass
                .prepare(&self.context.device, &self.context.queue, &packet);
            self.prepass.execute(
                &self.context.device,
                &self.context.queue,
                encoder,
                &dummy_view,
                None,
                None,
                &packet,
            );
        }

        // -- 2. SSAO passes (only when enabled) --------------------------------
        if self.ssao_enabled {
            log::debug!("[WGPU-Render] Phase 2: SSAO");
            let proj = self.camera_system.proj_matrix();
            let inv_proj = proj.inverse();
            let ssao_w = self.ssao_pass.ssao_texture.width;
            let ssao_h = self.ssao_pass.ssao_texture.height;
            self.ssao_resources
                .update_params(&self.context.queue, ssao_w, ssao_h, proj, inv_proj);

            self.ssao_pass.run(
                &self.context.device,
                encoder,
                &self.ssao_resources,
                &self.prepass.normal_depth,
            );

            self.ssao_blur_pass.run(
                &self.context.device,
                encoder,
                &self.ssao_pass.ssao_texture,
                &self.prepass.normal_depth,
            );

            let ssao_view = Arc::new(
                self.ssao_blur_pass
                    .blurred
                    .texture
                    .create_view(&wgpu::TextureViewDescriptor::default()),
            );
            // Linear filtering is critical: SSAO runs at half-res so we must
            // bilinearly upsample the blurred AO when the PBR pass reads it.
            // Nearest-filter would produce 2×2 block artifacts on every surface.
            let ssao_sampler = Arc::new(self.context.device.create_sampler(
                &wgpu::SamplerDescriptor {
                    label: Some("SSAO Result Sampler"),
                    mag_filter: wgpu::FilterMode::Linear,
                    min_filter: wgpu::FilterMode::Linear,
                    address_mode_u: wgpu::AddressMode::ClampToEdge,
                    address_mode_v: wgpu::AddressMode::ClampToEdge,
                    ..Default::default()
                },
            ));
            self.world_pass
                .update_ssao(&self.context.device, ssao_view, ssao_sampler);
        }

        // -- 3. Phase 11: GPU cull compute dispatch (if enabled) --------------
        #[cfg(feature = "gpu-driven")]
        {
            if self.gpu_culling_enabled {
                log::debug!("[WGPU-Render] Phase 3: GPU Culling");
                if let Some(cp) = &self.cull_pass {
                    cp.dispatch(encoder);
                    cp.copy_counters_to_staging(encoder);
                }
            }
        }

        // -- 4. World Pass (Opaque + Blended) ----------------------------------
        log::debug!("[WGPU-Render] Phase 4: World Pass");
        self.world_pass
            .prepare(&self.context.device, &self.context.queue, &packet);
        self.world_pass.execute(
            &self.context.device,
            &self.context.queue,
            encoder,
            &dummy_view,
            None,
            Some(&self.render_target.depth.view),
            &packet,
        );

        // -- 3b. Render Style Passes ------------------------------------------
        // Inject per-frame data for whichever style is active, then run its
        // pass(es).  All style passes render into the same HDR texture
        // (LoadOp::Load) so they composite on top of the world geometry.
        log::debug!("[WGPU-Render] Phase 5: Style Passes");
        
        let (scene_view, scene_rt) = if let Some(m_view) = &self.world_pass.hdr_texture.multisampled_view {
            (m_view, Some(&self.world_pass.hdr_texture.view))
        } else {
            (&self.world_pass.hdr_texture.view, None)
        };
        
        match &self.render_style {
            RenderStyle::CelShaded {
                toon_levels,
                outline_width,
            } => {
                let toon_levels = *toon_levels;
                let outline_width = *outline_width;
                packet.insert(crate::passes::CelFrameData {
                    light: self.current_dir_light,
                    toon_levels,
                    outline_width,
                });
                if outline_width > 0.0 {
                    packet.insert(crate::passes::OutlineFrameData {
                        light: self.current_dir_light,
                        toon_levels,
                        outline_width,
                        color: [0.0, 0.0, 0.0, 1.0],
                    });
                }
                if let Some(p) = &mut self.cel_pass {
                    p.prepare(&self.context.device, &self.context.queue, &packet);
                    p.execute(
                        &self.context.device,
                        &self.context.queue,
                        encoder,
                        scene_view,
                        scene_rt,
                        Some(&self.render_target.depth.view),
                        &packet,
                    );
                }
                if outline_width > 0.0 {
                    if let Some(p) = &mut self.outline_pass {
                        p.prepare(&self.context.device, &self.context.queue, &packet);
                        p.execute(
                            &self.context.device,
                            &self.context.queue,
                            encoder,
                            scene_view,
                            scene_rt,
                            Some(&self.render_target.depth.view),
                            &packet,
                        );
                    }
                }
            }
            RenderStyle::FlatShaded => {
                packet.insert(crate::passes::FlatFrameData {
                    light: self.current_dir_light,
                });
                if let Some(p) = &mut self.flat_pass {
                    p.prepare(&self.context.device, &self.context.queue, &packet);
                    p.execute(
                        &self.context.device,
                        &self.context.queue,
                        encoder,
                        scene_view,
                        scene_rt,
                        Some(&self.render_target.depth.view),
                        &packet,
                    );
                }
            }
            RenderStyle::Pbr => {}
        }

        // -- 4. Gizmo Pass -----------------------------------------------------
        log::debug!("[WGPU-Render] Phase 6: Gizmos");
        self.gizmo_system.execute(
            &self.context.device,
            encoder,
            scene_view,
            scene_rt,
            &self.render_target.depth.view,
            &self.camera_system.gpu.bind_group,
        );

        // -- 5. Post-Process (Tone Mapping) ------------------------------------
        log::debug!("[WGPU-Render] Phase 7: Post-Process");
        let target_view = match dest {
            RenderDest::Target => &self.render_target.color.view,
            RenderDest::View(v) => v,
        };

        self.post_process_pass.render(
            &self.context.device,
            encoder,
            &self.world_pass.hdr_texture,
            target_view,
            &self.camera_system.gpu.bind_group,
        );

        // -- 6. UI Pass --------------------------------------------------------
        log::debug!("[WGPU-Render] Phase 8: UI Pass");
        self.ui_pass
            .prepare(&self.context.device, &self.context.queue, &packet);
        self.ui_pass.execute(
            &self.context.device,
            &self.context.queue,
            encoder,
            target_view,
            None,
            None,
            &packet,
        );

        // -- 7. Extra Passes ---------------------------------------------------
        for pass in &mut self.extra_passes {
            pass.prepare(&self.context.device, &self.context.queue, &packet);
            pass.execute(
                &self.context.device,
                &self.context.queue,
                encoder,
                target_view,
                None,
                None,
                &packet,
            );
        }

        self.frame_builder.reclaim(packet);
    }

    /// Renders to the render target's color texture.
    #[cfg(feature = "gui")]
    pub fn render_to_target(
        &mut self,
        encoder: &mut CommandEncoder,
        ui_batch: Option<GuiBatch>,
    ) {
        self.do_render(encoder, RenderDest::Target, ui_batch);
    }

    /// Renders directly into an external `TextureView` (e.g. swapchain frame).
    #[cfg(feature = "gui")]
    pub fn render_to_view(
        &mut self,
        encoder: &mut CommandEncoder,
        view: &TextureView,
        ui_batch: Option<GuiBatch>,
    ) {
        self.do_render(encoder, RenderDest::View(view), ui_batch);
    }

    // -- Scene synchronization ------------------------------------------------

    /// Push a fully-assembled [`SceneData`] to the renderer for this frame.
    ///
    /// This is the **preferred** entry point going forward.  The application
    /// layer (e.g. `ferrous_app`) queries the ECS, builds a [`SceneData`],
    /// and calls this method — keeping the renderer free of ECS knowledge.
    ///
    /// `sync_world` is kept for backward compatibility; it converts ECS state
    /// into an equivalent `SceneData` and delegates here.
    pub fn set_scene(&mut self, scene: &SceneData) {
        // 1. Apply camera if provided
        if let Some(cam) = &scene.camera {
            self.camera_system.camera.eye = cam.eye;
            self.camera_system.camera.target = cam.target;
            self.camera_system.camera.set_fov_degrees(cam.fov_y.to_degrees());
            self.camera_system.camera.set_near_far(cam.z_near, cam.z_far);
        }

        // 2. Apply directional light if provided
        if let Some(light) = &scene.directional_light {
            self.set_directional_light(
                light.direction.to_array(),
                light.color.to_array(),
                light.intensity,
            );
        }

        // 3. Mark scene dirty so frame_builder rebuilds next frame.
        //    Instance uploads are still handled by sync_world / build_world_commands
        //    until the full ECS→SceneData migration is complete (Phase 3 step 2).
        if !scene.instances.is_empty() {
            self.frame_builder.scene_dirty = true;
        }
    }

    /// Synchronize the ECS world with the renderer's internal state.
    ///
    /// This method:
    /// 1. Syncs directional light from ECS → GPU uniform
    /// 2. Syncs camera from ECS → renderer camera
    /// 3. Syncs materials from ECS → MaterialDescriptors
    /// 4. Builds frustum and populates frame builder
    /// 5. Handles GPU-driven cull data upload (if enabled)
    /// 6. Updates point lights from ECS
    pub fn sync_world(&mut self, world: &World) {
        // 0. Sync DirectionalLight ECS component → GPU uniform (if present)
        {
            use ferrous_core::scene::DirectionalLight;
            let lights: Vec<DirectionalLight> = world
                .ecs
                .query::<DirectionalLight>()
                .map(|(_, l)| *l)
                .collect();
            if let Some(light) = lights.first() {
                self.set_directional_light(
                    [light.direction.x, light.direction.y, light.direction.z],
                    [light.color.r, light.color.g, light.color.b],
                    light.intensity,
                );
            }
        }

        // 0b. Sync Camera3D ECS component → renderer camera (if present)
        {
            use ferrous_core::scene::Camera3D;
            let cameras: Vec<Camera3D> = world.ecs.query::<Camera3D>().map(|(_, c)| *c).collect();
            if let Some(cam3d) = cameras.first() {
                self.camera_system.camera.eye = cam3d.eye;
                self.camera_system.camera.target = cam3d.target;
                self.camera_system.camera.set_fov_degrees(cam3d.fov_deg);
                self.camera_system.camera.set_near_far(cam3d.near, cam3d.far);
            }
        }

        // 0c. Sync Material ECS components → MaterialDescriptors
        {
            use ferrous_core::scene::Material;
            // Collect (ecs_entity_index, MaterialDescriptor) for entities
            // that have a Material component attached.
            let mat_only: Vec<(u32, ferrous_core::scene::MaterialDescriptor)> = world
                .ecs
                .query::<Material>()
                .map(|(e, m)| (e.index, m.to_descriptor()))
                .collect();
            for (ecs_idx, desc) in mat_only {
                let ecs_id = ecs_idx as u64;
                let needs_update = self
                    .world_material_descs
                    .get(&ecs_id)
                    .map(|prev| *prev != desc)
                    .unwrap_or(true);
                if needs_update {
                    // Match to an Element by its id (bridge stores ECS index as id)
                    for element in world.iter() {
                        if element.id == ecs_id {
                            self.material_registry.update_params(
                                &self.context.queue,
                                element.material.handle,
                                &desc,
                            );
                            self.world_material_descs.insert(ecs_id, desc.clone());
                            break;
                        }
                    }
                }
            }
        }

        // 1. Build frustum from current camera
        let camera_packet = crate::graph::frame_packet::CameraPacket {
            view_proj: self.camera_system.camera.build_view_projection_matrix(),
            eye: self.camera_system.camera.eye,
        };
        let frustum = Frustum::from_view_proj(&camera_packet.view_proj);

        // 2. ECS query -> populate frame_builder world instanced caches
        {
            let world_pass_ref = &mut self.world_pass;
            let prepass_ref = &mut self.prepass;
            self.frame_builder.build_world_commands(
                world,
                &self.context.device,
                &frustum,
                self.camera_system.camera.eye,
                &mut self.instance_buf,
                &self.instance_layout,
                &mut self.shadow_instance_buf,
                &mut |bg, shadow_bg| {
                    world_pass_ref.set_instance_buffer(bg.clone());
                    world_pass_ref.set_shadow_instance_buffer(shadow_bg);
                    prepass_ref.set_instance_buffer(bg);
                },
                &self.context.queue,
            );
        }
        self.frame_builder.scene_dirty = true;

        // -- Phase 11: GPU-driven cull data upload ---------------------------
        #[cfg(feature = "gpu-driven")]
        {
            if self.gpu_culling_enabled {
                // Ensure the CullPass exists.
                if self.cull_pass.is_none() {
                    self.cull_pass =
                        Some(CullPass::new(&self.context.device, &self.pipeline_layouts));
                }

                // `build_world_commands` populated world_instanced and
                // world_instance_matrices above.  We use those to build cull data.
                let instanced = &self.frame_builder.world_instanced;
                let matrices = &self.frame_builder.world_instance_matrices;

                if let Some(cp) = &mut self.cull_pass {
                    use crate::resources::draw_indirect::{
                        GpuDrawIndexedIndirect, InstanceCullData,
                    };

                    let mut cull_data: Vec<InstanceCullData> = Vec::with_capacity(matrices.len());
                    let mut templates: Vec<GpuDrawIndexedIndirect> =
                        Vec::with_capacity(instanced.len());

                    for (cmd_idx, cmd) in instanced.iter().enumerate() {
                        // Emit one template per batch (index_count, first_instance).
                        // instance_count = 0 — the cull shader fills it.
                        templates.push(GpuDrawIndexedIndirect {
                            index_count: cmd.index_count,
                            instance_count: 0,
                            first_index: 0,
                            base_vertex: 0,
                            first_instance: cmd.first_instance,
                        });

                        // Emit one InstanceCullData per entity within this batch.
                        let base = cmd.first_instance as usize;
                        for inst_idx in 0..cmd.instance_count as usize {
                            let model = matrices
                                .get(base + inst_idx)
                                .copied()
                                .unwrap_or(glam::Mat4::IDENTITY);
                            // Use a conservative world-space AABB that ensures nothing
                            // gets incorrectly culled for the Phase 11 baseline.
                            // Future passes (Phase 12) will extract real AABBs from mesh assets.
                            let aabb_half = glam::Vec3::splat(100.0);
                            let aabb_center = glam::Vec3::ZERO;
                            cull_data.push(InstanceCullData::new(
                                model,
                                aabb_center,
                                aabb_half,
                                cmd_idx as u32,
                            ));
                        }
                    }

                    if !cull_data.is_empty() {
                        cp.upload_instances(
                            &self.context.device,
                            &self.context.queue,
                            &cull_data,
                            &templates,
                        );
                        cp.reset_counters(&self.context.queue);
                        cp.update_params(&self.context.queue, &frustum);

                        // Arm WorldPass with the GPU-driven indirect buffer and
                        // compacted output instance bind group.
                        let indirect = cp.indirect_buf.buffer.clone();
                        let out_bg = cp.out_instance_bg.clone();
                        self.world_pass.set_indirect_buffer(indirect, out_bg);
                    } else {
                        self.world_pass.clear_indirect_buffer();
                    }
                }
            } else {
                // CPU-driven path — ensure WorldPass does not use stale indirect buf.
                self.world_pass.clear_indirect_buffer();
            }
        }
        for element in world.iter() {
            let id = element.id;
            let desc = &element.material.descriptor;
            let needs_update = self
                .world_material_descs
                .get(&id)
                .map(|prev| prev != desc)
                .unwrap_or(true);
            if needs_update {
                self.material_registry.update_params(
                    &self.context.queue,
                    element.material.handle,
                    desc,
                );
                self.world_material_descs.insert(id, desc.clone());
            }
        }

        // Prune entries for despawned entities
        let live_ids: std::collections::HashSet<u64> = world.iter().map(|e| e.id).collect();
        self.world_material_descs
            .retain(|id, _| live_ids.contains(id));

        // 4. Sync material table to style passes
        self.world_pass.set_material_table(
            &self.material_registry.bind_group_table(),
            &self.material_registry,
        );
        self.sync_style_material_table();

        // 5. Collect point lights from World entities
        let mut point_light_uniforms: Vec<crate::resources::PointLightUniform> = Vec::new();
        for element in world.iter() {
            if let Some(pl) = &element.point_light {
                let pos = element.transform.position;
                point_light_uniforms.push(crate::resources::PointLightUniform::new(
                    [pos.x, pos.y, pos.z],
                    pl.radius,
                    pl.color,
                    pl.intensity,
                ));
            }
        }
        self.world_pass.update_point_lights(
            &self.context.device,
            &self.context.queue,
            &point_light_uniforms,
        );
    }

    // -- Gizmo management -----------------------------------------------------

    /// Queue a gizmo for rendering this frame.
    ///
    /// Typically called by the `ferrous_app` runner which drains
    /// `AppContext::gizmos` after `FerrousApp::draw_3d` returns – app code
    /// should push to `ctx.gizmos` rather than calling this directly.
    ///
    /// The gizmo list is automatically cleared after
    /// [`GizmoSystem::execute`] runs, so there is no need to manage lifetime
    /// manually.
    pub fn queue_gizmo(&mut self, gizmo: GizmoDraw) {
        self.gizmo_system.queue(gizmo);
        // mark scene dirty so that the world pass will rebuild the packet; the
        // gizmos are drawn separately but the packet cache logic should reset
        // when an unrelated draw request arrives.
        self.frame_builder.scene_dirty = true;
    }

    // -- GPU-driven culling ---------------------------------------------------

    /// Enables or disables GPU-driven frustum culling.
    ///
    /// When `true`, `sync_world` uploads per-instance cull data to the GPU and
    /// `do_render` dispatches the cull compute shader before `WorldPass`.
    /// `WorldPass` will then use `draw_indexed_indirect` instead of `draw_indexed`.
    ///
    /// Disabling reverts to the CPU `draw_indexed` path using `instance_buf`.
    #[cfg(feature = "gpu-driven")]
    pub fn enable_gpu_culling(&mut self, enabled: bool) {
        self.gpu_culling_enabled = enabled;
        if enabled && self.cull_pass.is_none() {
            self.cull_pass = Some(CullPass::new(&self.context.device, &self.pipeline_layouts));
        }
        if !enabled {
            self.world_pass.clear_indirect_buffer();
        }
    }

    /// Returns visible-instance counts per batch from the most recent GPU cull pass.
    ///
    /// Performs a **synchronous** device poll + staging buffer readback.
    /// Call this *after* rendering a frame to obtain per-batch culling statistics.
    ///
    /// Returns an empty `Vec` if GPU culling is disabled or no batches were drawn.
    #[cfg(feature = "gpu-driven")]
    pub fn cull_visible_counts(&self) -> Vec<u32> {
        if let Some(cp) = &self.cull_pass {
            cp.sync_patch_indirect(&self.context.device, &self.context.queue)
        } else {
            vec![]
        }
    }

    // -- Private helpers ------------------------------------------------------

    /// Propagates the current material bind-group table to all active style passes.
    fn sync_style_material_table(&mut self) {
        let table = self.material_registry.bind_group_table();
        if let Some(p) = &mut self.cel_pass {
            p.set_material_table(&table);
        }
        if let Some(p) = &mut self.outline_pass {
            p.set_material_table(&table);
        }
        if let Some(p) = &mut self.flat_pass {
            p.set_material_table(&table);
        }
    }

    /// Propagates a new instance-buffer bind group to all active style passes.
    fn sync_style_instance_buffer(&mut self, bg: Arc<wgpu::BindGroup>) {
        if let Some(p) = &mut self.cel_pass {
            p.set_instance_buffer(bg.clone());
        }
        if let Some(p) = &mut self.outline_pass {
            p.set_instance_buffer(bg.clone());
        }
        if let Some(p) = &mut self.flat_pass {
            p.set_instance_buffer(bg);
        }
    }

    /// Helper method for setting directional light (used by both set_scene and sync_world).
    fn set_directional_light(&mut self, direction: [f32; 3], color: [f32; 3], intensity: f32) {
        use crate::resources::DirectionalLightUniform;
        self.current_dir_light = DirectionalLightUniform::new(direction, color, intensity);
        self.world_pass
            .update_light(&self.context.queue, self.current_dir_light);
    }
}

// -- Pass management ---------------------------------------------------------

/// Container for pass management methods.
pub struct PassManager {
    pub extra_passes: Vec<Box<dyn RenderPass>>,
    pub context: core_context,
    pub format: wgpu::TextureFormat,
    pub sample_count: u32,
}

impl PassManager {
    /// Appends a custom pass after the built-in ones.
    /// `on_attach` is called immediately with the current surface format.
    pub fn add_pass<P: RenderPass>(&mut self, mut pass: P) {
        pass.on_attach(
            &self.context.device,
            &self.context.queue,
            self.format,
            self.sample_count,
        );
        self.extra_passes.push(Box::new(pass));
    }

    /// Removes all user-supplied passes.  Built-in passes are NOT removed.
    pub fn clear_extra_passes(&mut self) {
        self.extra_passes.clear();
    }
}