use std::sync::Arc;

use crate::geometry::{Mesh, Vertex};
use crate::graph::{FramePacket, RenderPass};
use crate::pipeline::PipelineLayouts;
use wgpu::{
    CommandEncoder, Device, LoadOp, Operations, Queue, RenderPassColorAttachment,
    RenderPassDepthStencilAttachment, RenderPassDescriptor, StoreOp, TextureView,
};

/// Pipeline for a physically-inspired procedural HDR sky.
#[derive(Clone)]
pub struct ProceduralSkyPipeline {
    pub inner: Arc<wgpu::RenderPipeline>,
    pub layouts: PipelineLayouts,
}

impl ProceduralSkyPipeline {
    pub fn new(
        device: &wgpu::Device,
        target_format: wgpu::TextureFormat,
        sample_count: u32,
        layouts: PipelineLayouts,
    ) -> Self {
        let shader = device.create_shader_module(wgpu::include_wgsl!(
            "../../../../assets/shaders/procedural_sky.wgsl"
        ));

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("Procedural Sky Pipeline Layout"),
            // group0 = camera, group1 = lighting/sun
            bind_group_layouts: &[&layouts.camera, &layouts.lights],
            push_constant_ranges: &[],
        });

        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Procedural Sky Render Pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                buffers: &[Vertex::layout()],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_main"),
                targets: &[Some(wgpu::ColorTargetState {
                    format: target_format,
                    blend: Some(wgpu::BlendState::REPLACE),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                front_face: wgpu::FrontFace::Ccw,
                cull_mode: Some(wgpu::Face::Front),
                ..Default::default()
            },
            depth_stencil: Some(wgpu::DepthStencilState {
                format: wgpu::TextureFormat::Depth32Float,
                depth_write_enabled: false,
                depth_compare: wgpu::CompareFunction::LessEqual,
                stencil: wgpu::StencilState::default(),
                bias: wgpu::DepthBiasState::default(),
            }),
            multisample: wgpu::MultisampleState {
                count: sample_count,
                mask: !0,
                alpha_to_coverage_enabled: false,
            },
            multiview: None,
            cache: None,
        });

        Self {
            inner: Arc::new(pipeline),
            layouts,
        }
    }
}

/// Render pass that draws a procedurally generated HDR sky.
pub struct ProceduralSkyPass {
    pipeline: ProceduralSkyPipeline,
    mesh: Mesh,
    camera_bind_group: Arc<wgpu::BindGroup>,
    light_bind_group: Arc<wgpu::BindGroup>,
}

impl ProceduralSkyPass {
    pub fn new(
        device: &wgpu::Device,
        layouts: &PipelineLayouts,
        camera_bind_group: Arc<wgpu::BindGroup>,
        light_bind_group: Arc<wgpu::BindGroup>,
        target_format: wgpu::TextureFormat,
        sample_count: u32,
    ) -> Self {
        let pipeline = ProceduralSkyPipeline::new(device, target_format, sample_count, layouts.clone());
        let mesh = Mesh::sphere(device, 1.0, 32, 64);
        Self {
            pipeline,
            mesh,
            camera_bind_group,
            light_bind_group,
        }
    }

    pub fn set_light_bind_group(&mut self, bg: Arc<wgpu::BindGroup>) {
        self.light_bind_group = bg;
    }
}

impl RenderPass for ProceduralSkyPass {
    fn name(&self) -> &str {
        "ProceduralSkyPass"
    }

    fn prepare(&mut self, _device: &Device, _queue: &Queue, _packet: &FramePacket) {}

    fn execute(
        &mut self,
        _device: &Device,
        _queue: &Queue,
        encoder: &mut CommandEncoder,
        color_view: &TextureView,
        _resolve_target: Option<&TextureView>,
        depth_view: Option<&TextureView>,
        _packet: &FramePacket,
    ) {
        let mut rpass = encoder.begin_render_pass(&RenderPassDescriptor {
            label: Some("ProceduralSkyRenderPass"),
            color_attachments: &[Some(RenderPassColorAttachment {
                view: color_view,
                resolve_target: _resolve_target,
                ops: Operations {
                    load: LoadOp::Clear(wgpu::Color::BLACK),
                    store: StoreOp::Store,
                },
            })],
            depth_stencil_attachment: depth_view.map(|dv| RenderPassDepthStencilAttachment {
                view: dv,
                depth_ops: Some(Operations {
                    load: LoadOp::Load,
                    store: StoreOp::Store,
                }),
                stencil_ops: None,
            }),
            occlusion_query_set: None,
            timestamp_writes: None,
        });

        rpass.set_pipeline(&self.pipeline.inner);
        rpass.set_bind_group(0, &*self.camera_bind_group, &[]);
        rpass.set_bind_group(1, &*self.light_bind_group, &[]);

        rpass.set_vertex_buffer(0, self.mesh.vertex_buffer.slice(..));
        rpass.set_index_buffer(self.mesh.index_buffer.slice(..), self.mesh.index_format);
        rpass.draw_indexed(0..self.mesh.index_count, 0, 0..1);
    }
}
