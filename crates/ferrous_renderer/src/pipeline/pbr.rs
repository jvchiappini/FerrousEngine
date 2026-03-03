/// Physically-based rendering pipeline based on `assets/shaders/pbr.wgsl`.
///
/// Very similar to [`WorldPipeline`], but the pipeline layout also includes
/// a fourth bind-group for directional lights.
use std::sync::Arc;

use crate::geometry::Vertex;
use crate::pipeline::PipelineLayouts;

#[derive(Clone)]
pub struct PbrPipeline {
    pub inner: Arc<wgpu::RenderPipeline>,
    pub layouts: PipelineLayouts,
}

impl PbrPipeline {
    /// Create a pipeline with the provided render states.  This helper
    /// keeps all the boilerplate in one place; callers simply supply the
    /// parameters that vary between opaque/transparent and single/double
    /// sided variants.
    fn create_pipeline(
        device: &wgpu::Device,
        target_format: wgpu::TextureFormat,
        sample_count: u32,
        layouts: &PipelineLayouts,
        cull_mode: Option<wgpu::Face>,
        blend: Option<wgpu::BlendState>,
        depth_write: bool,
    ) -> wgpu::RenderPipeline {
        let shader = device.create_shader_module(wgpu::include_wgsl!(
            "../../../../assets/shaders/pbr.wgsl"
        ));

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("PBR Pipeline Layout"),
            bind_group_layouts: &[
                &layouts.camera,
                &layouts.model,
                &layouts.material,
                &layouts.lights,
            ],
            push_constant_ranges: &[],
        });

        device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("PBR Render Pipeline"),
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
                    blend,
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
                depth_write_enabled: depth_write,
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
        })
    }

    /// Convenience constructor used by the renderer.  `blend` should be
    /// `None` for opaque pipelines and `Some(wgpu::BlendState::ALPHA_BLENDING)`
    /// for translucent ones.  `depth_write` must be `false` when blending is
    /// enabled.
    pub fn new(
        device: &wgpu::Device,
        target_format: wgpu::TextureFormat,
        sample_count: u32,
        layouts: PipelineLayouts,
        cull_mode: Option<wgpu::Face>,
        blend: Option<wgpu::BlendState>,
        depth_write: bool,
    ) -> Self {
        let pipeline = Self::create_pipeline(
            device,
            target_format,
            sample_count,
            &layouts,
            cull_mode,
            blend,
            depth_write,
        );

        Self {
            inner: Arc::new(pipeline),
            layouts,
        }
    }
}
