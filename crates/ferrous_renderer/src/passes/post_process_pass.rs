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
use crate::render_target::{BloomTextures, HdrTexture};

pub struct PostProcessPass {
    pub pipeline: Option<Arc<RenderPipeline>>,
    pub bind_group_layout: Option<Arc<BindGroupLayout>>,
    /// The swapchain format supplied by on_attach; used to (re)build the pipeline.
    surface_format: Option<wgpu::TextureFormat>,
    // bloom-related resources ------------------------------------------------
    /// Chain of downsampled/upsampled bloom textures.  Created on first
    /// resize (renderer knows its width/height) and rebuilt as needed.
    ///
    /// exposed with `pub(crate)` so the renderer can sample the level-0
    /// view when constructing the post-process bind group.  It's still
    /// hidden from downstream consumers of the crate.
    pub(crate) bloom_textures: Option<BloomTextures>,
    /// sampler associated with the bloom textures (created alongside them).
    /// technically the sampler is stored inside `BloomTextures` but keeping
    /// a second handle simplifies some code paths.
    // (we could also pull it out of bloom_textures when needed)
    /// Bind group layout shared by both bloom pipelines (down & up).
    bloom_bind_group_layout: Option<Arc<BindGroupLayout>>,
    /// Pipeline used for the downsample pass.
    downsample_pipeline: Option<Arc<RenderPipeline>>,
    /// specialised pipeline used for the very first downsample where we
    /// apply a brightness threshold.
    initial_downsample_pipeline: Option<Arc<RenderPipeline>>,
    /// Pipeline used for the upsample pass (blend-add).
    upsample_pipeline: Option<Arc<RenderPipeline>>,
}

impl PostProcessPass {
    pub fn new() -> Self {
        Self {
            pipeline: None,
            bind_group_layout: None,
            surface_format: None,
            bloom_textures: None,
            bloom_bind_group_layout: None,
            downsample_pipeline: None,
            initial_downsample_pipeline: None,
            upsample_pipeline: None,
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
                // binding 2: bloom texture (always present once bloom is
                // initialised)
                BindGroupLayoutEntry {
                    binding: 2,
                    visibility: ShaderStages::FRAGMENT,
                    ty: BindingType::Texture {
                        multisampled: false,
                        view_dimension: TextureViewDimension::D2,
                        sample_type: TextureSampleType::Float { filterable: true },
                    },
                    count: None,
                },
                // binding 3: bloom sampler
                BindGroupLayoutEntry {
                    binding: 3,
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

    /// Build the two render pipelines used by the bloom chain and remember
    /// their common bind group layout.
    fn build_bloom_pipelines(&mut self, device: &Device) {
        let shader = device
            .create_shader_module(wgpu::include_wgsl!("../../../../assets/shaders/bloom.wgsl"));

        let bgl = device.create_bind_group_layout(&BindGroupLayoutDescriptor {
            label: Some("Bloom BGL"),
            entries: &[
                // input texture
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
                // sampler
                BindGroupLayoutEntry {
                    binding: 1,
                    visibility: ShaderStages::FRAGMENT,
                    ty: BindingType::Sampler(SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });

        let pipeline_layout = device.create_pipeline_layout(&PipelineLayoutDescriptor {
            label: Some("Bloom Pipeline Layout"),
            bind_group_layouts: &[&bgl],
            push_constant_ranges: &[],
        });

        // regular downsample (no threshold)
        let down = device.create_render_pipeline(&RenderPipelineDescriptor {
            label: Some("Bloom Downsample Pipeline"),
            layout: Some(&pipeline_layout),
            vertex: VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                buffers: &[],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            },
            fragment: Some(FragmentState {
                module: &shader,
                entry_point: Some("fs_downsample"),
                targets: &[Some(wgpu::ColorTargetState {
                    format: HdrTexture::FORMAT,
                    blend: None,
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            }),
            primitive: PrimitiveState::default(),
            depth_stencil: None,
            multisample: MultisampleState::default(),
            multiview: None,
            cache: None,
        });

        // initial downsample with brightness threshold hardcoded in shader
        let initial_down = device.create_render_pipeline(&RenderPipelineDescriptor {
            label: Some("Bloom Initial Downsample Pipeline"),
            layout: Some(&pipeline_layout),
            vertex: VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                buffers: &[],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            },
            fragment: Some(FragmentState {
                module: &shader,
                entry_point: Some("fs_downsample_threshold"),
                targets: &[Some(wgpu::ColorTargetState {
                    format: HdrTexture::FORMAT,
                    blend: None,
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            }),
            primitive: PrimitiveState::default(),
            depth_stencil: None,
            multisample: MultisampleState::default(),
            multiview: None,
            cache: None,
        });

        // upsample (blend-add to accumulate)
        let up = device.create_render_pipeline(&RenderPipelineDescriptor {
            label: Some("Bloom Upsample Pipeline"),
            layout: Some(&pipeline_layout),
            vertex: VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                buffers: &[],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            },
            fragment: Some(FragmentState {
                module: &shader,
                entry_point: Some("fs_upsample"),
                targets: &[Some(wgpu::ColorTargetState {
                    format: HdrTexture::FORMAT,
                    blend: Some(wgpu::BlendState {
                        color: wgpu::BlendComponent {
                            src_factor: wgpu::BlendFactor::One,
                            dst_factor: wgpu::BlendFactor::One,
                            operation: wgpu::BlendOperation::Add,
                        },
                        alpha: wgpu::BlendComponent::REPLACE,
                    }),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            }),
            primitive: PrimitiveState::default(),
            depth_stencil: None,
            multisample: MultisampleState::default(),
            multiview: None,
            cache: None,
        });

        self.bloom_bind_group_layout = Some(Arc::new(bgl));
        self.downsample_pipeline = Some(Arc::new(down));
        self.initial_downsample_pipeline = Some(Arc::new(initial_down));
        self.upsample_pipeline = Some(Arc::new(up));
    }

    /// Execute the bloom downsample/upsample chain.
    ///
    /// Assumes that `bloom_textures` has already been initialised (see
    /// `on_resize`).  Returns a reference to the level‑0 view which contains
    /// the final upsampled bloom contribution.
    pub fn run_bloom(
        &self,
        device: &Device,
        encoder: &mut CommandEncoder,
        hdr: &HdrTexture,
    ) -> &wgpu::TextureView {
        let bloom = self
            .bloom_textures
            .as_ref()
            .expect("bloom textures not initialised");
        let sampler = &bloom.sampler;
        let bgl = self
            .bloom_bind_group_layout
            .as_ref()
            .expect("bloom BGL not built");

        // downsample: hdr -> level0 -> level1 -> ...
        let mut input = &hdr.view;
        for (i, target) in bloom.views.iter().enumerate() {
            let pipeline = if i == 0 {
                self.initial_downsample_pipeline
                    .as_ref()
                    .expect("initial downsample pipeline missing")
            } else {
                self.downsample_pipeline
                    .as_ref()
                    .expect("downsample pipeline missing")
            };

            let bg = device.create_bind_group(&BindGroupDescriptor {
                label: Some(&format!("Bloom DS BG {}", i)),
                layout: bgl,
                entries: &[
                    BindGroupEntry {
                        binding: 0,
                        resource: BindingResource::TextureView(input),
                    },
                    BindGroupEntry {
                        binding: 1,
                        resource: BindingResource::Sampler(sampler),
                    },
                ],
            });

            let mut rpass = encoder.begin_render_pass(&RenderPassDescriptor {
                label: Some(&format!("Bloom Downsample pass {}", i)),
                color_attachments: &[Some(RenderPassColorAttachment {
                    view: target,
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
            rpass.set_bind_group(0, &bg, &[]);
            rpass.draw(0..3, 0..1);

            input = target;
        }

        // upsample: add smaller levels back into larger ones.
        //
        // The chain goes from the smallest mip up to views[1].  The result
        // of that is then upsampled once more into acc_view (a dedicated
        // accumulation texture at the same resolution as views[0]).  Using a
        // separate texture avoids overwriting the threshold-downsample data
        // that lives in views[0], which previously produced a corrupted bloom.
        for i in (1..bloom.views.len()).rev() {
            let src = &bloom.views[i];
            // Final upsample writes into acc_view; intermediate upsamples
            // write into the level below them in the downsample chain.
            let dst: &wgpu::TextureView = if i == 1 {
                &bloom.acc_view
            } else {
                &bloom.views[i - 1]
            };
            let pipeline = self
                .upsample_pipeline
                .as_ref()
                .expect("upsample pipeline missing");

            let bg = device.create_bind_group(&BindGroupDescriptor {
                label: Some(&format!("Bloom US BG {}", i)),
                layout: bgl,
                entries: &[
                    BindGroupEntry {
                        binding: 0,
                        resource: BindingResource::TextureView(src),
                    },
                    BindGroupEntry {
                        binding: 1,
                        resource: BindingResource::Sampler(sampler),
                    },
                ],
            });

            // All upsample passes accumulate additively with LoadOp::Load.
            // acc_view is cleared once at the start so it contains only the
            // upsampled glow and not residual data from a previous frame.
            let load_op = if i == 1 {
                LoadOp::Clear(wgpu::Color::BLACK)
            } else {
                LoadOp::Load
            };

            let mut rpass = encoder.begin_render_pass(&RenderPassDescriptor {
                label: Some(&format!("Bloom Upsample pass {}", i)),
                color_attachments: &[Some(RenderPassColorAttachment {
                    view: dst,
                    resolve_target: None,
                    ops: Operations {
                        load: load_op,
                        store: StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                occlusion_query_set: None,
                timestamp_writes: None,
            });
            rpass.set_pipeline(pipeline);
            rpass.set_bind_group(0, &bg, &[]);
            rpass.draw(0..3, 0..1);
        }

        &bloom.acc_view
    }

    /// Create a one-shot bind group pointing at the given HDR texture.
    fn make_bind_group(&self, device: &Device, hdr: &HdrTexture) -> BindGroup {
        let bgl = self
            .bind_group_layout
            .as_ref()
            .expect("PostProcessPass not initialised");
        // bloom textures should have been created during resize/attach before
        // the first render call.
        let bloom = self
            .bloom_textures
            .as_ref()
            .expect("bloom textures not initialised");

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
                BindGroupEntry {
                    binding: 2,
                    resource: BindingResource::TextureView(&bloom.acc_view),
                },
                BindGroupEntry {
                    binding: 3,
                    resource: BindingResource::Sampler(&bloom.sampler),
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
        // bloom pipelines do not depend on the swapchain format but they
        // need to exist so that a subsequent call to `on_resize` can
        // allocate the textures and we can render the chain.  Build them
        // once here so the shader modules are compiled up‑front.
        self.build_bloom_pipelines(device);
    }

    fn on_resize(&mut self, device: &Device, _queue: &Queue, width: u32, height: u32) {
        // recreate or create the bloom texture chain when the window size
        // changes.  we lazily allocate on first resize because we do not know
        // the renderer dimensions at construction time.
        if let Some(bt) = &mut self.bloom_textures {
            bt.resize(device, width, height);
        } else {
            self.bloom_textures = Some(BloomTextures::new(device, width, height, 5));
        }
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
