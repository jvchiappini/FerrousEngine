//! Fullscreen backdrop-blur pipelines: the 2x box downsample that prefilters the
//! scene and the separable Gaussian that blurs it at the matched level.
//!
//! The downsample runs once per level on the way down (scene -> level 1 -> ...
//! -> level L); the Gaussian runs twice (horizontal, then vertical) at level L.
//! Both pipelines share the same bind group layout (source texture, sampler,
//! parameters) and the same fullscreen vertex entry point.

/// Builds the fullscreen box-downsample pipeline for `format`.
#[must_use]
pub fn create_downsample_pipeline(
    device: &wgpu::Device,
    format: wgpu::TextureFormat,
    pipeline_layout: &wgpu::PipelineLayout,
    cache: Option<&wgpu::PipelineCache>,
) -> wgpu::RenderPipeline {
    create_pipeline(
        device,
        format,
        pipeline_layout,
        cache,
        "GUI Backdrop Downsample Shader",
        "GUI Backdrop Downsample Pipeline",
        "fs_downsample",
    )
}

/// Builds the fullscreen separable blur pipeline for `format`.
#[must_use]
pub fn create_blur_pipeline(
    device: &wgpu::Device,
    format: wgpu::TextureFormat,
    pipeline_layout: &wgpu::PipelineLayout,
    cache: Option<&wgpu::PipelineCache>,
) -> wgpu::RenderPipeline {
    create_pipeline(
        device,
        format,
        pipeline_layout,
        cache,
        "GUI Backdrop Blur Shader",
        "GUI Backdrop Blur Pipeline",
        "fs_blur",
    )
}

fn create_pipeline(
    device: &wgpu::Device,
    format: wgpu::TextureFormat,
    pipeline_layout: &wgpu::PipelineLayout,
    cache: Option<&wgpu::PipelineCache>,
    shader_label: &str,
    pipeline_label: &str,
    fragment_entry: &str,
) -> wgpu::RenderPipeline {
    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some(shader_label),
        source: wgpu::ShaderSource::Wgsl(
            include_str!("../../../../assets/shaders/backdrop_blur.wgsl").into(),
        ),
    });

    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some(pipeline_label),
        layout: Some(pipeline_layout),
        vertex: wgpu::VertexState {
            module: &shader,
            entry_point: Some("vs_main"),
            buffers: &[],
            compilation_options: wgpu::PipelineCompilationOptions::default(),
        },
        fragment: Some(wgpu::FragmentState {
            module: &shader,
            entry_point: Some(fragment_entry),
            targets: &[Some(wgpu::ColorTargetState {
                format,
                blend: Some(wgpu::BlendState::REPLACE),
                write_mask: wgpu::ColorWrites::ALL,
            })],
            compilation_options: wgpu::PipelineCompilationOptions::default(),
        }),
        primitive: wgpu::PrimitiveState::default(),
        depth_stencil: None,
        multisample: wgpu::MultisampleState::default(),
        multiview: None,
        cache,
    })
}
