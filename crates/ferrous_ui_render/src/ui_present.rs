//! Offscreen presentation of a [`PersistentUiTarget`] onto a swapchain image.
//!
//! The persistent-UI path renders the composition and the GUI into one
//! single-sample texture instead of directly into the acquired surface image.
//! That decouples the render target from the rotating swapchain (a swapchain
//! image may be recycled immediately after `present`, while the persistent
//! texture keeps the previous frame), which is the prerequisite for updating
//! only a damaged rectangle.
//!
//! [`UiPresenter`] owns the persistent target and the two ways to put it on a
//! surface image:
//!
//! * [`UiPresenter::copy_full`] — a texel-for-texel
//!   `copy_texture_to_texture`.  This is **bit-exact** and is preferred when
//!   the surface advertises `COPY_DST`.
//! * [`UiPresenter::blit`] — a full-screen triangle that samples the target
//!   with a nearest sampler.  It only needs the surface's guaranteed
//!   `RENDER_ATTACHMENT` usage, so it is the fallback when `COPY_DST` is
//!   unavailable.  At matching size and nearest filtering it is 1:1; on an
//!   sRGB surface the sample/write pair is a decode/encode round-trip, so the
//!   copy path is preferred whenever it is legal.
//!
//! Both cover the **whole** destination image every frame.  Writing only the
//! damaged rectangles to the swapchain would be wrong: which swapchain image is
//! acquired changes between frames, so the untouched pixels of this frame's
//! image would hold another frame's content.

use crate::persist::PersistentUiTarget;

const BLIT_SHADER: &str = r#"
struct VsOut {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
};

@vertex
fn vs_main(@builtin(vertex_index) index: u32) -> VsOut {
    // One oversized triangle covers the clip-space square.  `x`/`y` take the
    // values 0 or 2 so the triangle reaches the corners of the screen.
    let x = f32((index << 1u) & 2u);
    let y = f32(index & 2u);
    var out: VsOut;
    out.position = vec4<f32>(x * 2.0 - 1.0, y * 2.0 - 1.0, 0.0, 1.0);
    out.uv = vec2<f32>(x, 1.0 - y);
    return out;
}

@group(0) @binding(0) var src: texture_2d<f32>;
@group(0) @binding(1) var samp: sampler;

@fragment
fn fs_main(in: VsOut) -> @location(0) vec4<f32> {
    return textureSample(src, samp, in.uv);
}
"#;

/// Presentation helper for [`PersistentUiTarget`].
///
/// Create one per surface format, call [`Self::ensure`] with the current size
/// before recording, render the frame into [`Self::view`], then call
/// [`Self::copy_full`] or [`Self::blit`].  A format change requires a new
/// presenter; a size change is handled by `ensure`.
pub struct UiPresenter {
    target: PersistentUiTarget,
    sampler: wgpu::Sampler,
    bind_group_layout: wgpu::BindGroupLayout,
    pipeline: wgpu::RenderPipeline,
    bind_group: Option<wgpu::BindGroup>,
    format: wgpu::TextureFormat,
}

impl UiPresenter {
    /// Builds the blit pipeline for `format`.
    #[must_use]
    pub fn new(device: &wgpu::Device, format: wgpu::TextureFormat) -> Self {
        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("UI Present Bind Group Layout"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
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

        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("UI Present Sampler"),
            mag_filter: wgpu::FilterMode::Nearest,
            min_filter: wgpu::FilterMode::Nearest,
            mipmap_filter: wgpu::FilterMode::Nearest,
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            ..Default::default()
        });

        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("UI Present Shader"),
            source: wgpu::ShaderSource::Wgsl(BLIT_SHADER.into()),
        });

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("UI Present Pipeline Layout"),
            bind_group_layouts: &[&bind_group_layout],
            push_constant_ranges: &[],
        });

        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("UI Present Pipeline"),
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
                    format,
                    blend: None,
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            }),
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview: None,
            cache: None,
        });

        Self {
            target: PersistentUiTarget::new(),
            sampler,
            bind_group_layout,
            pipeline,
            bind_group: None,
            format,
        }
    }

    /// Ensures the persistent target matches `width x height`.
    ///
    /// Returns `true` when the target was (re)created, which also invalidates
    /// it: the caller must render a full frame before trusting a partial
    /// update.  A zero size releases the target and returns `false`.
    pub fn ensure(&mut self, device: &wgpu::Device, width: u32, height: u32) -> bool {
        let recreated = self.target.ensure(device, width, height, self.format);
        if self.target.view().is_none() {
            self.bind_group = None;
        } else if recreated || self.bind_group.is_none() {
            let layout = &self.bind_group_layout;
            let sampler = &self.sampler;
            let bind_group = self.target.view().map(|view| {
                device.create_bind_group(&wgpu::BindGroupDescriptor {
                    label: Some("UI Present Bind Group"),
                    layout,
                    entries: &[
                        wgpu::BindGroupEntry {
                            binding: 0,
                            resource: wgpu::BindingResource::TextureView(view),
                        },
                        wgpu::BindGroupEntry {
                            binding: 1,
                            resource: wgpu::BindingResource::Sampler(sampler),
                        },
                    ],
                })
            });
            self.bind_group = bind_group;
        }
        recreated
    }

    /// The surface format this presenter's pipeline was built for.
    #[must_use]
    pub fn format(&self) -> wgpu::TextureFormat {
        self.format
    }

    /// The persistent render target.
    #[must_use]
    pub fn target(&self) -> &PersistentUiTarget {
        &self.target
    }

    /// Mutable access, e.g. to feed `GuiBatch::damage_union` into
    /// [`PersistentUiTarget::invalidate`].
    pub fn target_mut(&mut self) -> &mut PersistentUiTarget {
        &mut self.target
    }

    /// The persistent target's render-attachment view, or `None` while
    /// unallocated.
    #[must_use]
    pub fn view(&self) -> Option<&wgpu::TextureView> {
        self.target.view()
    }

    /// Target size, or `(0, 0)` while unallocated.
    #[must_use]
    pub fn size(&self) -> (u32, u32) {
        self.target.size()
    }

    /// Copies the whole persistent texture into `dst`, texel for texel.
    ///
    /// `dst` must be single-sample, the same size/format and carry `COPY_DST`.
    /// This is the bit-exact present path.
    pub fn copy_full(&self, encoder: &mut wgpu::CommandEncoder, dst: &wgpu::Texture) {
        let Some(src) = self.target.texture() else {
            return;
        };
        let (width, height) = self.target.size();
        if width == 0 || height == 0 {
            return;
        }
        encoder.copy_texture_to_texture(
            wgpu::ImageCopyTexture {
                texture: src,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            wgpu::ImageCopyTexture {
                texture: dst,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
        );
    }

    /// Draws the whole persistent texture into `dst_view` with a full-screen
    /// triangle.  `dst_view` must match the presenter's format and size.
    pub fn blit(&self, encoder: &mut wgpu::CommandEncoder, dst_view: &wgpu::TextureView) {
        let Some(bind_group) = self.bind_group.as_ref() else {
            return;
        };
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("UI Present Blit"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: dst_view,
                resolve_target: None,
                ops: wgpu::Operations {
                    // The triangle covers the whole image; the clear only
                    // guarantees defined contents on the pixels no triangle
                    // sample can miss (none).
                    load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
        });
        pass.set_pipeline(&self.pipeline);
        pass.set_bind_group(0, bind_group, &[]);
        pass.draw(0..3, 0..1);
    }
}
