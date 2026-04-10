use wgpu::RenderPipeline;
use std::sync::Arc;
use crate::gpu_types::GuiQuad;
use crate::MAX_TEXTURE_SLOTS;

pub fn create_quad_pipeline(
    device: &wgpu::Device,
    format: wgpu::TextureFormat,
    pipeline_layout: &wgpu::PipelineLayout,
    sample_count: u32,
    depth_write_enabled: bool,
) -> RenderPipeline {
    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("GUI Shader"),
        source: wgpu::ShaderSource::Wgsl(
            #[cfg(not(target_arch = "wasm32"))]
            include_str!("../../../../assets/shaders/gui.wgsl").into(),
            #[cfg(target_arch = "wasm32")]
            include_str!("../../../../assets/shaders/gui_web.wgsl").into(),
        ),
    });

    let instance_size = std::mem::size_of::<GuiQuad>() as u64;

    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("GUI Render Pipeline"),
        layout: Some(pipeline_layout),
        vertex: wgpu::VertexState {
            module: &shader,
            entry_point: Some("vs_main"),
            buffers: &[
                wgpu::VertexBufferLayout {
                    array_stride: std::mem::size_of::<[f32; 2]>() as wgpu::BufferAddress,
                    step_mode: wgpu::VertexStepMode::Vertex,
                    attributes: &[wgpu::VertexAttribute {
                        format: wgpu::VertexFormat::Float32x2,
                        offset: 0,
                        shader_location: 0,
                    }],
                },
                wgpu::VertexBufferLayout {
                    array_stride: instance_size,
                    step_mode: wgpu::VertexStepMode::Instance,
                    attributes: &[
                        // offset  0: pos      [f32; 2]
                        wgpu::VertexAttribute { format: wgpu::VertexFormat::Float32x2, offset: 0,  shader_location: 1 },
                        // offset  8: size     [f32; 2]
                        wgpu::VertexAttribute { format: wgpu::VertexFormat::Float32x2, offset: 8,  shader_location: 2 },
                        // offset 16: uv0      [f32; 2]
                        wgpu::VertexAttribute { format: wgpu::VertexFormat::Float32x2, offset: 16, shader_location: 3 },
                        // offset 24: uv1      [f32; 2]
                        wgpu::VertexAttribute { format: wgpu::VertexFormat::Float32x2, offset: 24, shader_location: 4 },
                        // offset 32: color    [f32; 4]
                        wgpu::VertexAttribute { format: wgpu::VertexFormat::Float32x4, offset: 32, shader_location: 5 },
                        // offset 48: color_b  [f32; 4]
                        wgpu::VertexAttribute { format: wgpu::VertexFormat::Float32x4, offset: 48, shader_location: 6 },
                        // offset 64: radii    [f32; 4]
                        wgpu::VertexAttribute { format: wgpu::VertexFormat::Float32x4, offset: 64, shader_location: 7 },
                        // offset 80: tex_index u32
                        wgpu::VertexAttribute { format: wgpu::VertexFormat::Uint32,    offset: 80, shader_location: 8 },
                        // offset 84: flags    u32
                        wgpu::VertexAttribute { format: wgpu::VertexFormat::Uint32,    offset: 84, shader_location: 9 },
                        // offset 88: z_order  f32
                        wgpu::VertexAttribute { format: wgpu::VertexFormat::Float32,   offset: 88, shader_location: 10 },
                    ],
                },
            ],
            compilation_options: Default::default(),
        },
        fragment: Some(wgpu::FragmentState {
            module: &shader,
            entry_point: Some("fs_main"),
            targets: &[Some(wgpu::ColorTargetState {
                format,
                blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                write_mask: wgpu::ColorWrites::ALL,
            })],
            compilation_options: Default::default(),
        }),
        primitive: wgpu::PrimitiveState::default(),
        depth_stencil: Some(wgpu::DepthStencilState {
            format: wgpu::TextureFormat::Depth32Float,
            depth_write_enabled,
            depth_compare: wgpu::CompareFunction::LessEqual,
            stencil: wgpu::StencilState::default(),
            bias: wgpu::DepthBiasState::default(),
        }),
        multisample: wgpu::MultisampleState { count: sample_count, ..Default::default() },
        multiview: None,
        cache: None,
    })
}
