//! Runtime overdraw debug view for the GUI/2D pass (prompt 18 P5).
//!
//! The view replaces the normal quad and text pipelines with a diagnostic pair
//! that adds a fixed, low-intensity heat colour per fragment using additive
//! blending.  Overlapping primitives brighten monotonically, so a pixel that
//! accumulated `n` fragments shows `n * 0.02` in red and green.
//!
//! It is **opt-in** and default-off: `OverdrawConfig::default()` disables it,
//! so rendering, goldens and public behaviour are unchanged unless an
//! orchestrator explicitly enables it.  The pipelines share the existing
//! instance layouts and bind-group layouts:
//!
//! * [`create_overdraw_pipeline`] consumes the GUI quad pipeline layout
//!   (`GuiQuad`, 96-byte stride) and can be bound instead of
//!   `GuiRenderer::transparent_pipeline`.
//! * [`create_overdraw_text_pipeline`] consumes the GUI text pipeline layout
//!   (`TextQuad`, 56-byte stride) and can be bound instead of
//!   `GuiRenderer::text_pipeline` for glyphs and icons.
//!
//! Both pipelines write no depth and compare `Always`, so the depth buffer
//! cannot hide overlapping fragments.  The existing GUI pass may keep its
//! depth clear for early-z; the diagnostic simply ignores the depth attachment
//! and must not add a clear or a depth write of its own.
//!
//! The heat constant lives in `assets/shaders/gui_overdraw.wgsl` (WGSL cannot
//! read Rust constants).  [`OVERDRAW_HEAT`] mirrors it and a unit test keeps
//! the two in sync.

use crate::gpu_types::{GuiQuad, TextQuad};
use wgpu::RenderPipeline;

/// Heat added per fragment, in linear red and green.
///
/// Mirrors `OVERDRAW_HEAT` in `assets/shaders/gui_overdraw.wgsl`; the shader
/// constant is authoritative, this copy is the value exposed to Rust callers
/// and pinned by a test.
pub const OVERDRAW_HEAT: f32 = 0.02;

/// The full overdraw shader source, shared by the quad and text pipelines.
const OVERDRAW_SHADER: &str = include_str!("../../../../assets/shaders/gui_overdraw.wgsl");

/// Runtime toggle and default heat intensity for the overdraw view.
///
/// The orchestrator stores this next to `Renderer2dConfig` / `GuiRenderer` and
/// consults it before choosing a pipeline.  `enabled` is `false` by default so
/// the debug view never affects a normal render or a golden.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct OverdrawConfig {
    /// When `true`, the GUI/2D path draws with the overdraw pipelines.
    pub enabled: bool,
    /// Heat added per fragment.  The shader currently bakes
    /// [`OVERDRAW_HEAT`]; a different value is documented but not uploaded
    /// until the pipeline grows a dedicated uniform.
    pub intensity: f32,
}

impl OverdrawConfig {
    /// A disabled config with the shader's heat intensity.
    #[must_use]
    pub const fn off() -> Self {
        Self {
            enabled: false,
            intensity: OVERDRAW_HEAT,
        }
    }

    /// An enabled config with the shader's heat intensity.
    #[must_use]
    pub const fn on() -> Self {
        Self {
            enabled: true,
            intensity: OVERDRAW_HEAT,
        }
    }

    /// `true` when the debug view should be drawn this frame.
    #[must_use]
    pub const fn is_active(&self) -> bool {
        self.enabled
    }
}

impl Default for OverdrawConfig {
    fn default() -> Self {
        Self::off()
    }
}

/// CPU-side bookkeeping for one overdraw frame.
///
/// This does not count GPU fragments (the heat map is the visual counter); it
/// records how much geometry was rerouted through the diagnostic pipelines so
/// a harness can confirm the view actually ran.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct OverdrawCounters {
    /// Whether the view was active for the frame.
    pub enabled: bool,
    /// Draw segments (scissor changes) routed through the overdraw pipelines.
    pub segments: u32,
    /// Quads, glyphs and icons drawn with the heat shader.
    pub primitives: u32,
}

impl OverdrawCounters {
    /// Starts a new frame, keeping the configuration state.
    pub fn begin_frame(&mut self, config: OverdrawConfig) {
        self.enabled = config.is_active();
        self.segments = 0;
        self.primitives = 0;
    }

    /// Records one segment that drew `primitives` heat primitives.
    pub fn record_segment(&mut self, primitives: u32) {
        self.segments = self.segments.saturating_add(1);
        self.primitives = self.primitives.saturating_add(primitives);
    }
}

/// Owns the diagnostic pipelines and their configuration.
///
/// The renderer keeps one of these when the view is enabled (or always, and
/// simply never binds the pipelines while disabled).  Rebuilding on a
/// format/sample-count change uses the same pipeline cache as the normal GUI
/// pipelines.
pub struct OverdrawView {
    /// Quad pipeline (`GuiQuad` layout); bind instead of the normal quad
    /// pipeline.
    pub quad: RenderPipeline,
    /// Text/icon pipeline (`TextQuad` layout); bind instead of the normal text
    /// pipeline.
    pub text: RenderPipeline,
    /// Toggle and intensity.
    pub config: OverdrawConfig,
    /// Per-frame bookkeeping.
    pub counters: OverdrawCounters,
}

impl OverdrawView {
    /// Builds both diagnostic pipelines for the current target format.
    ///
    /// `quad_layout` is the existing GUI quad pipeline layout and `text_layout`
    /// the existing GUI text pipeline layout; no new layouts are created.
    #[must_use]
    pub fn new(
        device: &wgpu::Device,
        format: wgpu::TextureFormat,
        quad_layout: &wgpu::PipelineLayout,
        text_layout: &wgpu::PipelineLayout,
        sample_count: u32,
        cache: Option<&wgpu::PipelineCache>,
        config: OverdrawConfig,
    ) -> Self {
        Self {
            quad: create_overdraw_pipeline(
                device,
                format,
                quad_layout,
                sample_count,
                Some(wgpu::TextureFormat::Depth32Float),
                cache,
            ),
            text: create_overdraw_text_pipeline(
                device,
                format,
                text_layout,
                sample_count,
                Some(wgpu::TextureFormat::Depth32Float),
                cache,
            ),
            config,
            counters: OverdrawCounters::default(),
        }
    }
}

/// Additive blend state used by the overdraw pipelines.
///
/// Kept in a function so tests can assert the factors without inspecting a
/// built `RenderPipeline`.
fn overdraw_blend() -> wgpu::BlendState {
    let additive = wgpu::BlendComponent {
        src_factor: wgpu::BlendFactor::One,
        dst_factor: wgpu::BlendFactor::One,
        operation: wgpu::BlendOperation::Add,
    };
    wgpu::BlendState {
        color: additive,
        alpha: additive,
    }
}

fn overdraw_shader(device: &wgpu::Device) -> wgpu::ShaderModule {
    device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("GUI Overdraw Shader"),
        source: wgpu::ShaderSource::Wgsl(OVERDRAW_SHADER.into()),
    })
}

fn overdraw_depth_state(format: wgpu::TextureFormat) -> wgpu::DepthStencilState {
    wgpu::DepthStencilState {
        format,
        depth_write_enabled: false,
        depth_compare: wgpu::CompareFunction::Always,
        stencil: wgpu::StencilState::default(),
        bias: wgpu::DepthBiasState::default(),
    }
}

/// Creates the overdraw pipeline for GUI quads (`GuiQuad` instances).
///
/// The pipeline matches [`crate::pipelines::quad`]'s vertex/instance layout
/// and uses the caller's existing pipeline layout, so it can be swapped in
/// without a new bind-group layout.  It uses additive blending, writes no
/// depth and compares depth `Always`, and the fragment shader ignores the
/// instance colours.
#[must_use]
pub fn create_overdraw_pipeline(
    device: &wgpu::Device,
    format: wgpu::TextureFormat,
    layout: &wgpu::PipelineLayout,
    sample_count: u32,
    depth_format: Option<wgpu::TextureFormat>,
    cache: Option<&wgpu::PipelineCache>,
) -> RenderPipeline {
    // The instance stride must stay 96 bytes: the same buffer feeds the normal
    // quad and ID passes.
    const _: () = {
        assert!(std::mem::size_of::<GuiQuad>() == 96);
    };
    let shader = overdraw_shader(device);
    let instance_size = std::mem::size_of::<GuiQuad>() as u64;

    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("GUI Overdraw Pipeline"),
        layout: Some(layout),
        vertex: wgpu::VertexState {
            module: &shader,
            entry_point: Some("vs_main"),
            buffers: &[
                wgpu::VertexBufferLayout {
                    array_stride: std::mem::size_of::<[f32; 2]>() as wgpu::BufferAddress,
                    step_mode: wgpu::VertexStepMode::Vertex,
                    attributes: &[wgpu::VertexAttribute {
                        format: wgpu::VertexFormat::Float32x2,
                        offset: 0,
                        shader_location: 0,
                    }],
                },
                wgpu::VertexBufferLayout {
                    array_stride: instance_size,
                    step_mode: wgpu::VertexStepMode::Instance,
                    attributes: &[
                        wgpu::VertexAttribute {
                            format: wgpu::VertexFormat::Float32x2,
                            offset: 0,
                            shader_location: 1,
                        },
                        wgpu::VertexAttribute {
                            format: wgpu::VertexFormat::Float32x2,
                            offset: 8,
                            shader_location: 2,
                        },
                        wgpu::VertexAttribute {
                            format: wgpu::VertexFormat::Float32x2,
                            offset: 16,
                            shader_location: 3,
                        },
                        wgpu::VertexAttribute {
                            format: wgpu::VertexFormat::Float32x2,
                            offset: 24,
                            shader_location: 4,
                        },
                        wgpu::VertexAttribute {
                            format: wgpu::VertexFormat::Float32x4,
                            offset: 32,
                            shader_location: 5,
                        },
                        wgpu::VertexAttribute {
                            format: wgpu::VertexFormat::Float32x4,
                            offset: 48,
                            shader_location: 6,
                        },
                        wgpu::VertexAttribute {
                            format: wgpu::VertexFormat::Float32x4,
                            offset: 64,
                            shader_location: 7,
                        },
                        wgpu::VertexAttribute {
                            format: wgpu::VertexFormat::Uint32,
                            offset: 80,
                            shader_location: 8,
                        },
                        wgpu::VertexAttribute {
                            format: wgpu::VertexFormat::Uint32,
                            offset: 84,
                            shader_location: 9,
                        },
                        wgpu::VertexAttribute {
                            format: wgpu::VertexFormat::Float32,
                            offset: 88,
                            shader_location: 10,
                        },
                    ],
                },
            ],
            compilation_options: Default::default(),
        },
        fragment: Some(wgpu::FragmentState {
            module: &shader,
            entry_point: Some("fs_main"),
            targets: &[Some(wgpu::ColorTargetState {
                format,
                blend: Some(overdraw_blend()),
                write_mask: wgpu::ColorWrites::ALL,
            })],
            compilation_options: Default::default(),
        }),
        primitive: wgpu::PrimitiveState::default(),
        depth_stencil: depth_format.map(overdraw_depth_state),
        multisample: wgpu::MultisampleState {
            count: sample_count,
            ..Default::default()
        },
        multiview: None,
        cache,
    })
}

/// Creates the overdraw pipeline for glyphs and icons (`TextQuad` instances).
///
/// Same heat colour and additive blending as [`create_overdraw_pipeline`], but
/// with the text instance stride and the caller's text pipeline layout.
#[must_use]
pub fn create_overdraw_text_pipeline(
    device: &wgpu::Device,
    format: wgpu::TextureFormat,
    layout: &wgpu::PipelineLayout,
    sample_count: u32,
    depth_format: Option<wgpu::TextureFormat>,
    cache: Option<&wgpu::PipelineCache>,
) -> RenderPipeline {
    const _: () = {
        assert!(std::mem::size_of::<TextQuad>() == 56);
    };
    let shader = overdraw_shader(device);
    let instance_size = std::mem::size_of::<TextQuad>() as u64;

    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("GUI Overdraw Text Pipeline"),
        layout: Some(layout),
        vertex: wgpu::VertexState {
            module: &shader,
            entry_point: Some("vs_text"),
            buffers: &[
                wgpu::VertexBufferLayout {
                    array_stride: std::mem::size_of::<[f32; 2]>() as wgpu::BufferAddress,
                    step_mode: wgpu::VertexStepMode::Vertex,
                    attributes: &[wgpu::VertexAttribute {
                        format: wgpu::VertexFormat::Float32x2,
                        offset: 0,
                        shader_location: 0,
                    }],
                },
                wgpu::VertexBufferLayout {
                    array_stride: instance_size,
                    step_mode: wgpu::VertexStepMode::Instance,
                    attributes: &[
                        wgpu::VertexAttribute {
                            format: wgpu::VertexFormat::Float32x2,
                            offset: 0,
                            shader_location: 1,
                        },
                        wgpu::VertexAttribute {
                            format: wgpu::VertexFormat::Float32x2,
                            offset: 8,
                            shader_location: 2,
                        },
                        wgpu::VertexAttribute {
                            format: wgpu::VertexFormat::Float32x2,
                            offset: 16,
                            shader_location: 3,
                        },
                        wgpu::VertexAttribute {
                            format: wgpu::VertexFormat::Float32x2,
                            offset: 24,
                            shader_location: 4,
                        },
                        wgpu::VertexAttribute {
                            format: wgpu::VertexFormat::Float32x4,
                            offset: 32,
                            shader_location: 5,
                        },
                        wgpu::VertexAttribute {
                            format: wgpu::VertexFormat::Float32,
                            offset: 48,
                            shader_location: 6,
                        },
                        wgpu::VertexAttribute {
                            format: wgpu::VertexFormat::Uint32,
                            offset: 52,
                            shader_location: 7,
                        },
                    ],
                },
            ],
            compilation_options: Default::default(),
        },
        fragment: Some(wgpu::FragmentState {
            module: &shader,
            entry_point: Some("fs_text"),
            targets: &[Some(wgpu::ColorTargetState {
                format,
                blend: Some(overdraw_blend()),
                write_mask: wgpu::ColorWrites::ALL,
            })],
            compilation_options: Default::default(),
        }),
        primitive: wgpu::PrimitiveState::default(),
        depth_stencil: depth_format.map(overdraw_depth_state),
        multisample: wgpu::MultisampleState {
            count: sample_count,
            ..Default::default()
        },
        multiview: None,
        cache,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn config_defaults_to_off_and_keeps_the_heat() {
        let config = OverdrawConfig::default();
        assert!(!config.enabled);
        assert!(!config.is_active());
        assert_eq!(config.intensity, OVERDRAW_HEAT);
        assert!(OverdrawConfig::on().is_active());
        assert_eq!(OverdrawConfig::on().intensity, OVERDRAW_HEAT);
    }

    #[test]
    fn blend_is_additive_one_one() {
        let blend = overdraw_blend();
        assert_eq!(blend.color.src_factor, wgpu::BlendFactor::One);
        assert_eq!(blend.color.dst_factor, wgpu::BlendFactor::One);
        assert_eq!(blend.color.operation, wgpu::BlendOperation::Add);
        assert_eq!(blend.alpha.src_factor, wgpu::BlendFactor::One);
        assert_eq!(blend.alpha.dst_factor, wgpu::BlendFactor::One);
        assert_eq!(blend.alpha.operation, wgpu::BlendOperation::Add);
    }

    #[test]
    fn depth_is_never_written_or_tested() {
        let depth = overdraw_depth_state(wgpu::TextureFormat::Depth32Float);
        assert!(!depth.depth_write_enabled);
        assert_eq!(depth.depth_compare, wgpu::CompareFunction::Always);
        assert_eq!(depth.format, wgpu::TextureFormat::Depth32Float);
    }

    #[test]
    fn shader_declares_the_heat_and_an_alpha_one_payload() {
        assert!(OVERDRAW_SHADER.contains("const OVERDRAW_HEAT: f32 = 0.02;"));
        assert!(OVERDRAW_SHADER.contains("vec4<f32>(OVERDRAW_HEAT, OVERDRAW_HEAT, 0.0, 1.0)"));
        assert!(OVERDRAW_SHADER.contains("fn vs_main("));
        assert!(OVERDRAW_SHADER.contains("fn vs_text("));
    }

    #[test]
    fn counters_reset_per_frame_and_accumulate() {
        let mut counters = OverdrawCounters::default();
        counters.begin_frame(OverdrawConfig::on());
        assert!(counters.enabled);
        counters.record_segment(6);
        counters.record_segment(3);
        assert_eq!(counters.segments, 2);
        assert_eq!(counters.primitives, 9);

        counters.begin_frame(OverdrawConfig::off());
        assert!(!counters.enabled);
        assert_eq!(counters.segments, 0);
        assert_eq!(counters.primitives, 0);
    }
}
