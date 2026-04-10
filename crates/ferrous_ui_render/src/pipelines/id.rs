use wgpu::RenderPipeline;
use crate::gpu_types::GuiQuad;

pub fn create_id_pipeline(
    device: &wgpu::Device,
    pipeline_layout: &wgpu::PipelineLayout,
) -> RenderPipeline {
    let id_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("GUI ID Buffer Shader"),
        source: wgpu::ShaderSource::Wgsl(
            include_str!("../../../../assets/shaders/id_buffer.wgsl").into(),
        ),
    });

    let instance_size = std::mem::size_of::<GuiQuad>() as u64;

    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("GUI ID Render Pipeline"),
        layout: Some(pipeline_layout),
        vertex: wgpu::VertexState {
            module: &id_shader,
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
                        wgpu::VertexAttribute { format: wgpu::VertexFormat::Float32x2, offset: 0,  shader_location: 1 },
                        wgpu::VertexAttribute { format: wgpu::VertexFormat::Float32x2, offset: 8,  shader_location: 2 },
                        wgpu::VertexAttribute { format: wgpu::VertexFormat::Float32x2, offset: 16, shader_location: 3 },
                        wgpu::VertexAttribute { format: wgpu::VertexFormat::Float32x2, offset: 24, shader_location: 4 },
                        wgpu::VertexAttribute { format: wgpu::VertexFormat::Float32x4, offset: 32, shader_location: 5 },
                        wgpu::VertexAttribute { format: wgpu::VertexFormat::Float32x4, offset: 48, shader_location: 6 },
                        wgpu::VertexAttribute { format: wgpu::VertexFormat::Float32x4, offset: 64, shader_location: 7 },
                        wgpu::VertexAttribute { format: wgpu::VertexFormat::Uint32,    offset: 80, shader_location: 8 },
                        wgpu::VertexAttribute { format: wgpu::VertexFormat::Uint32,    offset: 84, shader_location: 9 },
                        wgpu::VertexAttribute { format: wgpu::VertexFormat::Float32,   offset: 88, shader_location: 10 },
                        // offset 92: node_id u32
                        wgpu::VertexAttribute { format: wgpu::VertexFormat::Uint32,    offset: 92, shader_location: 11 },
                    ],
                },
            ],
            compilation_options: Default::default(),
        },
        fragment: Some(wgpu::FragmentState {
            module: &id_shader,
            entry_point: Some("fs_main"),
            targets: &[Some(wgpu::ColorTargetState {
                format: wgpu::TextureFormat::R32Uint,
                blend: None,
                write_mask: wgpu::ColorWrites::ALL,
            })],
            compilation_options: Default::default(),
        }),
        primitive: wgpu::PrimitiveState::default(),
        depth_stencil: None,
        multisample: wgpu::MultisampleState { count: 1, ..Default::default() },
        multiview: None,
        cache: None,
    })
}
