//! `ferrous_ui_render` — Backend de renderizado GPU para la UI de FerrousEngine con WGPU.
//!
//! Este crate actúa como el traductor final entre el árbol de UI abstracto y las APIs gráficas.
//! Se encarga de la gestión de lotes (batching), sombreadores (shaders) y pipelines de renderizado.
//! Está optimizado para minimizar las llamadas de dibujo (Draw Calls) mediante el agrupamiento
//! masivo de primitivas.

use std::num::NonZeroU64;
use std::sync::Arc;
use wgpu::util::DeviceExt;

/// Número máximo de texturas distintas que se pueden referenciar en un solo lote de dibujo.
pub const MAX_TEXTURE_SLOTS: u32 = 8;

/// Flag que indica que el quad debe muestrear una textura del array de texturas.
pub const TEXTURED_BIT: u32 = 1 << 1;

/// Representación exacta en memoria de un rectángulo de la UI para la GPU.
#[repr(C)]
#[derive(Copy, Clone, bytemuck::Pod, bytemuck::Zeroable, Debug)]
pub struct GuiQuad {
    /// [x, y] en píxeles.
    pub pos: [f32; 2],
    /// [ancho, alto] en píxeles.
    pub size: [f32; 2],
    /// Coordenadas UV de origen (esquina superior izquierda).
    pub uv0: [f32; 2],
    /// Coordenadas UV de destino (esquina inferior derecha).
    pub uv1: [f32; 2],
    pub color: [f32; 4],
    /// Radios de las cuatro esquinas para bordes redondeados.
    pub radii: [f32; 4],
    /// Índice de la textura dentro del lote actual.
    pub tex_index: u32,
    /// Máscara de bits para configurar el sombreador (ej. texturizado, degradado, bordes).
    pub flags: u32,
}

/// Agrupación de `GuiQuad`s que comparten un estado de renderizado común.
/// Permite enviar cientos de rectángulos a la GPU en una sola operación `draw`.
#[derive(Clone)]
pub struct GuiBatch {
    quads: Vec<GuiQuad>,
    #[cfg(feature = "assets")]
    textures: Vec<std::sync::Arc<ferrous_assets::Texture2d>>,
}

impl GuiBatch {
    pub fn new() -> Self {
        Self {
            quads: Vec::new(),
            #[cfg(feature = "assets")]
            textures: Vec::new(),
        }
    }

    pub fn clear(&mut self) {
        self.quads.clear();
        #[cfg(feature = "assets")]
        {
            self.textures.clear();
        }
    }

    pub fn push(&mut self, quad: GuiQuad) {
        self.quads.push(quad);
    }

    /// Registra una textura en el lote actual y devuelve su índice de ranura.
    /// Si la textura ya existe en el lote, devuelve el índice previo.
    #[cfg(feature = "assets")]
    pub fn reserve_texture_slot(
        &mut self,
        texture: std::sync::Arc<ferrous_assets::Texture2d>,
    ) -> u32 {
        if let Some(pos) = self
            .textures
            .iter()
            .position(|t| std::sync::Arc::ptr_eq(t, &texture))
        {
            return pos as u32;
        }
        if self.textures.len() as u32 >= MAX_TEXTURE_SLOTS {
            panic!("Excedido el límite de texturas por lote de UI (max={})", MAX_TEXTURE_SLOTS);
        }
        let idx = self.textures.len() as u32;
        self.textures.push(texture);
        idx
    }

    pub fn rect(&mut self, x: f32, y: f32, w: f32, h: f32, color: [f32; 4]) {
        self.rect_radii(x, y, w, h, color, [0.0; 4]);
    }

    pub fn rect_r(&mut self, x: f32, y: f32, w: f32, h: f32, color: [f32; 4], radius: f32) {
        self.rect_radii(x, y, w, h, color, [radius; 4]);
    }

    pub fn rect_radii(&mut self, x: f32, y: f32, w: f32, h: f32, color: [f32; 4], radii: [f32; 4]) {
        self.push(GuiQuad {
            pos: [x, y],
            size: [w, h],
            uv0: [0.0, 0.0],
            uv1: [1.0, 1.0],
            color,
            radii,
            tex_index: 0,
            flags: 0,
        });
    }

    pub fn rect_textured(
        &mut self,
        x: f32,
        y: f32,
        w: f32,
        h: f32,
        color: [f32; 4],
        uv0: [f32; 2],
        uv1: [f32; 2],
        tex_index: u32,
    ) {
        self.push(GuiQuad {
            pos: [x, y],
            size: [w, h],
            uv0,
            uv1,
            color,
            radii: [0.0; 4],
            tex_index,
            flags: TEXTURED_BIT,
        });
    }
    
    #[cfg(feature = "assets")]
    pub fn image(
        &mut self,
        x: f32,
        y: f32,
        w: f32,
        h: f32,
        texture: std::sync::Arc<ferrous_assets::Texture2d>,
        uv0: [f32; 2],
        uv1: [f32; 2],
        color: [f32; 4],
    ) {
        let idx = self.reserve_texture_slot(texture);
        self.rect_textured(x, y, w, h, color, uv0, uv1, idx);
    }

    pub fn len(&self) -> usize {
        self.quads.len()
    }

    pub fn is_empty(&self) -> bool {
        self.quads.is_empty()
    }

    pub fn as_bytes(&self) -> &[u8] {
        bytemuck::cast_slice(&self.quads)
    }
}

/// Representación en memoria de un glifo de texto para la GPU.
#[repr(C)]
#[derive(Copy, Clone, bytemuck::Pod, bytemuck::Zeroable, Debug)]
pub struct TextQuad {
    pub pos: [f32; 2],
    pub size: [f32; 2],
    pub uv0: [f32; 2],
    pub uv1: [f32; 2],
    pub color: [f32; 4],
}

/// Lote optimizado para el dibujado de texto masivo.
#[derive(Clone)]
pub struct TextBatch {
    quads: Vec<TextQuad>,
}

impl TextBatch {
    pub fn new() -> Self {
        Self { quads: Vec::new() }
    }

    pub fn clear(&mut self) {
        self.quads.clear();
    }

    pub fn push(&mut self, quad: TextQuad) {
        self.quads.push(quad);
    }

    pub fn len(&self) -> usize {
        self.quads.len()
    }

    pub fn is_empty(&self) -> bool {
        self.quads.is_empty()
    }

    pub fn as_bytes(&self) -> &[u8] {
        bytemuck::cast_slice(&self.quads)
    }

    /// Convierte una cadena de texto en una serie de quads muestreando un atlas de fuentes.
    #[cfg(feature = "text")]
    pub fn draw_text(
        &mut self,
        font: &ferrous_assets::Font,
        text: &str,
        position: [f32; 2],
        size: f32,
        color: [f32; 4],
    ) {
        let atlas = &font.atlas;
        let mut x = position[0];
        let y = position[1];
        let box_scale = 1.6;
        let quad_size = size * box_scale;

        for c in text.chars() {
            if let Some(metric) = atlas.metrics.get(&c) {
                let qx = x - (0.3 * size);
                let qy = y - (0.3 * size);

                self.push(TextQuad {
                    pos: [qx, qy],
                    size: [quad_size, quad_size],
                    uv0: [metric.uv[0], metric.uv[1]],
                    uv1: [metric.uv[2], metric.uv[3]],
                    color,
                });
                x += metric.advance * size;
            }
        }
    }
}

/// Motor principal de dibujado de UI en GPU.
/// Encapsula los pipelines de WGPU para quads generales y texto.
pub struct GuiRenderer {
    device: Arc<wgpu::Device>,
    pipeline: wgpu::RenderPipeline,
    vertex_buffer: wgpu::Buffer,
    index_buffer: wgpu::Buffer,
    instance_buffer: wgpu::Buffer,
    uniform_buffer: wgpu::Buffer,
    uniform_bind_group: wgpu::BindGroup,
    max_instances: u32,
    resolution: [f32; 2],
    text_pipeline: wgpu::RenderPipeline,
    text_instance_buffer: wgpu::Buffer,
    text_max_instances: u32,
    font_bind_group_layout: wgpu::BindGroupLayout,
    font_bind_group: Option<wgpu::BindGroup>,
    #[cfg(feature = "assets")]
    image_bind_group_layout: wgpu::BindGroupLayout,
    #[cfg(feature = "assets")]
    image_bind_group: Option<wgpu::BindGroup>,
}

impl GuiRenderer {
    pub fn new(
        device: Arc<wgpu::Device>,
        format: wgpu::TextureFormat,
        max_instances: u32,
        width: u32,
        height: u32,
        sample_count: u32,
    ) -> Self {
        let vertices: &[f32] = &[
            0.0, 0.0,
            1.0, 0.0,
            1.0, 1.0,
            0.0, 1.0,
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

        let text_instance_size = std::mem::size_of::<TextQuad>() as u64;
        let text_instance_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("GUI Text Instance Buffer"),
            size: text_instance_size * max_instances as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

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

        let uniform_bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
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

        let font_bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("GUI Font Bind Group Layout"),
                entries: &[
                    wgpu::BindGroupLayoutEntry {
                        binding: 0,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Texture {
                            sample_type: wgpu::TextureSampleType::Float { filterable: true },
                            view_dimension: wgpu::TextureViewDimension::D2,
                            multisampled: false,
                        },
                        count: None,
                    },
                    wgpu::BindGroupLayoutEntry {
                        binding: 1,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                        count: None,
                    },
                ],
            });

        #[cfg(feature = "assets")]
        let image_bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("GUI Image Bind Group Layout"),
                entries: &[
                    wgpu::BindGroupLayoutEntry {
                        binding: 0,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Texture {
                            sample_type: wgpu::TextureSampleType::Float { filterable: true },
                            view_dimension: wgpu::TextureViewDimension::D2,
                            multisampled: false,
                        },
                        count: std::num::NonZeroU32::new(MAX_TEXTURE_SLOTS),
                    },
                    wgpu::BindGroupLayoutEntry {
                        binding: 1,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                        count: std::num::NonZeroU32::new(MAX_TEXTURE_SLOTS),
                    },
                ],
            });

        let text_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("GUI Text Shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("../../../assets/shaders/text.wgsl").into()),
        });
        
        let text_pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("GUI Text Pipeline Layout"),
            bind_group_layouts: &[&uniform_bind_group_layout, &font_bind_group_layout],
            push_constant_ranges: &[],
        });
        
        let text_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("GUI Text Render Pipeline"),
            layout: Some(&text_pipeline_layout),
            vertex: wgpu::VertexState {
                module: &text_shader,
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
                        array_stride: text_instance_size,
                        step_mode: wgpu::VertexStepMode::Instance,
                        attributes: &[
                            wgpu::VertexAttribute { format: wgpu::VertexFormat::Float32x2, offset: 0, shader_location: 1 },
                            wgpu::VertexAttribute { format: wgpu::VertexFormat::Float32x2, offset: 8, shader_location: 2 },
                            wgpu::VertexAttribute { format: wgpu::VertexFormat::Float32x2, offset: 16, shader_location: 3 },
                            wgpu::VertexAttribute { format: wgpu::VertexFormat::Float32x2, offset: 24, shader_location: 4 },
                            wgpu::VertexAttribute { format: wgpu::VertexFormat::Float32x4, offset: 32, shader_location: 5 },
                        ],
                    },
                ],
                compilation_options: Default::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &text_shader,
                entry_point: Some("fs_main"),
                targets: &[Some(wgpu::ColorTargetState {
                    format,
                    blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: Default::default(),
            }),
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: None,
            multisample: wgpu::MultisampleState {
                count: sample_count,
                mask: !0,
                alpha_to_coverage_enabled: false,
            },
            multiview: None,
            cache: None,
        });

        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("GUI Shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("../../../assets/shaders/gui.wgsl").into()),
        });

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("GUI Pipeline Layout"),
            bind_group_layouts: &[
                &uniform_bind_group_layout,
                #[cfg(feature = "assets")]
                &image_bind_group_layout,
            ],
            push_constant_ranges: &[],
        });

        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("GUI Render Pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
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
                            wgpu::VertexAttribute { format: wgpu::VertexFormat::Float32x2, offset: 0, shader_location: 1 },
                            wgpu::VertexAttribute { format: wgpu::VertexFormat::Float32x2, offset: 8, shader_location: 2 },
                            wgpu::VertexAttribute { format: wgpu::VertexFormat::Float32x2, offset: 16, shader_location: 3 },
                            wgpu::VertexAttribute { format: wgpu::VertexFormat::Float32x2, offset: 24, shader_location: 4 },
                            wgpu::VertexAttribute { format: wgpu::VertexFormat::Float32x4, offset: 32, shader_location: 5 },
                            wgpu::VertexAttribute { format: wgpu::VertexFormat::Float32x4, offset: 48, shader_location: 6 },
                            wgpu::VertexAttribute { format: wgpu::VertexFormat::Uint32, offset: 64, shader_location: 7 },
                            wgpu::VertexAttribute { format: wgpu::VertexFormat::Uint32, offset: 68, shader_location: 8 },
                        ],
                    },
                ],
                compilation_options: Default::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_main"),
                targets: &[Some(wgpu::ColorTargetState {
                    format,
                    blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: Default::default(),
            }),
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: None,
            multisample: wgpu::MultisampleState {
                count: sample_count,
                mask: !0,
                alpha_to_coverage_enabled: false,
            },
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
            text_pipeline,
            text_instance_buffer,
            text_max_instances: max_instances,
            font_bind_group_layout,
            font_bind_group: None,
            #[cfg(feature = "assets")]
            image_bind_group_layout,
            #[cfg(feature = "assets")]
            image_bind_group: None,
        }
    }

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

    /// Vincula un atlas de fuentes para poder renderizar texto con este renderer.
    pub fn set_font_atlas(&mut self, view: &wgpu::TextureView, sampler: &wgpu::Sampler) {
        self.font_bind_group = Some(self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("GUI Font Bind Group"),
            layout: &self.font_bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(sampler),
                },
            ],
        }));
    }

    /// Renderizado estándar manteniendo el contenido previo del frame buffer.
    pub fn render(
        &mut self,
        encoder: &mut wgpu::CommandEncoder,
        view: &wgpu::TextureView,
        resolve_target: Option<&wgpu::TextureView>,
        batch: &GuiBatch,
        queue: &wgpu::Queue,
        text_batch: Option<&TextBatch>,
    ) {
        self.render_impl(
            encoder,
            view,
            resolve_target,
            batch,
            queue,
            text_batch,
            wgpu::LoadOp::Load,
        );
    }

    /// Renderizado que limpia el frame buffer con un color específico antes de dibujar.
    pub fn render_clearing(
        &mut self,
        encoder: &mut wgpu::CommandEncoder,
        view: &wgpu::TextureView,
        resolve_target: Option<&wgpu::TextureView>,
        batch: &GuiBatch,
        queue: &wgpu::Queue,
        text_batch: Option<&TextBatch>,
        clear_color: wgpu::Color,
    ) {
        self.render_impl(
            encoder,
            view,
            resolve_target,
            batch,
            queue,
            text_batch,
            wgpu::LoadOp::Clear(clear_color),
        );
    }

    fn render_impl(
        &mut self,
        encoder: &mut wgpu::CommandEncoder,
        view: &wgpu::TextureView,
        resolve_target: Option<&wgpu::TextureView>,
        batch: &GuiBatch,
        queue: &wgpu::Queue,
        text_batch: Option<&TextBatch>,
        load_op: wgpu::LoadOp<wgpu::Color>,
    ) {
        if batch.is_empty() && text_batch.map_or(true, |tb| tb.is_empty()) {
            return;
        }

        if !batch.is_empty() {
            let instance_bytes = batch.as_bytes();
            let required_instances = batch.len() as u32;

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
        }

        if let Some(tb) = text_batch {
            if !tb.is_empty() {
                let text_bytes = tb.as_bytes();
                let required = tb.len() as u32;
                if required > self.text_max_instances {
                    let new_size = std::mem::size_of::<TextQuad>() as u64 * required as u64;
                    self.text_instance_buffer =
                        self.device.create_buffer(&wgpu::BufferDescriptor {
                            label: Some("GUI Text Instance Buffer (resized)"),
                            size: new_size,
                            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                            mapped_at_creation: false,
                        });
                    self.text_max_instances = required;
                }
                queue.write_buffer(&self.text_instance_buffer, 0, text_bytes);
            }
        }

        let mut rpass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("GUI Render Pass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view,
                resolve_target,
                ops: wgpu::Operations {
                    load: load_op,
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            occlusion_query_set: None,
            timestamp_writes: None,
        });

        if !batch.is_empty() {
            let required_instances = batch.len() as u32;
            rpass.set_pipeline(&self.pipeline);
            rpass.set_bind_group(0, &self.uniform_bind_group, &[]);
            
            #[cfg(feature = "assets")]
            if !batch.textures.is_empty() {
                let mut views = Vec::with_capacity(batch.textures.len());
                let mut samplers = Vec::with_capacity(batch.textures.len());
                for tex in &batch.textures {
                    views.push(&tex.view);
                    samplers.push(&tex.sampler);
                }
                while views.len() < MAX_TEXTURE_SLOTS as usize {
                    views.push(views.last().unwrap());
                    samplers.push(samplers.last().unwrap());
                }
                let bg = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
                    label: Some("GUI Image Bind Group"),
                    layout: &self.image_bind_group_layout,
                    entries: &[
                        wgpu::BindGroupEntry {
                            binding: 0,
                            resource: wgpu::BindingResource::TextureViewArray(&views),
                        },
                        wgpu::BindGroupEntry {
                            binding: 1,
                            resource: wgpu::BindingResource::SamplerArray(&samplers),
                        },
                    ],
                });
                self.image_bind_group = Some(bg);
            }
            #[cfg(feature = "assets")]
            if let Some(bg) = &self.image_bind_group {
                rpass.set_bind_group(1, bg, &[]);
            }
            
            rpass.set_vertex_buffer(0, self.vertex_buffer.slice(..));
            rpass.set_vertex_buffer(1, self.instance_buffer.slice(..));
            rpass.set_index_buffer(self.index_buffer.slice(..), wgpu::IndexFormat::Uint16);
            rpass.draw_indexed(0..6, 0, 0..required_instances);
        }

        if let Some(tb) = text_batch {
            if !tb.is_empty() {
                if let Some(font_bg) = &self.font_bind_group {
                    let required = tb.len() as u32;
                    rpass.set_pipeline(&self.text_pipeline);
                    rpass.set_bind_group(0, &self.uniform_bind_group, &[]);
                    rpass.set_bind_group(1, font_bg, &[]);
                    rpass.set_vertex_buffer(0, self.vertex_buffer.slice(..));
                    rpass.set_vertex_buffer(1, self.text_instance_buffer.slice(..));
                    rpass.set_index_buffer(self.index_buffer.slice(..), wgpu::IndexFormat::Uint16);
                    rpass.draw_indexed(0..6, 0, 0..required);
                }
            }
        }
    }
}

/// Extensión para convertir comandos de renderizado abstractos en lotes optimizados para GPU.
pub trait ToBatches {
    /// Versión con soporte de texto. Requiere una fuente para la rasterización de glifos.
    #[cfg(feature = "text")]
    fn to_batches(
        &self,
        quad_batch: &mut GuiBatch,
        text_batch: &mut TextBatch,
        font: Option<&ferrous_assets::Font>,
    );

    /// Versión ligera sin soporte de texto.
    #[cfg(not(feature = "text"))]
    fn to_batches(&self, quad_batch: &mut GuiBatch, text_batch: &mut TextBatch);
}

impl ToBatches for ferrous_ui_core::RenderCommand {
    #[cfg(feature = "text")]
    fn to_batches(
        &self,
        quad_batch: &mut GuiBatch,
        text_batch: &mut TextBatch,
        font: Option<&ferrous_assets::Font>,
    ) {
        use ferrous_ui_core::RenderCommand;
        match self {
            RenderCommand::Quad {
                rect,
                color,
                radii,
                flags,
            } => {
                quad_batch.push(GuiQuad {
                    pos: [rect.x, rect.y],
                    size: [rect.width, rect.height],
                    uv0: [0.0, 0.0],
                    uv1: [1.0, 1.0],
                    color: *color,
                    radii: *radii,
                    tex_index: 0,
                    flags: *flags,
                });
            }
            #[cfg(feature = "assets")]
            RenderCommand::Image {
                rect,
                texture,
                uv0,
                uv1,
                color,
            } => {
                let idx = quad_batch.reserve_texture_slot(texture.clone());
                quad_batch.push(GuiQuad {
                    pos: [rect.x, rect.y],
                    size: [rect.width, rect.height],
                    uv0: *uv0,
                    uv1: *uv1,
                    color: *color,
                    radii: [0.0; 4],
                    tex_index: idx,
                    flags: TEXTURED_BIT,
                });
            }
            #[cfg(not(feature = "assets"))]
            RenderCommand::Image { .. } => {
                // Sin soporte de assets, ignoramos imágenes
            }
            RenderCommand::Text {
                rect,
                text,
                color,
                font_size,
            } => {
                if let Some(f) = font {
                    text_batch.draw_text(f, text, [rect.x, rect.y], *font_size, *color);
                }
            }
            RenderCommand::PushClip { .. } | RenderCommand::PopClip => {
                // Scissoring no implementado aún en esta fase
            }
        }
    }

    #[cfg(not(feature = "text"))]
    fn to_batches(&self, quad_batch: &mut GuiBatch, _text_batch: &mut TextBatch) {
        use ferrous_ui_core::RenderCommand;
        match self {
            RenderCommand::Quad {
                rect,
                color,
                radii,
                flags,
            } => {
                quad_batch.push(GuiQuad {
                    pos: [rect.x, rect.y],
                    size: [rect.width, rect.height],
                    uv0: [0.0, 0.0],
                    uv1: [1.0, 1.0],
                    color: *color,
                    radii: *radii,
                    tex_index: 0,
                    flags: *flags,
                });
            }
            #[cfg(feature = "assets")]
            RenderCommand::Image {
                rect,
                texture,
                uv0,
                uv1,
                color,
            } => {
                let idx = quad_batch.reserve_texture_slot(texture.clone());
                quad_batch.push(GuiQuad {
                    pos: [rect.x, rect.y],
                    size: [rect.width, rect.height],
                    uv0: *uv0,
                    uv1: *uv1,
                    color: *color,
                    radii: [0.0; 4],
                    tex_index: idx,
                    flags: TEXTURED_BIT,
                });
            }
            #[cfg(not(feature = "assets"))]
            RenderCommand::Image { .. } => {}
            RenderCommand::Text { .. } => {}
            RenderCommand::PushClip { .. } | RenderCommand::PopClip => {}
        }
    }
}

