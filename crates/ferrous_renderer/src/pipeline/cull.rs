/// GPU-driven frustum culling compute pipeline.
///
/// Wraps the `cull.wgsl` compute shader and exposes a typed API for
/// creating the pipeline and tracking its bind-group layouts.
///
/// ## Typical usage
///
/// ```rust,ignore
/// let cull = GpuCullPipeline::new(device, &layouts);
/// // Build bind groups, then:
/// cull.dispatch(encoder, &bind_groups, instance_count);
/// ```
use std::sync::Arc;

use crate::pipeline::PipelineLayouts;

/// Compiled compute pipeline for per-instance GPU frustum culling.
pub struct GpuCullPipeline {
    /// The wgpu compute pipeline object.
    pub pipeline: Arc<wgpu::ComputePipeline>,
}

impl GpuCullPipeline {
    /// Creates the `GpuCullPipeline` from the embedded `cull.wgsl` shader source.
    pub fn new(device: &wgpu::Device, layouts: &PipelineLayouts) -> Self {
        let shader_source = include_str!("../../../../assets/shaders/cull.wgsl");

        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Shader: cull.wgsl"),
            source: wgpu::ShaderSource::Wgsl(shader_source.into()),
        });

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("Layout: GpuCullPipeline"),
            bind_group_layouts: &[
                &layouts.cull_instances,    // group(0): input instances (RO)
                &layouts.cull_indirect,     // group(1): draw_cmds (RO) + counters (RW)
                &layouts.cull_out_instances,// group(2): output instance matrices (RW)
                &layouts.cull_params,       // group(3): CullParams uniform
            ],
            push_constant_ranges: &[],
        });

        let pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some("Pipeline: GpuCull"),
            layout: Some(&pipeline_layout),
            module: &shader,
            entry_point: Some("main"),
            compilation_options: wgpu::PipelineCompilationOptions::default(),
            cache: None,
        });

        Self {
            pipeline: Arc::new(pipeline),
        }
    }

    /// Dispatches the culling compute shader.
    ///
    /// `encoder`       — current command encoder.
    /// `bind_groups`   — exactly 4 bind groups: [instances, indirect+counters, out_instances, params].
    /// `instance_count` — total number of instances to process. The dispatch
    ///                    covers `ceil(instance_count / 64)` workgroups.
    pub fn dispatch(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        bind_groups: [&wgpu::BindGroup; 4],
        instance_count: u32,
    ) {
        if instance_count == 0 {
            return;
        }
        let workgroups = (instance_count + 63) / 64;
        let mut cpass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
            label: Some("CullPass: compute"),
            timestamp_writes: None,
        });
        cpass.set_pipeline(&self.pipeline);
        for (i, bg) in bind_groups.iter().enumerate() {
            cpass.set_bind_group(i as u32, *bg, &[]);
        }
        cpass.dispatch_workgroups(workgroups, 1, 1);
    }
}
