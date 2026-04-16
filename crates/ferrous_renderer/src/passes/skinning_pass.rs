//! GPU Skinning Pass
//!
//! Transforms mesh vertices according to bone transforms using compute shaders.

use wgpu::{
    BindGroup, BindGroupLayout, CommandEncoder, ComputePipeline, Device,
};

pub struct SkinningPass {
    pipeline: ComputePipeline,
    group_layout0: BindGroupLayout, // Vertices + Influences
    group_layout1: BindGroupLayout, // Palette
    group_layout2: BindGroupLayout, // Output
}

impl SkinningPass {
    pub fn new(device: &Device) -> Self {
        let shader = device.create_shader_module(wgpu::include_wgsl!("../../../../assets/shaders/skinning.wgsl"));

        let group_layout0 = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("Skinning Inputs BGL"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Storage { read_only: true },
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Storage { read_only: true },
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
            ],
        });

        let group_layout1 = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("Skinning Palette BGL"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::COMPUTE,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            }],
        });

        let group_layout2 = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("Skinning Output BGL"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::COMPUTE,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Storage { read_only: false },
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            }],
        });

        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("Skinning Pipeline Layout"),
            bind_group_layouts: &[&group_layout0, &group_layout1, &group_layout2],
            push_constant_ranges: &[],
        });

        let pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some("Skinning Pipeline"),
            layout: Some(&layout),
            module: &shader,
            entry_point: Some("main"),
            compilation_options: wgpu::PipelineCompilationOptions::default(),
            cache: None,
        });

        Self {
            pipeline,
            group_layout0,
            group_layout1,
            group_layout2,
        }
    }

    pub fn dispatch(
        &self,
        encoder: &mut CommandEncoder,
        vertex_count: u32,
        bg0: &BindGroup,
        bg1: &BindGroup,
        bg2: &BindGroup,
    ) {
        let mut cpass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
            label: Some("Skinning Dispatch"),
            timestamp_writes: None,
        });
        cpass.set_pipeline(&self.pipeline);
        cpass.set_bind_group(0, bg0, &[]);
        cpass.set_bind_group(1, bg1, &[]);
        cpass.set_bind_group(2, bg2, &[]);
        
        let x = vertex_count.div_ceil(64);
        cpass.dispatch_workgroups(x, 1, 1);
    }
}
