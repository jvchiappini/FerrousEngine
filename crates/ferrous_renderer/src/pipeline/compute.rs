use std::sync::Arc;

/// A generic wrapper for a WGPU Compute Pipeline.
///
/// This provides a common abstraction for running compute shaders (e.g. for raymarching, 
/// particle simulations, or voxel grids) independently from the traditional vertex/fragment graph.
#[derive(Clone)]
pub struct ComputePipeline {
    pub inner: Arc<wgpu::ComputePipeline>,
}

impl ComputePipeline {
    /// Creates a new `ComputePipeline` from a given shader string and bind group layouts.
    ///
    /// The entry point is assumed to be `"main"` by default, but can be configured in your WGSL.
    pub fn new(
        device: &wgpu::Device,
        shader_source: &str,
        bind_group_layouts: &[&wgpu::BindGroupLayout],
        entry_point: &str,
        label: Option<&str>,
    ) -> Self {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: label.map(|l| format!("{}_shader", l)).as_deref(),
            source: wgpu::ShaderSource::Wgsl(shader_source.into()),
        });

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: label.map(|l| format!("{}_layout", l)).as_deref(),
            bind_group_layouts,
            push_constant_ranges: &[],
        });

        let pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label,
            layout: Some(&pipeline_layout),
            module: &shader,
            entry_point: Some(entry_point),
            compilation_options: wgpu::PipelineCompilationOptions::default(),
            cache: None,
        });

        Self {
            inner: Arc::new(pipeline),
        }
    }
}
