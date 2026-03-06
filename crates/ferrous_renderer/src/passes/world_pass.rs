use log::warn;
/// 3-D / 2-D opaque geometry pass.
///
/// Clears color + depth, binds the camera and renders geometry.
///
/// ## Instanced path
///
/// World entities that share the same mesh are batched into
/// `FramePacket::instanced_objects`.  A separate `InstancingPipeline`
/// (group 1 = storage buffer) renders all instances of one mesh in a single
/// `draw_indexed` call with `instance_count > 1`.
///
/// Supports both 3-D (perspective) and 2-D (orthographic) cameras — the
/// distinction is entirely in the camera's view-projection matrix.
use std::sync::Arc;

use wgpu::{
    Color, CommandEncoder, Device, LoadOp, Operations, Queue, RenderPassColorAttachment,
    RenderPassDepthStencilAttachment, RenderPassDescriptor, StoreOp, TextureView,
};

use crate::graph::{FramePacket, RenderPass};
use crate::pipeline::{InstancingPipeline, PbrPipeline, PipelineLayouts, ShadowPipeline};
use crate::render_target::HdrTexture;
use crate::resources::{DirectionalLightUniform, Environment, PointLightUniform, ShadowResources};
use crate::InstancedDrawCommand;
use ferrous_core::scene::MaterialHandle;

pub struct WorldPass {
    /// PBR pipeline used for opaque geometry.
    pbr_pipeline: PbrPipeline,
    /// Double-sided variant of the PBR pipeline (culling disabled).
    pbr_pipeline_double: PbrPipeline,
    /// Opaque blending pipeline (depth writes disabled).
    pbr_pipeline_blend: PbrPipeline,
    /// Double-sided blend pipeline.
    pbr_pipeline_blend_double: PbrPipeline,
    /// Pipeline for instanced draws (group 1 = storage buffer).
    instancing_pipeline: InstancingPipeline,
    /// Instancing pipeline variant with culling disabled.
    instancing_pipeline_double: InstancingPipeline,
    /// Instancing pipeline for blended geometry.
    instancing_pipeline_blend: InstancingPipeline,
    /// Double-sided blended instancing pipeline.
    instancing_pipeline_blend_double: InstancingPipeline,
    /// Pipeline used to render the depth-only shadow map.
    shadow_pipeline: ShadowPipeline,
    /// Shadow pipeline variant which supports instanced vertex data.
    shadow_pipeline_instanced: ShadowPipeline,
    /// Shadow map texture + sampler.  Stored here so the world pass can write
    /// to it before the main geometry pass.
    pub shadow_resources: ShadowResources,
    camera_bind_group: Arc<wgpu::BindGroup>,
    /// Bind group for the instance storage buffer.
    instance_bind_group: Option<Arc<wgpu::BindGroup>>,
    /// Separate bind group pointing at the shadow-caster instance buffer.
    /// This buffer contains ALL world-object matrices (not camera-culled),
    /// so that objects behind the camera can still cast shadows.
    shadow_instance_bind_group: Option<Arc<wgpu::BindGroup>>,
    /// Sky / clear color.
    pub clear_color: Color,
    /// Table of material bind groups, indexed by slot.  Populated by the
    /// renderer when materials are created or updated.
    material_bind_groups: Vec<Arc<wgpu::BindGroup>>,
    /// optional copy of the material registry used for flag queries.
    material_registry: Option<crate::materials::MaterialRegistry>,
    /// Light data and bind group for PBR shading.
    /// encapsulates light data plus IBL resources
    environment: Environment,
    /// Kept for environment bind-group reconstruction when point-light buffer grows.
    lights_layout: Arc<wgpu::BindGroupLayout>,
    /// Optional skybox pass drawn before geometry.
    skybox_pass: Option<crate::passes::SkyboxPass>,
    /// HDR off-screen render target. The world pass renders into this instead
    /// of the swapchain surface so values > 1.0 can be preserved.
    pub hdr_texture: HdrTexture,
    /// Dedicated bind group for the shadow pass (group 1).
    ///
    /// Contains only the directional light uniform buffer — no shadow map
    /// texture.  Using the full `environment.bind_group` in the shadow pass
    /// would cause a wgpu validation error because the shadow map texture
    /// would be bound as both DEPTH_STENCIL_WRITE (depth attachment) and
    /// RESOURCE (sampled texture) within the same render-pass scope.
    shadow_light_bind_group: Arc<wgpu::BindGroup>,

    #[cfg(feature = "bindless")]
    /// bindless descriptor set (one for all materials)
    bindless_bind_group: Option<Arc<wgpu::BindGroup>>,

    // -- Phase 11: GPU-Driven Rendering (indirect draw) ----------------------
    /// When `Some`, the render pass will use `draw_indexed_indirect` instead
    /// of `draw_indexed`, consuming the pre-filled indirect draw buffer.
    /// Set via `set_indirect_buffer`; cleared via `clear_indirect_buffer`.
        #[cfg(feature = "gpu-driven")]
        indirect_buf: Option<Arc<wgpu::Buffer>>,
        #[cfg(feature = "gpu-driven")]
        culled_instance_bind_group: Option<Arc<wgpu::BindGroup>>,
}

impl WorldPass {
    pub fn new(
        pbr_pipeline: PbrPipeline,
        pbr_pipeline_double: PbrPipeline,
        pbr_pipeline_blend: PbrPipeline,
        pbr_pipeline_blend_double: PbrPipeline,
        instancing_pipeline: InstancingPipeline,
        instancing_pipeline_double: InstancingPipeline,
        instancing_pipeline_blend: InstancingPipeline,
        instancing_pipeline_blend_double: InstancingPipeline,
        camera_bind_group: Arc<wgpu::BindGroup>,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        layouts: &PipelineLayouts,
        width: u32,
        height: u32,
        hdri_path: Option<&std::path::Path>,
    ) -> Self {
        // environment bundle includes directional light and IBL textures. if
        // caller supplied an HDRI path we perform the expensive compute
        // precomputation once; otherwise fall back to the dummy grey maps.
        let mut environment = if let Some(path) = hdri_path {
            match Environment::from_hdri(device, queue, &layouts.lights, path) {
                Ok(env) => env,
                Err(e) => {
                    warn!(
                        "Failed to load HDRI {:?}, falling back to dummy: {}",
                        path, e
                    );
                    Environment::new_dummy(device, queue, &layouts.lights)
                }
            }
        } else {
            Environment::new_dummy(device, queue, &layouts.lights)
        };
        let hdr_texture = HdrTexture::new(device, width, height);

        // create two shadow pipelines: one for regular objects and one for
        // instanced geometry.  They differ only in the first bind-group
        // layout (model vs instance).
        let shadow_pipeline = crate::pipeline::ShadowPipeline::new(device, layouts.clone(), false);
        let shadow_pipeline_instanced =
            crate::pipeline::ShadowPipeline::new(device, layouts.clone(), true);
        let shadow_resources = crate::resources::ShadowResources::new(device);
        environment.update_shadow(device, &layouts.lights, &shadow_resources);

        // create skybox pass now that we have a valid environment bind group
        let skybox_pass = Some(crate::passes::SkyboxPass::new(
            device,
            &layouts,
            camera_bind_group.clone(),
            environment.bind_group.clone(),
            HdrTexture::FORMAT,
            1, // HDR texture uses single sample count
        ));

        // Dedicated bind group for the shadow pass — only the directional
        // light uniform, matched to `layouts.shadow_lights`.
        let shadow_light_bind_group =
            Arc::new(device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("Shadow Light Bind Group"),
                layout: &layouts.shadow_lights,
                entries: &[wgpu::BindGroupEntry {
                    binding: 0,
                    resource: environment.light_buffer.as_entire_binding(),
                }],
            }));

        Self {
            pbr_pipeline,
            pbr_pipeline_double,
            pbr_pipeline_blend,
            pbr_pipeline_blend_double,
            instancing_pipeline,
            instancing_pipeline_double,
            instancing_pipeline_blend,
            instancing_pipeline_blend_double,
            camera_bind_group,
            instance_bind_group: None,
            shadow_instance_bind_group: None,
            clear_color: Color {
                r: 0.1,
                g: 0.2,
                b: 0.3,
                a: 1.0,
            },
            material_bind_groups: Vec::new(),
            material_registry: None,
            environment,
            lights_layout: Arc::clone(&layouts.lights),
            skybox_pass,
            hdr_texture,
            shadow_pipeline,
            shadow_pipeline_instanced,
            shadow_resources,
            shadow_light_bind_group,
            #[cfg(feature = "bindless")]
            bindless_bind_group: None,
            #[cfg(feature = "gpu-driven")]
            indirect_buf: None,
            #[cfg(feature = "gpu-driven")]
            culled_instance_bind_group: None,
        }
    }

    /// Called by `Renderer` whenever the `InstanceBuffer` is created or reallocated.
    pub fn set_instance_buffer(&mut self, bind_group: Arc<wgpu::BindGroup>) {
        self.instance_bind_group = Some(bind_group);
    }

    /// Called by `Renderer` whenever the shadow-caster `InstanceBuffer` is reallocated.
    pub fn set_shadow_instance_buffer(&mut self, bind_group: Arc<wgpu::BindGroup>) {
        self.shadow_instance_bind_group = Some(bind_group);
    }

    // -- Phase 11: GPU-driven helpers ----------------------------------------

    /// Arm the GPU-driven render path.
    ///
    /// After this is set, `execute` will call `draw_indexed_indirect` using
    /// `indirect_buf` for each mesh batch, and will bind `culled_bg` as group 1
    /// (the compacted output instance buffer written by the cull shader).
    #[cfg(feature = "gpu-driven")]
    pub fn set_indirect_buffer(
        &mut self,
        indirect_buf: Arc<wgpu::Buffer>,
        culled_bg: Arc<wgpu::BindGroup>,
    ) {
        self.indirect_buf = Some(indirect_buf);
        self.culled_instance_bind_group = Some(culled_bg);
    }

    /// Disarm the GPU-driven path, reverting to the CPU `draw_indexed` path.
    #[cfg(feature = "gpu-driven")]
    pub fn clear_indirect_buffer(&mut self) {
        self.indirect_buf = None;
        self.culled_instance_bind_group = None;
    }

    /// Update the material table used during draw.  The passed slice is
    /// cloned into the pass; the renderer should call this whenever it
    /// reallocates or adds new materials.
    /// Update the material table and remember the source registry so we can
    /// query flags during draw.
    pub fn set_material_table(
        &mut self,
        table: &[Arc<wgpu::BindGroup>],
        registry: &crate::materials::MaterialRegistry,
    ) {
        self.material_bind_groups.clear();
        self.material_bind_groups.extend_from_slice(table);
        // keep a clone of the registry for flag lookups; cloning is cheap
        // because most data inside is stored in Arcs.
        self.material_registry = Some(registry.clone());
        #[cfg(feature = "bindless")]
        {
            if table.len() == 1 {
                self.bindless_bind_group = Some(table[0].clone());
            }
        }
    }

    /// Push new light data into the GPU buffer.  The caller is responsible
    /// for providing a queue reference; the uniform struct will also be
    /// cached locally so that it may be inspected later if required.
    pub fn update_light(&mut self, queue: &wgpu::Queue, uniform: DirectionalLightUniform) {
        // compute light view-projection matrix based on the given direction
        // (simple orthographic camera centred on the origin and looking along
        // the light direction).
        let mut u = uniform;
        {
            let dir = glam::Vec3::new(u.direction[0], u.direction[1], u.direction[2]);
            // pick an arbitrary up vector that is not parallel to the light dir
            let up = if dir.abs().dot(glam::Vec3::Y) > 0.9 {
                glam::Vec3::Z
            } else {
                glam::Vec3::Y
            };
            // Pull the shadow camera far enough back that the whole scene is
            // within the depth range, even when the camera zooms out.
            let eye = -dir * 50.0;
            let view = glam::Mat4::look_at_rh(eye, glam::Vec3::ZERO, up);
            // Large ortho volume covers the visible playfield.  near=1 avoids
            // precision issues right at the shadow camera; far=200 ensures
            // distant geometry still receives/casts shadows.
            // Tighter ortho frustum → more shadow-map texels per world-unit
            // → sharper (but still PCF-filtered) shadow edges.
            let proj = glam::Mat4::orthographic_rh(-20.0, 20.0, -20.0, 20.0, 1.0, 150.0);
            let view_proj = proj * view;
            u.light_view_proj = view_proj.to_cols_array_2d();
        }
        // forward update to environment helper
        self.environment.update_light(queue, u);
    }

    /// Upload the list of point lights for this frame.
    ///
    /// If the count exceeds the current storage-buffer capacity the buffer is
    /// reallocated automatically and the bind group is rebuilt.
    pub fn update_point_lights(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        lights: &[PointLightUniform],
    ) {
        self.environment
            .update_point_lights(device, queue, &self.lights_layout, lights);
        // if the bind group changed, propagate to the skybox pass
        if let Some(sky) = &mut self.skybox_pass {
            sky.set_env_bind_group(self.environment.bind_group.clone());
        }
    }

    /// Plug the blurred SSAO texture into the environment bind group so the
    /// PBR shader samples it this frame.  Also propagates the updated bind
    /// group to the skybox pass (which uses the same group).
    pub fn update_ssao(
        &mut self,
        device: &wgpu::Device,
        ssao_view: Arc<wgpu::TextureView>,
        ssao_sampler: Arc<wgpu::Sampler>,
    ) {
        self.environment
            .update_ssao(device, &self.lights_layout, ssao_view, ssao_sampler);
        if let Some(sky) = &mut self.skybox_pass {
            sky.set_env_bind_group(self.environment.bind_group.clone());
        }
    }
}

impl RenderPass for WorldPass {
    fn name(&self) -> &str {
        "World Pass"
    }

    fn on_resize(&mut self, device: &Device, _queue: &Queue, width: u32, height: u32) {
        self.hdr_texture.resize(device, width, height);
    }

    fn prepare(&mut self, _device: &Device, _queue: &Queue, _packet: &FramePacket) {}

    fn execute(
        &mut self,
        _device: &Device,
        _queue: &Queue,
        encoder: &mut CommandEncoder,
        _color_view: &TextureView,
        resolve_target: Option<&TextureView>,
        depth_view: Option<&TextureView>,
        packet: &FramePacket,
    ) {
        // draw skybox before everything else (ignores depth writes)
        // Must render into the HDR texture, not the swapchain surface, so that
        // the skybox format matches the pipeline (Rgba16Float).
        if let Some(sky) = &mut self.skybox_pass {
            sky.execute(
                _device,
                _queue,
                encoder,
                &self.hdr_texture.view,
                resolve_target,
                depth_view,
                packet,
            );
        }
        // ── Shadow map pass ─────────────────────────────────────────────
        // Render the scene from the light's point of view into a depth-only
        // texture before doing the main world pass.  This cannot occur while
        // a render pass is active on the encoder, so we perform it first.
        {
            let mut spass = encoder.begin_render_pass(&RenderPassDescriptor {
                label: Some("Shadow Pass"),
                color_attachments: &[],
                depth_stencil_attachment: Some(RenderPassDepthStencilAttachment {
                    view: &self.shadow_resources.view,
                    depth_ops: Some(Operations {
                        load: LoadOp::Clear(1.0),
                        store: StoreOp::Store,
                    }),
                    stencil_ops: None,
                }),
                occlusion_query_set: None,
                timestamp_writes: None,
            });

            // draw instanced objects first using the instanced shadow pipeline.
            // Uses shadow_instanced_objects (all world objects, not camera-culled)
            // and the dedicated shadow instance buffer.
            if let Some(shadow_inst_bg) = &self.shadow_instance_bind_group {
                if !packet.shadow_instanced_objects.is_empty() {
                    spass.set_pipeline(&self.shadow_pipeline_instanced.inner);
                    // group0 = shadow instance storage, group1 = dir-light only (no shadow map texture)
                    spass.set_bind_group(0, shadow_inst_bg.as_ref(), &[]);
                    spass.set_bind_group(1, &*self.shadow_light_bind_group, &[]);
                    for cmd in &packet.shadow_instanced_objects {
                        spass.set_vertex_buffer(0, cmd.vertex_buffer.slice(..));
                        spass.set_index_buffer(cmd.index_buffer.slice(..), cmd.index_format);
                        spass.draw_indexed(
                            0..cmd.index_count,
                            0,
                            cmd.first_instance..cmd.first_instance + cmd.instance_count,
                        );
                    }
                }
            }
        }

        // Always render into the HDR texture, not the swapchain surface.
        // The post-process pass will read this and write to the final surface.
        // If the skybox already cleared and wrote to the HDR texture, use Load
        // so we preserve the skybox background.
        let hdr_load_op = if self.skybox_pass.is_some() {
            LoadOp::Load
        } else {
            LoadOp::Clear(self.clear_color)
        };
        let mut rpass = encoder.begin_render_pass(&RenderPassDescriptor {
            label: Some(self.name()),
            color_attachments: &[Some(RenderPassColorAttachment {
                view: &self.hdr_texture.view,
                resolve_target: None,
                ops: Operations {
                    load: hdr_load_op,
                    store: StoreOp::Store,
                },
            })],
            depth_stencil_attachment: depth_view.map(|v| RenderPassDepthStencilAttachment {
                view: v,
                depth_ops: Some(Operations {
                    load: LoadOp::Clear(1.0),
                    store: StoreOp::Store,
                }),
                stencil_ops: None,
            }),
            occlusion_query_set: None,
            timestamp_writes: None,
        });

        if let Some(vp) = &packet.viewport {
            rpass.set_viewport(
                vp.x as f32,
                vp.y as f32,
                vp.width as f32,
                vp.height as f32,
                0.0,
                1.0,
            );
            rpass.set_scissor_rect(vp.x, vp.y, vp.width, vp.height);
        }

        // helper: fetch material render flags, returning an owned AlphaMode so
        // that callers can sort or compare without borrowing the registry.
        let get_flags =
            |slot: usize, fallback_double: bool| -> (ferrous_core::scene::AlphaMode, bool) {
                if let Some(reg) = &self.material_registry {
                    let (alpha_ref, dbl) = reg.get_render_flags(MaterialHandle(slot as u32));
                    (alpha_ref.clone(), dbl)
                } else {
                    (ferrous_core::scene::AlphaMode::Opaque, fallback_double)
                }
            };

        // ── Instanced path (World entities) ──────────────────────────────────
        // Determine which instance bind group to use: GPU-culled (Phase 11)
        // takes priority over the CPU-uploaded bind group.
        #[cfg(feature = "gpu-driven")]
        let effective_inst_bg = self
            .culled_instance_bind_group
            .as_ref()
            .or(self.instance_bind_group.as_ref());
        
        #[cfg(not(feature = "gpu-driven"))]
        let effective_inst_bg = self.instance_bind_group.as_ref();

        if let Some(inst_bg) = effective_inst_bg {
            if !packet.instanced_objects.is_empty() {
                // render all instanced geometry in two passes (opaque then
                // transparent).  we reuse the same bind groups for both.
                rpass.set_bind_group(0, &*self.camera_bind_group, &[]);
                rpass.set_bind_group(1, inst_bg.as_ref(), &[]);

                #[cfg(feature = "bindless")]
                if let Some(bbg) = &self.bindless_bind_group {
                    // bindless descriptor set at binding slot 2 (arbitrary choice)
                    rpass.set_bind_group(2, bbg.as_ref(), &[]);
                }

                // helper closure to choose an instancing pipeline from render
                // flags.  we no longer consult `material_registry` here; the
                // caller will supply the resolved alpha mode / sidedness.
                let choose_inst_pipe = |alpha: &ferrous_core::scene::AlphaMode,
                                        double_sided: bool|
                 -> &wgpu::RenderPipeline {
                    match (alpha, double_sided) {
                        (ferrous_core::scene::AlphaMode::Opaque, false) => {
                            &self.instancing_pipeline.inner
                        }
                        (ferrous_core::scene::AlphaMode::Opaque, true) => {
                            &self.instancing_pipeline_double.inner
                        }
                        (ferrous_core::scene::AlphaMode::Mask { .. }, false) => {
                            &self.instancing_pipeline.inner
                        }
                        (ferrous_core::scene::AlphaMode::Mask { .. }, true) => {
                            &self.instancing_pipeline_double.inner
                        }
                        (ferrous_core::scene::AlphaMode::Blend, false) => {
                            &self.instancing_pipeline_blend.inner
                        }
                        (ferrous_core::scene::AlphaMode::Blend, true) => {
                            &self.instancing_pipeline_blend_double.inner
                        }
                    }
                };

                // ── GPU-driven path (Phase 11) ────────────────────────────
                // compute `maybe_indirect` only if the feature is enabled; the
                // unused variable will be optimized away otherwise.
                #[cfg(feature = "gpu-driven")]
                let maybe_indirect = self.indirect_buf.as_ref();
                #[cfg(not(feature = "gpu-driven"))]
                let maybe_indirect: Option<&Arc<wgpu::Buffer>> = None;

                if let Some(indirect) = maybe_indirect {
                    // GPU-driven branch: issue one indirect draw per batch.
                    for (i, cmd) in packet.instanced_objects.iter().enumerate() {
                        let (alpha_mode, double_sided) =
                            get_flags(cmd.material_slot, cmd.double_sided);
                        rpass.set_pipeline(choose_inst_pipe(&alpha_mode, double_sided));
                        if let Some(mat_bg) = self.material_bind_groups.get(cmd.material_slot) {
                            rpass.set_bind_group(2, mat_bg.as_ref(), &[]);
                        }
                        rpass.set_bind_group(3, self.environment.bind_group.as_ref(), &[]);
                        rpass.set_vertex_buffer(0, cmd.vertex_buffer.slice(..));
                        rpass.set_index_buffer(cmd.index_buffer.slice(..), cmd.index_format);
                        // Offset into indirect buffer: each DrawIndexedIndirect is 20 bytes.
                        let offset = i as u64 * 20;
                        rpass.draw_indexed_indirect(indirect.as_ref(), offset);
                    }
                } else {
                    // ── CPU-driven path (legacy) ──────────────────────────

                    // first draw opaque/masked objects (order not critical)
                    for cmd in &packet.instanced_objects {
                        let (alpha_mode, double_sided) =
                            get_flags(cmd.material_slot, cmd.double_sided);
                        if matches!(alpha_mode, ferrous_core::scene::AlphaMode::Blend) {
                            continue;
                        }
                        rpass.set_pipeline(choose_inst_pipe(&alpha_mode, double_sided));
                        if let Some(mat_bg) = self.material_bind_groups.get(cmd.material_slot) {
                            rpass.set_bind_group(2, mat_bg.as_ref(), &[]);
                        }
                        rpass.set_bind_group(3, self.environment.bind_group.as_ref(), &[]);
                        // shadow map is now included in the environment bind group
                        rpass.set_vertex_buffer(0, cmd.vertex_buffer.slice(..));
                        rpass.set_index_buffer(cmd.index_buffer.slice(..), cmd.index_format);
                        rpass.draw_indexed(
                            0..cmd.index_count,
                            0,
                            cmd.first_instance..cmd.first_instance + cmd.instance_count,
                        );
                    }

                    // then the transparent batches, sorted back-to-front
                    let mut transparent_cmds: Vec<&InstancedDrawCommand> = packet
                        .instanced_objects
                        .iter()
                        .filter(|cmd| {
                            let (alpha_mode, _) = get_flags(cmd.material_slot, cmd.double_sided);
                            matches!(alpha_mode, ferrous_core::scene::AlphaMode::Blend)
                        })
                        .collect();
                    transparent_cmds.sort_by(|a, b| {
                        b.distance_sq
                            .partial_cmp(&a.distance_sq)
                            .unwrap_or(std::cmp::Ordering::Equal)
                    });
                    for cmd in transparent_cmds {
                        let (alpha_mode, double_sided) =
                            get_flags(cmd.material_slot, cmd.double_sided);
                        rpass.set_pipeline(choose_inst_pipe(&alpha_mode, double_sided));
                        if let Some(mat_bg) = self.material_bind_groups.get(cmd.material_slot) {
                            rpass.set_bind_group(2, mat_bg.as_ref(), &[]);
                        }
                        rpass.set_bind_group(3, self.environment.bind_group.as_ref(), &[]);
                        // shadow map is now included in the environment bind group
                        rpass.set_vertex_buffer(0, cmd.vertex_buffer.slice(..));
                        rpass.set_index_buffer(cmd.index_buffer.slice(..), cmd.index_format);
                        rpass.draw_indexed(
                            0..cmd.index_count,
                            0,
                            cmd.first_instance..cmd.first_instance + cmd.instance_count,
                        );
                    }
                }
            }
        }
    }
}
