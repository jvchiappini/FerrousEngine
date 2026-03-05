/// Flat-shaded instanced geometry pass.
///
/// Replaces `WorldPass` when `RenderStyle::FlatShaded` is active.
/// Uses face normals derived with `dpdx`/`dpdy` — no vertex normal interpolation.
/// The result is the low-poly / faceted look.
///
/// ## Frame data
/// Reads `FlatFrameData` from `FramePacket::extras`.
use std::sync::Arc;

use bytemuck;
use wgpu::util::DeviceExt;
use wgpu::{
    CommandEncoder, Device, LoadOp, Operations, Queue, RenderPassColorAttachment,
    RenderPassDepthStencilAttachment, RenderPassDescriptor, StoreOp, TextureView,
};

use crate::graph::{FramePacket, RenderPass};
use crate::pipeline::{FlatPipeline, PipelineLayouts};
use crate::resources::DirectionalLightUniform;

// ── Per-frame packet marker ───────────────────────────────────────────────────

pub struct FlatFrameData {
    pub light: DirectionalLightUniform,
}

// ── Pass ─────────────────────────────────────────────────────────────────────

pub struct FlatShadedPass {
    pipeline: FlatPipeline,
    pipeline_double: FlatPipeline,
    camera_bind_group: Arc<wgpu::BindGroup>,
    instance_bind_group: Option<Arc<wgpu::BindGroup>>,
    material_bind_groups: Vec<Arc<wgpu::BindGroup>>,
    lights_layout: Arc<wgpu::BindGroupLayout>,
    flat_bind_group: Option<wgpu::BindGroup>,
    dir_light_buf: Option<wgpu::Buffer>,
}

impl FlatShadedPass {
    pub fn new(
        device: &Device,
        layouts: &PipelineLayouts,
        camera_bind_group: Arc<wgpu::BindGroup>,
        target_format: wgpu::TextureFormat,
        sample_count: u32,
    ) -> Self {
        let pipeline = FlatPipeline::new(
            device, target_format, sample_count, layouts.clone(), Some(wgpu::Face::Back),
        );
        let pipeline_double =
            FlatPipeline::new(device, target_format, sample_count, layouts.clone(), None);

        let dir_light = DirectionalLightUniform::default();
        let dir_light_buf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Flat DirLight Buffer"),
            contents: bytemuck::bytes_of(&dir_light),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });
        let flat_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Flat Lights BG"),
            layout: &layouts.flat_lights,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: dir_light_buf.as_entire_binding(),
            }],
        });

        Self {
            pipeline,
            pipeline_double,
            camera_bind_group,
            instance_bind_group: None,
            material_bind_groups: Vec::new(),
            lights_layout: Arc::clone(&layouts.flat_lights),
            flat_bind_group: Some(flat_bind_group),
            dir_light_buf: Some(dir_light_buf),
        }
    }

    pub fn set_instance_buffer(&mut self, bg: Arc<wgpu::BindGroup>) {
        self.instance_bind_group = Some(bg);
    }

    pub fn set_material_table(&mut self, table: &[Arc<wgpu::BindGroup>]) {
        self.material_bind_groups.clear();
        self.material_bind_groups.extend_from_slice(table);
    }
}

#[cfg(not(target_arch = "wasm32"))]
impl RenderPass for FlatShadedPass {
    fn name(&self) -> &str {
        "FlatShadedPass"
    }

    fn prepare(&mut self, device: &Device, queue: &Queue, packet: &FramePacket) {
        let Some(data) = packet.get::<FlatFrameData>() else {
            return;
        };

        if let Some(buf) = &self.dir_light_buf {
            queue.write_buffer(buf, 0, bytemuck::bytes_of(&data.light));
        } else {
            let buf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("Flat DirLight Buffer"),
                contents: bytemuck::bytes_of(&data.light),
                usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            });
            let bg = device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("Flat Lights BG"),
                layout: &self.lights_layout,
                entries: &[wgpu::BindGroupEntry {
                    binding: 0,
                    resource: buf.as_entire_binding(),
                }],
            });
            self.dir_light_buf = Some(buf);
            self.flat_bind_group = Some(bg);
        }
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
        let Some(flat_bg) = &self.flat_bind_group else { return; };
        let Some(inst_bg) = &self.instance_bind_group else { return; };

        let color_att = RenderPassColorAttachment {
            view: color_view,
            resolve_target,
            ops: Operations {
                load: LoadOp::Load,
                store: StoreOp::Store,
            },
        };
        let depth_att = depth_view.map(|dv| RenderPassDepthStencilAttachment {
            view: dv,
            depth_ops: Some(Operations {
                load: LoadOp::Load,
                store: StoreOp::Store,
            }),
            stencil_ops: None,
        });

        let mut rpass = encoder.begin_render_pass(&RenderPassDescriptor {
            label: Some("FlatShaded Pass"),
            color_attachments: &[Some(color_att)],
            depth_stencil_attachment: depth_att,
            timestamp_writes: None,
            occlusion_query_set: None,
        });

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
            rpass.set_bind_group(3, flat_bg, &[]);

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
impl RenderPass for FlatShadedPass {
    fn name(&self) -> &str { "FlatShadedPass" }
    fn prepare(&mut self, _: &Device, _: &Queue, _: &FramePacket) {}
    fn execute(
        &mut self, _: &Device, _: &Queue, _: &mut CommandEncoder,
        _: &TextureView, _: Option<&TextureView>, _: Option<&TextureView>, _: &FramePacket,
    ) {}
}
