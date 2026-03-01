/// Pipeline de instanced rendering.
///
/// Compila `assets/shaders/instanced.wgsl` y lo combina con el layout
/// `camera` (group 0) + `instance` (group 1, storage buffer).
///
/// El vertex shader lee la matriz de modelo desde el array de storage
/// usando `@builtin(instance_index)`, permitiendo que un único
/// `draw_indexed` con `instance_count > 1` renderice todo el batch.
use std::sync::Arc;

use crate::geometry::Vertex;
use crate::pipeline::PipelineLayouts;

#[derive(Clone)]
pub struct InstancingPipeline {
    pub inner: Arc<wgpu::RenderPipeline>,
    pub layouts: PipelineLayouts,
}

impl InstancingPipeline {
    pub fn new(
        device: &wgpu::Device,
        target_format: wgpu::TextureFormat,
        sample_count: u32,
        layouts: PipelineLayouts,
    ) -> Self {
        let shader = device.create_shader_module(wgpu::include_wgsl!(
            "../../../../assets/shaders/instanced.wgsl"
        ));

        let pipeline_layout =
            device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("Instancing Pipeline Layout"),
                bind_group_layouts: &[&layouts.camera, &layouts.instance],
                push_constant_ranges: &[],
            });

        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Instancing Render Pipeline"),
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
                cull_mode: Some(wgpu::Face::Back),
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
