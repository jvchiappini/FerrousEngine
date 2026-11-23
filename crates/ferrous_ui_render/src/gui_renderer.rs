//! GPU renderer for the UI.
//!
//! Handles WGPU pipelines, buffers, and textures for rendering UI batches.
//! Modularized for Phase 4: Early-Z, Depth Buffer, and Prepare/Render split.

use crate::gpu_types::{GuiQuad, TextQuad};
use crate::gui_batch::GuiBatch;
use crate::pipelines::{
    blur::{create_blur_pipeline, create_downsample_pipeline},
    id::create_id_pipeline,
    layout::Layouts,
    overdraw::{create_overdraw_pipeline, create_overdraw_text_pipeline},
    quad::create_quad_pipeline,
    text::create_text_pipeline,
};
use ferrous_core::memory::{self, Subsystem};
use std::num::NonZeroU64;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;
use wgpu::util::DeviceExt;

/// Size of one per-segment uniform: clip rect, radii and params (three vec4s).
const SEGMENT_UNIFORM_BYTES: u64 = 48;
/// Size of the blur/downsample parameters: texel, sigma, direction, tap count,
/// padding and the 16 padded Gaussian weights.
const BLUR_UNIFORM_BYTES: u64 = 288;
/// Initial number of per-segment uniform slots (grows on demand).
const INITIAL_SEGMENT_CAPACITY: usize = 64;
/// Deepest level of the 2x prefilter chain (1/32 per axis).
const BACKDROP_MAX_LEVEL: u32 = 5;
/// Widest half-kernel used by the separable Gaussian.
const BLUR_MAX_TAPS: usize = 13;
/// The requested radius is mapped to `sigma = radius / BLUR_SIGMA_DIVISOR`, so
/// the full width at half maximum is `2.355 * radius / 2.5 = 0.94 * radius`.
const BLUR_SIGMA_DIVISOR: f32 = 2.5;

/// GPU layout of the per-segment clip/backdrop uniform.
#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct SegmentUniforms {
    rect: [f32; 4],
    radii: [f32; 4],
    params: [f32; 4],
}

/// GPU layout of the backdrop blur/downsample parameters.
///
/// The shader declares `weights` as `array<vec4<f32>, 16>`: a uniform array of
/// `f32` has a 16-byte element stride, so each weight sits in the first slot of
/// its own `vec4`. `FLAT_BLUR_WEIGHTS` documents the packing; `pack` fills it.
#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct BlurParams {
    texel: [f32; 2],
    sigma: f32,
    horizontal: f32,
    taps: u32,
    pad: [u32; 3],
    weights: [[f32; 4]; 16],
}

impl BlurParams {
    /// Builds the parameters for one pass.
    fn new(texel: [f32; 2], sigma: f32, horizontal: bool, weights: &[f32]) -> Self {
        let mut packed = [[0.0_f32; 4]; 16];
        for (slot, weight) in packed.iter_mut().zip(weights) {
            slot[0] = *weight;
        }
        Self {
            texel,
            sigma,
            horizontal: f32::from(u8::from(horizontal)),
            taps: u32::try_from(weights.len()).unwrap_or(1),
            pad: [0; 3],
            weights: packed,
        }
    }

    /// Gaussian weights for `sigma` destination texels, taps one texel apart.
    ///
    /// Returns centre-first weights normalised so the 1-D kernel sums to one
    /// (the shader normalises by the actual sum as well).
    fn gaussian_weights(sigma: f32) -> ([f32; BLUR_MAX_TAPS], usize) {
        let sigma = sigma.max(0.05);
        // Three sigma covers 99.7 % of the energy; never exceed the array.
        let radius = (3.0 * sigma).ceil().clamp(0.0, (BLUR_MAX_TAPS - 1) as f32) as usize;
        let mut weights = [0.0_f32; BLUR_MAX_TAPS];
        let mut sum = 0.0_f32;
        for (tap, weight) in weights.iter_mut().enumerate().take(radius + 1) {
            let x = tap as f32;
            *weight = (-0.5 * (x / sigma) * (x / sigma)).exp();
            sum += if tap == 0 { *weight } else { *weight * 2.0 };
        }
        if sum > 0.0 {
            for weight in weights.iter_mut().take(radius + 1) {
                *weight /= sum;
            }
        }
        (weights, radius + 1)
    }
}

/// Forwards the `ferrous_font` / `ferrous_assets` cache inventory to
/// [`ferrous_core::memory`].
///
/// `ferrous_font` has no dependency on `ferrous_core`; the first
/// [`GuiRenderer`] installs this process-global sink, so every later atlas,
/// glyph table or decoded-image registration appears in `memory_report`.
#[cfg(feature = "text")]
fn install_memory_inventory() {
    ferrous_assets::inventory::install(register_cache_memory, unregister_cache_memory);
}

#[cfg(feature = "text")]
fn register_cache_memory(
    label: &str,
    bytes: u64,
    side: ferrous_assets::inventory::MemorySide,
    subsystem: ferrous_assets::inventory::Subsystem,
) {
    let subsystem = match subsystem {
        ferrous_assets::inventory::Subsystem::Fonts => Subsystem::Fonts,
        ferrous_assets::inventory::Subsystem::Assets => Subsystem::Assets,
    };
    match side {
        ferrous_assets::inventory::MemorySide::Cpu => memory::register_cpu(label, bytes, subsystem),
        ferrous_assets::inventory::MemorySide::Gpu => memory::register_gpu(label, bytes, subsystem),
    }
}

#[cfg(feature = "text")]
fn unregister_cache_memory(
    label: &str,
    side: ferrous_assets::inventory::MemorySide,
    subsystem: ferrous_assets::inventory::Subsystem,
) {
    let subsystem = match subsystem {
        ferrous_assets::inventory::Subsystem::Fonts => Subsystem::Fonts,
        ferrous_assets::inventory::Subsystem::Assets => Subsystem::Assets,
    };
    match side {
        ferrous_assets::inventory::MemorySide::Cpu => memory::unregister_cpu(label, subsystem),
        ferrous_assets::inventory::MemorySide::Gpu => memory::unregister_gpu(label, subsystem),
    }
}

/// Per-frame GPU work counters for the GUI pass (prompt 18 P5).
///
/// Every field counts *calls actually issued*, not primitive ranges or
/// identity switches.  Since the render loop caches bind state per pass, a
/// field only grows when a slot genuinely changes; before the cache existed
/// `pipeline_binds` always equalled `draw_calls`.  The numbers are what the
/// phase report compares before and after batching work.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct UiFrameCounters {
    /// Render passes opened by the GUI renderer this frame (0 or 1).
    pub passes: u32,
    /// Draw segments visited (one per scissor change).
    pub segments: u32,
    /// `set_scissor_rect` calls.
    pub scissor_changes: u32,
    /// `draw_indexed` calls.
    pub draw_calls: u32,
    /// `set_pipeline` calls actually issued (not the number of draws).
    pub pipeline_binds: u32,
    /// `set_bind_group` calls actually issued.
    pub bind_group_binds: u32,
    /// `set_vertex_buffer` + `set_index_buffer` calls actually issued.
    pub buffer_binds: u32,
}

/// Identity of the last object bound to each render-pass slot (prompt 18 P5).
///
/// The GUI pass used to rebind pipeline, bind groups and the shared quad
/// vertex/index buffers for every primitive range of every segment.  This
/// cache lets the loop skip a `set_*` call when the slot already holds the
/// same object.  It is local to one pass: the caller creates it with
/// `PassBindState::default()` after `begin_render_pass`, so stale state from a
/// previous pass can never leak in.
#[derive(Debug, Default, Clone, Copy)]
struct PassBindState {
    pipeline: Option<u64>,
    bind0: Option<u64>,
    bind1: Option<u64>,
    /// Group 2 keys on both the bind-group identity and the dynamic offset.
    bind2: Option<(u64, u32)>,
    vertex0: Option<u64>,
    vertex1: Option<u64>,
    index: Option<u64>,
}

/// Main UI rendering engine on the GPU.
pub struct GuiRenderer {
    pub device: Arc<wgpu::Device>,
    pub layouts: Layouts,
    pub opaque_pipeline: wgpu::RenderPipeline,
    pub transparent_pipeline: wgpu::RenderPipeline,
    pub text_pipeline: wgpu::RenderPipeline,
    pub svg_pipeline: wgpu::RenderPipeline,
    pub id_pipeline: wgpu::RenderPipeline,
    /// Diagnostic pipelines that add a fixed heat colour per fragment instead
    /// of the normal shading (prompt 18 P5).  Never selected unless
    /// [`Self::set_overdraw`] enables the view.
    pub overdraw_quad_pipeline: wgpu::RenderPipeline,
    pub overdraw_text_pipeline: wgpu::RenderPipeline,
    /// Runtime toggle for the overdraw debug view.
    overdraw_enabled: bool,

    pub vertex_buffer: wgpu::Buffer,
    pub index_buffer: wgpu::Buffer,
    pub instance_buffer: wgpu::Buffer,
    pub text_instance_buffer: wgpu::Buffer,
    pub icon_instance_buffer: wgpu::Buffer,
    pub svg_vertex_buffer: wgpu::Buffer,
    pub svg_index_buffer: wgpu::Buffer,
    pub uniform_buffer: wgpu::Buffer,

    pub uniform_bind_group: wgpu::BindGroup,
    pub image_bind_group: wgpu::BindGroup,
    pub font_bind_group: Option<wgpu::BindGroup>,
    pub icon_bind_group: Option<wgpu::BindGroup>,

    /// Identity of the texture set that `image_bind_group` was built from,
    /// stored as `[view_ptr, sampler_ptr, arc_ptr]` per texture.  `prepare`
    /// rebuilds the bind group only when this key no longer matches the batch
    /// (audit H8: it used to rebuild it every frame with textures).
    #[cfg(feature = "assets")]
    image_bind_group_key: Vec<usize>,
    /// When `false`, the bind group is rebuilt on every `prepare` with textures
    /// (the pre-optimization behaviour), for A/B comparison.  Default `true`.
    image_bind_group_cache: bool,

    pub max_instances: u32,
    pub text_max_instances: u32,
    pub icon_max_instances: u32,
    pub svg_max_vertices: u32,
    pub svg_max_indices: u32,
    /// Reused vertex/index scratch for SVG uploads (no per-frame `Vec`).
    svg_scratch_vertices: Vec<ferrous_svg::SvgVertex>,
    svg_scratch_indices: Vec<u32>,
    pub resolution: [f32; 2],

    // Depth Buffer for Early-Z.  `None` when the consumer disabled it
    // (`Render2dConfig::ui_depth == false`): the normal UI pipelines never
    // write depth, so the attachment is inert and its full-size texture is
    // pure VRAM (5.76 MB at 1600x900).
    pub depth_texture: Option<wgpu::Texture>,
    pub depth_view: Option<wgpu::TextureView>,
    /// Whether the GUI pass allocates/uses a depth attachment.
    ui_depth: bool,

    // GPU ID Hit-Testing.  The full-size id texture is allocated lazily on the
    // first hit test (see [`Self::ensure_id_target`]); most UI sessions never
    // pick, so paying for it at start-up was wasted VRAM.
    pub id_texture: Option<wgpu::Texture>,
    pub id_view: Option<wgpu::TextureView>,
    pub id_staging_buffer: wgpu::Buffer,

    pub format: wgpu::TextureFormat,
    pub sample_count: u32,

    /// Shared pipeline cache (Vulkan-only).  Threaded into every UI pipeline so
    /// a `format`/`sample_count` change reuses compiled shaders instead of
    /// recompiling (audit M18).  `None` on backends without
    /// `wgpu::Features::PIPELINE_CACHE`.
    pipeline_cache: Option<Arc<wgpu::PipelineCache>>,

    // Per-frame instrumentation (atomics: `render_with_timestamps` takes
    // `&self`).
    counters_passes: AtomicU32,
    counters_segments: AtomicU32,
    counters_scissor_changes: AtomicU32,
    counters_draw_calls: AtomicU32,
    counters_pipeline_binds: AtomicU32,
    counters_bind_group_binds: AtomicU32,
    counters_buffer_binds: AtomicU32,

    // ── Per-segment rounded clip + backdrop blur ──────────────────────────
    /// Stride between per-segment uniforms (device dynamic-offset alignment).
    segment_stride: u64,
    /// Number of uniform slots the segment buffer can hold.
    segment_capacity: usize,
    /// Dynamic uniform buffer holding every segment's clip shape.
    segment_buffer: wgpu::Buffer,
    /// Group 2: clip uniform (dynamic) + blurred backdrop texture/sampler.
    segment_bind_group: wgpu::BindGroup,
    /// CPU scratch for building the packed segment uniforms.
    segment_scratch: Vec<SegmentUniforms>,
    segment_bytes: Vec<u8>,
    /// The 1x1 placeholder texture used when no backdrop is bound.
    dummy_backdrop_texture: wgpu::Texture,
    backdrop_sampler: wgpu::Sampler,
    /// The texture currently bound at group 2 binding 1.
    backdrop_view: wgpu::TextureView,
    backdrop: BackdropState,
    backdrop_size: [u32; 2],
    backdrop_format: wgpu::TextureFormat,
    blur_pipeline: wgpu::RenderPipeline,
    downsample_pipeline: wgpu::RenderPipeline,
    /// Level 1..=BACKDROP_MAX_LEVEL of the 2x prefilter chain. Views keep their
    /// textures alive; the texture handles are not stored.
    downsample_views: Vec<wgpu::TextureView>,
    /// Bind groups that read the scene (index 0) or level `index` (index > 0)
    /// into level `index + 1`.
    downsample_bind_groups: Vec<wgpu::BindGroup>,
    downsample_params: Vec<wgpu::Buffer>,
    /// The level the Gaussian runs at and its two scratch targets.
    blur_level: u32,
    blur_h_view: Option<wgpu::TextureView>,
    blur_v_view: Option<wgpu::TextureView>,
    blur_h_bind_group: Option<wgpu::BindGroup>,
    blur_v_bind_group: Option<wgpu::BindGroup>,
    horizontal_blur_uniform: wgpu::Buffer,
    vertical_blur_uniform: wgpu::Buffer,
}

/// Whether a backdrop source is available and whether this frame uses it.
#[derive(Clone, Copy, Debug, Default)]
struct BackdropState {
    /// A source view is bound and the blur targets exist.
    ready: bool,
    /// The batch contains at least one backdrop-blur quad this frame.
    this_frame: bool,
}

impl GuiRenderer {
    pub fn new(
        device: Arc<wgpu::Device>,
        format: wgpu::TextureFormat,
        max_instances: u32,
        width: u32,
        height: u32,
        sample_count: u32,
        pipeline_cache: Option<Arc<wgpu::PipelineCache>>,
    ) -> Self {
        #[cfg(feature = "text")]
        install_memory_inventory();
        let layouts = Layouts::new(&device);

        // Standard buffers
        let vertices: &[f32] = &[0.0, 0.0, 1.0, 0.0, 1.0, 1.0, 0.0, 1.0];
        let indices: &[u16] = &[0, 1, 2, 2, 3, 0];

        let vertex_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("GUI Quad Vertex Buffer"),
            contents: bytemuck::cast_slice(vertices),
            usage: wgpu::BufferUsages::VERTEX,
        });

        let index_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("GUI Quad Index Buffer"),
            contents: bytemuck::cast_slice(indices),
            usage: wgpu::BufferUsages::INDEX,
        });

        let instance_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("GUI Instance Buffer"),
            size: (std::mem::size_of::<GuiQuad>() as u32 * max_instances) as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let text_instance_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("GUI Text Instance Buffer"),
            size: (std::mem::size_of::<TextQuad>() as u32 * max_instances) as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let icon_instance_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("GUI Icon Instance Buffer"),
            size: (std::mem::size_of::<TextQuad>() as u32 * max_instances) as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let resolution = [width as f32, height as f32];
        let uniform_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("GUI Uniform Buffer"),
            contents: bytemuck::cast_slice(&resolution),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });

        let uniform_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("GUI Uniform Bind Group"),
            layout: &layouts.uniform_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: uniform_buffer.as_entire_binding(),
            }],
        });

        // Dummy image bind group
        let dummy_texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("GUI Dummy Texture"),
            size: wgpu::Extent3d {
                width: 1,
                height: 1,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8UnormSrgb,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        let dummy_view = dummy_texture.create_view(&wgpu::TextureViewDescriptor::default());
        let dummy_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("GUI Dummy Sampler"),
            mag_filter: wgpu::FilterMode::Nearest,
            min_filter: wgpu::FilterMode::Nearest,
            ..Default::default()
        });
        #[cfg(not(target_arch = "wasm32"))]
        let dummy_views: Vec<&wgpu::TextureView> =
            std::iter::repeat_n(&dummy_view, crate::MAX_TEXTURE_SLOTS as usize).collect();
        #[cfg(not(target_arch = "wasm32"))]
        let dummy_samplers: Vec<&wgpu::Sampler> =
            std::iter::repeat_n(&dummy_sampler, crate::MAX_TEXTURE_SLOTS as usize).collect();

        #[cfg(not(target_arch = "wasm32"))]
        let image_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("GUI Image Bind Group (dummy)"),
            layout: &layouts.image_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureViewArray(&dummy_views),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::SamplerArray(&dummy_samplers),
                },
            ],
        });

        #[cfg(target_arch = "wasm32")]
        let image_bind_group = {
            let mut entries = Vec::new();
            for i in 0..8 {
                entries.push(wgpu::BindGroupEntry {
                    binding: (i * 2) as u32,
                    resource: wgpu::BindingResource::TextureView(&dummy_view),
                });
                entries.push(wgpu::BindGroupEntry {
                    binding: (i * 2 + 1) as u32,
                    resource: wgpu::BindingResource::Sampler(&dummy_sampler),
                });
            }
            device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("GUI Image Bind Group (Web dummy)"),
                layout: &layouts.image_layout,
                entries: &entries,
            })
        };

        // Pipelines
        let cache = pipeline_cache.as_deref();
        let depth_format = Some(wgpu::TextureFormat::Depth32Float);
        let opaque_pipeline = create_quad_pipeline(
            &device,
            format,
            &layouts.quad_pipeline_layout,
            sample_count,
            true,
            depth_format,
            cache,
        );
        let transparent_pipeline = create_quad_pipeline(
            &device,
            format,
            &layouts.quad_pipeline_layout,
            sample_count,
            false,
            depth_format,
            cache,
        );
        let text_pipeline = create_text_pipeline(
            &device,
            format,
            &layouts.text_pipeline_layout,
            sample_count,
            false,
            depth_format,
            cache,
        );
        let svg_pipeline = crate::pipelines::svg::create_svg_pipeline(
            &device,
            format,
            &layouts.quad_pipeline_layout,
            sample_count,
            depth_format,
            cache,
        );
        let id_pipeline = create_id_pipeline(&device, &layouts.quad_pipeline_layout, cache);
        let overdraw_quad_pipeline = create_overdraw_pipeline(
            &device,
            format,
            &layouts.quad_pipeline_layout,
            sample_count,
            depth_format,
            cache,
        );
        let overdraw_text_pipeline = create_overdraw_text_pipeline(
            &device,
            format,
            &layouts.text_pipeline_layout,
            sample_count,
            depth_format,
            cache,
        );

        // Depth Buffer for Early-Z
        let depth_texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("GUI Depth Texture"),
            size: wgpu::Extent3d {
                width: width.max(1),
                height: height.max(1),
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Depth32Float,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            view_formats: &[],
        });
        let depth_view = depth_texture.create_view(&wgpu::TextureViewDescriptor::default());
        let depth_texture = Some(depth_texture);
        let depth_view = Some(depth_view);

        // ID Buffer: only the 4-byte staging target is created eagerly.  The
        // full-size `R32Uint` texture is allocated by `ensure_id_target` on the
        // first pick/readback (it is used only by the separate id pass, so the
        // visual pass and every golden are unaffected).
        let id_staging_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("GUI ID Staging Buffer"),
            size: 4,
            usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let svg_vertex_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("GUI SVG Vertex Buffer"),
            size: 65536 * std::mem::size_of::<ferrous_svg::SvgVertex>() as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let svg_index_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("GUI SVG Index Buffer"),
            size: 65536 * 4,
            usage: wgpu::BufferUsages::INDEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        memory::register_gpu("gui:quad vertices", vertex_buffer.size(), Subsystem::Gui);
        memory::register_gpu("gui:quad indices", index_buffer.size(), Subsystem::Gui);
        memory::register_gpu(
            "gui:instance instances",
            instance_buffer.size(),
            Subsystem::Gui,
        );
        memory::register_gpu(
            "gui:text instances",
            text_instance_buffer.size(),
            Subsystem::Gui,
        );
        memory::register_gpu(
            "gui:icon instances",
            icon_instance_buffer.size(),
            Subsystem::Gui,
        );
        memory::register_gpu("gui:uniform", uniform_buffer.size(), Subsystem::Gui);
        memory::register_gpu("gui:dummy texture", 4, Subsystem::Gui);
        memory::register_gpu(
            "gui:depth texture",
            Self::depth_texture_bytes(width, height, sample_count),
            Subsystem::Gui,
        );
        memory::register_gpu("gui:id staging", id_staging_buffer.size(), Subsystem::Gui);
        memory::register_gpu("gui:svg vertices", svg_vertex_buffer.size(), Subsystem::Gui);
        memory::register_gpu("gui:svg indices", svg_index_buffer.size(), Subsystem::Gui);

        // Per-segment clip uniforms and the backdrop blur plumbing.
        let segment_stride = Self::segment_stride(&device);
        let segment_capacity = INITIAL_SEGMENT_CAPACITY;
        let segment_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("GUI Segment Uniform Buffer"),
            size: segment_stride * segment_capacity as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let dummy_backdrop_texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("GUI Dummy Backdrop Texture"),
            size: wgpu::Extent3d {
                width: 1,
                height: 1,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8UnormSrgb,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        let dummy_backdrop_view =
            dummy_backdrop_texture.create_view(&wgpu::TextureViewDescriptor::default());
        // A second view of the same 1x1 texture backs group 2 until a real
        // backdrop arrives (texture views are not cloneable in wgpu 23).
        let backdrop_view =
            dummy_backdrop_texture.create_view(&wgpu::TextureViewDescriptor::default());
        let backdrop_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("GUI Backdrop Sampler"),
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            ..Default::default()
        });
        let segment_bind_group = Self::create_segment_bind_group(
            &device,
            &layouts.segment_layout,
            &segment_buffer,
            &dummy_backdrop_view,
            &backdrop_sampler,
        );
        let horizontal_blur_uniform = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("GUI Backdrop Blur H Uniform"),
            size: BLUR_UNIFORM_BYTES,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let vertical_blur_uniform = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("GUI Backdrop Blur V Uniform"),
            size: BLUR_UNIFORM_BYTES,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let blur_pipeline = create_blur_pipeline(
            &device,
            format,
            &layouts.blur_pipeline_layout,
            pipeline_cache.as_deref(),
        );
        let downsample_pipeline = create_downsample_pipeline(
            &device,
            format,
            &layouts.blur_pipeline_layout,
            pipeline_cache.as_deref(),
        );
        let downsample_params = Self::create_downsample_params(&device);
        let downsample_views = Vec::new();
        let downsample_bind_groups = Vec::new();
        memory::register_gpu(
            "gui:segment uniforms",
            segment_buffer.size(),
            Subsystem::Gui,
        );

        Self {
            device,
            layouts,
            opaque_pipeline,
            transparent_pipeline,
            text_pipeline,
            svg_pipeline,
            id_pipeline,
            overdraw_quad_pipeline,
            overdraw_text_pipeline,
            overdraw_enabled: false,
            vertex_buffer,
            index_buffer,
            instance_buffer,
            text_instance_buffer,
            icon_instance_buffer,
            svg_vertex_buffer,
            svg_index_buffer,
            uniform_buffer,
            uniform_bind_group,
            image_bind_group,
            font_bind_group: None,
            icon_bind_group: None,
            #[cfg(feature = "assets")]
            image_bind_group_key: Vec::new(),
            image_bind_group_cache: true,
            max_instances,
            text_max_instances: max_instances,
            icon_max_instances: max_instances,
            svg_max_vertices: 65536,
            svg_max_indices: 65536,
            svg_scratch_vertices: Vec::new(),
            svg_scratch_indices: Vec::new(),
            resolution,
            depth_texture,
            depth_view,
            ui_depth: true,
            id_texture: None,
            id_view: None,
            id_staging_buffer,
            format,
            sample_count,
            pipeline_cache,
            counters_passes: AtomicU32::new(0),
            counters_segments: AtomicU32::new(0),
            counters_scissor_changes: AtomicU32::new(0),
            counters_draw_calls: AtomicU32::new(0),
            counters_pipeline_binds: AtomicU32::new(0),
            counters_bind_group_binds: AtomicU32::new(0),
            counters_buffer_binds: AtomicU32::new(0),
            segment_stride,
            segment_capacity,
            segment_buffer,
            segment_bind_group,
            segment_scratch: Vec::new(),
            segment_bytes: Vec::new(),
            dummy_backdrop_texture,
            backdrop_sampler,
            backdrop_view,
            backdrop: BackdropState::default(),
            backdrop_size: [0, 0],
            backdrop_format: format,
            blur_pipeline,
            downsample_pipeline,
            downsample_views,
            downsample_bind_groups,
            downsample_params,
            blur_level: 0,
            blur_h_view: None,
            blur_v_view: None,
            blur_h_bind_group: None,
            blur_v_bind_group: None,
            horizontal_blur_uniform,
            vertical_blur_uniform,
        }
    }

    /// Dynamic-offset stride: the device alignment rounded up to the uniform
    /// size (at least 48 bytes).
    fn segment_stride(device: &wgpu::Device) -> u64 {
        let alignment = u64::from(device.limits().min_uniform_buffer_offset_alignment).max(1);
        SEGMENT_UNIFORM_BYTES.div_ceil(alignment) * alignment
    }

    /// Builds group 2: the dynamic clip uniform plus the backdrop texture.
    fn create_segment_bind_group(
        device: &wgpu::Device,
        layout: &wgpu::BindGroupLayout,
        buffer: &wgpu::Buffer,
        backdrop: &wgpu::TextureView,
        sampler: &wgpu::Sampler,
    ) -> wgpu::BindGroup {
        device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("GUI Segment Bind Group"),
            layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                        buffer,
                        offset: 0,
                        size: NonZeroU64::new(SEGMENT_UNIFORM_BYTES),
                    }),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(backdrop),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::Sampler(sampler),
                },
            ],
        })
    }

    /// Rebuilds group 2 after the segment buffer or the backdrop changed.
    fn rebuild_segment_bind_group(&mut self) {
        self.segment_bind_group = Self::create_segment_bind_group(
            &self.device,
            &self.layouts.segment_layout,
            &self.segment_buffer,
            &self.backdrop_view,
            &self.backdrop_sampler,
        );
    }

    /// Creates one backdrop-blur target texture.
    fn create_blur_target(
        device: &wgpu::Device,
        width: u32,
        height: u32,
        format: wgpu::TextureFormat,
    ) -> (wgpu::Texture, wgpu::TextureView) {
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("GUI Backdrop Blur Target"),
            size: wgpu::Extent3d {
                width: width.max(1),
                height: height.max(1),
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        (texture, view)
    }

    /// Creates the per-level downsample uniform buffers (one per chain level).
    fn create_downsample_params(device: &wgpu::Device) -> Vec<wgpu::Buffer> {
        (0..BACKDROP_MAX_LEVEL)
            .map(|level| {
                device.create_buffer(&wgpu::BufferDescriptor {
                    label: Some(&format!("GUI Backdrop Downsample L{} Uniform", level + 1)),
                    size: BLUR_UNIFORM_BYTES,
                    usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
                    mapped_at_creation: false,
                })
            })
            .collect()
    }

    fn create_blur_bind_group(
        &self,
        source: &wgpu::TextureView,
        params: &wgpu::Buffer,
    ) -> wgpu::BindGroup {
        self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("GUI Backdrop Blur Bind Group"),
            layout: &self.layouts.blur_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(source),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&self.backdrop_sampler),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: params.as_entire_binding(),
                },
            ],
        })
    }

    /// Supplies the scene colour view the backdrop blur reads.
    ///
    /// `ferrous_ui_render` never depends on `ferrous_renderer`: the caller
    /// passes a `SceneTarget2d::view()` (or any sampled colour view of the
    /// same size) before [`Self::prepare`].
    ///
    /// The scene is box-downsampled through a 2x chain (up to 1/32 per axis)
    /// and blurred at the level whose texel is closest to the requested
    /// sigma, so a thin line integrates into one smooth peak instead of being
    /// copied at each tap. Positions and radii are in physical pixels.
    ///
    /// Call this every frame with the scene view; a same-size call only
    /// rebuilds the level-1 source bind group.
    pub fn set_backdrop_source(&mut self, source: &wgpu::TextureView, width: u32, height: u32) {
        let width = width.max(1);
        let height = height.max(1);
        let recreate = self.backdrop_size != [width, height]
            || self.backdrop_format != self.format
            || self.downsample_views.is_empty();

        if recreate {
            self.backdrop_size = [width, height];
            self.backdrop_format = self.format;
            let views = Self::create_downsample_views(&self.device, [width, height], self.format);
            // Index 0 reads the caller's scene view and is rebuilt every frame;
            // index `level` reads level `level` into level `level + 1`.
            let mut groups = Vec::with_capacity(views.len());
            groups.push(self.create_blur_bind_group(&views[0], &self.downsample_params[0]));
            for level in 1..views.len() {
                groups.push(
                    self.create_blur_bind_group(&views[level - 1], &self.downsample_params[level]),
                );
            }
            self.downsample_views = views;
            self.downsample_bind_groups = groups;
            self.clear_backdrop_targets();
        }

        // Level 1 always reads the caller's scene view, which cannot be stored.
        let scene_group = self.create_blur_bind_group(source, &self.downsample_params[0]);
        if let Some(slot) = self.downsample_bind_groups.first_mut() {
            *slot = scene_group;
        } else {
            self.downsample_bind_groups.push(scene_group);
        }

        self.backdrop.ready = true;
    }

    /// Creates level 1..=BACKDROP_MAX_LEVEL views from a scene of `size`.
    fn create_downsample_views(
        device: &wgpu::Device,
        [width, height]: [u32; 2],
        format: wgpu::TextureFormat,
    ) -> Vec<wgpu::TextureView> {
        (1..=BACKDROP_MAX_LEVEL)
            .map(|level| {
                let w = (width >> level).max(1);
                let h = (height >> level).max(1);
                Self::create_blur_target(device, w, h, format).1
            })
            .collect()
    }

    /// Writes each downsample pass's source texel size.
    fn write_downsample_params(&self, queue: &wgpu::Queue) {
        let mut source = [self.backdrop_size[0], self.backdrop_size[1]];
        for (level, buffer) in self.downsample_params.iter().enumerate() {
            let params = BlurParams::new(
                [1.0 / source[0] as f32, 1.0 / source[1] as f32],
                0.0,
                true,
                &[1.0],
            );
            queue.write_buffer(buffer, 0, bytemuck::bytes_of(&params));
            let target = self.level_size(u32::try_from(level).unwrap_or(0) + 1);
            source = [target[0], target[1]];
        }
    }

    /// The size of chain level `level` for the current scene size.
    fn level_size(&self, level: u32) -> [u32; 2] {
        [
            (self.backdrop_size[0] >> level).max(1),
            (self.backdrop_size[1] >> level).max(1),
        ]
    }

    /// Recreates the two Gaussian scratch targets at `level`.
    ///
    /// Returns `true` when the level or the target size changed, so the caller
    /// can rebuild the segment bind group that samples the result.
    fn set_blur_level(&mut self, level: u32) -> bool {
        if self.blur_level == level && self.blur_h_view.is_some() {
            return false;
        }
        let [width, height] = self.level_size(level);
        let (_h_texture, h_view) =
            Self::create_blur_target(&self.device, width, height, self.format);
        let (v_texture, v_view) =
            Self::create_blur_target(&self.device, width, height, self.format);
        self.backdrop_view = v_texture.create_view(&wgpu::TextureViewDescriptor::default());
        self.blur_level = level;
        self.blur_h_bind_group = Some(self.create_blur_bind_group(
            &self.downsample_views[level as usize - 1],
            &self.horizontal_blur_uniform,
        ));
        self.blur_v_bind_group =
            Some(self.create_blur_bind_group(&h_view, &self.vertical_blur_uniform));
        self.blur_h_view = Some(h_view);
        self.blur_v_view = Some(v_view);
        true
    }

    /// Drops the blur targets so the next `set_backdrop_source` recreates them
    /// (format changes and resizes).
    fn clear_backdrop_targets(&mut self) {
        self.blur_h_view = None;
        self.blur_v_view = None;
        self.blur_h_bind_group = None;
        self.blur_v_bind_group = None;
        self.blur_level = 0;
        self.backdrop = BackdropState::default();
        self.backdrop_view = self
            .dummy_backdrop_texture
            .create_view(&wgpu::TextureViewDescriptor::default());
        self.rebuild_segment_bind_group();
    }

    /// Enables or disables the overdraw debug view (prompt 18 P5).
    ///
    /// When enabled, the quad and text/icon ranges are drawn with additive
    /// diagnostic pipelines that accumulate `OVERDRAW_HEAT` per fragment, so a
    /// bright pixel means many overlapping fragments.  Default off; it changes
    /// the picture on purpose and is never used by goldens.
    pub fn set_overdraw(&mut self, enabled: bool) {
        self.overdraw_enabled = enabled;
    }

    /// Enables or disables the GUI depth attachment (prompt 18 P5).
    ///
    /// The normal UI pipelines never write depth, so in a 2D mode the
    /// full-size `Depth32Float` target is inert: disabling it frees 5.76 MB at
    /// 1600x900 with identical pixels. Re-enabling recreates it at the current
    /// resolution.
    pub fn set_ui_depth(&mut self, enabled: bool) {
        if self.ui_depth == enabled {
            return;
        }
        self.ui_depth = enabled;
        let width = self.resolution[0].max(1.0) as u32;
        let height = self.resolution[1].max(1.0) as u32;
        if enabled {
            let (texture, view) =
                Self::create_depth_target(&self.device, width, height, self.sample_count);
            self.depth_texture = Some(texture);
            self.depth_view = Some(view);
            memory::register_gpu(
                "gui:depth texture",
                Self::depth_texture_bytes(width, height, self.sample_count),
                Subsystem::Gui,
            );
        } else {
            self.depth_texture = None;
            self.depth_view = None;
            memory::unregister_gpu("gui:depth texture", Subsystem::Gui);
        }
        // The pipelines declare the depth attachment format, so they must be
        // rebuilt for the new pass layout.
        self.rebuild_pipelines(self.format, self.sample_count);
    }

    /// Snapshot of the GUI pass counters accumulated by the last
    /// [`Self::render_with_timestamps`] call.
    pub fn frame_counters(&self) -> UiFrameCounters {
        UiFrameCounters {
            passes: self.counters_passes.load(Ordering::Relaxed),
            segments: self.counters_segments.load(Ordering::Relaxed),
            scissor_changes: self.counters_scissor_changes.load(Ordering::Relaxed),
            draw_calls: self.counters_draw_calls.load(Ordering::Relaxed),
            pipeline_binds: self.counters_pipeline_binds.load(Ordering::Relaxed),
            bind_group_binds: self.counters_bind_group_binds.load(Ordering::Relaxed),
            buffer_binds: self.counters_buffer_binds.load(Ordering::Relaxed),
        }
    }

    fn reset_frame_counters(&self) {
        self.counters_passes.store(0, Ordering::Relaxed);
        self.counters_segments.store(0, Ordering::Relaxed);
        self.counters_scissor_changes.store(0, Ordering::Relaxed);
        self.counters_draw_calls.store(0, Ordering::Relaxed);
        self.counters_pipeline_binds.store(0, Ordering::Relaxed);
        self.counters_bind_group_binds.store(0, Ordering::Relaxed);
        self.counters_buffer_binds.store(0, Ordering::Relaxed);
    }

    pub fn resize(&mut self, queue: &wgpu::Queue, width: u32, height: u32) {
        self.update_output_config(queue, width, height, self.format, self.sample_count);
    }

    /// Rebuilds every GUI pipeline for `format`/`sample_count`.
    ///
    /// The depth attachment is part of the pipeline state: a pipeline built
    /// with `Depth32Float` cannot be used in a pass without a depth attachment
    /// (and vice versa), so toggling [`Self::set_ui_depth`] rebuilds them.
    fn rebuild_pipelines(&mut self, format: wgpu::TextureFormat, sample_count: u32) {
        log::debug!(
            "[GuiRenderer] Rebuilding pipelines for format={:?}, sample_count={}",
            format,
            sample_count
        );
        let cache = self.pipeline_cache.as_deref();
        let depth_format = if self.ui_depth {
            Some(wgpu::TextureFormat::Depth32Float)
        } else {
            None
        };
        self.opaque_pipeline = create_quad_pipeline(
            &self.device,
            format,
            &self.layouts.quad_pipeline_layout,
            sample_count,
            true,
            depth_format,
            cache,
        );
        self.transparent_pipeline = create_quad_pipeline(
            &self.device,
            format,
            &self.layouts.quad_pipeline_layout,
            sample_count,
            false,
            depth_format,
            cache,
        );
        self.text_pipeline = create_text_pipeline(
            &self.device,
            format,
            &self.layouts.text_pipeline_layout,
            sample_count,
            false,
            depth_format,
            cache,
        );
        self.svg_pipeline = crate::pipelines::svg::create_svg_pipeline(
            &self.device,
            format,
            &self.layouts.quad_pipeline_layout,
            sample_count,
            depth_format,
            cache,
        );
        self.overdraw_quad_pipeline = create_overdraw_pipeline(
            &self.device,
            format,
            &self.layouts.quad_pipeline_layout,
            sample_count,
            depth_format,
            cache,
        );
        self.overdraw_text_pipeline = create_overdraw_text_pipeline(
            &self.device,
            format,
            &self.layouts.text_pipeline_layout,
            sample_count,
            depth_format,
            cache,
        );
        self.blur_pipeline = create_blur_pipeline(
            &self.device,
            format,
            &self.layouts.blur_pipeline_layout,
            cache,
        );
        self.downsample_pipeline = create_downsample_pipeline(
            &self.device,
            format,
            &self.layouts.blur_pipeline_layout,
            cache,
        );
        // The blur targets use the color format; drop them so the next
        // `set_backdrop_source` recreates them at the new format.
        self.clear_backdrop_targets();
    }

    pub fn update_output_config(
        &mut self,
        queue: &wgpu::Queue,
        width: u32,
        height: u32,
        format: wgpu::TextureFormat,
        sample_count: u32,
    ) {
        let width = width.max(1);
        let height = height.max(1);
        self.resolution = [width as f32, height as f32];
        queue.write_buffer(
            &self.uniform_buffer,
            0,
            bytemuck::cast_slice(&self.resolution),
        );

        let state_changed = format != self.format || sample_count != self.sample_count;

        if state_changed {
            self.format = format;
            self.sample_count = sample_count;

            // Rebuild pipelines for new sample_count/format.  The shared
            // pipeline cache lets the driver reuse previously compiled shaders
            // across backends/formats instead of recompiling from scratch
            // (audit M18).
            log::debug!(
                "[GuiRenderer] Rebuilding pipelines for format={:?}, sample_count={}",
                format,
                sample_count
            );
            self.rebuild_pipelines(format, sample_count);
        }

        // Resize Depth.  There is exactly one depth target: replacing the view
        // and then the texture drops the previous pair, so a resize can never
        // keep two full-size depth buffers alive.  A disabled (`ui_depth ==
        // false`) pass allocates nothing.
        if self.ui_depth {
            let (depth_texture, depth_view) =
                Self::create_depth_target(&self.device, width, height, sample_count);
            self.depth_view = Some(depth_view);
            self.depth_texture = Some(depth_texture);
            memory::register_gpu(
                "gui:depth texture",
                Self::depth_texture_bytes(width, height, sample_count),
                Subsystem::Gui,
            );
        } else {
            self.depth_view = None;
            self.depth_texture = None;
            memory::unregister_gpu("gui:depth texture", Subsystem::Gui);
        }

        // Resize ID only when the target exists: it is lazy and must not be
        // allocated by a resize.  Dropping both handles before allocating the
        // replacement keeps the peak at one id texture, not two.
        if self.id_texture.is_some() {
            self.id_view = None;
            self.id_texture = None;
            let (id_texture, id_view) = Self::create_id_target(&self.device, width, height);
            memory::register_gpu(
                "gui:id texture",
                Self::id_texture_bytes(width, height),
                Subsystem::Gui,
            );
            self.id_texture = Some(id_texture);
            self.id_view = Some(id_view);
        }
    }

    /// Bytes of the full-size GUI depth texture for `width × height` at
    /// `sample_count` samples (`Depth32Float` = 4 bytes per texel).
    fn depth_texture_bytes(width: u32, height: u32, sample_count: u32) -> u64 {
        u64::from(width.max(1)) * u64::from(height.max(1)) * 4 * u64::from(sample_count)
    }

    /// Bytes of the full-size `R32Uint` id texture (`4` bytes per texel).
    fn id_texture_bytes(width: u32, height: u32) -> u64 {
        u64::from(width.max(1)) * u64::from(height.max(1)) * 4
    }

    /// Creates the single full-size depth target.
    fn create_depth_target(
        device: &wgpu::Device,
        width: u32,
        height: u32,
        sample_count: u32,
    ) -> (wgpu::Texture, wgpu::TextureView) {
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("GUI Depth Texture"),
            size: wgpu::Extent3d {
                width: width.max(1),
                height: height.max(1),
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Depth32Float,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            view_formats: &[],
        });
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        (texture, view)
    }

    /// Creates the single full-size id target used by GPU hit testing.
    fn create_id_target(
        device: &wgpu::Device,
        width: u32,
        height: u32,
    ) -> (wgpu::Texture, wgpu::TextureView) {
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("GUI ID Texture"),
            size: wgpu::Extent3d {
                width: width.max(1),
                height: height.max(1),
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::R32Uint,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        (texture, view)
    }

    /// Allocates the full-size id texture on first use, or after a resize.
    ///
    /// The id pass is separate from the visual pass, so a target that is never
    /// requested does not exist and no rendered pixel changes.  Callers that
    /// hit-test through [`Self::read_pixel_id`] or [`Self::hit_test_gpu`] do
    /// not need to call this: those methods ensure it themselves.
    pub fn ensure_id_target(&mut self) {
        let width = self.resolution[0].max(1.0) as u32;
        let height = self.resolution[1].max(1.0) as u32;
        if let (Some(texture), Some(_)) = (&self.id_texture, &self.id_view) {
            if texture.width() == width && texture.height() == height {
                return;
            }
        }
        // Drop the old target before allocating the replacement.
        self.id_view = None;
        self.id_texture = None;
        let (id_texture, id_view) = Self::create_id_target(&self.device, width, height);
        memory::register_gpu(
            "gui:id texture",
            Self::id_texture_bytes(width, height),
            Subsystem::Gui,
        );
        self.id_texture = Some(id_texture);
        self.id_view = Some(id_view);
    }

    /// Whether the lazy id target is currently allocated.
    #[must_use]
    pub fn has_id_target(&self) -> bool {
        self.id_texture.is_some() && self.id_view.is_some()
    }

    /// Enable/disable the UI image bind-group cache (audit H8).
    ///
    /// Defaults to `true`: the image bind group is rebuilt only when the
    /// texture set changes, instead of on every `prepare` that has textures.
    /// Set to `false` to restore the per-frame rebuild for A/B comparison.
    pub fn set_image_bind_group_cache(&mut self, enabled: bool) {
        self.image_bind_group_cache = enabled;
    }

    /// Whether the image bind-group cache is enabled.
    pub fn image_bind_group_cache(&self) -> bool {
        self.image_bind_group_cache
    }

    /// True when the stored key describes exactly `textures` (same views,
    /// samplers and `Arc` identities, in the same order).
    #[cfg(feature = "assets")]
    fn texture_key_matches(&self, textures: &[Arc<ferrous_assets::Texture2d>]) -> bool {
        if self.image_bind_group_key.len() != textures.len() * 3 {
            return false;
        }
        textures.iter().enumerate().all(|(i, tex)| {
            let base = i * 3;
            self.image_bind_group_key[base] == (&tex.view as *const wgpu::TextureView as usize)
                && self.image_bind_group_key[base + 1]
                    == (&tex.sampler as *const wgpu::Sampler as usize)
                && self.image_bind_group_key[base + 2] == (Arc::as_ptr(tex) as usize)
        })
    }

    #[cfg(feature = "assets")]
    fn store_texture_key(&mut self, textures: &[Arc<ferrous_assets::Texture2d>]) {
        self.image_bind_group_key.clear();
        self.image_bind_group_key.reserve(textures.len() * 3);
        for tex in textures {
            self.image_bind_group_key
                .push(&tex.view as *const wgpu::TextureView as usize);
            self.image_bind_group_key
                .push(&tex.sampler as *const wgpu::Sampler as usize);
            self.image_bind_group_key.push(Arc::as_ptr(tex) as usize);
        }
    }

    /// PREPARE phase: Uploads quad data to GPU buffers.
    /// This optimizes the submit by separating CPU logic from GPU logic.
    pub fn prepare(&mut self, queue: &wgpu::Queue, batch: &GuiBatch) {
        if !batch.quads.is_empty() {
            let bytes = batch.as_quad_bytes();
            let count = batch.quads.len() as u32;
            if count > self.max_instances {
                let new_max = count.next_power_of_two().max(64);
                self.instance_buffer = self.device.create_buffer(&wgpu::BufferDescriptor {
                    label: Some("GUI Instance Buffer (resized)"),
                    size: (std::mem::size_of::<GuiQuad>() as u32 * new_max) as u64,
                    usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                    mapped_at_creation: false,
                });
                memory::register_gpu(
                    "gui:instance instances",
                    self.instance_buffer.size(),
                    Subsystem::Gui,
                );
                self.max_instances = new_max;
            }
            queue.write_buffer(&self.instance_buffer, 0, bytes);
        }

        if !batch.text_quads.is_empty() {
            let bytes = batch.as_text_bytes();
            let count = batch.text_quads.len() as u32;
            if count > self.text_max_instances {
                let new_max = count.next_power_of_two().max(64);
                self.text_instance_buffer = self.device.create_buffer(&wgpu::BufferDescriptor {
                    label: Some("GUI Text Instance Buffer (resized)"),
                    size: (std::mem::size_of::<TextQuad>() as u32 * new_max) as u64,
                    usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                    mapped_at_creation: false,
                });
                memory::register_gpu(
                    "gui:text instances",
                    self.text_instance_buffer.size(),
                    Subsystem::Gui,
                );
                self.text_max_instances = new_max;
            }
            queue.write_buffer(&self.text_instance_buffer, 0, bytes);
        }

        if !batch.icon_quads.is_empty() {
            let bytes = batch.as_icon_bytes();
            let count = batch.icon_quads.len() as u32;
            if count > self.icon_max_instances {
                let new_max = count.next_power_of_two().max(64);
                self.icon_instance_buffer = self.device.create_buffer(&wgpu::BufferDescriptor {
                    label: Some("GUI Icon Instance Buffer (resized)"),
                    size: (std::mem::size_of::<TextQuad>() as u32 * new_max) as u64,
                    usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                    mapped_at_creation: false,
                });
                memory::register_gpu(
                    "gui:icon instances",
                    self.icon_instance_buffer.size(),
                    Subsystem::Gui,
                );
                self.icon_max_instances = new_max;
            }
            queue.write_buffer(&self.icon_instance_buffer, 0, bytes);
        }

        // Update the image bind group only when the texture set actually
        // changed.  Historically this rebuilt the group (plus two scratch
        // `Vec`s and a GPU object) on every `prepare` that had textures, i.e.
        // every frame of an immediate-mode UI (audit H8).  The cache is keyed
        // on the view/sampler/`Arc` identities, so it is invalidated whenever a
        // texture is added, removed, reordered or replaced.  `image_bind_group_cache`
        // toggles the optimization for A/B comparison.
        #[cfg(feature = "assets")]
        if !batch.textures.is_empty()
            && (!self.image_bind_group_cache || !self.texture_key_matches(&batch.textures))
        {
            let mut views = Vec::with_capacity(batch.textures.len());
            let mut samplers = Vec::with_capacity(batch.textures.len());
            for tex in &batch.textures {
                views.push(&tex.view);
                samplers.push(&tex.sampler);
            }
            while views.len() < crate::MAX_TEXTURE_SLOTS as usize {
                views.push(views.last().unwrap());
                samplers.push(samplers.last().unwrap());
            }
            #[cfg(not(target_arch = "wasm32"))]
            {
                self.image_bind_group = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
                    label: Some("GUI Image Bind Group Update"),
                    layout: &self.layouts.image_layout,
                    entries: &[
                        wgpu::BindGroupEntry {
                            binding: 0,
                            resource: wgpu::BindingResource::TextureViewArray(&views),
                        },
                        wgpu::BindGroupEntry {
                            binding: 1,
                            resource: wgpu::BindingResource::SamplerArray(&samplers),
                        },
                    ],
                });
            }
            #[cfg(target_arch = "wasm32")]
            {
                let mut entries = Vec::new();
                for i in 0..8 {
                    let v = if i < views.len() {
                        views[i]
                    } else {
                        views.last().unwrap()
                    };
                    let s = if i < samplers.len() {
                        samplers[i]
                    } else {
                        samplers.last().unwrap()
                    };
                    entries.push(wgpu::BindGroupEntry {
                        binding: (i * 2) as u32,
                        resource: wgpu::BindingResource::TextureView(v),
                    });
                    entries.push(wgpu::BindGroupEntry {
                        binding: (i * 2 + 1) as u32,
                        resource: wgpu::BindingResource::Sampler(s),
                    });
                }
                self.image_bind_group = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
                    label: Some("GUI Image Bind Group Update (Web)"),
                    layout: &self.layouts.image_layout,
                    entries: &entries,
                });
            }

            self.store_texture_key(&batch.textures);
        }

        if !batch.svg_commands.is_empty() {
            self.svg_scratch_vertices.clear();
            self.svg_scratch_indices.clear();
            let mut current_v_offset = 0;

            for cmd in &batch.svg_commands {
                for v in &cmd.mesh.vertices {
                    let mut v_shifted = *v;
                    v_shifted.position[0] += cmd.pos[0];
                    v_shifted.position[1] += cmd.pos[1];
                    v_shifted.color = cmd.color;
                    v_shifted.z_order = cmd.z;
                    self.svg_scratch_vertices.push(v_shifted);
                }
                for i in &cmd.mesh.indices {
                    self.svg_scratch_indices.push(i + current_v_offset);
                }
                current_v_offset += cmd.mesh.vertices.len() as u32;
            }

            if self.svg_scratch_vertices.len() as u32 > self.svg_max_vertices {
                let new_max = (self.svg_scratch_vertices.len() as u32)
                    .next_power_of_two()
                    .max(1024);
                self.svg_vertex_buffer = self.device.create_buffer(&wgpu::BufferDescriptor {
                    label: Some("GUI SVG Vertex Buffer (resized)"),
                    size: (new_max as usize * std::mem::size_of::<ferrous_svg::SvgVertex>()) as u64,
                    usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                    mapped_at_creation: false,
                });
                memory::register_gpu(
                    "gui:svg vertices",
                    self.svg_vertex_buffer.size(),
                    Subsystem::Gui,
                );
                self.svg_max_vertices = new_max;
            }
            if self.svg_scratch_indices.len() as u32 > self.svg_max_indices {
                let new_max = (self.svg_scratch_indices.len() as u32)
                    .next_power_of_two()
                    .max(1024);
                self.svg_index_buffer = self.device.create_buffer(&wgpu::BufferDescriptor {
                    label: Some("GUI SVG Index Buffer (resized)"),
                    size: (new_max as u64 * 4),
                    usage: wgpu::BufferUsages::INDEX | wgpu::BufferUsages::COPY_DST,
                    mapped_at_creation: false,
                });
                memory::register_gpu(
                    "gui:svg indices",
                    self.svg_index_buffer.size(),
                    Subsystem::Gui,
                );
                self.svg_max_indices = new_max;
            }

            queue.write_buffer(
                &self.svg_vertex_buffer,
                0,
                bytemuck::cast_slice(&self.svg_scratch_vertices),
            );
            queue.write_buffer(
                &self.svg_index_buffer,
                0,
                bytemuck::cast_slice(&self.svg_scratch_indices),
            );
        }

        self.prepare_segments(queue, batch);
    }

    /// Ensures the segment buffer can hold `segments` uniforms.
    fn ensure_segment_capacity(&mut self, segments: usize) {
        if segments <= self.segment_capacity {
            return;
        }
        let capacity = segments.next_power_of_two().max(INITIAL_SEGMENT_CAPACITY);
        self.segment_buffer = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("GUI Segment Uniform Buffer (resized)"),
            size: self.segment_stride * capacity as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        self.segment_capacity = capacity;
        memory::register_gpu(
            "gui:segment uniforms",
            self.segment_buffer.size(),
            Subsystem::Gui,
        );
        self.rebuild_segment_bind_group();
    }

    /// Packs the per-segment clip shapes and the backdrop blur parameters.
    ///
    /// The uniform stride (a device alignment, at most a few KiB) and the
    /// 48-byte uniform both fit `usize` and `u32` on every supported target;
    /// the segment count is bounded by the draw-segment ranges, which are
    /// already `u32`.
    #[allow(clippy::cast_possible_truncation)]
    fn prepare_segments(&mut self, queue: &wgpu::Queue, batch: &GuiBatch) {
        let count = batch.segments.len();
        if count == 0 {
            self.backdrop.this_frame = false;
            return;
        }
        self.ensure_segment_capacity(count);

        let stride = self.segment_stride as usize;
        self.segment_scratch.clear();
        self.segment_scratch.reserve(count);
        let mut radius = 0.0_f32;
        let mut has_backdrop = false;

        for (index, segment) in batch.segments.iter().enumerate() {
            let (rect, radii, enabled) = match segment.clip {
                Some(shape) if shape.is_rounded() => (shape.rect, shape.radii, 1.0_f32),
                _ => (
                    ferrous_ui_core::Rect {
                        x: 0.0,
                        y: 0.0,
                        width: 0.0,
                        height: 0.0,
                    },
                    [0.0; 4],
                    0.0_f32,
                ),
            };
            self.segment_scratch.push(SegmentUniforms {
                rect: [rect.x, rect.y, rect.width, rect.height],
                radii,
                params: [
                    enabled,
                    if self.backdrop.ready { 1.0 } else { 0.0 },
                    0.0,
                    0.0,
                ],
            });

            let offset = index * stride;
            if self.segment_bytes.len() < offset + SEGMENT_UNIFORM_BYTES as usize {
                self.segment_bytes.resize(offset + stride, 0);
            }
            let uniform = self.segment_scratch[index];
            self.segment_bytes[offset..offset + SEGMENT_UNIFORM_BYTES as usize]
                .copy_from_slice(bytemuck::bytes_of(&uniform));
        }

        for quad in &batch.quads {
            if quad.flags & crate::BACKDROP_BLUR_BIT != 0 {
                has_backdrop = true;
                radius = radius.max(quad.uv0[0].max(0.0));
            }
        }
        self.backdrop.this_frame = has_backdrop && self.backdrop.ready;

        queue.write_buffer(
            &self.segment_buffer,
            0,
            &self.segment_bytes[..count * stride],
        );

        if self.backdrop.this_frame {
            // Match the Gaussian to the level whose texel is closest to the
            // requested sigma: `level = floor(log2(sigma))`, so the kernel at
            // that level has sigma in [1, 2) texels and taps one texel apart.
            let sigma = radius / BLUR_SIGMA_DIVISOR;
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
            // reason: sigma is finite and >= 2 here, so the floor is a small
            // positive integer clamped to the chain depth.
            let level = if sigma >= 2.0 {
                sigma.log2().floor().clamp(0.0, BACKDROP_MAX_LEVEL as f32) as u32
            } else {
                1
            };
            let changed = self.set_blur_level(level);
            if changed {
                self.rebuild_segment_bind_group();
            }
            let [width, height] = self.level_size(level);
            let texel = [1.0 / width as f32, 1.0 / height as f32];
            #[allow(clippy::cast_precision_loss)]
            // reason: level <= 5, so 2^level is exact in f32.
            let level_scale = (1u32 << level) as f32;
            let (weights, taps) = BlurParams::gaussian_weights(sigma / level_scale);
            let weights = &weights[..taps];
            self.write_downsample_params(queue);
            queue.write_buffer(
                &self.horizontal_blur_uniform,
                0,
                bytemuck::bytes_of(&BlurParams::new(texel, sigma / level_scale, true, weights)),
            );
            queue.write_buffer(
                &self.vertical_blur_uniform,
                0,
                bytemuck::bytes_of(&BlurParams::new(texel, sigma / level_scale, false, weights)),
            );
        }
    }

    #[inline]
    fn bind_pipeline_cached(
        &self,
        rpass: &mut wgpu::RenderPass<'_>,
        state: &mut PassBindState,
        pipeline: &wgpu::RenderPipeline,
    ) {
        let id = pipeline as *const wgpu::RenderPipeline as u64;
        if state.pipeline != Some(id) {
            rpass.set_pipeline(pipeline);
            state.pipeline = Some(id);
            self.counters_pipeline_binds.fetch_add(1, Ordering::Relaxed);
        }
    }

    #[inline]
    fn bind_group_cached(
        &self,
        rpass: &mut wgpu::RenderPass<'_>,
        slot: u32,
        bound: &mut Option<u64>,
        bind_group: &wgpu::BindGroup,
    ) {
        let id = bind_group as *const wgpu::BindGroup as u64;
        if *bound != Some(id) {
            rpass.set_bind_group(slot, bind_group, &[]);
            *bound = Some(id);
            self.counters_bind_group_binds
                .fetch_add(1, Ordering::Relaxed);
        }
    }

    #[inline]
    fn bind_group0_cached(
        &self,
        rpass: &mut wgpu::RenderPass<'_>,
        state: &mut PassBindState,
        bind_group: &wgpu::BindGroup,
    ) {
        self.bind_group_cached(rpass, 0, &mut state.bind0, bind_group);
    }

    #[inline]
    fn bind_group1_cached(
        &self,
        rpass: &mut wgpu::RenderPass<'_>,
        state: &mut PassBindState,
        bind_group: &wgpu::BindGroup,
    ) {
        self.bind_group_cached(rpass, 1, &mut state.bind1, bind_group);
    }

    #[inline]
    fn bind_vertex0_cached(
        &self,
        rpass: &mut wgpu::RenderPass<'_>,
        state: &mut PassBindState,
        buffer: &wgpu::Buffer,
    ) {
        let id = buffer as *const wgpu::Buffer as u64;
        if state.vertex0 != Some(id) {
            rpass.set_vertex_buffer(0, buffer.slice(..));
            state.vertex0 = Some(id);
            self.counters_buffer_binds.fetch_add(1, Ordering::Relaxed);
        }
    }

    #[inline]
    fn bind_vertex1_cached(
        &self,
        rpass: &mut wgpu::RenderPass<'_>,
        state: &mut PassBindState,
        buffer: &wgpu::Buffer,
    ) {
        let id = buffer as *const wgpu::Buffer as u64;
        if state.vertex1 != Some(id) {
            rpass.set_vertex_buffer(1, buffer.slice(..));
            state.vertex1 = Some(id);
            self.counters_buffer_binds.fetch_add(1, Ordering::Relaxed);
        }
    }

    #[inline]
    fn bind_index_cached(
        &self,
        rpass: &mut wgpu::RenderPass<'_>,
        state: &mut PassBindState,
        buffer: &wgpu::Buffer,
        format: wgpu::IndexFormat,
    ) {
        let id = buffer as *const wgpu::Buffer as u64;
        if state.index != Some(id) {
            rpass.set_index_buffer(buffer.slice(..), format);
            state.index = Some(id);
            self.counters_buffer_binds.fetch_add(1, Ordering::Relaxed);
        }
    }

    #[inline]
    fn bind_segment_cached(
        &self,
        rpass: &mut wgpu::RenderPass<'_>,
        state: &mut PassBindState,
        bind_group: &wgpu::BindGroup,
        offset: u32,
    ) {
        let id = bind_group as *const wgpu::BindGroup as u64;
        if state.bind2 != Some((id, offset)) {
            rpass.set_bind_group(2, bind_group, &[offset]);
            state.bind2 = Some((id, offset));
            self.counters_bind_group_binds
                .fetch_add(1, Ordering::Relaxed);
        }
    }

    /// Dynamic-offset value for `index`.
    ///
    /// An offset cannot exceed the segment buffer (a few MiB at most) and is
    /// therefore representable as the `u32` that `set_bind_group` takes.
    #[allow(clippy::cast_possible_truncation)]
    fn segment_offset(&self, index: usize) -> u32 {
        if index < self.segment_capacity {
            (index as u64 * self.segment_stride) as u32
        } else {
            0
        }
    }

    /// Encodes the downsample chain and the two Gaussian passes.
    ///
    /// Levels `1..=L` are box-downsampled in order (the scene is the input of
    /// level 1), then the horizontal pass reads level `L` into `blur_h` and the
    /// vertical pass reads `blur_h` into `blur_v`, which the GUI quads sample.
    /// Every target is cleared, so the result is fully overwritten and no
    /// history leaks between frames.
    fn encode_backdrop_blur(&self, encoder: &mut wgpu::CommandEncoder) {
        if !self.backdrop.this_frame {
            return;
        }
        let (Some(h_view), Some(v_view), Some(h_bind), Some(v_bind)) = (
            &self.blur_h_view,
            &self.blur_v_view,
            &self.blur_h_bind_group,
            &self.blur_v_bind_group,
        ) else {
            return;
        };

        let clear = wgpu::Color {
            r: 0.0,
            g: 0.0,
            b: 0.0,
            a: 0.0,
        };
        let level_count = self.blur_level.min(BACKDROP_MAX_LEVEL) as usize;
        for level in 0..level_count {
            let (Some(view), Some(bind)) = (
                self.downsample_views.get(level),
                self.downsample_bind_groups.get(level),
            ) else {
                return;
            };
            self.run_fullscreen(
                encoder,
                "GUI Backdrop Downsample",
                view,
                &self.downsample_pipeline,
                bind,
                clear,
            );
        }
        for (label, view, bind) in [
            ("GUI Backdrop Blur Horizontal", h_view, h_bind),
            ("GUI Backdrop Blur Vertical", v_view, v_bind),
        ] {
            self.run_fullscreen(encoder, label, view, &self.blur_pipeline, bind, clear);
        }
    }

    /// Runs one fullscreen triangle into `view`, clearing it first.
    fn run_fullscreen(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        label: &str,
        view: &wgpu::TextureView,
        pipeline: &wgpu::RenderPipeline,
        bind: &wgpu::BindGroup,
        clear: wgpu::Color,
    ) {
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some(label),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(clear),
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            occlusion_query_set: None,
            timestamp_writes: None,
        });
        pass.set_pipeline(pipeline);
        pass.set_bind_group(0, bind, &[]);
        pass.draw(0..3, 0..1);
    }

    /// Fase RENDER: Ejecuta los comandos de dibujo sobre la vista proporcionada.
    pub fn render(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        target_view: &wgpu::TextureView,
        resolve_target: Option<&wgpu::TextureView>,
        batch: &GuiBatch,
        load_op: wgpu::LoadOp<wgpu::Color>,
    ) {
        self.render_with_timestamps(encoder, target_view, resolve_target, batch, load_op, None);
    }

    /// [`Self::render`] with an optional per-pass GPU timestamp pair.
    ///
    /// The caller keeps the `ferrous_renderer::gpu_timing::PassTiming` alive
    /// until the pass ends; this method only borrows its descriptor fragment.
    pub fn render_with_timestamps(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        target_view: &wgpu::TextureView,
        resolve_target: Option<&wgpu::TextureView>,
        batch: &GuiBatch,
        load_op: wgpu::LoadOp<wgpu::Color>,
        timestamp_writes: Option<wgpu::RenderPassTimestampWrites<'_>>,
    ) {
        self.reset_frame_counters();
        if batch.segments.is_empty() && matches!(load_op, wgpu::LoadOp::Load) {
            return;
        }

        // Blur the scene into the quarter-resolution targets before this pass
        // samples them. The GUI pass then overwrites the scene attachment, so
        // reading it in the earlier passes is safe. The blur passes must be
        // encoded before the GUI pass borrows `encoder`.
        self.encode_backdrop_blur(encoder);

        self.counters_passes.store(1, Ordering::Relaxed);
        let mut rpass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("GUI Render Pass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: target_view,
                resolve_target,
                ops: wgpu::Operations {
                    load: load_op,
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: self.depth_view.as_ref().map(|view| {
                wgpu::RenderPassDepthStencilAttachment {
                    view,
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Clear(1.0),
                        store: wgpu::StoreOp::Store,
                    }),
                    stencil_ops: None,
                }
            }),
            occlusion_query_set: None,
            timestamp_writes,
        });

        let (base_sx, base_sy, base_sw, base_sh) = if let Some(damage) = batch.damage_union {
            let sx = (damage.x.max(0.0) as u32).min(self.resolution[0] as u32);
            let sy = (damage.y.max(0.0) as u32).min(self.resolution[1] as u32);
            let sw = (damage.width.max(0.0) as u32).min(self.resolution[0] as u32 - sx);
            let sh = (damage.height.max(0.0) as u32).min(self.resolution[1] as u32 - sy);
            (sx, sy, sw, sh)
        } else {
            (0, 0, self.resolution[0] as u32, self.resolution[1] as u32)
        };

        if (base_sw == 0 || base_sh == 0) && matches!(load_op, wgpu::LoadOp::Load) {
            return;
        }

        // State cache for this pass.  `set_scissor_rect` (the only legitimate
        // per-segment state) does not invalidate any of these slots, so the
        // invariant bind group 0 / quad vertex buffer / Uint16 index buffer are
        // issued once and reused.  A segment that first draws SVG changes the
        // vertex slot 0 and the index buffer, so a later quad/text/icon range
        // re-establishes them through the cache instead of assuming they are
        // still bound.
        let mut state = PassBindState::default();

        for (segment_index, segment) in batch.segments.iter().enumerate() {
            self.counters_segments.fetch_add(1, Ordering::Relaxed);
            // Apply scissor if present, intersected with base_scissor.
            //
            // A segment may also carry `clip: Some(ClipShape)` with per-corner
            // radii. `set_scissor_rect` is axis-aligned, so it is only the
            // early reject; the exact rounded mask is delivered per segment
            // through group 2 and evaluated as an SDF in the fragment stage.
            let (sx, sy, sw, sh) = if let Some(s) = segment.scissor {
                let sx = (s.x.max(base_sx as f32) as u32).min(base_sx + base_sw);
                let sy = (s.y.max(base_sy as f32) as u32).min(base_sy + base_sh);
                let sw =
                    ((s.x + s.width).min((base_sx + base_sw) as f32) as u32).saturating_sub(sx);
                let sh =
                    ((s.y + s.height).min((base_sy + base_sh) as f32) as u32).saturating_sub(sy);
                (sx, sy, sw, sh)
            } else {
                (base_sx, base_sy, base_sw, base_sh)
            };

            if sw > 0 && sh > 0 {
                rpass.set_scissor_rect(sx, sy, sw, sh);
                self.counters_scissor_changes
                    .fetch_add(1, Ordering::Relaxed);
            } else {
                continue;
            }

            // Bind the per-segment clip uniform (and the blurred backdrop) for
            // every draw in this segment. The offset is the segment index in
            // the dynamic uniform buffer prepared by `prepare_segments`.
            let segment_offset = self.segment_offset(segment_index);
            self.bind_segment_cached(
                &mut rpass,
                &mut state,
                &self.segment_bind_group,
                segment_offset,
            );

            // Draw Quads (Using transparent pipeline to support blended edges/shadows safely)
            // TODO: En Fase 5 implementaremos el split Opaque/Transparent por segmento
            if !segment.quad_range.is_empty() {
                let quad_pipeline = if self.overdraw_enabled {
                    &self.overdraw_quad_pipeline
                } else {
                    &self.transparent_pipeline
                };
                self.bind_pipeline_cached(&mut rpass, &mut state, quad_pipeline);
                self.bind_group0_cached(&mut rpass, &mut state, &self.uniform_bind_group);
                self.bind_group1_cached(&mut rpass, &mut state, &self.image_bind_group);
                self.bind_vertex0_cached(&mut rpass, &mut state, &self.vertex_buffer);
                self.bind_vertex1_cached(&mut rpass, &mut state, &self.instance_buffer);
                self.bind_index_cached(
                    &mut rpass,
                    &mut state,
                    &self.index_buffer,
                    wgpu::IndexFormat::Uint16,
                );
                rpass.draw_indexed(0..6, 0, segment.quad_range.clone());
                self.counters_draw_calls.fetch_add(1, Ordering::Relaxed);
            }

            // Draw Text
            if !segment.text_range.is_empty() {
                if let Some(font_bg) = &self.font_bind_group {
                    // Shares the quad vertex buffer, Uint16 index buffer and
                    // (with icons) the text pipeline; only the instance buffer
                    // and the atlas bind group differ.
                    let text_pipeline = if self.overdraw_enabled {
                        &self.overdraw_text_pipeline
                    } else {
                        &self.text_pipeline
                    };
                    self.bind_pipeline_cached(&mut rpass, &mut state, text_pipeline);
                    self.bind_group0_cached(&mut rpass, &mut state, &self.uniform_bind_group);
                    self.bind_group1_cached(&mut rpass, &mut state, font_bg);
                    self.bind_vertex0_cached(&mut rpass, &mut state, &self.vertex_buffer);
                    self.bind_vertex1_cached(&mut rpass, &mut state, &self.text_instance_buffer);
                    self.bind_index_cached(
                        &mut rpass,
                        &mut state,
                        &self.index_buffer,
                        wgpu::IndexFormat::Uint16,
                    );
                    rpass.draw_indexed(0..6, 0, segment.text_range.clone());
                    self.counters_draw_calls.fetch_add(1, Ordering::Relaxed);
                }
            }

            // Draw Icons (Same pipeline as text, different bind group)
            if !segment.icon_range.is_empty() {
                if let Some(icon_bg) = &self.icon_bind_group {
                    let icon_pipeline = if self.overdraw_enabled {
                        &self.overdraw_text_pipeline
                    } else {
                        &self.text_pipeline
                    };
                    self.bind_pipeline_cached(&mut rpass, &mut state, icon_pipeline);
                    self.bind_group0_cached(&mut rpass, &mut state, &self.uniform_bind_group);
                    self.bind_group1_cached(&mut rpass, &mut state, icon_bg);
                    self.bind_vertex0_cached(&mut rpass, &mut state, &self.vertex_buffer);
                    self.bind_vertex1_cached(&mut rpass, &mut state, &self.icon_instance_buffer);
                    self.bind_index_cached(
                        &mut rpass,
                        &mut state,
                        &self.index_buffer,
                        wgpu::IndexFormat::Uint16,
                    );
                    rpass.draw_indexed(0..6, 0, segment.icon_range.clone());
                    self.counters_draw_calls.fetch_add(1, Ordering::Relaxed);
                }
            }

            // Draw SVG
            if !segment.svg_range.is_empty() {
                self.bind_pipeline_cached(&mut rpass, &mut state, &self.svg_pipeline);
                self.bind_group0_cached(&mut rpass, &mut state, &self.uniform_bind_group);
                // The SVG shader uses group 2 (the clip mask); wgpu requires
                // every lower group in the pipeline layout to be bound, even
                // though the shader does not sample group 1.
                self.bind_group1_cached(&mut rpass, &mut state, &self.image_bind_group);
                self.bind_vertex0_cached(&mut rpass, &mut state, &self.svg_vertex_buffer);
                self.bind_index_cached(
                    &mut rpass,
                    &mut state,
                    &self.svg_index_buffer,
                    wgpu::IndexFormat::Uint32,
                );

                // We need to calculate the index range.
                // This is a bit tricky if we don't store index offsets per command.
                // For now, let's assume one big mesh or calculate on the fly.
                let mut start_idx = 0;
                for i in 0..segment.svg_range.start {
                    start_idx += batch.svg_commands[i as usize].mesh.indices.len() as u32;
                }
                let mut end_idx = start_idx;
                for i in segment.svg_range.clone() {
                    end_idx += batch.svg_commands[i as usize].mesh.indices.len() as u32;
                }

                if start_idx < end_idx {
                    rpass.draw_indexed(start_idx..end_idx, 0, 0..1);
                    self.counters_draw_calls.fetch_add(1, Ordering::Relaxed);
                }
            }
        }
    }

    pub fn set_font_atlas(&mut self, view: &wgpu::TextureView, sampler: &wgpu::Sampler) {
        self.font_bind_group = Some(self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("GUI Font Bind Group"),
            layout: &self.layouts.font_layout,
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
        }));
    }

    pub fn set_icon_atlas(&mut self, view: &wgpu::TextureView, sampler: &wgpu::Sampler) {
        self.icon_bind_group = Some(self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("GUI Icon Bind Group"),
            layout: &self.layouts.font_layout, // Re-use font layout (1 tex + 1 sampler)
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
        }));
    }

    /// Encodes the id pass and copies the id of pixel `(x, y)` into the staging
    /// buffer, allocating the id target on first use.
    ///
    /// The returned receiver fires once the staging buffer is mapped.  Call
    /// [`Self::fetch_pixel_id`] after the command buffer has been submitted.
    pub fn read_pixel_id(
        &mut self,
        encoder: &mut wgpu::CommandEncoder,
        x: u32,
        y: u32,
        batch: &GuiBatch,
    ) -> crossbeam_channel::Receiver<u32> {
        self.ensure_id_target();
        // `ensure_id_target` always leaves both handles set; the `else`
        // branches keep the runtime path panic-free if a future implementation
        // changes that.
        let Some(id_view) = self.id_view.as_ref() else {
            return crossbeam_channel::bounded(1).1;
        };
        let Some(id_texture) = self.id_texture.as_ref() else {
            return crossbeam_channel::bounded(1).1;
        };

        if !batch.quads.is_empty() {
            let mut id_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("GUI ID Pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: id_view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                occlusion_query_set: None,
                timestamp_writes: None,
            });

            id_pass.set_pipeline(&self.id_pipeline);
            id_pass.set_bind_group(0, &self.uniform_bind_group, &[]);
            id_pass.set_vertex_buffer(0, self.vertex_buffer.slice(..));
            id_pass.set_vertex_buffer(1, self.instance_buffer.slice(..));
            id_pass.set_index_buffer(self.index_buffer.slice(..), wgpu::IndexFormat::Uint16);

            for segment in &batch.segments {
                if !segment.quad_range.is_empty() {
                    id_pass.draw_indexed(0..6, 0, segment.quad_range.clone());
                }
            }
        }

        encoder.copy_texture_to_buffer(
            wgpu::ImageCopyTexture {
                texture: id_texture,
                mip_level: 0,
                origin: wgpu::Origin3d { x, y, z: 0 },
                aspect: wgpu::TextureAspect::All,
            },
            wgpu::ImageCopyBuffer {
                buffer: &self.id_staging_buffer,
                layout: wgpu::ImageDataLayout {
                    offset: 0,
                    bytes_per_row: None,
                    rows_per_image: None,
                },
            },
            wgpu::Extent3d {
                width: 1,
                height: 1,
                depth_or_array_layers: 1,
            },
        );

        let (sender, receiver) = crossbeam_channel::bounded(1);
        let slice = self.id_staging_buffer.slice(..);
        slice.map_async(wgpu::MapMode::Read, move |v| {
            if v.is_ok() {
                let _ = sender.send(1);
            }
        });

        receiver
    }

    /// Reads the id staged by the last [`Self::read_pixel_id`] call.
    ///
    /// Returns `0` when the id target was never allocated, so a stray call
    /// cannot touch an unmapped buffer.
    pub fn fetch_pixel_id(&self) -> u32 {
        if self.id_texture.is_none() {
            return 0;
        }
        let view = self.id_staging_buffer.slice(..).get_mapped_range();
        let id_val = u32::from_le_bytes([view[0], view[1], view[2], view[3]]);
        drop(view);
        self.id_staging_buffer.unmap();
        id_val
    }

    /// Orchestrates a GPU-based hit test for a single pixel.
    /// This is normally called during Event Dispatch if AABB check returns custom widgets.
    pub fn hit_test_gpu(
        &mut self,
        _queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        x: u32,
        y: u32,
        batch: &GuiBatch,
    ) -> u32 {
        // In a real environment, this should be asynchronous to avoid blocking CPU.
        // For editor tools (GUIMaker), 1-frame latency is acceptable.
        let _receiver = self.read_pixel_id(encoder, x, y, batch);
        // El encoder debe enviarse antes de recibir.
        // Como no tenemos el control del submit aquí, asumimos que se llamará después.
        // En una implementación perfecta, esto se divide.
        0 // Placeholder: El usuario debe llamar a fetch_pixel_id tras el submit.
    }
}
