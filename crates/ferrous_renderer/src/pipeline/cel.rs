/// Cel-shaded render pipeline.
///
/// Compiles `assets/shaders/cel.wgsl` with bind groups:
///   group(0) — camera
///   group(1) — instance storage buffer
///   group(2) — material (albedo texture + uniform)
///   group(3) — cel lights: dir light (0) + CelParams (10)
use std::sync::Arc;

use crate::geometry::Vertex;
use crate::pipeline::PipelineLayouts;

#[derive(Clone)]
pub struct CelPipeline {
    pub inner: Arc<wgpu::RenderPipeline>,
    pub layouts: PipelineLayouts,
}

impl CelPipeline {
    pub fn new(
        device: &wgpu::Device,
        target_format: wgpu::TextureFormat,
        sample_count: u32,
        layouts: PipelineLayouts,
        cull_mode: Option<wgpu::Face>,
    ) -> Self {
        let shader = device.create_shader_module(wgpu::include_wgsl!(
            "../../../../assets/shaders/cel.wgsl"
        ));

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("Cel Pipeline Layout"),
            bind_group_layouts: &[
                &layouts.camera,
                &layouts.instance,
                &layouts.material,
                &layouts.cel_lights,
            ],
            push_constant_ranges: &[],
        });

        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Cel Render Pipeline"),
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
                    blend: None,
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                front_face: wgpu::FrontFace::Ccw,
                cull_mode,
                ..Default::default()
            },
            depth_stencil: Some(wgpu::DepthStencilState {
                format: wgpu::TextureFormat::Depth32Float,
                depth_write_enabled: true,
                depth_compare: wgpu::CompareFunction::Less,
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
