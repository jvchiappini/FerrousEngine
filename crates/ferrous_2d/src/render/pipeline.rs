//! wgpu render pipelines and bind-group layouts for the Pure2D path.
//!
//! # Alpha contract
//!
//! Every Pure2D fragment shader writes **premultiplied** linear colour
//! (`vec4(rgb * a, a)`), and every Pure2D pipeline uses a premultiplied blend
//! state.  The public instance types keep **straight** linear RGBA colours, so
//! callers never change; the shader does the premultiply.  Sprite textures are
//! uploaded as straight-alpha `Rgba8UnormSrgb` by default; the shader
//! premultiplies after sampling and tinting.  Callers that already store
//! premultiplied (sRGB-encoded) data can opt in with the renderer's
//! `register_texture_rgba8_premultiplied` method.
//!
//! # Blend modes
//!
//! [`BlendMode2d`] is a per-draw property.  Each pipeline kind owns one
//! `wgpu::RenderPipeline` per mode (see [`SpritePipeline::pipeline`]), all built
//! by the single private `build_2d_pipeline` helper, so no descriptor
//! boilerplate is repeated per kind or per mode.

use std::sync::Arc;

use crate::render::types::{ShapeGpu, SpriteInstance, TextInstance};

/// Number of [`BlendMode2d`] variants; the length of every pipeline bank.
pub const BLEND_MODE_COUNT: usize = 4;

/// Per-draw blend mode for the Pure2D path.
///
/// All formulas are expressed in **premultiplied linear light** (`Cs`, `As` are
/// the premultiplied source, `Cd`, `Ad` the destination).  Instance colours stay
/// straight RGBA in the public API; see the module docs.
///
/// | mode | colour | alpha |
/// |---|---|---|
/// | `Normal` | `Cs + Cd·(1−As)` | `As + Ad·(1−As)` |
/// | `Add` | `Cs + Cd` | `As + Ad·(1−As)` |
/// | `Screen` | `Cs + Cd·(1−Cs)` | `As + Ad·(1−As)` |
/// | `Multiply` | `Cs·Cd + Cd·(1−As)` | `As + Ad·(1−As)` |
///
/// `Multiply` is exact over an opaque backdrop.  Over a transparent one it
/// omits the `Cs·(1−Ad)` term (a full `multiply` would need the backdrop alpha
/// in the shader), so semi-transparent multiply composites are darker than a
/// reference compositor.  This limitation is shared by layers, which composite
/// with the same state onto the parent's backdrop.
///
/// Advanced modes (overlay, soft light, colour dodge, hue, ...) need the
/// backdrop available in the fragment shader.  They are out of scope for this
/// path; a future implementation would sample the destination through a
/// `texture_2d` copy of the target, because wgpu render attachments cannot be
/// sampled in the same pass.
#[non_exhaustive]
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub enum BlendMode2d {
    /// Standard source-over compositing (`PREMULTIPLIED_ALPHA_BLENDING`).
    #[default]
    Normal,
    /// Additive: colour `src + dst`, alpha source-over.
    Add,
    /// Multiplicative over the backdrop: colour `src·dst + dst·(1−αs)`.
    Multiply,
    /// Screen: colour `src + dst·(1−src)`, alpha source-over.
    Screen,
}

impl BlendMode2d {
    /// Every mode, in pipeline-bank order.
    pub const ALL: [Self; BLEND_MODE_COUNT] =
        [Self::Normal, Self::Add, Self::Multiply, Self::Screen];

    /// Index into a pipeline bank (`0..BLEND_MODE_COUNT`).
    pub const fn index(self) -> usize {
        match self {
            Self::Normal => 0,
            Self::Add => 1,
            Self::Multiply => 2,
            Self::Screen => 3,
        }
    }

    /// Stable, human-readable name used in pipeline labels and logs.
    pub const fn label(self) -> &'static str {
        match self {
            Self::Normal => "normal",
            Self::Add => "add",
            Self::Multiply => "multiply",
            Self::Screen => "screen",
        }
    }

    /// The exact wgpu blend state for this mode (see the type docs).
    pub const fn wgpu_blend(self) -> wgpu::BlendState {
        use wgpu::{BlendComponent, BlendFactor, BlendOperation};

        const NORMAL: BlendComponent = BlendComponent {
            src_factor: BlendFactor::One,
            dst_factor: BlendFactor::OneMinusSrcAlpha,
            operation: BlendOperation::Add,
        };
        match self {
            Self::Normal => wgpu::BlendState {
                color: NORMAL,
                alpha: NORMAL,
            },
            Self::Add => wgpu::BlendState {
                color: BlendComponent {
                    src_factor: BlendFactor::One,
                    dst_factor: BlendFactor::One,
                    operation: BlendOperation::Add,
                },
                alpha: NORMAL,
            },
            Self::Multiply => wgpu::BlendState {
                color: BlendComponent {
                    src_factor: BlendFactor::Dst,
                    dst_factor: BlendFactor::OneMinusSrcAlpha,
                    operation: BlendOperation::Add,
                },
                alpha: NORMAL,
            },
            Self::Screen => wgpu::BlendState {
                color: BlendComponent {
                    src_factor: BlendFactor::One,
                    dst_factor: BlendFactor::OneMinusSrc,
                    operation: BlendOperation::Add,
                },
                alpha: NORMAL,
            },
        }
    }
}

/// Everything a Pure2D pipeline needs besides its shader and vertex layout.
struct Pipeline2dDesc<'a> {
    label: &'a str,
    format: wgpu::TextureFormat,
    depth_format: Option<wgpu::TextureFormat>,
    sample_count: u32,
    topology: wgpu::PrimitiveTopology,
    shader: &'a wgpu::ShaderModule,
    vertex_entry: &'a str,
    fragment_entry: &'a str,
    vertex_buffers: &'a [wgpu::VertexBufferLayout<'a>],
    bind_group_layouts: &'a [&'a wgpu::BindGroupLayout],
    depth_write_enabled: bool,
    depth_compare: wgpu::CompareFunction,
    blend: Option<wgpu::BlendState>,
}

/// The single render-pipeline constructor shared by every Pure2D pipeline.
fn build_2d_pipeline(
    device: &wgpu::Device,
    desc: Pipeline2dDesc<'_>,
    cache: Option<&wgpu::PipelineCache>,
) -> wgpu::RenderPipeline {
    let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some(desc.label),
        bind_group_layouts: desc.bind_group_layouts,
        push_constant_ranges: &[],
    });
    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some(desc.label),
        layout: Some(&pipeline_layout),
        vertex: wgpu::VertexState {
            module: desc.shader,
            entry_point: Some(desc.vertex_entry),
            buffers: desc.vertex_buffers,
            compilation_options: wgpu::PipelineCompilationOptions::default(),
        },
        fragment: Some(wgpu::FragmentState {
            module: desc.shader,
            entry_point: Some(desc.fragment_entry),
            targets: &[Some(wgpu::ColorTargetState {
                format: desc.format,
                blend: desc.blend,
                write_mask: wgpu::ColorWrites::ALL,
            })],
            compilation_options: wgpu::PipelineCompilationOptions::default(),
        }),
        primitive: wgpu::PrimitiveState {
            topology: desc.topology,
            strip_index_format: None,
            front_face: wgpu::FrontFace::Ccw,
            cull_mode: None,
            polygon_mode: wgpu::PolygonMode::Fill,
            unclipped_depth: false,
            conservative: false,
        },
        depth_stencil: desc.depth_format.map(|format| wgpu::DepthStencilState {
            format,
            depth_write_enabled: desc.depth_write_enabled,
            depth_compare: desc.depth_compare,
            stencil: wgpu::StencilState::default(),
            bias: wgpu::DepthBiasState::default(),
        }),
        multisample: wgpu::MultisampleState {
            count: desc.sample_count,
            mask: !0,
            alpha_to_coverage_enabled: false,
        },
        multiview: None,
        cache,
    })
}

/// Builds the four pipelines (one per [`BlendMode2d`]) of one kind.
fn build_blend_bank<'a>(
    device: &wgpu::Device,
    cache: Option<&wgpu::PipelineCache>,
    make_desc: impl Fn(BlendMode2d) -> Pipeline2dDesc<'a>,
) -> [wgpu::RenderPipeline; BLEND_MODE_COUNT] {
    std::array::from_fn(|index| {
        let mode = BlendMode2d::ALL[index];
        build_2d_pipeline(device, make_desc(mode), cache)
    })
}

/// Shared bind-group layout for the 2D camera uniform (group 0).
fn create_camera_bind_group_layout(device: &wgpu::Device, label: &str) -> wgpu::BindGroupLayout {
    device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some(label),
        entries: &[wgpu::BindGroupLayoutEntry {
            binding: 0,
            visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
            ty: wgpu::BindingType::Buffer {
                ty: wgpu::BufferBindingType::Uniform,
                has_dynamic_offset: false,
                min_binding_size: None,
            },
            count: None,
        }],
    })
}

/// Shared bind-group layout for a sampled texture plus filtering sampler
/// (group 1 of the sprite and text pipelines).
fn create_texture_bind_group_layout(device: &wgpu::Device, label: &str) -> wgpu::BindGroupLayout {
    device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some(label),
        entries: &[
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
            wgpu::BindGroupLayoutEntry {
                binding: 1,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                count: None,
            },
        ],
    })
}

fn create_shader(device: &wgpu::Device, label: &str, source: String) -> wgpu::ShaderModule {
    device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some(label),
        source: wgpu::ShaderSource::Wgsl(source.into()),
    })
}

/// Render pipelines and bind-group layouts for textured 2D sprites.
pub struct SpritePipeline {
    /// GPU device the pipelines were created on.
    pub device: Arc<wgpu::Device>,
    /// Pipelines indexed by [`BlendMode2d::index`].
    pipelines: [wgpu::RenderPipeline; BLEND_MODE_COUNT],
    /// Bind group 0: the shared 2D camera uniform.
    pub camera_bind_group_layout: wgpu::BindGroupLayout,
    /// Bind group 1: the per-texture view + sampler.
    pub texture_bind_group_layout: wgpu::BindGroupLayout,
}

impl SpritePipeline {
    /// Build the sprite pipelines for the given output `format` and
    /// `sample_count` (pass `depth_format: None` for the depth-less Pure2D path).
    pub fn new(
        device: Arc<wgpu::Device>,
        output_format: wgpu::TextureFormat,
        depth_format: Option<wgpu::TextureFormat>,
        sample_count: u32,
    ) -> Self {
        Self::new_with_cache(device, output_format, depth_format, sample_count, None)
    }

    /// Same as [`SpritePipeline::new`] but threads a shared
    /// [`wgpu::PipelineCache`] into `create_render_pipeline`.
    pub fn new_with_cache(
        device: Arc<wgpu::Device>,
        output_format: wgpu::TextureFormat,
        depth_format: Option<wgpu::TextureFormat>,
        sample_count: u32,
        cache: Option<&wgpu::PipelineCache>,
    ) -> Self {
        let camera_bind_group_layout =
            create_camera_bind_group_layout(&device, "Sprite Camera Bind Group Layout");
        let texture_bind_group_layout =
            create_texture_bind_group_layout(&device, "Sprite Texture Bind Group Layout");
        let shader = create_shader(
            &device,
            "Sprite Shader",
            include_str!("../../../../assets/shaders/sprite_2d.wgsl").to_string(),
        );
        let bgls = [&camera_bind_group_layout, &texture_bind_group_layout];
        let vertex_buffers = [SpriteInstance::descriptor()];
        let pipelines = build_blend_bank(&device, cache, |mode| Pipeline2dDesc {
            label: "Sprite WGPU Pipeline",
            format: output_format,
            depth_format,
            sample_count,
            topology: wgpu::PrimitiveTopology::TriangleStrip,
            shader: &shader,
            vertex_entry: "vs_main",
            fragment_entry: "fs_main",
            vertex_buffers: &vertex_buffers,
            bind_group_layouts: &bgls,
            depth_write_enabled: true,
            depth_compare: wgpu::CompareFunction::Less,
            blend: Some(mode.wgpu_blend()),
        });

        Self {
            device,
            pipelines,
            camera_bind_group_layout,
            texture_bind_group_layout,
        }
    }

    /// The pipeline for `mode`.
    pub fn pipeline(&self, mode: BlendMode2d) -> &wgpu::RenderPipeline {
        &self.pipelines[mode.index()]
    }
}

/// Render pipelines and bind-group layout for untextured SDF shapes.
pub struct ShapePipeline {
    /// GPU device the pipelines were created on.
    pub device: Arc<wgpu::Device>,
    /// Pipelines indexed by [`BlendMode2d::index`].
    pipelines: [wgpu::RenderPipeline; BLEND_MODE_COUNT],
    /// Bind group 0: the shared 2D camera uniform.
    pub camera_bind_group_layout: wgpu::BindGroupLayout,
}

impl ShapePipeline {
    /// Build the shape pipelines for the given output `format` and
    /// `sample_count` (pass `depth_format: None` for the depth-less Pure2D path).
    pub fn new(
        device: Arc<wgpu::Device>,
        output_format: wgpu::TextureFormat,
        depth_format: Option<wgpu::TextureFormat>,
        sample_count: u32,
    ) -> Self {
        Self::new_with_cache(device, output_format, depth_format, sample_count, None)
    }

    /// Same as [`ShapePipeline::new`] but threads a shared
    /// [`wgpu::PipelineCache`] into `create_render_pipeline`.
    pub fn new_with_cache(
        device: Arc<wgpu::Device>,
        output_format: wgpu::TextureFormat,
        depth_format: Option<wgpu::TextureFormat>,
        sample_count: u32,
        cache: Option<&wgpu::PipelineCache>,
    ) -> Self {
        let camera_bind_group_layout =
            create_camera_bind_group_layout(&device, "Shape Camera Bind Group Layout");
        const SHARED_2D: &str = include_str!("../../../../assets/shaders/pure2d_common.wgsl");
        const SHAPE_2D: &str = include_str!("../../../../assets/shaders/shape_2d.wgsl");
        let shader = create_shader(
            &device,
            "Shape Shader",
            format!(
                "{SHARED_2D}
{SHAPE_2D}"
            ),
        );
        let bgls = [&camera_bind_group_layout];
        let vertex_buffers = [ShapeGpu::descriptor()];
        let pipelines = build_blend_bank(&device, cache, |mode| Pipeline2dDesc {
            label: "Shape WGPU Pipeline",
            format: output_format,
            depth_format,
            sample_count,
            topology: wgpu::PrimitiveTopology::TriangleStrip,
            shader: &shader,
            vertex_entry: "vs_main",
            fragment_entry: "fs_main",
            vertex_buffers: &vertex_buffers,
            bind_group_layouts: &bgls,
            depth_write_enabled: false,
            depth_compare: wgpu::CompareFunction::Always,
            blend: Some(mode.wgpu_blend()),
        });

        Self {
            device,
            pipelines,
            camera_bind_group_layout,
        }
    }

    /// The pipeline for `mode`.
    pub fn pipeline(&self, mode: BlendMode2d) -> &wgpu::RenderPipeline {
        &self.pipelines[mode.index()]
    }
}

/// Render pipelines and bind-group layouts for MSDF text quads.
///
/// Group 0 is the same 2D camera uniform as the shape/sprite pipelines; group 1
/// is the font atlas texture + sampler.  The shader module is built from
/// `assets/shaders/msdf_common.wgsl` + `assets/shaders/text_2d.wgsl`, so the
/// MSDF decode logic is shared with the GUI text pipeline.
pub struct Text2dPipeline {
    /// GPU device the pipelines were created on.
    pub device: Arc<wgpu::Device>,
    /// Pipelines indexed by [`BlendMode2d::index`].
    pipelines: [wgpu::RenderPipeline; BLEND_MODE_COUNT],
    /// Bind group 0: the shared 2D camera uniform.
    pub camera_bind_group_layout: wgpu::BindGroupLayout,
    /// Bind group 1: the font atlas view + sampler.
    pub texture_bind_group_layout: wgpu::BindGroupLayout,
}

impl Text2dPipeline {
    /// Build the text pipelines for the given output `format` and
    /// `sample_count` (pass `depth_format: None` for the depth-less Pure2D path).
    pub fn new(
        device: Arc<wgpu::Device>,
        output_format: wgpu::TextureFormat,
        depth_format: Option<wgpu::TextureFormat>,
        sample_count: u32,
    ) -> Self {
        Self::new_with_cache(device, output_format, depth_format, sample_count, None)
    }

    /// Same as [`Text2dPipeline::new`] but threads a shared
    /// [`wgpu::PipelineCache`] into `create_render_pipeline`.
    pub fn new_with_cache(
        device: Arc<wgpu::Device>,
        output_format: wgpu::TextureFormat,
        depth_format: Option<wgpu::TextureFormat>,
        sample_count: u32,
        cache: Option<&wgpu::PipelineCache>,
    ) -> Self {
        let camera_bind_group_layout =
            create_camera_bind_group_layout(&device, "Text Camera Bind Group Layout");
        let texture_bind_group_layout =
            create_texture_bind_group_layout(&device, "Text Texture Bind Group Layout");

        // Prepend the shared camera/gradient layout and MSDF helpers so the GUI
        // and 2D shaders cannot drift apart.
        const SHARED_2D: &str = include_str!("../../../../assets/shaders/pure2d_common.wgsl");
        const SHARED_MSDF: &str = include_str!("../../../../assets/shaders/msdf_common.wgsl");
        const TEXT_2D: &str = include_str!("../../../../assets/shaders/text_2d.wgsl");
        let shader = create_shader(
            &device,
            "2D Text Shader",
            format!("{SHARED_2D}\n{SHARED_MSDF}\n{TEXT_2D}"),
        );
        let bgls = [&camera_bind_group_layout, &texture_bind_group_layout];
        let vertex_buffers = [TextInstance::descriptor()];
        let pipelines = build_blend_bank(&device, cache, |mode| Pipeline2dDesc {
            label: "2D Text WGPU Pipeline",
            format: output_format,
            depth_format,
            sample_count,
            topology: wgpu::PrimitiveTopology::TriangleStrip,
            shader: &shader,
            vertex_entry: "vs_main",
            fragment_entry: "fs_main",
            vertex_buffers: &vertex_buffers,
            bind_group_layouts: &bgls,
            depth_write_enabled: false,
            depth_compare: wgpu::CompareFunction::Always,
            blend: Some(mode.wgpu_blend()),
        });

        Self {
            device,
            pipelines,
            camera_bind_group_layout,
            texture_bind_group_layout,
        }
    }

    /// The pipeline for `mode`.
    pub fn pipeline(&self, mode: BlendMode2d) -> &wgpu::RenderPipeline {
        &self.pipelines[mode.index()]
    }
}

/// Render pipeline for the procedural 2D grid.
///
/// The grid is a full-screen triangle (no vertex buffers) evaluated in the
/// fragment shader from the inverse view-projection. It is drawn right after
/// the clear and before the retained/immediate geometry, so it stays behind
/// the model.  The grid is not a per-draw blendable element: it always
/// composites with `Normal`.
pub struct GridPipeline {
    /// GPU device the pipeline was created on.
    pub device: Arc<wgpu::Device>,
    /// The compiled wgpu render pipeline.
    pub wgpu_pipeline: wgpu::RenderPipeline,
    /// Bind group 0: the grid uniform.
    pub camera_bind_group_layout: wgpu::BindGroupLayout,
}

impl GridPipeline {
    /// Build the grid pipeline for the given output `format`.
    pub fn new(
        device: Arc<wgpu::Device>,
        output_format: wgpu::TextureFormat,
        sample_count: u32,
    ) -> Self {
        Self::new_with_cache(device, output_format, sample_count, None)
    }

    /// Same as [`GridPipeline::new`] but threads a shared
    /// [`wgpu::PipelineCache`] into `create_render_pipeline`.
    pub fn new_with_cache(
        device: Arc<wgpu::Device>,
        output_format: wgpu::TextureFormat,
        sample_count: u32,
        cache: Option<&wgpu::PipelineCache>,
    ) -> Self {
        let camera_bind_group_layout =
            create_camera_bind_group_layout(&device, "Grid Uniform Bind Group Layout");
        let shader = create_shader(
            &device,
            "Grid Shader",
            include_str!("../../../../assets/shaders/grid_2d.wgsl").to_string(),
        );
        let bgls = [&camera_bind_group_layout];
        let wgpu_pipeline = build_2d_pipeline(
            &device,
            Pipeline2dDesc {
                label: "Grid WGPU Pipeline",
                format: output_format,
                depth_format: None,
                sample_count,
                topology: wgpu::PrimitiveTopology::TriangleList,
                shader: &shader,
                vertex_entry: "vs_main",
                fragment_entry: "fs_main",
                vertex_buffers: &[],
                bind_group_layouts: &bgls,
                depth_write_enabled: false,
                depth_compare: wgpu::CompareFunction::Always,
                blend: Some(BlendMode2d::Normal.wgpu_blend()),
            },
            cache,
        );

        Self {
            device,
            wgpu_pipeline,
            camera_bind_group_layout,
        }
    }
}

/// Render pipelines for compositing a finished offscreen layer into its parent.
///
/// The full-target quad samples the layer texture (linear, premultiplied) and
/// multiplies all four channels by the layer opacity; the mode selects the
/// blend state used to write it into the parent target. Group 0 is the layer
/// texture + sampler, reusing the same binding shape as the sprite texture
/// group.
pub struct CompositePipeline {
    /// GPU device the pipelines were created on.
    pub device: Arc<wgpu::Device>,
    /// Pipelines indexed by [`BlendMode2d::index`].
    pipelines: [wgpu::RenderPipeline; BLEND_MODE_COUNT],
    /// Bind group 0: the finished layer texture view + sampler.
    pub texture_bind_group_layout: wgpu::BindGroupLayout,
}

impl CompositePipeline {
    /// Builds the composite pipelines for `output_format`.
    pub fn new(
        device: Arc<wgpu::Device>,
        output_format: wgpu::TextureFormat,
        sample_count: u32,
        cache: Option<&wgpu::PipelineCache>,
    ) -> Self {
        Self::new_with_cache(device, output_format, sample_count, cache)
    }

    /// Same as [`CompositePipeline::new`] but explicit about the cache.
    pub fn new_with_cache(
        device: Arc<wgpu::Device>,
        output_format: wgpu::TextureFormat,
        sample_count: u32,
        cache: Option<&wgpu::PipelineCache>,
    ) -> Self {
        let texture_bind_group_layout =
            create_texture_bind_group_layout(&device, "Composite Layer Bind Group Layout");
        let shader = create_shader(
            &device,
            "2D Composite Shader",
            include_str!("../../../../assets/shaders/composite_2d.wgsl").to_string(),
        );
        let bgls = [&texture_bind_group_layout];
        let vertex_buffers = [crate::render::types::CompositeInstance::descriptor()];
        let pipelines = build_blend_bank(&device, cache, |mode| Pipeline2dDesc {
            label: "2D Composite WGPU Pipeline",
            format: output_format,
            depth_format: None,
            sample_count,
            topology: wgpu::PrimitiveTopology::TriangleList,
            shader: &shader,
            vertex_entry: "vs_main",
            fragment_entry: "fs_main",
            vertex_buffers: &vertex_buffers,
            bind_group_layouts: &bgls,
            depth_write_enabled: false,
            depth_compare: wgpu::CompareFunction::Always,
            blend: Some(mode.wgpu_blend()),
        });

        Self {
            device,
            pipelines,
            texture_bind_group_layout,
        }
    }

    /// The pipeline for `mode`.
    pub fn pipeline(&self, mode: BlendMode2d) -> &wgpu::RenderPipeline {
        &self.pipelines[mode.index()]
    }
}

/// Render pipeline for backdrop-aware layer compositing.
///
/// Used for [`CompositeMode2d`](crate::render::CompositeMode2d) modes that depend on
/// the destination colour ([`CompositeMode2d::needs_backdrop`](crate::render::CompositeMode2d::needs_backdrop)).
/// (`0`/`1`) and a snapshot of the destination (`2`/`3`); blending is disabled
/// because the fragment shader returns the fully composited premultiplied
/// result.
pub struct BackdropCompositePipeline {
    /// GPU device the pipeline was created on.
    pub device: Arc<wgpu::Device>,
    pipeline: wgpu::RenderPipeline,
    /// Bind group 0: layer texture + sampler, backdrop texture + sampler.
    pub bind_group_layout: wgpu::BindGroupLayout,
}

impl BackdropCompositePipeline {
    /// Builds the pipeline for `output_format`.
    pub fn new(
        device: Arc<wgpu::Device>,
        output_format: wgpu::TextureFormat,
        sample_count: u32,
        cache: Option<&wgpu::PipelineCache>,
    ) -> Self {
        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("Backdrop Composite Bind Group Layout"),
            entries: &[
                texture_entry(0),
                sampler_entry(1),
                texture_entry(2),
                sampler_entry(3),
            ],
        });
        let shader = create_shader(
            &device,
            "2D Backdrop Composite Shader",
            include_str!("../../../../assets/shaders/composite_2d_blend.wgsl").to_string(),
        );
        let bgls = [&bind_group_layout];
        let vertex_buffers = [crate::render::types::CompositeInstance::descriptor()];
        let pipeline = build_2d_pipeline(
            &device,
            Pipeline2dDesc {
                label: "2D Backdrop Composite WGPU Pipeline",
                format: output_format,
                depth_format: None,
                sample_count,
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
            cache,
        );
        Self {
            device,
            pipeline,
            bind_group_layout,
        }
    }

    /// The composite pipeline.
    #[must_use]
    pub const fn pipeline(&self) -> &wgpu::RenderPipeline {
        &self.pipeline
    }
}

/// Render pipeline for one direction of the separable Gaussian blur.
///
/// Group 0 binds the source texture (`0`/`1`) and the per-pass uniform (`2`,
/// see [`BlurUniform2d`](crate::render::BlurUniform2d)).  The pass is a
/// full-target triangle with blending disabled: it replaces the destination
/// with the blurred source.
pub struct BlurEffectPipeline {
    /// GPU device the pipeline was created on.
    pub device: Arc<wgpu::Device>,
    pipeline: wgpu::RenderPipeline,
    /// Bind group 0: source texture + sampler + blur uniform.
    pub bind_group_layout: wgpu::BindGroupLayout,
}

impl BlurEffectPipeline {
    /// Builds the pipeline for `output_format`.
    pub fn new(
        device: Arc<wgpu::Device>,
        output_format: wgpu::TextureFormat,
        sample_count: u32,
        cache: Option<&wgpu::PipelineCache>,
    ) -> Self {
        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("2D Blur Effect Bind Group Layout"),
            entries: &[texture_entry(0), sampler_entry(1), uniform_entry(2)],
        });
        let shader = create_shader(
            &device,
            "2D Blur Effect Shader",
            include_str!("../../../../assets/shaders/effect_blur_2d.wgsl").to_string(),
        );
        let bgls = [&bind_group_layout];
        let pipeline = build_2d_pipeline(
            &device,
            Pipeline2dDesc {
                label: "2D Blur Effect WGPU Pipeline",
                format: output_format,
                depth_format: None,
                sample_count,
                topology: wgpu::PrimitiveTopology::TriangleList,
                shader: &shader,
                vertex_entry: "vs_main",
                fragment_entry: "fs_main",
                vertex_buffers: &[],
                bind_group_layouts: &bgls,
                depth_write_enabled: false,
                depth_compare: wgpu::CompareFunction::Always,
                blend: None,
            },
            cache,
        );
        Self {
            device,
            pipeline,
            bind_group_layout,
        }
    }

    /// The blur pipeline.
    #[must_use]
    pub const fn pipeline(&self) -> &wgpu::RenderPipeline {
        &self.pipeline
    }
}

/// Render pipeline that turns a layer image plus its blur into a drop shadow or
/// a glow.
///
/// Group 0 binds the content texture (`0`/`1`), the blurred texture (`2`/`3`)
/// and the per-pass uniform (`4`, see
/// [`EffectUniform2d`](crate::render::EffectUniform2d)).  Blending is disabled:
/// the fragment shader returns the combined premultiplied image.
pub struct EffectCompositePipeline {
    /// GPU device the pipeline was created on.
    pub device: Arc<wgpu::Device>,
    pipeline: wgpu::RenderPipeline,
    /// Bind group 0: content + sampler, blurred + sampler, effect uniform.
    pub bind_group_layout: wgpu::BindGroupLayout,
}

impl EffectCompositePipeline {
    /// Builds the pipeline for `output_format`.
    pub fn new(
        device: Arc<wgpu::Device>,
        output_format: wgpu::TextureFormat,
        sample_count: u32,
        cache: Option<&wgpu::PipelineCache>,
    ) -> Self {
        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("2D Effect Composite Bind Group Layout"),
            entries: &[
                texture_entry(0),
                sampler_entry(1),
                texture_entry(2),
                sampler_entry(3),
                uniform_entry(4),
            ],
        });
        let shader = create_shader(
            &device,
            "2D Effect Composite Shader",
            include_str!("../../../../assets/shaders/effect_composite_2d.wgsl").to_string(),
        );
        let bgls = [&bind_group_layout];
        let pipeline = build_2d_pipeline(
            &device,
            Pipeline2dDesc {
                label: "2D Effect Composite WGPU Pipeline",
                format: output_format,
                depth_format: None,
                sample_count,
                topology: wgpu::PrimitiveTopology::TriangleList,
                shader: &shader,
                vertex_entry: "vs_main",
                fragment_entry: "fs_main",
                vertex_buffers: &[],
                bind_group_layouts: &bgls,
                depth_write_enabled: false,
                depth_compare: wgpu::CompareFunction::Always,
                blend: None,
            },
            cache,
        );
        Self {
            device,
            pipeline,
            bind_group_layout,
        }
    }

    /// The composite pipeline.
    #[must_use]
    pub const fn pipeline(&self) -> &wgpu::RenderPipeline {
        &self.pipeline
    }
}

/// Render pipeline that multiplies a finished layer by a mask source's
/// coverage.
///
/// Group 0 binds the content texture (`0`/`1`), the mask source texture
/// (`2`/`3`) and the per-pass uniform (`4`, see
/// [`MaskUniform2d`](crate::render::MaskUniform2d)).  Blending is disabled: the
/// fragment shader returns the masked premultiplied image.
pub struct MaskPipeline {
    /// GPU device the pipeline was created on.
    pub device: Arc<wgpu::Device>,
    pipeline: wgpu::RenderPipeline,
    /// Bind group 0: content + sampler, mask + sampler, mask uniform.
    pub bind_group_layout: wgpu::BindGroupLayout,
}

impl MaskPipeline {
    /// Builds the pipeline for `output_format`.
    pub fn new(
        device: Arc<wgpu::Device>,
        output_format: wgpu::TextureFormat,
        sample_count: u32,
        cache: Option<&wgpu::PipelineCache>,
    ) -> Self {
        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("2D Mask Bind Group Layout"),
            entries: &[
                texture_entry(0),
                sampler_entry(1),
                texture_entry(2),
                sampler_entry(3),
                uniform_entry(4),
            ],
        });
        let shader = create_shader(
            &device,
            "2D Mask Shader",
            include_str!("../../../../assets/shaders/mask_2d.wgsl").to_string(),
        );
        let bgls = [&bind_group_layout];
        let pipeline = build_2d_pipeline(
            &device,
            Pipeline2dDesc {
                label: "2D Mask WGPU Pipeline",
                format: output_format,
                depth_format: None,
                sample_count,
                topology: wgpu::PrimitiveTopology::TriangleList,
                shader: &shader,
                vertex_entry: "vs_main",
                fragment_entry: "fs_main",
                vertex_buffers: &[],
                bind_group_layouts: &bgls,
                depth_write_enabled: false,
                depth_compare: wgpu::CompareFunction::Always,
                blend: None,
            },
            cache,
        );
        Self {
            device,
            pipeline,
            bind_group_layout,
        }
    }

    /// The mask pipeline.
    #[must_use]
    pub const fn pipeline(&self) -> &wgpu::RenderPipeline {
        &self.pipeline
    }
}

/// Bind-group entry for a fragment-visible uniform buffer.
pub(crate) const fn uniform_entry(binding: u32) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding,
        visibility: wgpu::ShaderStages::FRAGMENT,
        ty: wgpu::BindingType::Buffer {
            ty: wgpu::BufferBindingType::Uniform,
            has_dynamic_offset: false,
            min_binding_size: None,
        },
        count: None,
    }
}

const fn texture_entry(binding: u32) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding,
        visibility: wgpu::ShaderStages::FRAGMENT,
        ty: wgpu::BindingType::Texture {
            multisampled: false,
            view_dimension: wgpu::TextureViewDimension::D2,
            sample_type: wgpu::TextureSampleType::Float { filterable: true },
        },
        count: None,
    }
}

const fn sampler_entry(binding: u32) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding,
        visibility: wgpu::ShaderStages::FRAGMENT,
        ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
        count: None,
    }
}
