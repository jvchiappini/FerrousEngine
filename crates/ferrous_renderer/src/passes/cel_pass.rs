/// Cel / Toon-shaded instanced geometry pass.
///
/// Replaces the `WorldPass` when `RenderStyle::CelShaded` is active.
/// Renders all instanced world objects using the toon-ramp shader.
/// If `outline_width > 0` an `OutlinePass` should be rendered immediately
/// after this pass (before post-process) to draw the inverted-hull outline.
///
/// ## Bind group setup (group 3)
/// The pass owns two GPU buffers that are rebuilt whenever `prepare` is called:
/// - `dir_light_buf` — mirrors the directional light uniform.
/// - `cel_params_buf` — carries `{ toon_levels, outline_width, _pad×2 }`.
///
/// Both are bound together with the shared `cel_lights` layout so the WGSL
/// shader can sample them at bindings 0 and 10 respectively.
use std::sync::Arc;

use bytemuck::{Pod, Zeroable};
use wgpu::util::DeviceExt;
use wgpu::{
    CommandEncoder, Device, LoadOp, Operations, Queue, RenderPassColorAttachment,
    RenderPassDepthStencilAttachment, RenderPassDescriptor, StoreOp, TextureView,
};

use crate::graph::{FramePacket, RenderPass};
use crate::pipeline::{CelPipeline, PipelineLayouts};
use crate::resources::DirectionalLightUniform;

// ── GPU-facing structs ────────────────────────────────────────────────────────

/// Matches `CelParams` in `cel.wgsl`.
#[repr(C)]
#[derive(Copy, Clone, Pod, Zeroable)]
pub struct CelParamsUniform {
    pub toon_levels: u32,
    pub outline_width: f32,
    pub _pad0: u32,
    pub _pad1: u32,
}

// ── Per-frame packet marker ───────────────────────────────────────────────────

/// Inserted into `FramePacket::extras` by the renderer when
/// `RenderStyle::CelShaded` is active.  The cel pass reads this each frame.
pub struct CelFrameData {
    pub light: DirectionalLightUniform,
    pub toon_levels: u32,
    pub outline_width: f32,
}

// ── Pass ─────────────────────────────────────────────────────────────────────

pub struct CelShadedPass {
    pipeline: CelPipeline,
    pipeline_double: CelPipeline,
    camera_bind_group: Arc<wgpu::BindGroup>,
    instance_bind_group: Option<Arc<wgpu::BindGroup>>,
    material_bind_groups: Vec<Arc<wgpu::BindGroup>>,
    lights_layout: Arc<wgpu::BindGroupLayout>,
    /// Current dir-light + cel-params bind group (rebuilt each frame in prepare).
    cel_bind_group: Option<wgpu::BindGroup>,
    /// Persistent GPU buffers (recreated only when params change).
    dir_light_buf: Option<wgpu::Buffer>,
    cel_params_buf: Option<wgpu::Buffer>,
    /// Cached params to detect when we need to recreate the bind group.
    last_light: Option<DirectionalLightUniform>,
    last_toon_levels: u32,
    last_outline_width: f32,
}

impl CelShadedPass {
    pub fn new(
        device: &Device,
        layouts: &PipelineLayouts,
        camera_bind_group: Arc<wgpu::BindGroup>,
        target_format: wgpu::TextureFormat,
        sample_count: u32,
        toon_levels: u32,
        outline_width: f32,
    ) -> Self {
        let pipeline = CelPipeline::new(
            device,
            target_format,
            sample_count,
            layouts.clone(),
            Some(wgpu::Face::Back),
        );
        let pipeline_double = CelPipeline::new(
            device,
            target_format,
            sample_count,
            layouts.clone(),
            None,
        );

        // Create initial buffers with defaults.
        let dir_light = DirectionalLightUniform::default();
        let cel_params = CelParamsUniform {
            toon_levels: toon_levels.max(2),
            outline_width,
            _pad0: 0,
            _pad1: 0,
        };
        let dir_light_buf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Cel DirLight Buffer"),
            contents: bytemuck::bytes_of(&dir_light),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });
        let cel_params_buf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Cel Params Buffer"),
            contents: bytemuck::bytes_of(&cel_params),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });
        let cel_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Cel Lights BG"),
            layout: &layouts.cel_lights,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: dir_light_buf.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 10,
                    resource: cel_params_buf.as_entire_binding(),
                },
            ],
        });

        Self {
            pipeline,
            pipeline_double,
            camera_bind_group,
            instance_bind_group: None,
            material_bind_groups: Vec::new(),
            lights_layout: Arc::clone(&layouts.cel_lights),
            cel_bind_group: Some(cel_bind_group),
            dir_light_buf: Some(dir_light_buf),
            cel_params_buf: Some(cel_params_buf),
            last_light: None,
            last_toon_levels: toon_levels,
            last_outline_width: outline_width,
        }
    }

    /// Called by the renderer when the instance buffer is reallocated.
    pub fn set_instance_buffer(&mut self, bg: Arc<wgpu::BindGroup>) {
        self.instance_bind_group = Some(bg);
    }

    /// Called by the renderer when materials change.
    pub fn set_material_table(&mut self, table: &[Arc<wgpu::BindGroup>]) {
        self.material_bind_groups.clear();
        self.material_bind_groups.extend_from_slice(table);
    }
}

#[cfg(not(target_arch = "wasm32"))]
impl RenderPass for CelShadedPass {
    fn name(&self) -> &str {
        "CelShadedPass"
    }

    fn prepare(&mut self, device: &Device, queue: &Queue, packet: &FramePacket) {
        let Some(data) = packet.get::<CelFrameData>() else {
            return;
        };

        // Upload dir light if changed.
        if let Some(buf) = &self.dir_light_buf {
            queue.write_buffer(buf, 0, bytemuck::bytes_of(&data.light));
        }

        // Rebuild CelParams buffer if toon_levels or outline_width changed.
        let need_rebuild = self.last_toon_levels != data.toon_levels
            || (self.last_outline_width - data.outline_width).abs() > 1e-6;

        if need_rebuild || self.cel_params_buf.is_none() {
            let params = CelParamsUniform {
                toon_levels: data.toon_levels.max(2),
                outline_width: data.outline_width,
                _pad0: 0,
                _pad1: 0,
            };
            if let Some(buf) = &self.cel_params_buf {
                queue.write_buffer(buf, 0, bytemuck::bytes_of(&params));
            } else {
                let buf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                    label: Some("Cel Params Buffer"),
                    contents: bytemuck::bytes_of(&params),
                    usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
                });
                self.cel_params_buf = Some(buf);
            }
            self.last_toon_levels = data.toon_levels;
            self.last_outline_width = data.outline_width;

            // Rebuild bind group if buffers exist.
            if let (Some(light_buf), Some(params_buf)) =
                (&self.dir_light_buf, &self.cel_params_buf)
            {
                self.cel_bind_group = Some(device.create_bind_group(&wgpu::BindGroupDescriptor {
                    label: Some("Cel Lights BG"),
                    layout: &self.lights_layout,
                    entries: &[
                        wgpu::BindGroupEntry {
                            binding: 0,
                            resource: light_buf.as_entire_binding(),
                        },
                        wgpu::BindGroupEntry {
                            binding: 10,
                            resource: params_buf.as_entire_binding(),
                        },
                    ],
                }));
            }
        }

        self.last_light = Some(data.light);
    }

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
        let Some(cel_bg) = &self.cel_bind_group else {
            return;
        };
        let Some(inst_bg) = &self.instance_bind_group else {
            return;
        };

        let color_attachment = RenderPassColorAttachment {
            view: color_view,
            resolve_target,
            ops: Operations {
                load: LoadOp::Load, // cel pass renders on top of clear done by WorldPass
                store: StoreOp::Store,
            },
        };

        let depth_attachment = depth_view.map(|dv| RenderPassDepthStencilAttachment {
            view: dv,
            depth_ops: Some(Operations {
                load: LoadOp::Load,
                store: StoreOp::Store,
            }),
            stencil_ops: None,
        });

        let mut rpass = encoder.begin_render_pass(&RenderPassDescriptor {
            label: Some("CelShaded Pass"),
            color_attachments: &[Some(color_attachment)],
            depth_stencil_attachment: depth_attachment,
            timestamp_writes: None,
            occlusion_query_set: None,
        });

        // Draw instanced objects.
        for cmd in &packet.instanced_objects {
            let pipeline = if cmd.double_sided {
                &self.pipeline_double
            } else {
                &self.pipeline
            };
            rpass.set_pipeline(&pipeline.inner);
            rpass.set_bind_group(0, &*self.camera_bind_group, &[]);
            rpass.set_bind_group(1, inst_bg.as_ref(), &[]);

            let mat_idx = cmd.material_slot.min(self.material_bind_groups.len().saturating_sub(1));
            if let Some(mat_bg) = self.material_bind_groups.get(mat_idx) {
                rpass.set_bind_group(2, mat_bg.as_ref(), &[]);
            } else {
                continue;
            }
            rpass.set_bind_group(3, cel_bg, &[]);

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

#[cfg(target_arch = "wasm32")]
impl RenderPass for CelShadedPass {
    fn name(&self) -> &str { "CelShadedPass" }
    fn prepare(&mut self, _: &Device, _: &Queue, _: &FramePacket) {}
    fn execute(
        &mut self, _: &Device, _: &Queue, _: &mut CommandEncoder,
        _: &TextureView, _: Option<&TextureView>, _: Option<&TextureView>, _: &FramePacket,
    ) {}
}
