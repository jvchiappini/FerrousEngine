//! GPU Particle Pass
//!
//! Manages simulation and rendering of millions of particles using
//! compute shaders and vertex pulling.

use bytemuck::{Pod, Zeroable};
use wgpu::{
    BindGroup, BindGroupLayout, CommandEncoder, ComputePipeline, Device,
    Queue, RenderPipeline,
};


#[repr(C)]
#[derive(Copy, Clone, Debug, Pod, Zeroable)]
struct Particle {
    position: [f32; 3],
    life: f32,
    velocity: [f32; 3],
    size: f32,
}

#[repr(C)]
#[derive(Copy, Clone, Debug, Pod, Zeroable)]
struct EmitterParams {
    origin: [f32; 3],
    spawn_count: u32,
    direction: [f32; 3],
    randomness: f32,
    gravity: [f32; 3],
    lifetime: f32,
    delta_time: f32,
    max_particles: u32,
    time: f32,
    _pad2: u32, // pad to 16-byte alignment
}

pub struct ParticleSystem {
    particles_buffer: wgpu::Buffer,
    emitter_buffer: wgpu::Buffer,
    
    update_pipeline: ComputePipeline,
    render_pipeline: RenderPipeline,
    
    bind_group0: BindGroup,        // Emitter Uniforms
    bind_group1_update: BindGroup, // Particles Storage (RW)
    bind_group1_render: BindGroup, // Particles Storage (RO)
    
    max_particles: u32,
    current_time: f32,
}

impl ParticleSystem {
    pub fn new(device: &Device, camera_layout: &BindGroupLayout, max_particles: u32, sample_count: u32) -> Self {
        // 1. Create Buffers
        let particles_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Particles Storage Buffer"),
            size: (std::mem::size_of::<Particle>() * max_particles as usize) as u64,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let emitter_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Emitter Uniform Buffer"),
            size: std::mem::size_of::<EmitterParams>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        // 2. Layouts
        let bgl0 = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("Emitter BGL"),
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

        // Compute BGL: needs write access
        let update_bgl1 = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("Update Particles BGL"),
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

        // Render BGL: must be read-only for vertex stage
        let render_bgl1 = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("Render Particles BGL"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Storage { read_only: true },
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            }],
        });

        // 3. Update Pipeline (Compute)
        let update_shader = device.create_shader_module(wgpu::include_wgsl!("../../../../assets/shaders/particles_update.wgsl"));
        let update_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("Particle Update Layout"),
            bind_group_layouts: &[&bgl0, &update_bgl1],
            push_constant_ranges: &[],
        });
        let update_pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some("Particle Update Pipeline"),
            layout: Some(&update_layout),
            module: &update_shader,
            entry_point: Some("main"),
            compilation_options: wgpu::PipelineCompilationOptions::default(),
            cache: None,
        });

        // 4. Render Pipeline
        let render_shader = device.create_shader_module(wgpu::include_wgsl!("../../../../assets/shaders/particles_render.wgsl"));
        let render_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("Particle Render Layout"),
            bind_group_layouts: &[camera_layout, &render_bgl1],
            push_constant_ranges: &[],
        });
        
        let render_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Particle Render Pipeline"),
            layout: Some(&render_layout),
            vertex: wgpu::VertexState {
                module: &render_shader,
                entry_point: Some("vs_main"),
                buffers: &[],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &render_shader,
                entry_point: Some("fs_main"),
                targets: &[Some(wgpu::ColorTargetState {
                    format: wgpu::TextureFormat::Rgba16Float, // HDR
                    blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                ..Default::default()
            },
            depth_stencil: Some(wgpu::DepthStencilState {
                format: wgpu::TextureFormat::Depth32Float,
                depth_write_enabled: false,
                depth_compare: wgpu::CompareFunction::Less,
                stencil: wgpu::StencilState::default(),
                bias: wgpu::DepthBiasState::default(),
            }),
            multisample: wgpu::MultisampleState {
                count: sample_count,
                mask: !0,
                alpha_to_coverage_enabled: false,
            },
            multiview: None,
            cache: None,
        });

        // 5. Bind Groups
        let bind_group0 = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Emitter Bind Group"),
            layout: &bgl0,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: emitter_buffer.as_entire_binding(),
            }],
        });

        let bind_group1_update = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Particles Update Bind Group"),
            layout: &update_bgl1,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: particles_buffer.as_entire_binding(),
            }],
        });

        let bind_group1_render = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Particles Render Bind Group"),
            layout: &render_bgl1,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: particles_buffer.as_entire_binding(),
            }],
        });

        Self {
            particles_buffer,
            emitter_buffer,
            update_pipeline,
            render_pipeline,
            bind_group0,
            bind_group1_update,
            bind_group1_render,
            max_particles,
            current_time: 0.0,
        }
    }

    pub fn update(&mut self, queue: &Queue, dt: f32, origin: [f32; 3], spawn_rate: f32) {
        self.current_time += dt;
        let params = EmitterParams {
            origin,
            spawn_count: (spawn_rate * dt) as u32,
            direction: [0.0, 3.0, 0.0],
            randomness: 1.0,
            gravity: [0.0, -9.81, 0.0],
            lifetime: 2.0,
            delta_time: dt,
            max_particles: self.max_particles,
            time: self.current_time,
            _pad2: 0,
        };
        queue.write_buffer(&self.emitter_buffer, 0, bytemuck::bytes_of(&params));
    }

    pub fn run_compute(&self, encoder: &mut CommandEncoder) {
        let mut cpass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
            label: Some("Particle Update Pass"),
            timestamp_writes: None,
        });
        cpass.set_pipeline(&self.update_pipeline);
        cpass.set_bind_group(0, &self.bind_group0, &[]);
        cpass.set_bind_group(1, &self.bind_group1_update, &[]);
        
        let x = self.max_particles.div_ceil(64);
        cpass.dispatch_workgroups(x, 1, 1);
    }

    pub fn run_render<'a>(
        &'a self,
        rpass: &mut wgpu::RenderPass<'a>,
        camera_bg: &'a wgpu::BindGroup,
    ) {
        rpass.set_pipeline(&self.render_pipeline);
        rpass.set_bind_group(0, camera_bg, &[]);
        rpass.set_bind_group(1, &self.bind_group1_render, &[]);
        rpass.draw(0..6, 0..self.max_particles);
    }
}
