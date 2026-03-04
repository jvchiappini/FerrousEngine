/// Depth-Normal Prepass
///
/// Renders all opaque geometry into a full-resolution `Rgba16Float` texture
/// called `normal_depth_texture`.  The RGB channels store the view-space
/// normal packed into [0, 1] and the A channel stores the linear
/// view-space depth (positive, i.e. `-view_pos.z`).
///
/// This texture is consumed by the SSAO pass which runs immediately after.
///
/// ## Bind group layout (mirrors prepass.wgsl)
///
/// | Group | Binding | Resource                                   |
/// |-------|---------|--------------------------------------------|
/// |   0   |    0    | `PrepassCamera` uniform buffer             |
/// |   1   |    0    | `Model` dynamic uniform buffer             |
///
/// The prepass camera uniform includes the raw **view** and **projection**
/// matrices so the shader can transform positions and normals into view space.
use std::sync::Arc;

use bytemuck::{Pod, Zeroable};
use wgpu::util::DeviceExt;
use wgpu::{
    BindGroupLayout, CommandEncoder, Device, LoadOp, Operations, Queue, RenderPassColorAttachment,
    RenderPassDepthStencilAttachment, RenderPassDescriptor, RenderPipeline, StoreOp, TextureView,
};

use crate::geometry::Vertex;
use crate::graph::{FramePacket, RenderPass};
use crate::resources::texture::{self, RenderTextureDesc};

// ── GPU-facing camera struct (prepass variant) ────────────────────────────────

/// Camera uniform uploaded specifically for the prepass.  Includes the
/// raw view and projection matrices (not just `view_proj`) so the shader
/// can transform into view space.
#[repr(C)]
#[derive(Copy, Clone, Pod, Zeroable)]
pub struct PrepassCameraUniform {
    pub view: [[f32; 4]; 4],
    pub proj: [[f32; 4]; 4],
    pub view_proj: [[f32; 4]; 4],
    pub eye_pos: [f32; 4],
}

// ── Normal-depth texture ──────────────────────────────────────────────────────

/// Full-resolution `Rgba16Float` render target that holds packed normals
/// and linear depth.  Created/resized by the prepass.
pub struct NormalDepthTexture {
    pub texture: wgpu::Texture,
    pub view: wgpu::TextureView,
    pub sampler: wgpu::Sampler,
    pub width: u32,
    pub height: u32,
}

impl NormalDepthTexture {
    pub const FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba16Float;

    pub fn new(device: &Device, width: u32, height: u32) -> Self {
        let texture = texture::create_render_texture(
            device,
            &RenderTextureDesc {
                label: "Normal-Depth Texture",
                width,
                height,
                format: Self::FORMAT,
                sample_count: 1,
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT
                    | wgpu::TextureUsages::TEXTURE_BINDING,
            },
        );
        let view = texture::default_view(&texture);
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("Normal-Depth Sampler"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::FilterMode::Nearest,
            ..Default::default()
        });
        Self {
            texture,
            view,
            sampler,
            width,
            height,
        }
    }

    pub fn resize(&mut self, device: &Device, width: u32, height: u32) {
        if self.width == width && self.height == height {
            return;
        }
        *self = Self::new(device, width, height);
    }
}

// ── Prepass ───────────────────────────────────────────────────────────────────

pub struct PrepassCamera {
    pub buffer: wgpu::Buffer,
    pub bind_group: Arc<wgpu::BindGroup>,
    pub layout: Arc<BindGroupLayout>,
}

impl PrepassCamera {
    pub fn new(device: &Device) -> Self {
        let layout = Arc::new(
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("Prepass Camera BGL"),
                entries: &[wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::VERTEX | wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                }],
            }),
        );

        let zero_cam = PrepassCameraUniform {
            view: glam::Mat4::IDENTITY.to_cols_array_2d(),
            proj: glam::Mat4::IDENTITY.to_cols_array_2d(),
            view_proj: glam::Mat4::IDENTITY.to_cols_array_2d(),
            eye_pos: [0.0; 4],
        };

        let buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Prepass Camera Buffer"),
            contents: bytemuck::bytes_of(&zero_cam),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });

        let bind_group = Arc::new(device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Prepass Camera BG"),
            layout: &layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: buffer.as_entire_binding(),
            }],
        }));

        Self {
            buffer,
            bind_group,
            layout,
        }
    }

    pub fn update(&self, queue: &Queue, uniform: &PrepassCameraUniform) {
        queue.write_buffer(&self.buffer, 0, bytemuck::bytes_of(uniform));
    }
}

// ── Prepass render pass ───────────────────────────────────────────────────────

pub struct PrePass {
    pub normal_depth: NormalDepthTexture,

    pipeline: Arc<RenderPipeline>,
    /// BGL for the per-object dynamic-uniform path (group 1 = model matrix).
    #[allow(dead_code)]
    model_layout: Arc<BindGroupLayout>,
    /// BGL for the instanced storage-buffer path (group 1 = instance array).
    instance_layout: Arc<BindGroupLayout>,

    /// Camera resources (separate from the main camera BG because we need
    /// view + proj separately).
    prepass_camera: PrepassCamera,

    /// Depth buffer borrowed from the main render target (same texture,
    /// not owned — the main pass clears it immediately after we write).
    /// We keep a depth texture of our own so the prepass can clear it
    /// independently without conflicting with the main depth pass.
    depth_texture: wgpu::Texture,
    depth_view: wgpu::TextureView,

    // Shared buffers from WorldPass (set by the renderer after construction)
    model_bind_group: Option<Arc<wgpu::BindGroup>>,
    model_stride: u32,
    instance_bind_group: Option<Arc<wgpu::BindGroup>>,
    instanced_pipeline: Arc<RenderPipeline>,
}

impl PrePass {
    pub fn new(
        device: &Device,
        model_layout: Arc<BindGroupLayout>,
        instance_layout: Arc<BindGroupLayout>,
        width: u32,
        height: u32,
    ) -> Self {
        let normal_depth = NormalDepthTexture::new(device, width, height);

        let prepass_camera = PrepassCamera::new(device);

        // ── Pipeline for per-object (dynamic offset) geometry ─────────────────
        let pipeline = Self::build_pipeline(device, &prepass_camera.layout, &model_layout, false);
        // ── Instanced pipeline: group 1 = storage buffer (not dynamic uniform) ─
        let instanced_pipeline =
            Self::build_pipeline(device, &prepass_camera.layout, &instance_layout, true);

        // ── Dedicated depth target for the prepass ────────────────────────────
        let (depth_texture, depth_view) = Self::make_depth(device, width, height);

        Self {
            normal_depth,
            pipeline: Arc::new(pipeline),
            model_layout,
            instance_layout,
            prepass_camera,
            depth_texture,
            depth_view,
            model_bind_group: None,
            model_stride: 256,
            instance_bind_group: None,
            instanced_pipeline: Arc::new(instanced_pipeline),
        }
    }

    // ── Public setters (called by Renderer) ───────────────────────────────────

    pub fn set_model_buffer(&mut self, bind_group: Arc<wgpu::BindGroup>, stride: u32) {
        self.model_bind_group = Some(bind_group);
        self.model_stride = stride;
    }

    pub fn set_instance_buffer(&mut self, bind_group: Arc<wgpu::BindGroup>) {
        self.instance_bind_group = Some(bind_group);
    }

    /// Sync the prepass camera from the main camera matrices.
    pub fn update_camera(
        &self,
        queue: &Queue,
        view: glam::Mat4,
        proj: glam::Mat4,
        eye: glam::Vec3,
    ) {
        let uniform = PrepassCameraUniform {
            view: view.to_cols_array_2d(),
            proj: proj.to_cols_array_2d(),
            view_proj: (proj * view).to_cols_array_2d(),
            eye_pos: [eye.x, eye.y, eye.z, 0.0],
        };
        self.prepass_camera.update(queue, &uniform);
    }

    // ── Private helpers ───────────────────────────────────────────────────────

    fn build_pipeline(
        device: &Device,
        camera_layout: &BindGroupLayout,
        group1_layout: &BindGroupLayout,
        instanced: bool,
    ) -> RenderPipeline {
        // Instanced path uses a separate shader with a storage-buffer at group 1.
        let shader = if instanced {
            device.create_shader_module(wgpu::include_wgsl!(
                "../../../../assets/shaders/prepass_instanced.wgsl"
            ))
        } else {
            device.create_shader_module(wgpu::include_wgsl!(
                "../../../../assets/shaders/prepass.wgsl"
            ))
        };

        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("Prepass Pipeline Layout"),
            bind_group_layouts: &[camera_layout, group1_layout],
            push_constant_ranges: &[],
        });

        device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some(if instanced {
                "Prepass Pipeline (instanced)"
            } else {
                "Prepass Pipeline"
            }),
            layout: Some(&layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                buffers: &[Vertex::layout()],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_main"),
                targets: &[Some(wgpu::ColorTargetState {
                    format: NormalDepthTexture::FORMAT,
                    blend: None,
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                front_face: wgpu::FrontFace::Ccw,
                cull_mode: Some(wgpu::Face::Back),
                ..Default::default()
            },
            depth_stencil: Some(wgpu::DepthStencilState {
                format: wgpu::TextureFormat::Depth32Float,
                depth_write_enabled: true,
                depth_compare: wgpu::CompareFunction::Less,
                stencil: wgpu::StencilState::default(),
                bias: wgpu::DepthBiasState::default(),
            }),
            multisample: wgpu::MultisampleState {
                count: 1,
                mask: !0,
                alpha_to_coverage_enabled: false,
            },
            multiview: None,
            cache: None,
        })
    }

    fn make_depth(device: &Device, width: u32, height: u32) -> (wgpu::Texture, wgpu::TextureView) {
        let tex = texture::create_render_texture(
            device,
            &RenderTextureDesc {
                label: "Prepass Depth",
                width,
                height,
                format: wgpu::TextureFormat::Depth32Float,
                sample_count: 1,
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            },
        );
        let view = texture::default_view(&tex);
        (tex, view)
    }
}

impl RenderPass for PrePass {
    fn name(&self) -> &str {
        "Depth-Normal Prepass"
    }

    fn on_resize(&mut self, device: &Device, _queue: &Queue, width: u32, height: u32) {
        self.normal_depth.resize(device, width, height);
        let (dt, dv) = Self::make_depth(device, width, height);
        self.depth_texture = dt;
        self.depth_view = dv;
    }

    fn prepare(&mut self, _device: &Device, _queue: &Queue, _packet: &FramePacket) {}

    fn execute(
        &mut self,
        _device: &Device,
        _queue: &Queue,
        encoder: &mut CommandEncoder,
        _color_view: &TextureView,
        _resolve_target: Option<&TextureView>,
        _depth_view: Option<&TextureView>,
        packet: &FramePacket,
    ) {
        let mut rpass = encoder.begin_render_pass(&RenderPassDescriptor {
            label: Some("Prepass"),
            color_attachments: &[Some(RenderPassColorAttachment {
                view: &self.normal_depth.view,
                resolve_target: None,
                ops: Operations {
                    load: LoadOp::Clear(wgpu::Color {
                        r: 0.5,
                        g: 0.5,
                        b: 1.0,
                        a: 0.0,
                    }),
                    store: StoreOp::Store,
                },
            })],
            depth_stencil_attachment: Some(RenderPassDepthStencilAttachment {
                view: &self.depth_view,
                depth_ops: Some(Operations {
                    load: LoadOp::Clear(1.0),
                    store: StoreOp::Store,
                }),
                stencil_ops: None,
            }),
            occlusion_query_set: None,
            timestamp_writes: None,
        });

        // ── Instanced path ─────────────────────────────────────────────────
        if let (Some(inst_bg), true) = (
            &self.instance_bind_group,
            !packet.instanced_objects.is_empty(),
        ) {
            rpass.set_pipeline(&self.instanced_pipeline);
            rpass.set_bind_group(0, self.prepass_camera.bind_group.as_ref(), &[]);
            rpass.set_bind_group(1, inst_bg.as_ref(), &[]);
            for cmd in &packet.instanced_objects {
                rpass.set_vertex_buffer(0, cmd.vertex_buffer.slice(..));
                rpass.set_index_buffer(cmd.index_buffer.slice(..), cmd.index_format);
                rpass.draw_indexed(
                    0..cmd.index_count,
                    0,
                    cmd.first_instance..cmd.first_instance + cmd.instance_count,
                );
            }
        }

        // ── Per-object path ────────────────────────────────────────────────
        if let (Some(model_bg), true) = (&self.model_bind_group, !packet.scene_objects.is_empty()) {
            rpass.set_pipeline(&self.pipeline);
            rpass.set_bind_group(0, self.prepass_camera.bind_group.as_ref(), &[]);
            for cmd in &packet.scene_objects {
                let offset = (cmd.model_slot as u32).wrapping_mul(self.model_stride);
                rpass.set_bind_group(1, model_bg.as_ref(), &[offset]);
                rpass.set_vertex_buffer(0, cmd.vertex_buffer.slice(..));
                rpass.set_index_buffer(cmd.index_buffer.slice(..), cmd.index_format);
                rpass.draw_indexed(0..cmd.index_count, 0, 0..1);
            }
        }
    }
}
