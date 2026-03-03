/// Post-process pass: ACES tone mapping + gamma correction.
///
/// Reads the HDR `Rgba16Float` texture produced by [`WorldPass`] and writes
/// a tone-mapped, gamma-corrected image to the final swapchain surface.
///
/// ## Pipeline design
/// - No vertex buffer — a fullscreen triangle is synthesised in the vertex
///   shader using `@builtin(vertex_index)`.
/// - Bind group 0: `texture_2d<f32>` + `sampler` pointing at the HDR texture.
/// - Output format: the swapchain format (supplied via `on_attach`).
///
/// The bind group is rebuilt every `execute` call because the `HdrTexture`
/// view can change on window resize.  Bind group creation is cheap (it's a
/// GPU-side descriptor, not a resource allocation).
use std::sync::Arc;

use wgpu::{
    BindGroup, BindGroupDescriptor, BindGroupEntry, BindGroupLayout, BindGroupLayoutDescriptor,
    BindGroupLayoutEntry, BindingResource, BindingType, CommandEncoder, Device, FragmentState,
    LoadOp, MultisampleState, Operations, PipelineLayoutDescriptor, PrimitiveState, Queue,
    RenderPassColorAttachment, RenderPassDescriptor, RenderPipeline, RenderPipelineDescriptor,
    SamplerBindingType, ShaderStages, StoreOp, TextureSampleType, TextureView,
    TextureViewDimension, VertexState,
};

use crate::graph::{FramePacket, RenderPass};
use crate::render_target::HdrTexture;

pub struct PostProcessPass {
    pub pipeline: Option<Arc<RenderPipeline>>,
    pub bind_group_layout: Option<Arc<BindGroupLayout>>,
    /// The swapchain format supplied by on_attach; used to (re)build the pipeline.
    surface_format: Option<wgpu::TextureFormat>,
}

impl PostProcessPass {
    pub fn new() -> Self {
        Self {
            pipeline: None,
            bind_group_layout: None,
            surface_format: None,
        }
    }

    /// Build (or rebuild) the render pipeline for the given swapchain format.
    fn build_pipeline(&mut self, device: &Device, surface_format: wgpu::TextureFormat) {
        let bgl = device.create_bind_group_layout(&BindGroupLayoutDescriptor {
            label: Some("PostProcess BGL"),
            entries: &[
                // binding 0: HDR texture
                BindGroupLayoutEntry {
                    binding: 0,
                    visibility: ShaderStages::FRAGMENT,
                    ty: BindingType::Texture {
                        multisampled: false,
                        view_dimension: TextureViewDimension::D2,
                        sample_type: TextureSampleType::Float { filterable: true },
                    },
                    count: None,
                },
                // binding 1: sampler
                BindGroupLayoutEntry {
                    binding: 1,
                    visibility: ShaderStages::FRAGMENT,
                    ty: BindingType::Sampler(SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });

        let pipeline_layout = device.create_pipeline_layout(&PipelineLayoutDescriptor {
            label: Some("PostProcess Pipeline Layout"),
            bind_group_layouts: &[&bgl],
            push_constant_ranges: &[],
        });

        let shader = device
            .create_shader_module(wgpu::include_wgsl!("../../../../assets/shaders/post.wgsl"));

        let pipeline = device.create_render_pipeline(&RenderPipelineDescriptor {
            label: Some("PostProcess Pipeline"),
            layout: Some(&pipeline_layout),
            vertex: VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                // No vertex buffers — fullscreen triangle generated in shader.
                buffers: &[],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            },
            fragment: Some(FragmentState {
                module: &shader,
                entry_point: Some("fs_main"),
                // Write to the SWAPCHAIN surface format, NOT Rgba16Float.
                targets: &[Some(wgpu::ColorTargetState {
                    format: surface_format,
                    blend: None,
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            }),
            primitive: PrimitiveState::default(),
            depth_stencil: None, // post-process needs no depth test
            multisample: MultisampleState::default(),
            multiview: None,
            cache: None,
        });

        self.bind_group_layout = Some(Arc::new(bgl));
        self.pipeline = Some(Arc::new(pipeline));
        self.surface_format = Some(surface_format);
    }

    /// Create a one-shot bind group pointing at the given HDR texture.
    fn make_bind_group(&self, device: &Device, hdr: &HdrTexture) -> BindGroup {
        let bgl = self
            .bind_group_layout
            .as_ref()
            .expect("PostProcessPass not initialised");
        device.create_bind_group(&BindGroupDescriptor {
            label: Some("PostProcess BindGroup"),
            layout: bgl,
            entries: &[
                BindGroupEntry {
                    binding: 0,
                    resource: BindingResource::TextureView(&hdr.view),
                },
                BindGroupEntry {
                    binding: 1,
                    resource: BindingResource::Sampler(&hdr.sampler),
                },
            ],
        })
    }

    /// Issue the fullscreen blit into `output_view` (the swapchain surface).
    pub fn render(
        &self,
        device: &Device,
        encoder: &mut CommandEncoder,
        hdr: &HdrTexture,
        output_view: &TextureView,
    ) {
        let pipeline = self
            .pipeline
            .as_ref()
            .expect("PostProcessPass not initialised");
        let bind_group = self.make_bind_group(device, hdr);

        let mut rpass = encoder.begin_render_pass(&RenderPassDescriptor {
            label: Some("PostProcess Pass"),
            color_attachments: &[Some(RenderPassColorAttachment {
                view: output_view,
                resolve_target: None,
                ops: Operations {
                    load: LoadOp::Clear(wgpu::Color::BLACK),
                    store: StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            occlusion_query_set: None,
            timestamp_writes: None,
        });

        rpass.set_pipeline(pipeline);
        rpass.set_bind_group(0, &bind_group, &[]);
        // Three vertices → one fullscreen triangle (no vertex buffer).
        rpass.draw(0..3, 0..1);
    }
}

impl Default for PostProcessPass {
    fn default() -> Self {
        Self::new()
    }
}

impl RenderPass for PostProcessPass {
    fn name(&self) -> &str {
        "Post-Process Pass"
    }

    fn on_attach(
        &mut self,
        device: &Device,
        _queue: &Queue,
        format: wgpu::TextureFormat,
        _sample_count: u32,
    ) {
        self.build_pipeline(device, format);
    }

    fn prepare(&mut self, _device: &Device, _queue: &Queue, _packet: &FramePacket) {}

    fn execute(
        &mut self,
        _device: &Device,
        _queue: &Queue,
        _encoder: &mut CommandEncoder,
        _color_view: &TextureView,
        _resolve_target: Option<&TextureView>,
        _depth_view: Option<&TextureView>,
        _packet: &FramePacket,
    ) {
        // This pass has a bespoke `render()` API that takes `hdr` + `output_view`
        // directly, so the generic RenderPass::execute path is a no-op.
        // The Renderer calls `post_process_pass.render(...)` explicitly after
        // the world pass.
    }
}
