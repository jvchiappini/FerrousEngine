use std::sync::Arc;

use crate::geometry::Vertex;
use crate::pipeline::PipelineLayouts;

#[derive(Clone)]
pub struct ShadowPipeline {
    pub inner: Arc<wgpu::RenderPipeline>,
    pub layouts: PipelineLayouts,
}

impl ShadowPipeline {
    /// Create a shadow-only pipeline.  This pipeline writes only to the depth
    /// buffer and therefore does not need a fragment stage.  The layout only
    /// requires the model and light bind-group layouts (camera and material are
    /// unused).
    /// If `instanced` is `true` the pipeline expects an instance-storage
    /// buffer in group(0) rather than a dynamic model uniform.
    pub fn new(device: &wgpu::Device, layouts: PipelineLayouts, instanced: bool) -> Self {
        // embed a minimal WGSL shader that simply transforms the vertex
        // position by the light's view-projection matrix.  No fragment shader
        // is needed because we only care about depth.
        // choose the correct shader variant depending on whether we need
        // instanced model data.  The non-instanced version reads a single
        // uniform matrix, whereas the instanced variant uses a storage
        // buffer indexed by `instance_index`.
        let shader_source = if instanced {
            include_str!("../../../../assets/shaders/shadow_instanced.wgsl")
        } else {
            include_str!("../../../../assets/shaders/shadow.wgsl")
        };
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Shadow Shader"),
            source: wgpu::ShaderSource::Wgsl(shader_source.into()),
        });

        let bind_layouts: Vec<&wgpu::BindGroupLayout> = if instanced {
            vec![&layouts.instance, &layouts.shadow_lights]
        } else {
            vec![&layouts.model, &layouts.shadow_lights]
        };
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("Shadow Pipeline Layout"),
            bind_group_layouts: &bind_layouts,
            push_constant_ranges: &[],
        });

        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Shadow Render Pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                buffers: &[Vertex::layout()],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            },
            fragment: None,
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
                // Depth bias pushes shadow-map values slightly away from the
                // light so that the surface doesn't self-shadow (acne).
                // constant: integer units of depth precision to add.
                // slope_scale: bias proportional to the surface slope angle.
                bias: wgpu::DepthBiasState {
                    constant: 2,
                    slope_scale: 2.0,
                    clamp: 0.0,
                },
            }),
            multisample: wgpu::MultisampleState {
                count: 1,
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
