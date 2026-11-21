//! Temporal accumulation for the Pure2D path.
//!
//! Several sub-frame renders can be averaged in linear light without a CPU
//! readback: each sub-frame is sampled into an `Rgba16Float` target with a
//! weighted additive blend, and a final resolve writes the weighted average
//! into the caller's normal output target.  The API is generic; motion blur,
//! supersampling and any other "average N renders" use are built on it.
//!
//! The order of operations is deterministic: every pass is a full-screen
//! triangle over the same pixels, the accumulation target starts cleared to
//! zero, and the resolve divides by the exact sum of the weights the caller
//! supplied.  No wall clock, no unordered work.

use std::collections::HashMap;
use std::sync::Arc;

use crate::render::pipeline::{
    build_2d_pipeline, create_shader, create_texture_bind_group_layout, Pipeline2dDesc,
};
use crate::render::types::CompositeInstance;

/// The float format used for the accumulation target.
///
/// `Rgba16Float` is a core WebGPU render-attachment and filterable format; it
/// keeps enough headroom for linear-light sums of many sub-frames while using
/// half the memory of `Rgba32Float`.
pub const ACCUMULATION_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba16Float;

/// Why an accumulation pass could not run.
#[derive(Debug, Clone, PartialEq)]
pub enum AccumulationError {
    /// `accumulate` or `resolve` was called before `begin`.
    NotBegun,
    /// The sum of the weights is zero, so the average is undefined.
    ZeroWeight,
    /// A weight was negative or not finite.
    InvalidWeight {
        /// The rejected weight.
        weight: f32,
    },
    /// The requested target is larger than the device allows.
    TooLarge {
        /// Requested width.
        width: u32,
        /// Requested height.
        height: u32,
        /// Device limit.
        limit: u32,
    },
    /// The renderer cannot accumulate in its current mode.
    Unsupported {
        /// Human-readable reason.
        reason: &'static str,
    },
}

impl std::fmt::Display for AccumulationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NotBegun => write!(f, "accumulation was not begun"),
            Self::ZeroWeight => write!(f, "accumulation weights sum to zero"),
            Self::InvalidWeight { weight } => {
                write!(f, "accumulation weight must be finite and non-negative, got {weight}")
            }
            Self::TooLarge {
                width,
                height,
                limit,
            } => write!(
                f,
                "accumulation target {width}x{height} exceeds the device limit of {limit} per dimension"
            ),
            Self::Unsupported { reason } => write!(f, "accumulation is unsupported: {reason}"),
        }
    }
}

impl std::error::Error for AccumulationError {}

/// One open accumulation sequence.
///
/// Create it with [`Accumulation2d::new`], call [`Self::begin`] to clear it,
/// [`Self::accumulate`] once per sub-frame and [`Self::resolve`] to write the
/// weighted average into the normal output target.
pub struct Accumulation2d {
    device: Arc<wgpu::Device>,
    width: u32,
    height: u32,
    /// Held only to keep the view alive; never read directly.
    _texture: wgpu::Texture,
    view: wgpu::TextureView,
    add_pipeline: wgpu::RenderPipeline,
    resolve_pipelines: HashMap<wgpu::TextureFormat, wgpu::RenderPipeline>,
    bind_group_layout: wgpu::BindGroupLayout,
    sampler: wgpu::Sampler,
    instance_buffer: wgpu::Buffer,
    total_weight: f32,
    begun: bool,
}

impl Accumulation2d {
    /// Allocates the accumulation target for `width` x `height`.
    ///
    /// # Errors
    ///
    /// Returns [`AccumulationError::TooLarge`] when either side exceeds the
    /// device's `max_texture_dimension_2d`.
    pub fn new(
        device: Arc<wgpu::Device>,
        width: u32,
        height: u32,
    ) -> Result<Self, AccumulationError> {
        let limit = device.limits().max_texture_dimension_2d;
        if width == 0 || height == 0 || width > limit || height > limit {
            return Err(AccumulationError::TooLarge {
                width,
                height,
                limit,
            });
        }
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("2D Accumulation Target (Rgba16Float)"),
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: ACCUMULATION_FORMAT,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        let bind_group_layout =
            create_texture_bind_group_layout(&device, "2D Accumulation Bind Group Layout");
        let shader = create_shader(
            &device,
            "2D Accumulation Shader",
            include_str!("../../../../assets/shaders/accumulation_2d.wgsl").to_string(),
        );
        let vertex_buffers = [CompositeInstance::descriptor()];
        let bgls = [&bind_group_layout];
        let add_blend = wgpu::BlendState {
            color: wgpu::BlendComponent {
                src_factor: wgpu::BlendFactor::One,
                dst_factor: wgpu::BlendFactor::One,
                operation: wgpu::BlendOperation::Add,
            },
            alpha: wgpu::BlendComponent {
                src_factor: wgpu::BlendFactor::One,
                dst_factor: wgpu::BlendFactor::One,
                operation: wgpu::BlendOperation::Add,
            },
        };
        let add_pipeline = build_2d_pipeline(
            &device,
            Pipeline2dDesc {
                label: "2D Accumulation Add Pipeline",
                format: ACCUMULATION_FORMAT,
                depth_format: None,
                sample_count: 1,
                topology: wgpu::PrimitiveTopology::TriangleList,
                shader: &shader,
                vertex_entry: "vs_main",
                fragment_entry: "fs_main",
                vertex_buffers: &vertex_buffers,
                bind_group_layouts: &bgls,
                depth_write_enabled: false,
                depth_compare: wgpu::CompareFunction::Always,
                blend: Some(add_blend),
            },
            None,
        );
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("2D Accumulation Sampler"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Nearest,
            min_filter: wgpu::FilterMode::Nearest,
            mipmap_filter: wgpu::FilterMode::Nearest,
            ..Default::default()
        });
        let instance_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("2D Accumulation Instance Buffer"),
            size: std::mem::size_of::<CompositeInstance>() as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        Ok(Self {
            device,
            width,
            height,
            _texture: texture,
            view,
            add_pipeline,
            resolve_pipelines: HashMap::new(),
            bind_group_layout,
            sampler,
            instance_buffer,
            total_weight: 0.0,
            begun: false,
        })
    }

    /// Target size.
    #[must_use]
    pub const fn size(&self) -> (u32, u32) {
        (self.width, self.height)
    }

    /// Whether a sequence is open.
    #[must_use]
    pub const fn is_begun(&self) -> bool {
        self.begun
    }

    /// Sum of the weights accumulated since [`Self::begin`].
    #[must_use]
    pub const fn total_weight(&self) -> f32 {
        self.total_weight
    }

    /// Clears the accumulator to zero and opens a sequence.
    pub fn begin(&mut self, queue: &wgpu::Queue) {
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("2D Accumulation Begin"),
            });
        encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("2D Accumulation Clear Pass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &self.view,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
        });
        queue.submit(Some(encoder.finish()));
        self.total_weight = 0.0;
        self.begun = true;
    }

    /// Adds one finished sub-frame `source` with `weight` to the accumulator.
    ///
    /// The source may be any sampled texture view of the render target; sRGB
    /// formats are decoded to linear light by the sampler.  A weight of zero is
    /// accepted (it contributes nothing) as long as the total stays positive.
    ///
    /// # Errors
    ///
    /// [`AccumulationError::NotBegun`] before [`Self::begin`] and
    /// [`AccumulationError::InvalidWeight`] for a negative or non-finite
    /// weight.
    pub fn accumulate(
        &mut self,
        encoder: &mut wgpu::CommandEncoder,
        queue: &wgpu::Queue,
        source: &wgpu::TextureView,
        weight: f32,
    ) -> Result<(), AccumulationError> {
        if !self.begun {
            return Err(AccumulationError::NotBegun);
        }
        if !weight.is_finite() || weight < 0.0 {
            return Err(AccumulationError::InvalidWeight { weight });
        }
        let instance = CompositeInstance {
            opacity: weight,
            mode: 0,
        };
        queue.write_buffer(&self.instance_buffer, 0, bytemuck::bytes_of(&instance));
        let bind_group = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("2D Accumulation Source Bind Group"),
            layout: &self.bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(source),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&self.sampler),
                },
            ],
        });
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("2D Accumulation Add Pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &self.view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Load,
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });
            pass.set_pipeline(&self.add_pipeline);
            pass.set_bind_group(0, &bind_group, &[]);
            pass.set_vertex_buffer(0, self.instance_buffer.slice(..));
            pass.draw(0..3, 0..1);
        }
        self.total_weight += weight;
        Ok(())
    }

    /// Writes the weighted average into `target`.
    ///
    /// The target format is usually an sRGB 8-bit target; the shader writes
    /// linear premultiplied values and wgpu encodes them.  The sequence closes
    /// on success, so a second resolve without a new `begin` is
    /// [`AccumulationError::NotBegun`].
    ///
    /// # Errors
    ///
    /// [`AccumulationError::NotBegun`] before [`Self::begin`] and
    /// [`AccumulationError::ZeroWeight`] when the weights sum to zero.
    pub fn resolve(
        &mut self,
        encoder: &mut wgpu::CommandEncoder,
        queue: &wgpu::Queue,
        target: &wgpu::TextureView,
        target_format: wgpu::TextureFormat,
    ) -> Result<(), AccumulationError> {
        if !self.begun {
            return Err(AccumulationError::NotBegun);
        }
        if self.total_weight <= 0.0 {
            return Err(AccumulationError::ZeroWeight);
        }
        if !self.resolve_pipelines.contains_key(&target_format) {
            let pipeline = self.build_resolve_pipeline(target_format);
            self.resolve_pipelines.insert(target_format, pipeline);
        }
        let pipeline = &self.resolve_pipelines[&target_format];
        let instance = CompositeInstance {
            opacity: 1.0 / self.total_weight,
            mode: 0,
        };
        queue.write_buffer(&self.instance_buffer, 0, bytemuck::bytes_of(&instance));
        let bind_group = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("2D Accumulation Resolve Bind Group"),
            layout: &self.bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&self.view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&self.sampler),
                },
            ],
        });
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("2D Accumulation Resolve Pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: target,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });
            pass.set_pipeline(pipeline);
            pass.set_bind_group(0, &bind_group, &[]);
            pass.set_vertex_buffer(0, self.instance_buffer.slice(..));
            pass.draw(0..3, 0..1);
        }
        self.begun = false;
        Ok(())
    }

    fn build_resolve_pipeline(&self, target_format: wgpu::TextureFormat) -> wgpu::RenderPipeline {
        let shader = create_shader(
            &self.device,
            "2D Accumulation Resolve Shader",
            include_str!("../../../../assets/shaders/accumulation_2d.wgsl").to_string(),
        );
        let vertex_buffers = [CompositeInstance::descriptor()];
        let bgls = [&self.bind_group_layout];
        build_2d_pipeline(
            &self.device,
            Pipeline2dDesc {
                label: "2D Accumulation Resolve Pipeline",
                format: target_format,
                depth_format: None,
                sample_count: 1,
                topology: wgpu::PrimitiveTopology::TriangleList,
                shader: &shader,
                vertex_entry: "vs_main",
                fragment_entry: "fs_main",
                vertex_buffers: &vertex_buffers,
                bind_group_layouts: &bgls,
                depth_write_enabled: false,
                depth_compare: wgpu::CompareFunction::Always,
                blend: None,
            },
            None,
        )
    }
}
