/// Inverted-hull outline pass.
///
/// Renders each instanced mesh a second time with front-face culling enabled.
/// Vertices are extruded along their normals by `outline_width` world units.
/// The result is a solid-colour outline shell visible around objects.
///
/// This pass must run **after** `CelShadedPass` (or `WorldPass`) and **before**
/// post-process so the outline is tone-mapped together with the scene.
///
/// ## Frame data
/// Reads `OutlineFrameData` from `FramePacket::extras`.  The renderer inserts
/// this when `RenderStyle::CelShaded { outline_width > 0.0 }` is active.
use std::sync::Arc;

use bytemuck::{Pod, Zeroable};
use wgpu::util::DeviceExt;
use wgpu::{
    CommandEncoder, Device, LoadOp, Operations, Queue, RenderPassColorAttachment,
    RenderPassDepthStencilAttachment, RenderPassDescriptor, StoreOp, TextureView,
};

use crate::graph::{FramePacket, RenderPass};
use crate::passes::cel_pass::CelParamsUniform;
use crate::pipeline::{OutlinePipeline, PipelineLayouts};
use crate::resources::DirectionalLightUniform;

// ── Per-frame packet marker ───────────────────────────────────────────────────

/// Inserted into `FramePacket::extras` by the renderer when outlines are active.
pub struct OutlineFrameData {
    pub light: DirectionalLightUniform,
    pub toon_levels: u32,
    pub outline_width: f32,
    /// RGBA colour for the outline (default: black, fully opaque).
    pub color: [f32; 4],
}

// ── Outline colour GPU struct ─────────────────────────────────────────────────

#[repr(C)]
#[derive(Copy, Clone, Pod, Zeroable)]
struct OutlineColorUniform {
    pub color: [f32; 4],
}

// ── Pass ─────────────────────────────────────────────────────────────────────

pub struct OutlinePass {
    pipeline: OutlinePipeline,
    camera_bind_group: Arc<wgpu::BindGroup>,
    instance_bind_group: Option<Arc<wgpu::BindGroup>>,
    material_bind_groups: Vec<Arc<wgpu::BindGroup>>,
    lights_layout: Arc<wgpu::BindGroupLayout>,
    outline_bind_group: Option<wgpu::BindGroup>,
    dir_light_buf: Option<wgpu::Buffer>,
    cel_params_buf: Option<wgpu::Buffer>,
    outline_color_buf: Option<wgpu::Buffer>,
}

impl OutlinePass {
    pub fn new(
        device: &Device,
        layouts: &PipelineLayouts,
        camera_bind_group: Arc<wgpu::BindGroup>,
        target_format: wgpu::TextureFormat,
        sample_count: u32,
        toon_levels: u32,
        outline_width: f32,
        outline_color: [f32; 4],
    ) -> Self {
        let pipeline = OutlinePipeline::new(device, target_format, sample_count, layouts.clone());

        let dir_light = DirectionalLightUniform::default();
        let cel_params = CelParamsUniform {
            toon_levels: toon_levels.max(2),
            outline_width,
            _pad0: 0,
            _pad1: 0,
        };
        let oc = OutlineColorUniform { color: outline_color };

        let dir_light_buf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Outline DirLight Buffer"),
            contents: bytemuck::bytes_of(&dir_light),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });
        let cel_params_buf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Outline Cel Params Buffer"),
            contents: bytemuck::bytes_of(&cel_params),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });
        let outline_color_buf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Outline Color Buffer"),
            contents: bytemuck::bytes_of(&oc),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });

        let outline_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Outline Lights BG"),
            layout: &layouts.outline_lights,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: dir_light_buf.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 10,
                    resource: cel_params_buf.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 11,
                    resource: outline_color_buf.as_entire_binding(),
                },
            ],
        });

        Self {
            pipeline,
            camera_bind_group,
            instance_bind_group: None,
            material_bind_groups: Vec::new(),
            lights_layout: Arc::clone(&layouts.outline_lights),
            outline_bind_group: Some(outline_bind_group),
            dir_light_buf: Some(dir_light_buf),
            cel_params_buf: Some(cel_params_buf),
            outline_color_buf: Some(outline_color_buf),
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
impl RenderPass for OutlinePass {
    fn name(&self) -> &str {
        "OutlinePass"
    }

    fn prepare(&mut self, device: &Device, queue: &Queue, packet: &FramePacket) {
        let Some(data) = packet.get::<OutlineFrameData>() else {
            return;
        };

        if let Some(buf) = &self.dir_light_buf {
            queue.write_buffer(buf, 0, bytemuck::bytes_of(&data.light));
        }
        let cel = CelParamsUniform {
            toon_levels: data.toon_levels.max(2),
            outline_width: data.outline_width,
            _pad0: 0,
            _pad1: 0,
        };
        if let Some(buf) = &self.cel_params_buf {
            queue.write_buffer(buf, 0, bytemuck::bytes_of(&cel));
        }
        let oc = OutlineColorUniform { color: data.color };
        if let Some(buf) = &self.outline_color_buf {
            queue.write_buffer(buf, 0, bytemuck::bytes_of(&oc));
        }

        // Rebuild bind group (lazily, once all buffers are present).
        if self.outline_bind_group.is_none() {
            if let (Some(dl), Some(cp), Some(oc_buf)) = (
                &self.dir_light_buf,
                &self.cel_params_buf,
                &self.outline_color_buf,
            ) {
                self.outline_bind_group =
                    Some(device.create_bind_group(&wgpu::BindGroupDescriptor {
                        label: Some("Outline Lights BG"),
                        layout: &self.lights_layout,
                        entries: &[
                            wgpu::BindGroupEntry { binding: 0, resource: dl.as_entire_binding() },
                            wgpu::BindGroupEntry { binding: 10, resource: cp.as_entire_binding() },
                            wgpu::BindGroupEntry {
                                binding: 11,
                                resource: oc_buf.as_entire_binding(),
                            },
                        ],
                    }));
            }
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
        let Some(outline_bg) = &self.outline_bind_group else { return; };
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
            label: Some("Outline Pass"),
            color_attachments: &[Some(color_att)],
            depth_stencil_attachment: depth_att,
            timestamp_writes: None,
            occlusion_query_set: None,
        });

        rpass.set_pipeline(&self.pipeline.inner);

        for cmd in &packet.instanced_objects {
            rpass.set_bind_group(0, &*self.camera_bind_group, &[]);
            rpass.set_bind_group(1, inst_bg.as_ref(), &[]);

            let mat_idx = cmd.material_slot.min(self.material_bind_groups.len().saturating_sub(1));
            if let Some(mat_bg) = self.material_bind_groups.get(mat_idx) {
                rpass.set_bind_group(2, mat_bg.as_ref(), &[]);
            } else {
                continue;
            }
            rpass.set_bind_group(3, outline_bg, &[]);

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
impl RenderPass for OutlinePass {
    fn name(&self) -> &str { "OutlinePass" }
    fn prepare(&mut self, _: &Device, _: &Queue, _: &FramePacket) {}
    fn execute(
        &mut self, _: &Device, _: &Queue, _: &mut CommandEncoder,
        _: &TextureView, _: Option<&TextureView>, _: Option<&TextureView>, _: &FramePacket,
    ) {}
}
