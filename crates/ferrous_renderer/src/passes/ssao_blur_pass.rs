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
    BindGroupLayout, CommandEncoder, Device, ComputePipeline,
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

    pipeline: Arc<ComputePipeline>,
    params_layout: Arc<BindGroupLayout>,
    textures_layout: Arc<BindGroupLayout>,

    /// Depth threshold for the bilateral filter.
    pub depth_thresh: f32,
}

impl SsaoBlurPass {
    pub const DEFAULT_DEPTH_THRESH: f32 = 0.2;

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
            normal_depth,
            &self.intermediate.view,
            &self.intermediate.texture,
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
            normal_depth,
            &self.blurred.view,
            &self.blurred.texture,
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
        normal_depth: &NormalDepthTexture,
        dst_view: &wgpu::TextureView,
        dst_texture: &wgpu::Texture,
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

        // Layout: binding 0 = ssao_tex (non-filterable, textureLoad),
        //         binding 1 = nd_tex (filterable), binding 2 = nd_sampler,
        //         binding 3 = out_tex (storage write)
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
                    resource: wgpu::BindingResource::TextureView(&normal_depth.view),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::Sampler(&normal_depth.sampler),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: wgpu::BindingResource::TextureView(dst_view),
                },
            ],
        });

        let mut cpass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
            label: Some("SSAO Blur Compute Pass"),
            timestamp_writes: None,
        });

        cpass.set_pipeline(&self.pipeline);
        cpass.set_bind_group(0, &bg0, &[]);
        cpass.set_bind_group(1, &bg1, &[]);
        
        let w = dst_texture.width();
        let h = dst_texture.height();
        let x = (w + 7) / 8;
        let y = (h + 7) / 8;
        cpass.dispatch_workgroups(x, y, 1);
    }

    fn build_pipeline(device: &Device) -> (BindGroupLayout, BindGroupLayout, ComputePipeline) {
        let params_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("Blur Params BGL"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::COMPUTE,
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
                // binding 0: raw SSAO texture (R32Float, non-filterable → textureLoad only)
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Texture {
                        multisampled: false,
                        view_dimension: wgpu::TextureViewDimension::D2,
                        sample_type: wgpu::TextureSampleType::Float { filterable: false },
                    },
                    count: None,
                },
                // binding 1: normal-depth texture (filterable, used with textureSampleLevel)
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Texture {
                        multisampled: false,
                        view_dimension: wgpu::TextureViewDimension::D2,
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                    },
                    count: None,
                },
                // binding 2: normal-depth sampler (filtering)
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
                // binding 3: output storage texture
                wgpu::BindGroupLayoutEntry {
                    binding: 3,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::StorageTexture {
                        access: wgpu::StorageTextureAccess::WriteOnly,
                        format: SsaoTexture::FORMAT,
                        view_dimension: wgpu::TextureViewDimension::D2,
                    },
                    count: None,
                },
            ],
        });

        let shader = device.create_shader_module(
            wgpu::include_wgsl!("../../../../assets/shaders/ssao_blur_compute.wgsl")
        );

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("Blur Pipeline Layout"),
            bind_group_layouts: &[&params_layout, &textures_layout],
            push_constant_ranges: &[],
        });

        let pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some("SSAO Blur Compute Pipeline"),
            layout: Some(&pipeline_layout),
            module: &shader,
            entry_point: Some("main"),
            compilation_options: wgpu::PipelineCompilationOptions::default(),
            cache: None,
        });

        (params_layout, textures_layout, pipeline)
    }
}
