/// 3-D / 2-D opaque geometry pass.
///
/// Clears color + depth, binds the camera and the shared model-matrix buffer,
/// and emits one indexed draw call per `DrawCommand` in the `FramePacket`.
///
/// ## Dynamic model buffer (legacy path)
///
/// All per-object model matrices live in a single `ModelBuffer`.  This pass
/// receives a reference to the current bind group + stride via
/// [`WorldPass::set_model_buffer`].  Each draw call sets only the dynamic
/// offset (4 bytes on the CPU), so the total CPU overhead for model matrix
/// binding is O(1) GPU-API calls instead of O(N).
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
use crate::pipeline::{InstancingPipeline, PbrPipeline, PipelineLayouts};
use ferrous_core::scene::MaterialHandle;
use crate::resources::{DirectionalLightUniform, Environment};

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
    camera_bind_group: Arc<wgpu::BindGroup>,
    /// Shared dynamic model-matrix bind group (legacy path).
    model_bind_group: Option<Arc<wgpu::BindGroup>>,
    /// Byte stride between matrix slots in the model buffer.
    model_stride: u32,
    /// Bind group for the instance storage buffer.
    instance_bind_group: Option<Arc<wgpu::BindGroup>>,
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
    ) -> Self {
        // environment bundle includes directional light and placeholder IBL
        let environment = Environment::new_dummy(device, queue, &layouts.lights);

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
            model_bind_group: None,
            model_stride: 256, // safe default; overwritten by set_model_buffer
            instance_bind_group: None,
            clear_color: Color {
                r: 0.1,
                g: 0.2,
                b: 0.3,
                a: 1.0,
            },
            material_bind_groups: Vec::new(),
            material_registry: None,
            environment,
        }
    }

    /// Called by `Renderer` whenever the `ModelBuffer` is created or reallocated.
    pub fn set_model_buffer(&mut self, bind_group: Arc<wgpu::BindGroup>, stride: u32) {
        self.model_bind_group = Some(bind_group);
        self.model_stride = stride;
    }

    /// Called by `Renderer` whenever the `InstanceBuffer` is created or reallocated.
    pub fn set_instance_buffer(&mut self, bind_group: Arc<wgpu::BindGroup>) {
        self.instance_bind_group = Some(bind_group);
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
    }

    /// Push new light data into the GPU buffer.  The caller is responsible
    /// for providing a queue reference; the uniform struct will also be
    /// cached locally so that it may be inspected later if required.
    pub fn update_light(&mut self, queue: &wgpu::Queue, uniform: DirectionalLightUniform) {
        // forward update to environment helper
        self.environment.update_light(queue, uniform);
    }
}

impl RenderPass for WorldPass {
    fn name(&self) -> &str {
        "World Pass"
    }

    fn prepare(&mut self, _device: &Device, _queue: &Queue, _packet: &FramePacket) {}

    fn execute(
        &mut self,
        _device: &Device,
        _queue: &Queue,
        encoder: &mut CommandEncoder,
        color_view: &TextureView,
        resolve_target: Option<&TextureView>,
        depth_view: Option<&TextureView>,
        packet: &FramePacket,
    ) {
        let mut rpass = encoder.begin_render_pass(&RenderPassDescriptor {
            label: Some(self.name()),
            color_attachments: &[Some(RenderPassColorAttachment {
                view: color_view,
                resolve_target,
                ops: Operations {
                    load: LoadOp::Clear(self.clear_color),
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

        // ── Instanced path (World entities) ──────────────────────────────────
        if let Some(inst_bg) = &self.instance_bind_group {
            if !packet.instanced_objects.is_empty() {
                rpass.set_bind_group(0, &*self.camera_bind_group, &[]);
                rpass.set_bind_group(1, inst_bg.as_ref(), &[]);

                for cmd in &packet.instanced_objects {
                    // choose appropriate instancing pipeline based on material
                    if let Some(reg) = &self.material_registry {
                        let (alpha_mode, double_sided) =
                            reg.get_render_flags(MaterialHandle(cmd.material_slot as u32));
                        let pipe = match (alpha_mode, double_sided) {
                            (ferrous_core::scene::AlphaMode::Opaque, false) =>
                                &self.instancing_pipeline.inner,
                            (ferrous_core::scene::AlphaMode::Opaque, true) =>
                                &self.instancing_pipeline_double.inner,
                            (ferrous_core::scene::AlphaMode::Mask { .. }, false) =>
                                &self.instancing_pipeline.inner,
                            (ferrous_core::scene::AlphaMode::Mask { .. }, true) =>
                                &self.instancing_pipeline_double.inner,
                            (ferrous_core::scene::AlphaMode::Blend, false) =>
                                &self.instancing_pipeline_blend.inner,
                            (ferrous_core::scene::AlphaMode::Blend, true) =>
                                &self.instancing_pipeline_blend_double.inner,
                        };
                        rpass.set_pipeline(pipe);
                    } else {
                        if cmd.double_sided {
                            rpass.set_pipeline(&self.instancing_pipeline_double.inner);
                        } else {
                            rpass.set_pipeline(&self.instancing_pipeline.inner);
                        }
                    }
                    // bind material if present
                    if let Some(mat_bg) = self.material_bind_groups.get(cmd.material_slot) {
                        rpass.set_bind_group(2, mat_bg.as_ref(), &[]);
                    }
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

        // ── Legacy per-object path (manually-spawned objects) ─────────────────
        if let Some(model_bg) = &self.model_bind_group {
            if !packet.scene_objects.is_empty() {
                rpass.set_bind_group(0, &*self.camera_bind_group, &[]);

                for cmd in &packet.scene_objects {
                    let offset = (cmd.model_slot as u32).wrapping_mul(self.model_stride);
                    rpass.set_bind_group(1, model_bg.as_ref(), &[offset]);
                    // bind material group if available
                    if let Some(mat_bg) = self.material_bind_groups.get(cmd.material_slot) {
                        rpass.set_bind_group(2, mat_bg.as_ref(), &[]);
                    }
                    // bind lights for PBR
                    rpass.set_bind_group(3, self.environment.bind_group.as_ref(), &[]);
                        // pick pipeline based on material flags and sidedness
                        if let Some(reg) = &self.material_registry {
                            let (alpha_mode, double_sided) = reg.get_render_flags(MaterialHandle(cmd.material_slot as u32));
                            let pipeline_ref = match (alpha_mode, double_sided) {
                                (ferrous_core::scene::AlphaMode::Opaque, false) => &self.pbr_pipeline.inner,
                                (ferrous_core::scene::AlphaMode::Opaque, true) => &self.pbr_pipeline_double.inner,
                                (ferrous_core::scene::AlphaMode::Mask { .. }, false) => &self.pbr_pipeline.inner,
                                (ferrous_core::scene::AlphaMode::Mask { .. }, true) => &self.pbr_pipeline_double.inner,
                                (ferrous_core::scene::AlphaMode::Blend, false) => &self.pbr_pipeline_blend.inner,
                                (ferrous_core::scene::AlphaMode::Blend, true) => &self.pbr_pipeline_blend_double.inner,
                            };
                            rpass.set_pipeline(pipeline_ref);
                        } else {
                            // fallback: use regular pipeline with culling as requested
                            if cmd.double_sided {
                                rpass.set_pipeline(&self.pbr_pipeline_double.inner);
                            } else {
                                rpass.set_pipeline(&self.pbr_pipeline.inner);
                            }
                        }
                    rpass.set_vertex_buffer(0, cmd.vertex_buffer.slice(..));
                    rpass.set_index_buffer(cmd.index_buffer.slice(..), cmd.index_format);
                    rpass.draw_indexed(0..cmd.index_count, 0, 0..1);
                }
            }
        }
    }
}
