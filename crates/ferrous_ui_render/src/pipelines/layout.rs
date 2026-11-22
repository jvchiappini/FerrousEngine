use crate::MAX_TEXTURE_SLOTS;
use std::num::NonZeroU64;
use wgpu::{
    BindGroupLayout, BindGroupLayoutDescriptor, BindGroupLayoutEntry, BindingType,
    BufferBindingType, PipelineLayout, PipelineLayoutDescriptor, SamplerBindingType, ShaderStages,
    TextureSampleType, TextureViewDimension,
};

pub struct Layouts {
    pub uniform_layout: BindGroupLayout,
    pub font_layout: BindGroupLayout,
    pub image_layout: BindGroupLayout,
    /// Per-segment rounded-clip uniform plus the blurred backdrop texture.
    pub segment_layout: BindGroupLayout,
    /// Fullscreen backdrop-blur pass: source texture, sampler and parameters.
    pub blur_layout: BindGroupLayout,
    pub quad_pipeline_layout: PipelineLayout,
    pub text_pipeline_layout: PipelineLayout,
    pub blur_pipeline_layout: PipelineLayout,
}

impl Layouts {
    pub fn new(device: &wgpu::Device) -> Self {
        let uniform_layout = device.create_bind_group_layout(&BindGroupLayoutDescriptor {
            label: Some("GUI Uniform Bind Group Layout"),
            entries: &[BindGroupLayoutEntry {
                binding: 0,
                // The backdrop branch samples `uniforms.resolution` in the
                // fragment stage to map the framebuffer pixel to the blurred
                // texture's UV space.
                visibility: ShaderStages::VERTEX | ShaderStages::FRAGMENT,
                ty: BindingType::Buffer {
                    ty: BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: NonZeroU64::new(8), // [f32; 2] resolution
                },
                count: None,
            }],
        });

        let font_layout = device.create_bind_group_layout(&BindGroupLayoutDescriptor {
            label: Some("GUI Font Bind Group Layout"),
            entries: &[
                BindGroupLayoutEntry {
                    binding: 0,
                    visibility: ShaderStages::FRAGMENT,
                    ty: BindingType::Texture {
                        sample_type: TextureSampleType::Float { filterable: true },
                        view_dimension: TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                BindGroupLayoutEntry {
                    binding: 1,
                    visibility: ShaderStages::FRAGMENT,
                    ty: BindingType::Sampler(SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });

        #[cfg(not(target_arch = "wasm32"))]
        let image_layout = device.create_bind_group_layout(&BindGroupLayoutDescriptor {
            label: Some("GUI Image Bind Group Layout"),
            entries: &[
                BindGroupLayoutEntry {
                    binding: 0,
                    visibility: ShaderStages::FRAGMENT,
                    ty: BindingType::Texture {
                        sample_type: TextureSampleType::Float { filterable: true },
                        view_dimension: TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: std::num::NonZeroU32::new(MAX_TEXTURE_SLOTS),
                },
                BindGroupLayoutEntry {
                    binding: 1,
                    visibility: ShaderStages::FRAGMENT,
                    ty: BindingType::Sampler(SamplerBindingType::Filtering),
                    count: std::num::NonZeroU32::new(MAX_TEXTURE_SLOTS),
                },
            ],
        });

        #[cfg(target_arch = "wasm32")]
        let image_layout = {
            let mut entries = Vec::new();
            // We use 8 slots on Web for better compatibility with binding limits (max 16 textures per stage usually).
            for i in 0..8 {
                entries.push(BindGroupLayoutEntry {
                    binding: (i * 2) as u32,
                    visibility: ShaderStages::FRAGMENT,
                    ty: BindingType::Texture {
                        sample_type: TextureSampleType::Float { filterable: true },
                        view_dimension: TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                });
                entries.push(BindGroupLayoutEntry {
                    binding: (i * 2 + 1) as u32,
                    visibility: ShaderStages::FRAGMENT,
                    ty: BindingType::Sampler(SamplerBindingType::Filtering),
                    count: None,
                });
            }
            device.create_bind_group_layout(&BindGroupLayoutDescriptor {
                label: Some("GUI Image Bind Group Layout (Web)"),
                entries: &entries,
            })
        };

        // Segment clip + backdrop: binding 0 is a dynamically-offset uniform
        // (48 bytes: clip rect, radii, params), binding 1 the blurred backdrop
        // color and binding 2 its sampler.
        let segment_layout = device.create_bind_group_layout(&BindGroupLayoutDescriptor {
            label: Some("GUI Segment Bind Group Layout"),
            entries: &[
                BindGroupLayoutEntry {
                    binding: 0,
                    visibility: ShaderStages::FRAGMENT,
                    ty: BindingType::Buffer {
                        ty: BufferBindingType::Uniform,
                        has_dynamic_offset: true,
                        min_binding_size: NonZeroU64::new(48),
                    },
                    count: None,
                },
                BindGroupLayoutEntry {
                    binding: 1,
                    visibility: ShaderStages::FRAGMENT,
                    ty: BindingType::Texture {
                        sample_type: TextureSampleType::Float { filterable: true },
                        view_dimension: TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                BindGroupLayoutEntry {
                    binding: 2,
                    visibility: ShaderStages::FRAGMENT,
                    ty: BindingType::Sampler(SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });

        let blur_layout = device.create_bind_group_layout(&BindGroupLayoutDescriptor {
            label: Some("GUI Backdrop Blur Bind Group Layout"),
            entries: &[
                BindGroupLayoutEntry {
                    binding: 0,
                    visibility: ShaderStages::FRAGMENT,
                    ty: BindingType::Texture {
                        sample_type: TextureSampleType::Float { filterable: true },
                        view_dimension: TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                BindGroupLayoutEntry {
                    binding: 1,
                    visibility: ShaderStages::FRAGMENT,
                    ty: BindingType::Sampler(SamplerBindingType::Filtering),
                    count: None,
                },
                BindGroupLayoutEntry {
                    binding: 2,
                    visibility: ShaderStages::FRAGMENT,
                    ty: BindingType::Buffer {
                        ty: BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: NonZeroU64::new(16),
                    },
                    count: None,
                },
            ],
        });

        let quad_pipeline_layout = device.create_pipeline_layout(&PipelineLayoutDescriptor {
            label: Some("GUI Quad Pipeline Layout"),
            bind_group_layouts: &[&uniform_layout, &image_layout, &segment_layout],
            push_constant_ranges: &[],
        });

        let text_pipeline_layout = device.create_pipeline_layout(&PipelineLayoutDescriptor {
            label: Some("GUI Text Pipeline Layout"),
            bind_group_layouts: &[&uniform_layout, &font_layout, &segment_layout],
            push_constant_ranges: &[],
        });

        let blur_pipeline_layout = device.create_pipeline_layout(&PipelineLayoutDescriptor {
            label: Some("GUI Backdrop Blur Pipeline Layout"),
            bind_group_layouts: &[&blur_layout],
            push_constant_ranges: &[],
        });

        Self {
            uniform_layout,
            font_layout,
            image_layout,
            segment_layout,
            blur_layout,
            quad_pipeline_layout,
            text_pipeline_layout,
            blur_pipeline_layout,
        }
    }
}
