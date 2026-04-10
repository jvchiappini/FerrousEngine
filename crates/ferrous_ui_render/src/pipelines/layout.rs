use std::num::NonZeroU64;
use wgpu::{BindGroupLayout, BindGroupLayoutDescriptor, BindGroupLayoutEntry, PipelineLayout, PipelineLayoutDescriptor, ShaderStages, BindingType, BufferBindingType, TextureSampleType, TextureViewDimension, SamplerBindingType};
use crate::MAX_TEXTURE_SLOTS;

pub struct Layouts {
    pub uniform_layout: BindGroupLayout,
    pub font_layout: BindGroupLayout,
    pub image_layout: BindGroupLayout,
    pub quad_pipeline_layout: PipelineLayout,
    pub text_pipeline_layout: PipelineLayout,
}

impl Layouts {
    pub fn new(device: &wgpu::Device) -> Self {
        let uniform_layout = device.create_bind_group_layout(&BindGroupLayoutDescriptor {
            label: Some("GUI Uniform Bind Group Layout"),
            entries: &[BindGroupLayoutEntry {
                binding: 0,
                visibility: ShaderStages::VERTEX,
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

        let quad_pipeline_layout = device.create_pipeline_layout(&PipelineLayoutDescriptor {
            label: Some("GUI Quad Pipeline Layout"),
            bind_group_layouts: &[&uniform_layout, &image_layout],
            push_constant_ranges: &[],
        });

        let text_pipeline_layout = device.create_pipeline_layout(&PipelineLayoutDescriptor {
            label: Some("GUI Text Pipeline Layout"),
            bind_group_layouts: &[&uniform_layout, &font_layout],
            push_constant_ranges: &[],
        });

        Self {
            uniform_layout,
            font_layout,
            image_layout,
            quad_pipeline_layout,
            text_pipeline_layout,
        }
    }
}
