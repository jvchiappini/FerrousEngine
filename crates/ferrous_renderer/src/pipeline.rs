// pipelines de renderizado

/// Encapsula el `wgpu::RenderPipeline` usado para dibujar la escena básica.
///
/// El pipeline se construye a partir del shader minimalista ubicado en
/// `assets/shaders/base.wgsl` y se configura para ser compatible con el
/// formato de render target que se le pase al crearlo. Los estados de
/// primitiva y profundidad se ajustan para dibujar triángulos con un buffer
/// de profundidad de 32 bits.
pub struct FerrousPipeline {
    pub pipeline: wgpu::RenderPipeline,
    /// layout for camera uniform bind group (binding 0)
    pub camera_bind_group_layout: wgpu::BindGroupLayout,
    /// layout for model transform uniform (binding 0 in group 1)
    pub model_bind_group_layout: wgpu::BindGroupLayout,
}

impl FerrousPipeline {
    /// Crea un pipeline de renderizado básico.
    ///
    /// `device` se utiliza para construir todos los objetos de GPU y
    /// `render_format` debe coincidir con el formato de la textura de color
    /// del `RenderTarget` al que se dibujará (generalmente un SRGB).
    pub fn new(
        device: &wgpu::Device,
        render_format: wgpu::TextureFormat,
        sample_count: u32,
    ) -> Self {
        // cargamos el shader WGSL en tiempo de compilación; el macro ya devuelve
        // un `ShaderModuleDescriptor` listo para usar.
        let shader =
            device.create_shader_module(wgpu::include_wgsl!("../../../assets/shaders/base.wgsl"));

        // layout for the camera uniform that will be updated each frame
        let camera_bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("Camera Bind Group Layout"),
                entries: &[wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::VERTEX,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                }],
            });

        // layout for per-object model matrix; callers will create one bind
        // group per mesh that supplies this uniform. we keep it separate so
        // we can bind group 0 (camera) and then 1 (model) when drawing.
        let model_bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("Model Bind Group Layout"),
                entries: &[wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::VERTEX,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                }],
            });

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("Base Pipeline Layout"),
            bind_group_layouts: &[&camera_bind_group_layout, &model_bind_group_layout],
            push_constant_ranges: &[],
        });

        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Base Render Pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                buffers: &[crate::mesh::Vertex::desc()],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_main"),
                targets: &[Some(wgpu::ColorTargetState {
                    format: render_format,
                    blend: Some(wgpu::BlendState::REPLACE),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                strip_index_format: None,
                front_face: wgpu::FrontFace::Ccw,
                cull_mode: Some(wgpu::Face::Back),
                unclipped_depth: false,
                polygon_mode: wgpu::PolygonMode::Fill,
                conservative: false,
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
            pipeline,
            camera_bind_group_layout,
            model_bind_group_layout,
        }
    }
}
