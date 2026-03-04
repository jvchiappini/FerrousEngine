/// SSAO Bilateral Blur Pass
///
/// Runs two separable blur passes (horizontal then vertical) over the raw
/// half-resolution SSAO texture.  Samples that differ in depth by more
/// than `depth_thresh` from the centre pixel are excluded so that
/// occlusion doesn't bleed across geometric edges.
///
/// The blurred result is stored in a second `R8Unorm` texture which is
/// then sampled by the PBR pass.
use std::sync::Arc;

use bytemuck::{Pod, Zeroable};
use wgpu::util::DeviceExt;
use wgpu::{
    BindGroupLayout, CommandEncoder, Device, LoadOp, Operations,
    RenderPassColorAttachment, RenderPassDescriptor, RenderPipeline, StoreOp,
};

use crate::passes::ssao_pass::SsaoTexture;
use crate::passes::prepass::NormalDepthTexture;

// ── Blur params (matches ssao_blur.wgsl) ──────────────────────────────────────

#[repr(C)]
#[derive(Copy, Clone, Pod, Zeroable)]
struct BlurParams {
    texel_size:   [f32; 2],
    direction:    u32,       // 0 = horizontal, 1 = vertical
    depth_thresh: f32,
}

// ── Blur pass ─────────────────────────────────────────────────────────────────

pub struct SsaoBlurPass {
    /// Final (blurred) SSAO texture sampled by the PBR pass.
    pub blurred: SsaoTexture,
    /// Intermediate texture for the horizontal pass.
    intermediate: SsaoTexture,

    pipeline: Arc<RenderPipeline>,
    params_layout: Arc<BindGroupLayout>,
    textures_layout: Arc<BindGroupLayout>,

    /// Depth threshold for the bilateral filter.
    pub depth_thresh: f32,
}

impl SsaoBlurPass {
    pub const DEFAULT_DEPTH_THRESH: f32 = 0.1;

    pub fn new(device: &Device, width: u32, height: u32) -> Self {
        let blurred      = SsaoTexture::new(device, width, height);
        let intermediate = SsaoTexture::new(device, width, height);

        let (params_layout, textures_layout, pipeline) = Self::build_pipeline(device);

        Self {
            blurred,
            intermediate,
            pipeline: Arc::new(pipeline),
            params_layout: Arc::new(params_layout),
            textures_layout: Arc::new(textures_layout),
            depth_thresh: Self::DEFAULT_DEPTH_THRESH,
        }
    }

    /// Execute horizontal then vertical blur.
    ///
    /// * `raw_ssao`     — the `SsaoTexture` produced by `SsaoPass`.
    /// * `normal_depth` — the prepass normal-depth texture (for depth gating).
    pub fn run(
        &self,
        device: &Device,
        encoder: &mut CommandEncoder,
        raw_ssao: &SsaoTexture,
        normal_depth: &NormalDepthTexture,
    ) {
        let w = raw_ssao.width  as f32;
        let h = raw_ssao.height as f32;

        // ── Pass 1: horizontal blur → intermediate ────────────────────────────
        let params_h = BlurParams {
            texel_size:   [1.0 / w, 1.0 / h],
            direction:    0,
            depth_thresh: self.depth_thresh,
        };
        self.run_single_pass(
            device,
            encoder,
            &params_h,
            &raw_ssao.view,
            &raw_ssao.sampler,
            normal_depth,
            &self.intermediate.view,
        );

        // ── Pass 2: vertical blur → blurred ───────────────────────────────────
        let params_v = BlurParams {
            texel_size:   [1.0 / w, 1.0 / h],
            direction:    1,
            depth_thresh: self.depth_thresh,
        };
        self.run_single_pass(
            device,
            encoder,
            &params_v,
            &self.intermediate.view,
            &self.intermediate.sampler,
            normal_depth,
            &self.blurred.view,
        );
    }

    /// Resize both internal textures.
    pub fn on_resize(&mut self, device: &Device, width: u32, height: u32) {
        self.blurred.resize(device, width, height);
        self.intermediate.resize(device, width, height);
    }

    // ── Private ───────────────────────────────────────────────────────────────

    fn run_single_pass(
        &self,
        device: &Device,
        encoder: &mut CommandEncoder,
        params: &BlurParams,
        src_view: &wgpu::TextureView,
        src_sampler: &wgpu::Sampler,
        normal_depth: &NormalDepthTexture,
        dst_view: &wgpu::TextureView,
    ) {
        let params_buf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Blur Params Buffer"),
            contents: bytemuck::bytes_of(params),
            usage: wgpu::BufferUsages::UNIFORM,
        });

        let bg0 = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Blur BG0 (params)"),
            layout: &self.params_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: params_buf.as_entire_binding(),
            }],
        });

        let bg1 = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Blur BG1 (textures)"),
            layout: &self.textures_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(src_view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(src_sampler),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::TextureView(&normal_depth.view),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: wgpu::BindingResource::Sampler(&normal_depth.sampler),
                },
            ],
        });

        let mut rpass = encoder.begin_render_pass(&RenderPassDescriptor {
            label: Some("SSAO Blur Pass"),
            color_attachments: &[Some(RenderPassColorAttachment {
                view: dst_view,
                resolve_target: None,
                ops: Operations {
                    load: LoadOp::Clear(wgpu::Color::WHITE),
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

    fn build_pipeline(device: &Device) -> (BindGroupLayout, BindGroupLayout, RenderPipeline) {
        let params_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("Blur Params BGL"),
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

        let textures_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("Blur Textures BGL"),
            entries: &[
                // binding 0: raw SSAO texture
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
                // binding 1: sampler
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
                // binding 2: normal-depth texture
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        multisampled: false,
                        view_dimension: wgpu::TextureViewDimension::D2,
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                    },
                    count: None,
                },
                // binding 3: normal-depth sampler
                wgpu::BindGroupLayoutEntry {
                    binding: 3,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });

        let shader = device.create_shader_module(
            wgpu::include_wgsl!("../../../../assets/shaders/ssao_blur.wgsl")
        );

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("Blur Pipeline Layout"),
            bind_group_layouts: &[&params_layout, &textures_layout],
            push_constant_ranges: &[],
        });

        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("SSAO Blur Pipeline"),
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
