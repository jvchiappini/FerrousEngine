use crate::gpu_types::GuiQuad;
use wgpu::{PipelineCompilationOptions, RenderPipeline};

// `gui.wgsl` reads locations 1..=10, whose last attribute (`z_order`) ends at
// byte 92. The instance stride stays at the full 96 bytes because the same
// buffer feeds the ID pass, which additionally reads `node_id` at 92..96.
const _: () = {
    assert!(std::mem::size_of::<GuiQuad>() == 96);
    assert!(std::mem::offset_of!(GuiQuad, z_order) == 88);
    assert!(std::mem::offset_of!(GuiQuad, node_id) == 92);
};

/// The shared rounded-clip mask, prepended to both backend shader bodies.
const CLIP: &str = include_str!("../../../../assets/shaders/gui_clip_common.wgsl");

/// The native quad shader body (`binding_array` textures).
#[cfg(not(target_arch = "wasm32"))]
const QUAD_BODY: &str = include_str!("../../../../assets/shaders/gui.wgsl");

/// The Web-compatible quad shader body (separate texture bindings).
#[cfg(any(target_arch = "wasm32", test))]
const QUAD_WEB_BODY: &str = include_str!("../../../../assets/shaders/gui_web.wgsl");

/// Assembles a backend shader body with the shared rounded-clip mask.
fn assemble(clip: &str, body: &str) -> String {
    format!("{clip}\n{body}")
}

/// The quad shader source for the current backend.
///
/// Both backends receive the shared `gui_clip_common.wgsl` mask, so a rounded
/// clip cuts quads, text and SVG identically on native and on the web.
#[cfg(not(target_arch = "wasm32"))]
fn quad_shader_source() -> String {
    assemble(CLIP, QUAD_BODY)
}

/// The quad shader source for the current backend.
///
/// Both backends receive the shared `gui_clip_common.wgsl` mask, so a rounded
/// clip cuts quads, text and SVG identically on native and on the web.
#[cfg(target_arch = "wasm32")]
fn quad_shader_source() -> String {
    assemble(CLIP, QUAD_WEB_BODY)
}

/// Builds the quad shader module, so a second renderer can reuse it instead of
/// compiling the same WGSL again (Phase 8E popup sharing).
#[must_use]
pub(crate) fn quad_shader(device: &wgpu::Device) -> wgpu::ShaderModule {
    device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("GUI Shader"),
        source: wgpu::ShaderSource::Wgsl(quad_shader_source().into()),
    })
}

#[must_use]
#[allow(clippy::too_many_lines)] // reason: one flat pipeline descriptor table
pub fn create_quad_pipeline(
    device: &wgpu::Device,
    format: wgpu::TextureFormat,
    pipeline_layout: &wgpu::PipelineLayout,
    sample_count: u32,
    depth_write_enabled: bool,
    depth_format: Option<wgpu::TextureFormat>,
    cache: Option<&wgpu::PipelineCache>,
) -> RenderPipeline {
    create_quad_pipeline_with_shader(
        device,
        format,
        pipeline_layout,
        sample_count,
        depth_write_enabled,
        depth_format,
        cache,
        &quad_shader(device),
    )
}

/// [`create_quad_pipeline`] reusing an already-created shader module.
#[must_use]
#[allow(clippy::too_many_arguments, clippy::too_many_lines)] // reason: one flat pipeline descriptor table
pub fn create_quad_pipeline_with_shader(
    device: &wgpu::Device,
    format: wgpu::TextureFormat,
    pipeline_layout: &wgpu::PipelineLayout,
    sample_count: u32,
    depth_write_enabled: bool,
    depth_format: Option<wgpu::TextureFormat>,
    cache: Option<&wgpu::PipelineCache>,
    shader: &wgpu::ShaderModule,
) -> RenderPipeline {
    let instance_size = std::mem::size_of::<GuiQuad>() as u64;

    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("GUI Render Pipeline"),
        layout: Some(pipeline_layout),
        vertex: wgpu::VertexState {
            module: shader,
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
                        // offset  0: pos      [f32; 2]
                        wgpu::VertexAttribute {
                            format: wgpu::VertexFormat::Float32x2,
                            offset: 0,
                            shader_location: 1,
                        },
                        // offset  8: size     [f32; 2]
                        wgpu::VertexAttribute {
                            format: wgpu::VertexFormat::Float32x2,
                            offset: 8,
                            shader_location: 2,
                        },
                        // offset 16: uv0      [f32; 2]
                        wgpu::VertexAttribute {
                            format: wgpu::VertexFormat::Float32x2,
                            offset: 16,
                            shader_location: 3,
                        },
                        // offset 24: uv1      [f32; 2]
                        wgpu::VertexAttribute {
                            format: wgpu::VertexFormat::Float32x2,
                            offset: 24,
                            shader_location: 4,
                        },
                        // offset 32: color    [f32; 4]
                        wgpu::VertexAttribute {
                            format: wgpu::VertexFormat::Float32x4,
                            offset: 32,
                            shader_location: 5,
                        },
                        // offset 48: color_b  [f32; 4]
                        wgpu::VertexAttribute {
                            format: wgpu::VertexFormat::Float32x4,
                            offset: 48,
                            shader_location: 6,
                        },
                        // offset 64: radii    [f32; 4]
                        wgpu::VertexAttribute {
                            format: wgpu::VertexFormat::Float32x4,
                            offset: 64,
                            shader_location: 7,
                        },
                        // offset 80: tex_index u32
                        wgpu::VertexAttribute {
                            format: wgpu::VertexFormat::Uint32,
                            offset: 80,
                            shader_location: 8,
                        },
                        // offset 84: flags    u32
                        wgpu::VertexAttribute {
                            format: wgpu::VertexFormat::Uint32,
                            offset: 84,
                            shader_location: 9,
                        },
                        // offset 88: z_order  f32
                        wgpu::VertexAttribute {
                            format: wgpu::VertexFormat::Float32,
                            offset: 88,
                            shader_location: 10,
                        },
                    ],
                },
            ],
            compilation_options: PipelineCompilationOptions::default(),
        },
        fragment: Some(wgpu::FragmentState {
            module: shader,
            entry_point: Some("fs_main"),
            targets: &[Some(wgpu::ColorTargetState {
                format,
                blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                write_mask: wgpu::ColorWrites::ALL,
            })],
            compilation_options: PipelineCompilationOptions::default(),
        }),
        primitive: wgpu::PrimitiveState::default(),
        depth_stencil: depth_format.map(|format| wgpu::DepthStencilState {
            format,
            depth_write_enabled,
            depth_compare: wgpu::CompareFunction::LessEqual,
            stencil: wgpu::StencilState::default(),
            bias: wgpu::DepthBiasState::default(),
        }),
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
    use super::{assemble, CLIP, QUAD_WEB_BODY};

    #[test]
    fn the_web_quad_shader_applies_the_rounded_clip() {
        // The web body must call the shared mask in every fragment path; the
        // mask itself is prepended by `quad_shader_source` on wasm32.
        assert!(CLIP.contains("fn clip_coverage("));
        assert!(
            QUAD_WEB_BODY.contains("clip_coverage(in.clip_pos.xy)"),
            "the web quad shader must sample the rounded clip"
        );
        let assembled = assemble(CLIP, QUAD_WEB_BODY);
        assert!(assembled.contains("@group(2) @binding(0)"));
        assert!(assembled.contains("struct SegmentUniforms"));
        assert!(assembled.contains("clip_coverage(in.clip_pos.xy)"));
    }
}
