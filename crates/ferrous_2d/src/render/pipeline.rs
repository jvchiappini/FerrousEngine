use crate::render::types::SpriteInstance;
use std::sync::Arc;

/// Render pipeline and bind-group layouts for textured 2D sprites.
pub struct SpritePipeline {
    /// GPU device the pipeline was created on.
    pub device: Arc<wgpu::Device>,
    /// The compiled wgpu render pipeline.
    pub wgpu_pipeline: wgpu::RenderPipeline,
    /// Bind group 0: the shared 2D camera uniform.
    pub camera_bind_group_layout: wgpu::BindGroupLayout,
    /// Bind group 1: the per-texture view + sampler.
    pub texture_bind_group_layout: wgpu::BindGroupLayout,
}

impl SpritePipeline {
    /// Build the sprite pipeline for the given output `format` and
    /// `sample_count` (pass `depth_format: None` for the depth-less Pure2D path).
    pub fn new(
        device: Arc<wgpu::Device>,
        output_format: wgpu::TextureFormat,
        depth_format: Option<wgpu::TextureFormat>,
        sample_count: u32,
    ) -> Self {
        Self::new_with_cache(device, output_format, depth_format, sample_count, None)
    }

    /// Same as [`SpritePipeline::new`] but threads a shared [`wgpu::PipelineCache`]
    /// into `create_render_pipeline`, so repeated pipeline creation for the same
    /// device can reuse the driver's compiled shaders.
    pub fn new_with_cache(
        device: Arc<wgpu::Device>,
        output_format: wgpu::TextureFormat,
        depth_format: Option<wgpu::TextureFormat>,
        sample_count: u32,
        cache: Option<&wgpu::PipelineCache>,
    ) -> Self {
        // ... (resto del código de layouts se mantiene igual)
        // 1. Camera Layout
        let camera_bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("Sprite Camera Bind Group Layout"),
                entries: &[wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                }],
            });

        // 2. Texture Layout
        let texture_bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("Sprite Texture Bind Group Layout"),
                entries: &[
                    wgpu::BindGroupLayoutEntry {
                        binding: 0,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Texture {
                            multisampled: false,
                            view_dimension: wgpu::TextureViewDimension::D2,
                            sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        },
                        count: None,
                    },
                    wgpu::BindGroupLayoutEntry {
                        binding: 1,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                        count: None,
                    },
                ],
            });

        // 3. Pipeline Layout
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("Sprite Render Pipeline Layout"),
            bind_group_layouts: &[&camera_bind_group_layout, &texture_bind_group_layout],
            push_constant_ranges: &[],
        });

        // 4. Shader
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Sprite Shader"),
            source: wgpu::ShaderSource::Wgsl(
                include_str!("../../../../assets/shaders/sprite_2d.wgsl").into(),
            ),
        });

        // 5. Build WGPU Pipeline
        let wgpu_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Sprite WGPU Pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                buffers: &[SpriteInstance::descriptor()],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_main"),
                targets: &[Some(wgpu::ColorTargetState {
                    format: output_format,
                    blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleStrip,
                strip_index_format: None,
                front_face: wgpu::FrontFace::Ccw,
                cull_mode: None,
                polygon_mode: wgpu::PolygonMode::Fill,
                unclipped_depth: false,
                conservative: false,
            },
            depth_stencil: depth_format.map(|format| wgpu::DepthStencilState {
                format,
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
            cache,
        });

        Self {
            device,
            wgpu_pipeline,
            camera_bind_group_layout,
            texture_bind_group_layout,
        }
    }
}

/// Render pipeline and bind-group layout for untextured SDF shapes.
pub struct ShapePipeline {
    /// GPU device the pipeline was created on.
    pub device: Arc<wgpu::Device>,
    /// The compiled wgpu render pipeline.
    pub wgpu_pipeline: wgpu::RenderPipeline,
    /// Bind group 0: the shared 2D camera uniform.
    pub camera_bind_group_layout: wgpu::BindGroupLayout,
}

impl ShapePipeline {
    /// Build the shape pipeline for the given output `format` and
    /// `sample_count` (pass `depth_format: None` for the depth-less Pure2D path).
    pub fn new(
        device: Arc<wgpu::Device>,
        output_format: wgpu::TextureFormat,
        depth_format: Option<wgpu::TextureFormat>,
        sample_count: u32,
    ) -> Self {
        Self::new_with_cache(device, output_format, depth_format, sample_count, None)
    }

    /// Same as [`ShapePipeline::new`] but threads a shared [`wgpu::PipelineCache`]
    /// into `create_render_pipeline`.
    pub fn new_with_cache(
        device: Arc<wgpu::Device>,
        output_format: wgpu::TextureFormat,
        depth_format: Option<wgpu::TextureFormat>,
        sample_count: u32,
        cache: Option<&wgpu::PipelineCache>,
    ) -> Self {
        use crate::render::types::ShapeGpu;

        // 1. Camera Layout
        let camera_bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("Shape Camera Bind Group Layout"),
                entries: &[wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                }],
            });

        // 2. Pipeline Layout
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("Shape Render Pipeline Layout"),
            bind_group_layouts: &[&camera_bind_group_layout],
            push_constant_ranges: &[],
        });

        // 3. Shader
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Shape Shader"),
            source: wgpu::ShaderSource::Wgsl(
                include_str!("../../../../assets/shaders/shape_2d.wgsl").into(),
            ),
        });

        // 4. Build WGPU Pipeline
        let wgpu_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Shape WGPU Pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                buffers: &[ShapeGpu::descriptor()],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_main"),
                targets: &[Some(wgpu::ColorTargetState {
                    format: output_format,
                    blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleStrip,
                strip_index_format: None,
                front_face: wgpu::FrontFace::Ccw,
                cull_mode: None,
                polygon_mode: wgpu::PolygonMode::Fill,
                unclipped_depth: false,
                conservative: false,
            },
            depth_stencil: depth_format.map(|format| wgpu::DepthStencilState {
                format,
                depth_write_enabled: false,
                depth_compare: wgpu::CompareFunction::Always,
                stencil: wgpu::StencilState::default(),
                bias: wgpu::DepthBiasState::default(),
            }),
            multisample: wgpu::MultisampleState {
                count: sample_count,
                mask: !0,
                alpha_to_coverage_enabled: false,
            },
            multiview: None,
            cache,
        });

        Self {
            device,
            wgpu_pipeline,
            camera_bind_group_layout,
        }
    }
}

/// Render pipeline and bind-group layouts for MSDF text quads.
///
/// Group 0 is the same 2D camera uniform as the shape/sprite pipelines; group 1
/// is the font atlas texture + sampler.  The shader module is built from
/// `assets/shaders/msdf_common.wgsl` + `assets/shaders/text_2d.wgsl`, so the
/// MSDF decode logic is shared with the GUI text pipeline.
pub struct Text2dPipeline {
    /// GPU device the pipeline was created on.
    pub device: Arc<wgpu::Device>,
    /// The compiled wgpu render pipeline.
    pub wgpu_pipeline: wgpu::RenderPipeline,
    /// Bind group 0: the shared 2D camera uniform.
    pub camera_bind_group_layout: wgpu::BindGroupLayout,
    /// Bind group 1: the font atlas view + sampler.
    pub texture_bind_group_layout: wgpu::BindGroupLayout,
}

impl Text2dPipeline {
    /// Build the text pipeline for the given output `format` and
    /// `sample_count` (pass `depth_format: None` for the depth-less Pure2D path).
    pub fn new(
        device: Arc<wgpu::Device>,
        output_format: wgpu::TextureFormat,
        depth_format: Option<wgpu::TextureFormat>,
        sample_count: u32,
    ) -> Self {
        Self::new_with_cache(device, output_format, depth_format, sample_count, None)
    }

    /// Same as [`Text2dPipeline::new`] but threads a shared
    /// [`wgpu::PipelineCache`] into `create_render_pipeline`.
    pub fn new_with_cache(
        device: Arc<wgpu::Device>,
        output_format: wgpu::TextureFormat,
        depth_format: Option<wgpu::TextureFormat>,
        sample_count: u32,
        cache: Option<&wgpu::PipelineCache>,
    ) -> Self {
        use crate::render::types::TextInstance;

        let camera_bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("Text Camera Bind Group Layout"),
                entries: &[wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                }],
            });

        let texture_bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("Text Texture Bind Group Layout"),
                entries: &[
                    wgpu::BindGroupLayoutEntry {
                        binding: 0,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Texture {
                            multisampled: false,
                            view_dimension: wgpu::TextureViewDimension::D2,
                            sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        },
                        count: None,
                    },
                    wgpu::BindGroupLayoutEntry {
                        binding: 1,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                        count: None,
                    },
                ],
            });

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("Text Render Pipeline Layout"),
            bind_group_layouts: &[&camera_bind_group_layout, &texture_bind_group_layout],
            push_constant_ranges: &[],
        });

        // Prepend the shared MSDF helpers so the GUI and 2D shaders cannot drift
        // apart.
        const SHARED_MSDF: &str = include_str!("../../../../assets/shaders/msdf_common.wgsl");
        const TEXT_2D: &str = include_str!("../../../../assets/shaders/text_2d.wgsl");
        let source = format!("{SHARED_MSDF}\n{TEXT_2D}");
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("2D Text Shader"),
            source: wgpu::ShaderSource::Wgsl(source.into()),
        });

        let wgpu_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("2D Text WGPU Pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                buffers: &[TextInstance::descriptor()],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_main"),
                targets: &[Some(wgpu::ColorTargetState {
                    format: output_format,
                    blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleStrip,
                strip_index_format: None,
                front_face: wgpu::FrontFace::Ccw,
                cull_mode: None,
                polygon_mode: wgpu::PolygonMode::Fill,
                unclipped_depth: false,
                conservative: false,
            },
            depth_stencil: depth_format.map(|format| wgpu::DepthStencilState {
                format,
                depth_write_enabled: false,
                depth_compare: wgpu::CompareFunction::Always,
                stencil: wgpu::StencilState::default(),
                bias: wgpu::DepthBiasState::default(),
            }),
            multisample: wgpu::MultisampleState {
                count: sample_count,
                mask: !0,
                alpha_to_coverage_enabled: false,
            },
            multiview: None,
            cache,
        });

        Self {
            device,
            wgpu_pipeline,
            camera_bind_group_layout,
            texture_bind_group_layout,
        }
    }
}

/// Render pipeline for the procedural 2D grid.
///
/// The grid is a full-screen triangle (no vertex buffers) evaluated in the
/// fragment shader from the inverse view-projection. It is drawn right after
/// the clear and before the retained/immediate geometry, so it stays behind
/// the model.
pub struct GridPipeline {
    pub device: Arc<wgpu::Device>,
    pub wgpu_pipeline: wgpu::RenderPipeline,
    /// Bind group 0: the grid uniform.
    pub camera_bind_group_layout: wgpu::BindGroupLayout,
}

impl GridPipeline {
    pub fn new(
        device: Arc<wgpu::Device>,
        output_format: wgpu::TextureFormat,
        sample_count: u32,
    ) -> Self {
        Self::new_with_cache(device, output_format, sample_count, None)
    }

    pub fn new_with_cache(
        device: Arc<wgpu::Device>,
        output_format: wgpu::TextureFormat,
        sample_count: u32,
        cache: Option<&wgpu::PipelineCache>,
    ) -> Self {
        let camera_bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("Grid Uniform Bind Group Layout"),
                entries: &[wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                }],
            });

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("Grid Render Pipeline Layout"),
            bind_group_layouts: &[&camera_bind_group_layout],
            push_constant_ranges: &[],
        });

        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Grid Shader"),
            source: wgpu::ShaderSource::Wgsl(
                include_str!("../../../../assets/shaders/grid_2d.wgsl").into(),
            ),
        });

        let wgpu_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Grid WGPU Pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                buffers: &[],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_main"),
                targets: &[Some(wgpu::ColorTargetState {
                    format: output_format,
                    blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                strip_index_format: None,
                front_face: wgpu::FrontFace::Ccw,
                cull_mode: None,
                polygon_mode: wgpu::PolygonMode::Fill,
                unclipped_depth: false,
                conservative: false,
            },
            depth_stencil: None,
            multisample: wgpu::MultisampleState {
                count: sample_count,
                mask: !0,
                alpha_to_coverage_enabled: false,
            },
            multiview: None,
            cache,
        });

        Self {
            device,
            wgpu_pipeline,
            camera_bind_group_layout,
        }
    }
}
