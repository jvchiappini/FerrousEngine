use wgpu::{CommandEncoder, ComputePassDescriptor, Device, Queue, TextureView};

use crate::graph::{FramePacket, RenderPass};
use crate::pipeline::ComputePipeline;

/// A generic pass for executing compute shader workloads seamlessly inside the render graph.
///
/// Despite being part of the `RenderPass` trait pipeline, this pass explicitly opens a
/// `wgpu::ComputePass` inside the `execute` method, issuing dispatch commands instead of draw calls.
/// This makes it ideal for Raymarching, particle simulations, or voxel data generation.
pub struct ComputePass {
    name: String,
    pipeline: ComputePipeline,
    workgroup_count: (u32, u32, u32),
    bind_groups: Vec<wgpu::BindGroup>,
}

impl ComputePass {
    /// Creates a new generic Compute Pass.
    ///
    /// The `workgroup_count` specifies how many workgroups to dispatch in X, Y, and Z.
    pub fn new(
        name: impl Into<String>,
        pipeline: ComputePipeline,
        workgroup_count: (u32, u32, u32),
        bind_groups: Vec<wgpu::BindGroup>,
    ) -> Self {
        Self {
            name: name.into(),
            pipeline,
            workgroup_count,
            bind_groups,
        }
    }

    /// Update the bind groups dynamically (e.g., swapping ping-pong buffers).
    pub fn set_bind_groups(&mut self, bind_groups: Vec<wgpu::BindGroup>) {
        self.bind_groups = bind_groups;
    }

    /// Update the workgroup dispatch dimensions dynamically.
    pub fn set_workgroup_count(&mut self, x: u32, y: u32, z: u32) {
        self.workgroup_count = (x, y, z);
    }
}

impl RenderPass for ComputePass {
    fn name(&self) -> &str {
        &self.name
    }

    fn prepare(&mut self, _device: &Device, _queue: &Queue, _packet: &FramePacket) {
        // Any CPU-to-GPU buffer writing or state updates needed before the pass executes
        // can be performed here.
    }

    fn execute(
        &mut self,
        _device: &Device,
        _queue: &Queue,
        encoder: &mut CommandEncoder,
        _color_view: &TextureView,
        _resolve_target: Option<&TextureView>,
        _depth_view: Option<&TextureView>,
        _packet: &FramePacket,
    ) {
        let mut cpass = encoder.begin_compute_pass(&ComputePassDescriptor {
            label: Some(self.name()),
            timestamp_writes: None,
        });

        cpass.set_pipeline(&self.pipeline.inner);
        
        for (i, bind_group) in self.bind_groups.iter().enumerate() {
            cpass.set_bind_group(i as u32, bind_group, &[]);
        }

        // Issue the compute dispatch command
        cpass.dispatch_workgroups(
            self.workgroup_count.0,
            self.workgroup_count.1,
            self.workgroup_count.2,
        );
    }
}
