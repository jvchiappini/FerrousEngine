//! Antialiasing Post-Process Pass — Ferrous Engine
//!
//! Provides three configurable antialiasing modes that operate on the final
//! HDR texture **before** tone-mapping:
//!
//! | Mode   | Quality | GPU Cost | Notes                                     |
//! |--------|---------|----------|-------------------------------------------|
//! | `None` | —       | 0        | Passthrough; downstream reads hdr directly|
//! | `Fxaa` | Good    | Very low | Single-pass NVIDIA FXAA 3.11              |
//! | `Smaa` | Best    | Low-Med  | 3-pass SMAA 1x                            |
//!
//! ## Integration
//!
//! Call [`AntialiasingPass::run_aa`] between the Gizmo pass and the
//! Post-Process (tone-map) pass.  Then query [`AntialiasingPass::output`] to
//! get the `TextureView` that post-process should read.

use std::sync::Arc;
use wgpu::{
    util::DeviceExt as _,
    AddressMode, BindGroup, BindGroupDescriptor, BindGroupEntry, BindGroupLayout,
    BindGroupLayoutDescriptor, BindGroupLayoutEntry, BindingResource, BindingType,
    Buffer, BufferBindingType, BufferUsages, Color, ColorTargetState, ColorWrites,
    CommandEncoder, Device, Extent3d, FilterMode, FragmentState, LoadOp, MultisampleState,
    Operations, PipelineCompilationOptions, PipelineLayoutDescriptor, PrimitiveState,
    Queue, RenderPassColorAttachment, RenderPassDescriptor, RenderPipeline,
    RenderPipelineDescriptor, Sampler, SamplerBindingType, SamplerDescriptor, ShaderStages,
    StoreOp, Texture, TextureDescriptor, TextureDimension, TextureFormat,
    TextureSampleType, TextureUsages, TextureView, TextureViewDescriptor,
    TextureViewDimension, VertexState,
};

use crate::render_target::HdrTexture;

// ── Public API types ─────────────────────────────────────────────────────────

/// Which anti-aliasing algorithm to apply each frame.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum AntialiasingMode {
    /// Disabled — no extra GPU work; downstream reads the raw HDR image.
    None,
    /// FXAA 3.11 (Fast Approximate Anti-Aliasing) — single post-process pass.
    Fxaa(FxaaParams),
    /// SMAA 1x (Sub-pixel Morphological AA) — three sub-passes, sharper than FXAA.
    Smaa,
}

impl Default for AntialiasingMode {
    fn default() -> Self {
        Self::Fxaa(FxaaParams::default())
    }
}

/// Quality parameters for [`AntialiasingMode::Fxaa`].
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FxaaParams {
    /// Minimum contrast to consider a pixel an edge.
    /// Typical range: `0.0312` (ultra) – `0.125` (console).
    pub edge_threshold: f32,
    /// Low-luma absolute cutoff below which no sharpening occurs.
    /// Typical range: `0.0312` – `0.0833`.
    pub edge_threshold_min: f32,
    /// Sub-pixel aliasing removal strength.  `0.75` is the default.
    pub subpix_quality: f32,
}

impl Default for FxaaParams {
    fn default() -> Self {
        Self {
            edge_threshold: 0.0312,
            edge_threshold_min: 0.0833,
            subpix_quality: 0.75,
        }
    }
}

// ── GPU uniform layout (must match AaParams in antialiasing.wgsl) ────────────

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct AaParamsGpu {
    resolution_x: f32,
    resolution_y: f32,
    fxaa_edge_threshold: f32,
    fxaa_edge_threshold_min: f32,
    fxaa_subpix: f32,
    _pad: [f32; 3],
}

// ── Internal texture wrapper ─────────────────────────────────────────────────

struct AaTex {
    #[allow(dead_code)]
    texture: Texture,
    view: TextureView,
    sampler: Sampler,
    width: u32,
    height: u32,
}

impl AaTex {
    fn new(device: &Device, w: u32, h: u32, fmt: TextureFormat, label: &str) -> Self {
        let texture = device.create_texture(&TextureDescriptor {
            label: Some(label),
            size: Extent3d { width: w, height: h, depth_or_array_layers: 1 },
            mip_level_count: 1,
            sample_count: 1,
            dimension: TextureDimension::D2,
            format: fmt,
            usage: TextureUsages::RENDER_ATTACHMENT | TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });
        let view = texture.create_view(&TextureViewDescriptor::default());
        let sampler = device.create_sampler(&SamplerDescriptor {
            label: Some(&format!("{} Sampler", label)),
            address_mode_u: AddressMode::ClampToEdge,
            address_mode_v: AddressMode::ClampToEdge,
            address_mode_w: AddressMode::ClampToEdge,
            mag_filter: FilterMode::Linear,
            min_filter: FilterMode::Linear,
            ..Default::default()
        });
        Self { texture, view, sampler, width: w, height: h }
    }

    fn resize(&mut self, device: &Device, w: u32, h: u32, fmt: TextureFormat, label: &str) {
        if self.width == w && self.height == h { return; }
        *self = Self::new(device, w, h, fmt, label);
    }
}

// ── AntialiasingPass ─────────────────────────────────────────────────────────

/// Post-process antialiasing pass.  Sits between Gizmo and Post-Process passes.
pub struct AntialiasingPass {
    /// Current active mode.  Change with [`set_mode`].
    pub mode: AntialiasingMode,

    // GPU param buffer & layouts
    params_buf: Buffer,
    params_bgl: Arc<BindGroupLayout>,
    input_bgl:  Arc<BindGroupLayout>,

    // Compiled pipelines
    fxaa_pipeline:       Option<Arc<RenderPipeline>>,
    smaa_edge_pipeline:  Option<Arc<RenderPipeline>>,
    smaa_blend_pipeline: Option<Arc<RenderPipeline>>,
    smaa_final_pipeline: Option<Arc<RenderPipeline>>,

    // Managed textures (allocated on first on_resize)
    aa_out:     Option<AaTex>,   // RGBA16Float — final AA colour
    smaa_edge:  Option<AaTex>,   // Rgba8Unorm  — edge flags
    smaa_blend: Option<AaTex>,   // Rgba8Unorm  — blend weights
    dummy:      Option<AaTex>,   // 1×1 RGBA16Float used as "no aux" slot

    hdr_format: TextureFormat,
}

impl AntialiasingPass {
    // -- Construction ----------------------------------------------------------

    pub fn new(device: &Device) -> Self {
        let params_bgl = Arc::new(device.create_bind_group_layout(&BindGroupLayoutDescriptor {
            label: Some("AA Params BGL"),
            entries: &[BindGroupLayoutEntry {
                binding: 0,
                visibility: ShaderStages::FRAGMENT,
                ty: BindingType::Buffer {
                    ty: BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            }],
        }));

        let tex_entry = |binding: u32| BindGroupLayoutEntry {
            binding,
            visibility: ShaderStages::FRAGMENT,
            ty: BindingType::Texture {
                multisampled: false,
                view_dimension: TextureViewDimension::D2,
                sample_type: TextureSampleType::Float { filterable: true },
            },
            count: None,
        };
        let samp_entry = |binding: u32| BindGroupLayoutEntry {
            binding,
            visibility: ShaderStages::FRAGMENT,
            ty: BindingType::Sampler(SamplerBindingType::Filtering),
            count: None,
        };

        let input_bgl = Arc::new(device.create_bind_group_layout(&BindGroupLayoutDescriptor {
            label: Some("AA Input BGL"),
            entries: &[tex_entry(0), samp_entry(1), tex_entry(2), samp_entry(3)],
        }));

        let params_buf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("AA Params Buf"),
            contents: bytemuck::bytes_of(&AaParamsGpu::default_hd()),
            usage: BufferUsages::UNIFORM | BufferUsages::COPY_DST,
        });

        Self {
            mode: AntialiasingMode::default(),
            params_buf,
            params_bgl,
            input_bgl,
            fxaa_pipeline:       None,
            smaa_edge_pipeline:  None,
            smaa_blend_pipeline: None,
            smaa_final_pipeline: None,
            aa_out:     None,
            smaa_edge:  None,
            smaa_blend: None,
            dummy:      None,
            hdr_format: HdrTexture::FORMAT,
        }
    }

    // -- Lifecycle -------------------------------------------------------------

    /// Compile GPU pipelines.  Must be called once before the first frame.
    pub fn on_attach(&mut self, device: &Device, hdr_format: TextureFormat) {
        self.hdr_format = hdr_format;
        self.build_all_pipelines(device);
    }

    /// Reallocate intermediate textures after a window resize.
    pub fn on_resize(&mut self, device: &Device, w: u32, h: u32) {
        let hdr = self.hdr_format;
        let r8  = TextureFormat::Rgba8Unorm;

        Self::maybe_resize(&mut self.aa_out,    device, w, h, hdr, "AA Output");
        Self::maybe_resize(&mut self.smaa_edge, device, w, h, r8,  "SMAA Edge");
        Self::maybe_resize(&mut self.smaa_blend,device, w, h, r8,  "SMAA Blend");

        if self.dummy.is_none() {
            self.dummy = Some(AaTex::new(device, 1, 1, hdr, "AA Dummy"));
        }
    }

    fn maybe_resize(slot: &mut Option<AaTex>, device: &Device, w: u32, h: u32, fmt: TextureFormat, label: &str) {
        if let Some(t) = slot {
            t.resize(device, w, h, fmt, label);
        } else {
            *slot = Some(AaTex::new(device, w, h, fmt, label));
        }
    }

    // -- Runtime API -----------------------------------------------------------

    /// Change the antialiasing mode at runtime (no recompile needed).
    pub fn set_mode(&mut self, mode: AntialiasingMode) {
        self.mode = mode;
    }

    /// Upload the per-frame uniform (resolution + FXAA params) to the GPU.
    pub fn update_params(&self, queue: &Queue, w: u32, h: u32) {
        let (et, etm, sub) = match &self.mode {
            AntialiasingMode::Fxaa(p) => (p.edge_threshold, p.edge_threshold_min, p.subpix_quality),
            _ => (0.0312, 0.0833, 0.75),
        };
        let gpu = AaParamsGpu {
            resolution_x:        w as f32,
            resolution_y:        h as f32,
            fxaa_edge_threshold:     et,
            fxaa_edge_threshold_min: etm,
            fxaa_subpix:             sub,
            _pad: [0.0; 3],
        };
        queue.write_buffer(&self.params_buf, 0, bytemuck::bytes_of(&gpu));
    }

    /// Execute the anti-aliasing sub-passes for this frame.
    ///
    /// Must be called **after** [`update_params`] and **after** the Gizmo pass
    /// has finished writing to `hdr`.
    pub fn run_aa(&self, device: &Device, encoder: &mut CommandEncoder, hdr: &HdrTexture) {
        let dummy = match &self.dummy { Some(d) => d, None => return };

        match &self.mode {
            AntialiasingMode::None => { /* downstream reads hdr.view directly */ }

            AntialiasingMode::Fxaa(_) => {
                let pipeline = match &self.fxaa_pipeline { Some(p) => p, None => return };
                let aa_out   = match &self.aa_out         { Some(t) => t, None => return };
                let bg = self.input_bg(device, &hdr.view, &hdr.sampler, &dummy.view, &dummy.sampler);
                self.blit(device, encoder, pipeline, &bg, &aa_out.view, true);
            }

            AntialiasingMode::Smaa => {
                let aa_out    = match &self.aa_out     { Some(t) => t, None => return };
                let edge_tex  = match &self.smaa_edge  { Some(t) => t, None => return };
                let blend_tex = match &self.smaa_blend { Some(t) => t, None => return };
                let ep = match &self.smaa_edge_pipeline  { Some(p) => p, None => return };
                let bp = match &self.smaa_blend_pipeline { Some(p) => p, None => return };
                let fp = match &self.smaa_final_pipeline { Some(p) => p, None => return };

                // Pass 1: edge detection
                let bg1 = self.input_bg(device, &hdr.view, &hdr.sampler, &dummy.view, &dummy.sampler);
                self.blit(device, encoder, ep, &bg1, &edge_tex.view, true);

                // Pass 2: blend weights
                let bg2 = self.input_bg(device, &edge_tex.view, &edge_tex.sampler, &dummy.view, &dummy.sampler);
                self.blit(device, encoder, bp, &bg2, &blend_tex.view, true);

                // Pass 3: neighbourhood blending
                let bg3 = self.input_bg(device, &hdr.view, &hdr.sampler, &blend_tex.view, &blend_tex.sampler);
                self.blit(device, encoder, fp, &bg3, &aa_out.view, true);
            }
        }
    }

    /// The `TextureView` that the Post-Process (tone-map) pass should read.
    ///
    /// * If mode is `None`, returns the raw `hdr.view` (zero-copy passthrough).
    /// * Otherwise returns the AA output texture view.
    pub fn output<'a>(&'a self, hdr: &'a HdrTexture) -> &'a TextureView {
        match &self.mode {
            AntialiasingMode::None => &hdr.view,
            _ => self.aa_out.as_ref().map(|t| &t.view).unwrap_or(&hdr.view),
        }
    }

    /// The `Sampler` that should be used when binding the output.
    pub fn output_sampler<'a>(&'a self, hdr: &'a HdrTexture) -> &'a Sampler {
        match &self.mode {
            AntialiasingMode::None => &hdr.sampler,
            _ => self.aa_out.as_ref().map(|t| &t.sampler).unwrap_or(&hdr.sampler),
        }
    }

    // -- Private ---------------------------------------------------------------

    fn input_bg(
        &self,
        device: &Device,
        cv: &TextureView, cs: &Sampler,
        av: &TextureView, as_: &Sampler,
    ) -> BindGroup {
        device.create_bind_group(&BindGroupDescriptor {
            label: Some("AA Input BG"),
            layout: &self.input_bgl,
            entries: &[
                BindGroupEntry { binding: 0, resource: BindingResource::TextureView(cv) },
                BindGroupEntry { binding: 1, resource: BindingResource::Sampler(cs)     },
                BindGroupEntry { binding: 2, resource: BindingResource::TextureView(av) },
                BindGroupEntry { binding: 3, resource: BindingResource::Sampler(as_)   },
            ],
        })
    }

    fn params_bg(&self, device: &Device) -> BindGroup {
        device.create_bind_group(&BindGroupDescriptor {
            label: Some("AA Params BG"),
            layout: &self.params_bgl,
            entries: &[BindGroupEntry { binding: 0, resource: self.params_buf.as_entire_binding() }],
        })
    }

    fn blit(
        &self,
        device: &Device,
        encoder: &mut CommandEncoder,
        pipeline: &RenderPipeline,
        input_bg: &BindGroup,
        output_view: &TextureView,
        clear: bool,
    ) {
        let pbg  = self.params_bg(device);
        let load = if clear { LoadOp::Clear(Color::BLACK) } else { LoadOp::Load };

        let mut rpass = encoder.begin_render_pass(&RenderPassDescriptor {
            label: Some("AA Blit Pass"),
            color_attachments: &[Some(RenderPassColorAttachment {
                view: output_view,
                resolve_target: None,
                ops: Operations { load, store: StoreOp::Store },
            })],
            depth_stencil_attachment: None,
            occlusion_query_set: None,
            timestamp_writes: None,
        });
        rpass.set_pipeline(pipeline);
        rpass.set_bind_group(0, input_bg, &[]);
        rpass.set_bind_group(1, &pbg, &[]);
        rpass.draw(0..3, 0..1);
    }

    fn build_all_pipelines(&mut self, device: &Device) {
        let shader = device.create_shader_module(wgpu::include_wgsl!(
            "../../../../assets/shaders/antialiasing.wgsl"
        ));
        let layout = device.create_pipeline_layout(&PipelineLayoutDescriptor {
            label: Some("AA Pipeline Layout"),
            bind_group_layouts: &[&self.input_bgl, &self.params_bgl],
            push_constant_ranges: &[],
        });

        let hdr  = self.hdr_format;
        let r8   = TextureFormat::Rgba8Unorm;

        let make = |entry: &str, fmt: TextureFormat| -> RenderPipeline {
            device.create_render_pipeline(&RenderPipelineDescriptor {
                label:  Some(&format!("AA: {}", entry)),
                layout: Some(&layout),
                vertex: VertexState {
                    module: &shader,
                    entry_point: Some("vs_main"),
                    buffers: &[],
                    compilation_options: PipelineCompilationOptions::default(),
                },
                fragment: Some(FragmentState {
                    module: &shader,
                    entry_point: Some(entry),
                    targets: &[Some(ColorTargetState {
                        format: fmt,
                        blend: None,
                        write_mask: ColorWrites::ALL,
                    })],
                    compilation_options: PipelineCompilationOptions::default(),
                }),
                primitive:    PrimitiveState::default(),
                depth_stencil: None,
                multisample:  MultisampleState::default(),
                multiview:    None,
                cache:        None,
            })
        };

        self.fxaa_pipeline       = Some(Arc::new(make("fs_fxaa",        hdr)));
        self.smaa_edge_pipeline  = Some(Arc::new(make("fs_smaa_edge",   r8)));
        self.smaa_blend_pipeline = Some(Arc::new(make("fs_smaa_blend",  r8)));
        self.smaa_final_pipeline = Some(Arc::new(make("fs_smaa_final",  hdr)));
    }
}

// ── Default GPU params ────────────────────────────────────────────────────────

impl AaParamsGpu {
    fn default_hd() -> Self {
        Self {
            resolution_x:            1280.0,
            resolution_y:             720.0,
            fxaa_edge_threshold:      0.0312,
            fxaa_edge_threshold_min:  0.0833,
            fxaa_subpix:              0.75,
            _pad: [0.0; 3],
        }
    }
}
