/// SSAO Pass
///
/// Runs at **half resolution** to save GPU time.  Reads the
/// `normal_depth_texture` from the prepass and the SSAO kernel + noise
/// resources to produce a single-channel (R8Unorm) occlusion texture.
///
/// The raw output is intentionally noisy; it is smoothed by the
/// [`SsaoBlurPass`] that executes immediately after.
use std::sync::Arc;

use wgpu::{
    BindGroupLayout, CommandEncoder, Device, LoadOp, Operations,
    RenderPassColorAttachment, RenderPassDescriptor, RenderPipeline, StoreOp,
};

use crate::passes::prepass::NormalDepthTexture;
use crate::resources::ssao::SsaoResources;
use crate::resources::texture::{self, RenderTextureDesc};

// ── SSAO occlusion texture ───────────────────────────────────────────────────

/// Half-resolution `R8Unorm` render target that stores raw per-pixel
/// ambient occlusion (0 = fully occluded, 1 = fully lit).
pub struct SsaoTexture {
    pub texture: wgpu::Texture,
    pub view: wgpu::TextureView,
    pub sampler: wgpu::Sampler,
    pub width: u32,
    pub height: u32,
}

impl SsaoTexture {
    pub const FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::R8Unorm;

    pub fn new(device: &Device, width: u32, height: u32) -> Self {
        // Half resolution
        let w = (width / 2).max(1);
        let h = (height / 2).max(1);

        let texture = texture::create_render_texture(device, &RenderTextureDesc {
            label: "SSAO Texture",
            width: w,
            height: h,
            format: Self::FORMAT,
            sample_count: 1,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
        });
        let view    = texture::default_view(&texture);
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("SSAO Sampler"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::FilterMode::Nearest,
            ..Default::default()
        });
        Self { texture, view, sampler, width: w, height: h }
    }

    pub fn resize(&mut self, device: &Device, full_width: u32, full_height: u32) {
        let w = (full_width / 2).max(1);
        let h = (full_height / 2).max(1);
        if self.width == w && self.height == h { return; }
        *self = Self::new(device, full_width, full_height);
    }
}

// ── SSAO pass ─────────────────────────────────────────────────────────────────

pub struct SsaoPass {
    pub ssao_texture: SsaoTexture,
    pipeline: Arc<RenderPipeline>,

    /// BGL for group 0 (params + kernel uniform buffers).
    params_layout: Arc<BindGroupLayout>,
    /// BGL for group 1 (normal-depth + noise textures).
    textures_layout: Arc<BindGroupLayout>,
}

impl SsaoPass {
    pub fn new(device: &Device, width: u32, height: u32) -> Self {
        let ssao_texture = SsaoTexture::new(device, width, height);

        let (params_layout, textures_layout, pipeline) = Self::build_pipeline(device);

        Self {
            ssao_texture,
            pipeline: Arc::new(pipeline),
            params_layout: Arc::new(params_layout),
            textures_layout: Arc::new(textures_layout),
        }
    }

    /// Execute the SSAO pass.  Requires references to the prepass normal-depth
    /// texture and the SSAO CPU resources.
    pub fn run(
        &self,
        device: &Device,
        encoder: &mut CommandEncoder,
        ssao_res: &SsaoResources,
        normal_depth: &NormalDepthTexture,
    ) {
        // Group 0: params + kernel
        let bg0 = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("SSAO BG0 (params/kernel)"),
            layout: &self.params_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: ssao_res.params_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: ssao_res.kernel_buffer.as_entire_binding(),
                },
            ],
        });

        // Group 1: normal-depth + noise textures
        let bg1 = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("SSAO BG1 (textures)"),
            layout: &self.textures_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&normal_depth.view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&normal_depth.sampler),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::TextureView(&ssao_res.noise_view),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: wgpu::BindingResource::Sampler(&ssao_res.noise_sampler),
                },
            ],
        });

        let mut rpass = encoder.begin_render_pass(&RenderPassDescriptor {
            label: Some("SSAO Pass"),
            color_attachments: &[Some(RenderPassColorAttachment {
                view: &self.ssao_texture.view,
                resolve_target: None,
                ops: Operations {
                    load: LoadOp::Clear(wgpu::Color { r: 1.0, g: 0.0, b: 0.0, a: 1.0 }),
                    store: StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            occlusion_query_set: None,
            timestamp_writes: None,
        });

        rpass.set_pipeline(&self.pipeline);
        rpass.set_bind_group(0, &bg0, &[]);
        rpass.set_bind_group(1, &bg1, &[]);
        rpass.draw(0..3, 0..1);
    }

    // ── Private ───────────────────────────────────────────────────────────────

    fn build_pipeline(device: &Device) -> (BindGroupLayout, BindGroupLayout, RenderPipeline) {
        let params_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("SSAO Params BGL"),
            entries: &[
                // binding 0: SsaoParams uniform
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                // binding 1: SsaoKernel uniform
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
            ],
        });

        let textures_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("SSAO Textures BGL"),
            entries: &[
                // binding 0: normal-depth texture
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
                // binding 1: normal-depth sampler
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
                // binding 2: noise texture
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        multisampled: false,
                        view_dimension: wgpu::TextureViewDimension::D2,
                        sample_type: wgpu::TextureSampleType::Float { filterable: false },
                    },
                    count: None,
                },
                // binding 3: noise sampler
                wgpu::BindGroupLayoutEntry {
                    binding: 3,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::NonFiltering),
                    count: None,
                },
            ],
        });

        let shader = device.create_shader_module(
            wgpu::include_wgsl!("../../../../assets/shaders/ssao.wgsl")
        );

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("SSAO Pipeline Layout"),
            bind_group_layouts: &[&params_layout, &textures_layout],
            push_constant_ranges: &[],
        });

        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("SSAO Pipeline"),
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
                    format: SsaoTexture::FORMAT,
                    blend: None,
                    write_mask: wgpu::ColorWrites::RED,
                })],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                ..Default::default()
            },
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview: None,
            cache: None,
        });

        (params_layout, textures_layout, pipeline)
    }
}

// The SSAO pass doesn't implement the generic `RenderPass` trait directly
// because it requires extra arguments (the prepass and resource handles).
// The renderer calls `SsaoPass::run` directly in `do_render` instead.
// We still provide a stub for on_resize so the renderer can forward it.
impl SsaoPass {
    pub fn on_resize(&mut self, device: &Device, width: u32, height: u32) {
        self.ssao_texture.resize(device, width, height);
    }
}
