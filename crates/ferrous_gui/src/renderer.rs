use std::num::{NonZeroU32, NonZeroU64};
use std::sync::Arc;

use wgpu::util::DeviceExt;

/// maximum number of distinct textures that can be referenced by a single
/// GUI batch.  this choice is somewhat arbitrary; eight slots has proven
/// more than enough for typical UIs (icons, cursors, window backgrounds)
/// and keeps the bind group sizes small.  bump if necessary.
pub const MAX_TEXTURE_SLOTS: u32 = 8;

/// flag bit inserted into [`GuiQuad::flags`] when the quad should sample a
/// texture from the bound texture array instead of using its solid colour.
pub const TEXTURED_BIT: u32 = 1 << 1;

/// Representa un rectángulo de la UI (un "quad").
///
/// Todos los campos están en coordenadas de píxeles: `pos` es la esquina
/// superior izquierda, `size` el ancho/alto y `color` es un RGBA con componentes
/// en el rango 0.0..1.0.
///
/// El campo `radius` permite dibujar esquinas redondeadas; su valor se
/// interpreta en píxeles y se recorta automáticamente si es mayor que la
/// mitad de la dimensión más pequeña del rectángulo. Un `radius` de 0 deja
/// las esquinas afiladas (comportamiento original).
#[repr(C)]
#[derive(Copy, Clone, bytemuck::Pod, bytemuck::Zeroable, Debug)]
pub struct GuiQuad {
    pub pos: [f32; 2],
    pub size: [f32; 2],
    /// UV coordinates within the current texture. `uv0` is the upper-left
    /// corner of the sub-region and `uv1` the lower-right.  When the quad is
    /// not textured these fields are ignored and will typically be set to the
    /// default `[0.0,0.0]` / `[1.0,1.0]` pair.
    pub uv0: [f32; 2],
    pub uv1: [f32; 2],
    pub color: [f32; 4],
    /// per-corner radii in pixels: [top-left, top-right, bottom-left, bottom-right].
    /// a value of 0 means the corner is sharp. providing distinct values
    /// allows fine-grained control over each corner's curvature.
    pub radii: [f32; 4],
    /// index into the texture array that will be bound by the renderer.  If
    /// the quad is untextured this value is ignored (and may be zero).  Valid
    /// indices are assigned via [`GuiBatch::reserve_texture_slot`].
    pub tex_index: u32,
    /// bitflags controlling special rendering behaviour. bit 0=colour wheel.
    pub flags: u32,
}

/// Lote de `GuiQuad` que será enviado al GPU en un draw call único.
///
/// El `GuiRenderer` consumirá un `GuiBatch` para rellenar el buffer de
/// instancias antes de emitir el paso de renderizado.
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

    /// Ensure the given texture is present in the batch's slot list and
    /// return its index.  If the texture is already present the existing
    /// index is returned; otherwise the texture is appended.  Panics if the
    /// batch already contains `MAX_TEXTURE_SLOTS` distinct textures.
    #[cfg(feature = "assets")]
    pub fn reserve_texture_slot(
        &mut self,
        texture: std::sync::Arc<ferrous_assets::Texture2d>,
    ) -> u32 {
        // linear search is fine since the slot count is tiny
        if let Some(pos) = self
            .textures
            .iter()
            .position(|t| std::sync::Arc::ptr_eq(t, &texture))
        {
            return pos as u32;
        }
        if self.textures.len() as u32 >= MAX_TEXTURE_SLOTS {
            panic!("ran out of GUI texture slots (max={})", MAX_TEXTURE_SLOTS);
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

    /// Add a quad whose fragments are textured.  `uv0`/`uv1` specify the
    /// sub-rectangle of the currently bound texture to sample, and
    /// `tex_index` identifies which texture slot contains the image.  The
    /// supplied colour is multiplied with the sampled value (tint).
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

    /// Helper that both reserves a texture slot and emits the textured quad
    /// in one call.  The caller may simply supply an `Arc<Texture2d>` and the
    /// batch will ensure the same texture is not duplicated within the slot
    /// list.
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

    /// Dibuja solo el borde (stroke) de un rectángulo, con radio opcional.
    /// El borde es *inset* (dibujado hacia adentro del rect original).
    /// `stroke_px`: grosor del borde en píxeles.
    /// `radius`: radio de las esquinas (0.0 = esquinas rectas).
    pub fn rect_stroke(
        &mut self,
        x: f32,
        y: f32,
        w: f32,
        h: f32,
        color: [f32; 4],
        radius: f32,
        stroke_px: f32,
    ) {
        if radius == 0.0 {
            // four thin rects inset
            // top
            self.rect(x, y, w, stroke_px, color);
            // bottom
            self.rect(x, y + h - stroke_px, w, stroke_px, color);
            // left
            self.rect(x, y + stroke_px, stroke_px, h - 2.0 * stroke_px, color);
            // right
            self.rect(
                x + w - stroke_px,
                y + stroke_px,
                stroke_px,
                h - 2.0 * stroke_px,
                color,
            );
        } else {
            // TODO: replace with native outline primitive when renderer supports it
            // top and bottom use rounded rect helper, sides remain plain
            self.rect_r(x, y, w, stroke_px, color, radius);
            self.rect_r(x, y + h - stroke_px, w, stroke_px, color, radius);
            self.rect(x, y + stroke_px, stroke_px, h - 2.0 * stroke_px, color);
            self.rect(
                x + w - stroke_px,
                y + stroke_px,
                stroke_px,
                h - 2.0 * stroke_px,
                color,
            );
        }
    }

    pub fn line(&mut self, x1: f32, y1: f32, x2: f32, y2: f32, thickness: f32, color: [f32; 4]) {
        let width = thickness.max(1.0);
        let delta_x = x2 - x1;
        let delta_y = y2 - y1;
        let length = (delta_x * delta_x + delta_y * delta_y).sqrt();
        if length <= f32::EPSILON {
            self.rect_r(
                x1 - width * 0.5,
                y1 - width * 0.5,
                width,
                width,
                color,
                width * 0.5,
            );
            return;
        }

        let step = (width * 0.5).max(1.0);
        let segments = (length / step).ceil() as u32;
        for segment_index in 0..=segments {
            let t = segment_index as f32 / segments as f32;
            let x = x1 + delta_x * t;
            let y = y1 + delta_y * t;
            self.rect_r(
                x - width * 0.5,
                y - width * 0.5,
                width,
                width,
                color,
                width * 0.5,
            );
        }
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

// --- text batching --------------------------------------------------------

/// Instance data for a glyph quad that will be textured with the font atlas.
#[repr(C)]
#[derive(Copy, Clone, bytemuck::Pod, bytemuck::Zeroable, Debug)]
pub struct TextQuad {
    pub pos: [f32; 2],
    pub size: [f32; 2],
    pub uv0: [f32; 2],
    pub uv1: [f32; 2],
    pub color: [f32; 4],
}

/// Batch of text quads.
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

    pub(crate) fn as_bytes(&self) -> &[u8] {
        bytemuck::cast_slice(&self.quads)
    }

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

        // Coincidimos con la escala EM fija del generador (-0.3 a 1.3 = 1.6 de ancho)
        let box_scale = 1.6;
        let quad_size = size * box_scale;

        for c in text.chars() {
            if let Some(metric) = atlas.metrics.get(&c) {
                // El punto base (baseline) está desplazado por el padding de 0.3
                let qx = x - (0.3 * size);
                let qy = y - (0.3 * size);

                self.push(TextQuad {
                    pos: [qx, qy],
                    size: [quad_size, quad_size],
                    uv0: [metric.uv[0], metric.uv[1]],
                    uv1: [metric.uv[2], metric.uv[3]],
                    color,
                });

                // Avance horizontal usando la métrica de la fuente
                x += metric.advance * size;
            }
        }
    }

    #[cfg(not(feature = "text"))]
    pub fn draw_text(
        &mut self,
        _font: &(),
        _text: &str,
        _position: [f32; 2],
        _size: f32,
        _color: [f32; 4],
    ) {
        // text rendering disabled; no-op
    }
}

// -------------------------------------------------------------------------

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
    // text rendering resources
    text_pipeline: wgpu::RenderPipeline,
    text_instance_buffer: wgpu::Buffer,
    text_max_instances: u32,
    font_bind_group_layout: wgpu::BindGroupLayout,
    font_bind_group: Option<wgpu::BindGroup>,
    /// layout used by both text and quad pipelines when textures are bound.
    #[cfg(feature = "assets")]
    image_bind_group_layout: wgpu::BindGroupLayout,
    /// temporary storage for the bind group that is built right before a
    /// draw call; we keep it on the renderer so it can be re-used across
    /// render() invocations when the set of textures is unchanged.
    #[cfg(feature = "assets")]
    image_bind_group: Option<wgpu::BindGroup>,
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
        sample_count: u32,
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

        // --- text instance buffer (same quad geometry, different instance data)
        let text_instance_size = std::mem::size_of::<TextQuad>() as u64;
        let text_instance_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("GUI Text Instance Buffer"),
            size: text_instance_size * max_instances as u64,
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

        // layout for font atlas texture+sampler
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
                        count: NonZeroU32::new(MAX_TEXTURE_SLOTS as u32),
                    },
                    wgpu::BindGroupLayoutEntry {
                        binding: 1,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                        count: NonZeroU32::new(MAX_TEXTURE_SLOTS as u32),
                    },
                ],
            });

        // create text pipeline (separate shader file)
        let text_shader =
            device.create_shader_module(wgpu::include_wgsl!("../../../assets/shaders/text.wgsl"));
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
                    // same quad vertex buffer used for geometry
                    wgpu::VertexBufferLayout {
                        array_stride: std::mem::size_of::<[f32; 2]>() as wgpu::BufferAddress,
                        step_mode: wgpu::VertexStepMode::Vertex,
                        attributes: &[wgpu::VertexAttribute {
                            format: wgpu::VertexFormat::Float32x2,
                            offset: 0,
                            shader_location: 0,
                        }],
                    },
                    // text instance buffer
                    wgpu::VertexBufferLayout {
                        array_stride: text_instance_size,
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
                                // uv0
                                format: wgpu::VertexFormat::Float32x2,
                                offset: 16,
                                shader_location: 3,
                            },
                            wgpu::VertexAttribute {
                                // uv1
                                format: wgpu::VertexFormat::Float32x2,
                                offset: 24,
                                shader_location: 4,
                            },
                            wgpu::VertexAttribute {
                                // color
                                format: wgpu::VertexFormat::Float32x4,
                                offset: 32,
                                shader_location: 5,
                            },
                        ],
                    },
                ],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &text_shader,
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
            multisample: wgpu::MultisampleState {
                count: sample_count,
                mask: !0,
                alpha_to_coverage_enabled: false,
            },
            multiview: None,
            cache: None,
        });

        // font bind group will be created later when atlas is available
        let font_bind_group = None;

        let shader =
            device.create_shader_module(wgpu::include_wgsl!("../../../assets/shaders/gui.wgsl"));

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
                                // uv0
                                format: wgpu::VertexFormat::Float32x2,
                                offset: 16,
                                shader_location: 3,
                            },
                            wgpu::VertexAttribute {
                                // uv1
                                format: wgpu::VertexFormat::Float32x2,
                                offset: 24,
                                shader_location: 4,
                            },
                            wgpu::VertexAttribute {
                                // color
                                format: wgpu::VertexFormat::Float32x4,
                                offset: 32,
                                shader_location: 5,
                            },
                            // radii array (4 floats)
                            wgpu::VertexAttribute {
                                format: wgpu::VertexFormat::Float32x4,
                                offset: 48,
                                shader_location: 6,
                            },
                            // texture slot index
                            wgpu::VertexAttribute {
                                format: wgpu::VertexFormat::Uint32,
                                offset: 64,
                                shader_location: 7,
                            },
                            // flags field (u32)
                            wgpu::VertexAttribute {
                                format: wgpu::VertexFormat::Uint32,
                                offset: 68,
                                shader_location: 8,
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
            font_bind_group,
            #[cfg(feature = "assets")]
            image_bind_group_layout,
            #[cfg(feature = "assets")]
            image_bind_group: None,
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

    /// Provide a font atlas texture/sampler pair that will be used by the text
    /// pipeline.  This must be called before attempting to render any
    /// `TextBatch` content.
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

    /// Emite los comandos necesarios para dibujar el contenido del
    /// `GuiBatch` sobre la vista indicada.
    /// Render a batch of GUI quads (and optional text) into the provided
    /// texture view.  When MSAA is in use the caller should pass the
    /// multisampled view here and also supply `resolve_target` so that the
    /// contents of the pass are resolved into the single‑sampled "presentable"
    /// texture afterwards.  When not using MSAA the caller can pass `None`.
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

    /// Like [`render`] but clears the target to `clear_color` before drawing.
    /// Use this when the UI pass is the first (or only) pass writing to the
    /// surface — e.g. in `Desktop2D` mode where the world/post-process passes
    /// are skipped entirely.
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
        // we may render GUI quads and/or text quads; if both are empty we
        // can early out.
        if batch.is_empty() && text_batch.map_or(true, |tb| tb.is_empty()) {
            return;
        }

        // first, handle solid quads
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

        // handle text quads before starting pass
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

        // draw colored quads first
        if !batch.is_empty() {
            let required_instances = batch.len() as u32;
            rpass.set_pipeline(&self.pipeline);
            rpass.set_bind_group(0, &self.uniform_bind_group, &[]);
            // if there are any textures in the batch we need to bind them as
            // well.  we lazily construct a bind group and cache it on the
            // renderer; it will be re-created whenever the set of textures
            // changes (the `GuiBatch` takes care of de-duplicating).
            #[cfg(feature = "assets")]
            if !batch.textures.is_empty() {
                // build vector of texture views and samplers
                let mut views = Vec::with_capacity(batch.textures.len());
                let mut samplers = Vec::with_capacity(batch.textures.len());
                for tex in &batch.textures {
                    views.push(&tex.view);
                    samplers.push(&tex.sampler);
                }
                // pad to MAX_TEXTURE_SLOTS with dummy resources (required by
                // wgpu when binding arrays larger than the number we supply)
                while views.len() < MAX_TEXTURE_SLOTS as usize {
                    // a 1x1 white texture could be created once and reused; for
                    // simplicity we just duplicate the last entry (should be
                    // safe since we never sample unused slots)
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

        // now draw text if requested
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
        // `rpass` drops here automatically
    }
}
// end of module
