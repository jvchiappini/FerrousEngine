use crate::gpu_types::GuiQuad;
use crate::image_binding::ImageBindingMode;
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

/// The bindless quad shader body (`binding_array` textures).
const QUAD_BODY: &str = include_str!("../../../../assets/shaders/gui.wgsl");

/// The compatibility quad shader body (separate texture bindings).
const QUAD_WEB_BODY: &str = include_str!("../../../../assets/shaders/gui_web.wgsl");

/// Assembles a backend shader body with the shared rounded-clip mask.
fn assemble(clip: &str, body: &str) -> String {
    format!("{clip}\n{body}")
}

/// The quad shader source for a binding strategy.
///
/// Both strategies receive the shared `gui_clip_common.wgsl` mask, so a rounded
/// clip cuts quads, text and SVG identically on either path.
fn quad_shader_source(mode: ImageBindingMode) -> String {
    assemble(
        CLIP,
        if mode.is_bindless() {
            QUAD_BODY
        } else {
            QUAD_WEB_BODY
        },
    )
}

/// Builds the quad shader module, so a second renderer can reuse it instead of
/// compiling the same WGSL again (Phase 8E popup sharing).
///
/// The source is selected from the device's enabled features: a device with
/// the descriptor-array features gets `gui.wgsl`, any other (GL/WebGL2) gets
/// the separate-binding `gui_web.wgsl`.
#[must_use]
pub(crate) fn quad_shader(device: &wgpu::Device) -> wgpu::ShaderModule {
    let mode = ImageBindingMode::for_features(device.features());
    device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some(if mode.is_bindless() {
            "GUI Shader (bindless)"
        } else {
            "GUI Shader (separate)"
        }),
        source: wgpu::ShaderSource::Wgsl(quad_shader_source(mode).into()),
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
    use super::{assemble, quad_shader_source, CLIP, QUAD_WEB_BODY};
    use crate::image_binding::ImageBindingMode;

    #[test]
    fn the_shader_is_selected_from_the_binding_mode() {
        let bindless = quad_shader_source(ImageBindingMode::Bindless);
        assert!(bindless.contains("var gui_textures: binding_array"));
        assert!(bindless.contains("gui_textures"));

        let separate = quad_shader_source(ImageBindingMode::Separate);
        // Check the declaration, not the word: `gui_web.wgsl`'s header comment
        // says "instead of binding_array", which a bare substring test caught.
        assert!(!separate.contains("var gui_textures: binding_array"));
        assert!(separate.contains("var tex0: texture_2d<f32>;"));
    }

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
