use std::num::NonZeroU64;
use std::sync::Arc;

use wgpu::util::DeviceExt;

/// Representa un rectángulo de la UI (un "quad").
///
/// Todos los campos están en coordenadas de píxeles: `pos` es la esquina
/// superior izquierda, `size` el ancho/alto y `color` es un RGBA con componentes
/// en el rango 0.0..1.0.
#[repr(C)]
#[derive(Copy, Clone, bytemuck::Pod, bytemuck::Zeroable, Debug)]
pub struct GuiQuad {
    pub pos: [f32; 2],
    pub size: [f32; 2],
    pub color: [f32; 4],
}

/// Lote de `GuiQuad` que será enviado al GPU en un draw call único.
///
/// El `GuiRenderer` consumirá un `GuiBatch` para rellenar el buffer de
/// instancias antes de emitir el paso de renderizado.
pub struct GuiBatch {
    quads: Vec<GuiQuad>,
}

impl GuiBatch {
    pub fn new() -> Self {
        Self { quads: Vec::new() }
    }

    pub fn clear(&mut self) {
        self.quads.clear();
    }

    pub fn push(&mut self, quad: GuiQuad) {
        self.quads.push(quad);
    }

    pub fn len(&self) -> usize {
        self.quads.len()
    }

    pub fn is_empty(&self) -> bool {
        self.quads.is_empty()
    }

    pub(crate) fn as_bytes(&self) -> &[u8] {
        bytemuck::cast_slice(&self.quads)
    }
}

/// Motor de renderizado de UI que sabe dibujar `GuiBatch` sobre una textura
/// existente.
pub struct GuiRenderer {
    /// guardamos un `Arc` del dispositivo para recrear buffers cuando sea
    /// necesario (por ejemplo si cambiamos el número máximo de instancias).
    device: Arc<wgpu::Device>,
    pipeline: wgpu::RenderPipeline,
    vertex_buffer: wgpu::Buffer,
    index_buffer: wgpu::Buffer,
    instance_buffer: wgpu::Buffer,
    uniform_buffer: wgpu::Buffer,
    uniform_bind_group: wgpu::BindGroup,
    max_instances: u32,
    resolution: [f32; 2],
}

impl GuiRenderer {
    /// Crea un `GuiRenderer` inicializado para una determinada resolución de
    /// pantalla y formato de destino.
    pub fn new(
        device: Arc<wgpu::Device>,
        format: wgpu::TextureFormat,
        max_instances: u32,
        width: u32,
        height: u32,
    ) -> Self {
        // buffers de vértice/índice para un quad unitario (0..1 en UV)
        let vertices: &[f32] = &[
            0.0, 0.0, // bottom-left
            1.0, 0.0, // bottom-right
            1.0, 1.0, // top-right
            0.0, 1.0, // top-left
        ];

        let indices: &[u16] = &[0, 1, 2, 2, 3, 0];

        let vertex_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("GUI Quad Vertex Buffer"),
            contents: bytemuck::cast_slice(vertices),
            usage: wgpu::BufferUsages::VERTEX,
        });

        let index_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("GUI Quad Index Buffer"),
            contents: bytemuck::cast_slice(indices),
            usage: wgpu::BufferUsages::INDEX,
        });

        let instance_size = std::mem::size_of::<GuiQuad>() as u64;
        let instance_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("GUI Instance Buffer"),
            size: instance_size * max_instances as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        // uniform para la resolución de pantalla
        #[repr(C)]
        #[derive(Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
        struct Uniforms {
            resolution: [f32; 2],
        }
        let resolution = [width as f32, height as f32];
        let uniforms = Uniforms { resolution };
        let uniform_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("GUI Uniform Buffer"),
            contents: bytemuck::bytes_of(&uniforms),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });

        let uniform_bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("GUI Uniform Bind Group Layout"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: NonZeroU64::new(std::mem::size_of::<Uniforms>() as u64),
                },
                count: None,
            }],
        });

        let uniform_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("GUI Uniform Bind Group"),
            layout: &uniform_bind_group_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: uniform_buffer.as_entire_binding(),
            }],
        });

        let shader = device.create_shader_module(wgpu::include_wgsl!("../../../assets/shaders/gui.wgsl"));

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("GUI Pipeline Layout"),
            bind_group_layouts: &[&uniform_bind_group_layout],
            push_constant_ranges: &[],
        });

        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("GUI Render Pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                buffers: &[
                    // vertex buffer: uv coords
                    wgpu::VertexBufferLayout {
                        array_stride: std::mem::size_of::<[f32; 2]>() as wgpu::BufferAddress,
                        step_mode: wgpu::VertexStepMode::Vertex,
                        attributes: &[wgpu::VertexAttribute {
                            format: wgpu::VertexFormat::Float32x2,
                            offset: 0,
                            shader_location: 0,
                        }],
                    },
                    // instance buffer: GuiQuad
                    wgpu::VertexBufferLayout {
                        array_stride: instance_size,
                        step_mode: wgpu::VertexStepMode::Instance,
                        attributes: &[
                            wgpu::VertexAttribute {
                                // pos
                                format: wgpu::VertexFormat::Float32x2,
                                offset: 0,
                                shader_location: 1,
                            },
                            wgpu::VertexAttribute {
                                // size
                                format: wgpu::VertexFormat::Float32x2,
                                offset: 8,
                                shader_location: 2,
                            },
                            wgpu::VertexAttribute {
                                // color
                                format: wgpu::VertexFormat::Float32x4,
                                offset: 16,
                                shader_location: 3,
                            },
                        ],
                    },
                ],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_main"),
                targets: &[Some(wgpu::ColorTargetState {
                    format,
                    blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            }),
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview: None,
            cache: None,
        });

        Self {
            device: device.clone(),
            pipeline,
            vertex_buffer,
            index_buffer,
            instance_buffer,
            uniform_buffer,
            uniform_bind_group,
            max_instances,
            resolution,
        }
    }

    /// Notifica al renderer que la resolución ha cambiado.
    pub fn resize(&mut self, queue: &wgpu::Queue, width: u32, height: u32) {
        self.resolution = [width as f32, height as f32];
        #[repr(C)]
        #[derive(Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
        struct Uniforms {
            resolution: [f32; 2],
        }
        let uniforms = Uniforms {
            resolution: self.resolution,
        };
        queue.write_buffer(&self.uniform_buffer, 0, bytemuck::bytes_of(&uniforms));
    }

    /// Emite los comandos necesarios para dibujar el contenido del
    /// `GuiBatch` sobre la vista indicada.
    pub fn render(
        &mut self,
        encoder: &mut wgpu::CommandEncoder,
        view: &wgpu::TextureView,
        batch: &GuiBatch,
        queue: &wgpu::Queue,
    ) {
        if batch.is_empty() {
            return;
        }

        let instance_bytes = batch.as_bytes();
        let required_instances = batch.len() as u32;

        // si la capacidad no alcanza, re-creamos el buffer
        if required_instances > self.max_instances {
            let new_size = std::mem::size_of::<GuiQuad>() as u64 * required_instances as u64;
            self.instance_buffer = self.device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("GUI Instance Buffer (resized)"),
                size: new_size,
                usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
            self.max_instances = required_instances;
        }

        queue.write_buffer(&self.instance_buffer, 0, instance_bytes);

        let mut rpass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("GUI Render Pass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Load,
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            occlusion_query_set: None,
            timestamp_writes: None,
        });

        rpass.set_pipeline(&self.pipeline);
        rpass.set_bind_group(0, &self.uniform_bind_group, &[]);
        rpass.set_vertex_buffer(0, self.vertex_buffer.slice(..));
        rpass.set_vertex_buffer(1, self.instance_buffer.slice(..));
        rpass.set_index_buffer(self.index_buffer.slice(..), wgpu::IndexFormat::Uint16);
        rpass.draw_indexed(0..6, 0, 0..required_instances);
    }
}
