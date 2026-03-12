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

/// Flag for a two-color gradient quad (single full-rect quad).
/// When set:
///   - `color`  = left-edge color (RGBA linear)
///   - `uv0`   = right-edge color .rg
///   - `uv1`   = right-edge color .ba
///   - The shader linearly blends color → color1 using the interpolated uv.x.
///   - `radii` = full rect radii (SDF against full `size`).
pub const GRADIENT_BIT: u32 = 1 << 2;

/// Flag for a thin gradient strip (radial/conic, many strips per rect).
/// When set:
///   - `color`  = this strip's flat color sample
///   - `uv0.x` = normalised left-edge X of this strip inside the full rect (0..1)
///   - `uv1`   = (full_w, full_h) of the full rect in pixels
///   - `radii` = full rect radii
///   The shader uses uv1 for SDF so corners are clipped against the full rect.
pub const GRADIENT_STRIP_BIT: u32 = 1 << 3;

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

/// Segmento de dibujo que define un rango de instancias y opcionalmente un área de recorte.
#[derive(Clone, Debug)]
pub struct DrawSegment {
    pub quad_range: std::ops::Range<u32>,
    pub text_range: std::ops::Range<u32>,
    pub scissor: Option<ferrous_ui_core::Rect>,
}

/// Agrupación de primitivas de UI organizadas por segmentos de renderizado.
#[derive(Clone)]
pub struct GuiBatch {
    pub quads: Vec<GuiQuad>,
    pub text_quads: Vec<TextQuad>,
    pub segments: Vec<DrawSegment>,
    pub current_scissor: Option<ferrous_ui_core::Rect>,
    pub scissor_stack: Vec<ferrous_ui_core::Rect>,
    #[cfg(feature = "assets")]
    pub textures: Vec<std::sync::Arc<ferrous_assets::Texture2d>>,
}

impl GuiBatch {
    pub fn new() -> Self {
        Self {
            quads: Vec::new(),
            text_quads: Vec::new(),
            segments: Vec::new(),
            current_scissor: None,
            scissor_stack: Vec::new(),
            #[cfg(feature = "assets")]
            textures: Vec::new(),
        }
    }

    pub fn clear(&mut self) {
        self.quads.clear();
        self.text_quads.clear();
        self.segments.clear();
        self.current_scissor = None;
        self.scissor_stack.clear();
        #[cfg(feature = "assets")]
        {
            self.textures.clear();
        }
    }

    pub fn push_quad(&mut self, quad: GuiQuad) {
        self.quads.push(quad);
        self.update_last_segment();
    }

    pub fn push_text(&mut self, quad: TextQuad) {
        self.text_quads.push(quad);
    }

    pub fn extend(&mut self, other: GuiBatch) {
        self.quads.extend(other.quads);
        self.text_quads.extend(other.text_quads);
        self.segments.extend(other.segments);
        #[cfg(feature = "assets")]
        {
            // Nota: Al igual que en la versión anterior, esto es una simplificación.
            // Una implementación robusta debería mapear tex_index para evitar colisiones.
            self.textures.extend(other.textures);
        }
    }

    /// Asegura que hay un segmento activo para el scissor actual.
    pub fn ensure_segment(&mut self) {
        let needs_new = match self.segments.last() {
            Some(last) => last.scissor != self.current_scissor,
            None => true,
        };

        if needs_new {
            let q_start = self.quads.len() as u32;
            let t_start = self.text_quads.len() as u32;
            self.segments.push(DrawSegment {
                quad_range: q_start..q_start,
                text_range: t_start..t_start,
                scissor: self.current_scissor,
            });
        }
    }

    fn update_last_segment(&mut self) {
        self.ensure_segment();
        if let Some(last) = self.segments.last_mut() {
            last.quad_range.end = self.quads.len() as u32;
            last.text_range.end = self.text_quads.len() as u32;
        }
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
            panic!(
                "Excedido el límite de texturas por lote de UI (max={})",
                MAX_TEXTURE_SLOTS
            );
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
        self.push_quad(GuiQuad {
            pos: [x, y],
            size: [w, h],
            uv0: [0.0, 0.0],
            uv1: [1.0, 1.0],
            color,
            radii,
            tex_index: 0,
            flags: 0,
        });
        self.update_last_segment();
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
        self.push_quad(GuiQuad {
            pos: [x, y],
            size: [w, h],
            uv0,
            uv1,
            color,
            radii: [0.0; 4],
            tex_index,
            flags: TEXTURED_BIT,
        });
        self.update_last_segment();
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

    /// Dibuja un botón y devuelve `true` si fue presionado en este frame.
    ///
    /// Todo el hit-testing se realiza internamente — el llamador solo necesita
    /// el valor de retorno para ejecutar la acción.
    ///
    /// - `mx`, `my`: posición actual del mouse en píxeles de pantalla.
    /// - `clicked`: `true` si el botón izquierdo del mouse acaba de ser presionado.
    #[cfg(feature = "assets")]
    pub fn button(
        &mut self,
        font: &ferrous_assets::Font,
        x: f32,
        y: f32,
        w: f32,
        h: f32,
        label: &str,
        mx: f32,
        my: f32,
        clicked: bool,
    ) -> bool {
        let hovered = mx >= x && mx < x + w && my >= y && my < y + h;
        let bg: [f32; 4] = if hovered {
            [0.0, 0.298, 0.612, 1.0] // #0078D4 hover
        } else {
            [0.086, 0.086, 0.086, 1.0] // #161616 idle
        };
        self.rect(x, y, w, h, bg);
        self.draw_text(
            font,
            label,
            [x + 4.0, y + (h - 10.0) * 0.5],
            10.0,
            [1.0, 1.0, 1.0, 1.0],
        );
        hovered && clicked
    }

    /// Igual que `button()` pero con colores personalizados.
    ///
    /// - `idle_color`: color de fondo cuando el mouse no está encima.
    /// - `hover_color`: color de fondo cuando el mouse está encima.
    #[cfg(feature = "assets")]
    pub fn button_colored(
        &mut self,
        font: &ferrous_assets::Font,
        x: f32,
        y: f32,
        w: f32,
        h: f32,
        label: &str,
        mx: f32,
        my: f32,
        clicked: bool,
        idle_color: [f32; 4],
        hover_color: [f32; 4],
    ) -> bool {
        let hovered = mx >= x && mx < x + w && my >= y && my < y + h;
        let bg = if hovered { hover_color } else { idle_color };
        self.rect(x, y, w, h, bg);
        self.draw_text(
            font,
            label,
            [x + 4.0, y + (h - 10.0) * 0.5],
            10.0,
            [1.0, 1.0, 1.0, 1.0],
        );
        hovered && clicked
    }

    pub fn len(&self) -> usize {
        self.quads.len()
    }

    pub fn is_empty(&self) -> bool {
        self.quads.is_empty()
    }

    pub fn as_quad_bytes(&self) -> &[u8] {
        bytemuck::cast_slice(&self.quads)
    }

    pub fn as_text_bytes(&self) -> &[u8] {
        bytemuck::cast_slice(&self.text_quads)
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
    image_bind_group_layout: wgpu::BindGroupLayout,
    image_bind_group: wgpu::BindGroup,
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
        let vertices: &[f32] = &[0.0, 0.0, 1.0, 0.0, 1.0, 1.0, 0.0, 1.0];
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

        // Create a 1×1 dummy texture + sampler repeated MAX_TEXTURE_SLOTS times so
        // that group 1 is always valid even when no images are drawn.
        let dummy_texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("GUI Dummy Texture"),
            size: wgpu::Extent3d {
                width: 1,
                height: 1,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8UnormSrgb,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        let dummy_view = dummy_texture.create_view(&wgpu::TextureViewDescriptor::default());
        let dummy_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("GUI Dummy Sampler"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Nearest,
            min_filter: wgpu::FilterMode::Nearest,
            mipmap_filter: wgpu::FilterMode::Nearest,
            ..Default::default()
        });
        let dummy_views: Vec<&wgpu::TextureView> = std::iter::repeat(&dummy_view)
            .take(MAX_TEXTURE_SLOTS as usize)
            .collect();
        let dummy_samplers: Vec<&wgpu::Sampler> = std::iter::repeat(&dummy_sampler)
            .take(MAX_TEXTURE_SLOTS as usize)
            .collect();
        let image_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("GUI Image Bind Group (dummy)"),
            layout: &image_bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureViewArray(&dummy_views),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::SamplerArray(&dummy_samplers),
                },
            ],
        });

        let text_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("GUI Text Shader"),
            source: wgpu::ShaderSource::Wgsl(
                include_str!("../../../assets/shaders/text.wgsl").into(),
            ),
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
                            wgpu::VertexAttribute {
                                format: wgpu::VertexFormat::Float32x2,
                                offset: 0,
                                shader_location: 1,
                            },
                            wgpu::VertexAttribute {
                                format: wgpu::VertexFormat::Float32x2,
                                offset: 8,
                                shader_location: 2,
                            },
                            wgpu::VertexAttribute {
                                format: wgpu::VertexFormat::Float32x2,
                                offset: 16,
                                shader_location: 3,
                            },
                            wgpu::VertexAttribute {
                                format: wgpu::VertexFormat::Float32x2,
                                offset: 24,
                                shader_location: 4,
                            },
                            wgpu::VertexAttribute {
                                format: wgpu::VertexFormat::Float32x4,
                                offset: 32,
                                shader_location: 5,
                            },
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
            source: wgpu::ShaderSource::Wgsl(
                include_str!("../../../assets/shaders/gui.wgsl").into(),
            ),
        });

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("GUI Pipeline Layout"),
            bind_group_layouts: &[&uniform_bind_group_layout, &image_bind_group_layout],
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
                            wgpu::VertexAttribute {
                                format: wgpu::VertexFormat::Float32x2,
                                offset: 0,
                                shader_location: 1,
                            },
                            wgpu::VertexAttribute {
                                format: wgpu::VertexFormat::Float32x2,
                                offset: 8,
                                shader_location: 2,
                            },
                            wgpu::VertexAttribute {
                                format: wgpu::VertexFormat::Float32x2,
                                offset: 16,
                                shader_location: 3,
                            },
                            wgpu::VertexAttribute {
                                format: wgpu::VertexFormat::Float32x2,
                                offset: 24,
                                shader_location: 4,
                            },
                            wgpu::VertexAttribute {
                                format: wgpu::VertexFormat::Float32x4,
                                offset: 32,
                                shader_location: 5,
                            },
                            wgpu::VertexAttribute {
                                format: wgpu::VertexFormat::Float32x4,
                                offset: 48,
                                shader_location: 6,
                            },
                            wgpu::VertexAttribute {
                                format: wgpu::VertexFormat::Uint32,
                                offset: 64,
                                shader_location: 7,
                            },
                            wgpu::VertexAttribute {
                                format: wgpu::VertexFormat::Uint32,
                                offset: 68,
                                shader_location: 8,
                            },
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
            image_bind_group_layout,
            image_bind_group,
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
    ) {
        self.render_impl(
            encoder,
            view,
            resolve_target,
            batch,
            queue,
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
        clear_color: wgpu::Color,
    ) {
        self.render_impl(
            encoder,
            view,
            resolve_target,
            batch,
            queue,
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
        load_op: wgpu::LoadOp<wgpu::Color>,
    ) {
        if batch.segments.is_empty() {
            return;
        }

        // Subir quads generales
        if !batch.quads.is_empty() {
            let bytes = batch.as_quad_bytes();
            let count = batch.quads.len() as u32;
            if count > self.max_instances {
                self.instance_buffer = self.device.create_buffer(&wgpu::BufferDescriptor {
                    label: Some("GUI Instance Buffer (resized)"),
                    size: std::mem::size_of::<GuiQuad>() as u64 * count as u64,
                    usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                    mapped_at_creation: false,
                });
                self.max_instances = count;
            }
            queue.write_buffer(&self.instance_buffer, 0, bytes);
        }

        // Subir quads de texto
        if !batch.text_quads.is_empty() {
            let bytes = batch.as_text_bytes();
            let count = batch.text_quads.len() as u32;
            if count > self.text_max_instances {
                self.text_instance_buffer = self.device.create_buffer(&wgpu::BufferDescriptor {
                    label: Some("GUI Text Instance Buffer (resized)"),
                    size: std::mem::size_of::<TextQuad>() as u64 * count as u64,
                    usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                    mapped_at_creation: false,
                });
                self.text_max_instances = count;
            }
            queue.write_buffer(&self.text_instance_buffer, 0, bytes);
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

        // Configurar lotes de texturas (si aplica)
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
            self.image_bind_group = bg;
        }

        for segment in &batch.segments {
            // Aplicar Scissor si existe
            if let Some(s) = segment.scissor {
                // Pinzar las coordenadas para evitar errores de WGPU si el rect es negativo o mayor que la resolución.
                let sx = (s.x.max(0.0) as u32).min(self.resolution[0] as u32);
                let sy = (s.y.max(0.0) as u32).min(self.resolution[1] as u32);
                let sw = (s.width.max(0.0) as u32).min(self.resolution[0] as u32 - sx);
                let sh = (s.height.max(0.0) as u32).min(self.resolution[1] as u32 - sy);

                if sw > 0 && sh > 0 {
                    rpass.set_scissor_rect(sx, sy, sw, sh);
                } else {
                    // Rectángulo de recorte vacío, saltamos el segmento
                    continue;
                }
            } else {
                rpass.set_scissor_rect(0, 0, self.resolution[0] as u32, self.resolution[1] as u32);
            }

            // 1. Dibujar Quads del segmento
            if !segment.quad_range.is_empty() {
                rpass.set_pipeline(&self.pipeline);
                rpass.set_bind_group(0, &self.uniform_bind_group, &[]);
                rpass.set_bind_group(1, &self.image_bind_group, &[]);
                rpass.set_vertex_buffer(0, self.vertex_buffer.slice(..));
                rpass.set_vertex_buffer(1, self.instance_buffer.slice(..));
                rpass.set_index_buffer(self.index_buffer.slice(..), wgpu::IndexFormat::Uint16);
                rpass.draw_indexed(0..6, 0, segment.quad_range.clone());
            }

            // 2. Dibujar Texto del segmento
            if !segment.text_range.is_empty() {
                if let Some(font_bg) = &self.font_bind_group {
                    rpass.set_pipeline(&self.text_pipeline);
                    rpass.set_bind_group(0, &self.uniform_bind_group, &[]);
                    rpass.set_bind_group(1, font_bg, &[]);
                    rpass.set_vertex_buffer(0, self.vertex_buffer.slice(..));
                    rpass.set_vertex_buffer(1, self.text_instance_buffer.slice(..));
                    rpass.set_index_buffer(self.index_buffer.slice(..), wgpu::IndexFormat::Uint16);
                    rpass.draw_indexed(0..6, 0, segment.text_range.clone());
                }
            }
        }
    }
}

/// Extensión para convertir comandos de renderizado abstractos en lotes optimizados para GPU.
pub trait ToBatches {
    /// Versión con soporte de texto. Requiere una fuente para la rasterización de glifos.
    #[cfg(feature = "text")]
    fn to_batches(&self, batch: &mut GuiBatch, font: Option<&ferrous_assets::Font>);

    /// Versión ligera sin soporte de texto.
    #[cfg(not(feature = "text"))]
    fn to_batches(&self, quad_batch: &mut GuiBatch);
}

impl GuiBatch {
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

                self.push_text(TextQuad {
                    pos: [qx, qy],
                    size: [quad_size, quad_size],
                    uv0: [metric.uv[0], metric.uv[1]],
                    uv1: [metric.uv[2], metric.uv[3]],
                    color,
                });
                x += metric.advance * size;
            }
        }
        self.update_last_segment();
    }

    /// Returns the pixel width of `text` rendered at `size` using the font atlas.
    #[cfg(feature = "text")]
    pub fn measure_text(font: &ferrous_assets::Font, text: &str, size: f32) -> f32 {
        text.chars()
            .map(|c| {
                font.atlas
                    .metrics
                    .get(&c)
                    .map(|m| m.advance * size)
                    .unwrap_or(size * 0.6)
            })
            .sum()
    }

    /// Returns the byte-index in `text` that is closest to `target_px` pixels
    /// from the start of the string, when rendered at `size`.
    #[cfg(feature = "text")]
    pub fn char_at_px(font: &ferrous_assets::Font, text: &str, size: f32, target_px: f32) -> usize {
        let mut x = 0.0f32;
        for (byte_idx, c) in text.char_indices() {
            let adv = font
                .atlas
                .metrics
                .get(&c)
                .map(|m| m.advance * size)
                .unwrap_or(size * 0.6);
            if x + adv * 0.5 >= target_px {
                return byte_idx;
            }
            x += adv;
        }
        text.len()
    }

    /// Draws a text-field (input box) with cursor, selection highlight, and
    /// horizontal scroll — all computed from real font metrics.
    ///
    /// # Arguments
    /// * `font`         — font atlas used for measurement and rendering
    /// * `x`, `y`       — top-left of the field box
    /// * `w`, `h`       — size of the field box
    /// * `text`         — full string value
    /// * `size`         — font size in pixels
    /// * `focused`      — draw focused border + cursor
    /// * `cursor_visible` — whether the blinking cursor should be shown
    /// * `cursor_pos`   — byte-index of the cursor inside `text`
    /// * `selection`    — optional `(start_byte, end_byte)` selection range
    /// * `text_color`   — RGBA color for the text
    /// * `bg_color`     — RGBA background fill
    /// * `border_color` — RGBA border when focused (pass `None` for no border)
    /// * `sel_color`    — RGBA selection highlight
    /// * `pad`          — horizontal text padding inside the box
    #[cfg(feature = "text")]
    #[allow(clippy::too_many_arguments)]
    pub fn draw_text_field(
        &mut self,
        font: &ferrous_assets::Font,
        x: f32,
        y: f32,
        w: f32,
        h: f32,
        text: &str,
        size: f32,
        focused: bool,
        cursor_visible: bool,
        cursor_pos: usize,
        selection: Option<(usize, usize)>,
        text_color: [f32; 4],
        bg_color: [f32; 4],
        border_color: Option<[f32; 4]>,
        sel_color: [f32; 4],
        pad: f32,
    ) {
        let inner_w = w - pad * 2.0;

        // ── Background ───────────────────────────────────────────────────────
        self.rect(x, y, w, h, bg_color);

        // ── Focused border ───────────────────────────────────────────────────
        if focused {
            if let Some(bc) = border_color {
                self.rect(x, y, w, 1.0, bc); // top
                self.rect(x, y + h - 1.0, w, 1.0, bc); // bottom
                self.rect(x, y, 1.0, h, bc); // left
                self.rect(x + w - 1.0, y, 1.0, h, bc); // right
            }
        }

        // ── Compute scroll so cursor stays visible ───────────────────────────
        let cursor_byte = cursor_pos.min(text.len());
        let cursor_px_from_start = Self::measure_text(font, &text[..cursor_byte], size);

        // scroll_px: how many pixels from the start of `text` are scrolled out
        let scroll_px = if cursor_px_from_start > inner_w {
            cursor_px_from_start - inner_w
        } else {
            0.0
        };

        // byte offset into `text` that maps to scroll_px
        let scroll_byte: usize = {
            let mut acc = 0.0f32;
            let mut result = 0usize;
            for (b, c) in text.char_indices() {
                let adv = font
                    .atlas
                    .metrics
                    .get(&c)
                    .map(|m| m.advance * size)
                    .unwrap_or(size * 0.6);
                if acc + adv > scroll_px {
                    result = b;
                    break;
                }
                acc += adv;
                result = b + c.len_utf8();
            }
            result
        };

        // visible slice clipped to inner_w
        let visible_str = {
            let after = &text[scroll_byte..];
            let mut end = after.len();
            let mut acc = 0.0f32;
            for (b, c) in after.char_indices() {
                let adv = font
                    .atlas
                    .metrics
                    .get(&c)
                    .map(|m| m.advance * size)
                    .unwrap_or(size * 0.6);
                if acc + adv > inner_w + 1.0 {
                    end = b;
                    break;
                }
                acc += adv;
            }
            &after[..end]
        };

        // helper: pixel X offset of a byte-index relative to the visible area
        let px_of_byte = |byte: usize| -> f32 {
            let b = byte.min(text.len());
            Self::measure_text(font, &text[..b], size) - scroll_px
        };

        // ── Clip rendering to inner area ─────────────────────────────────────
        self.push_clip(ferrous_ui_core::Rect {
            x,
            y,
            width: w,
            height: h,
        });

        // ── Selection highlight ──────────────────────────────────────────────
        if focused {
            if let Some((sel_start, sel_end)) = selection {
                let vis_start = px_of_byte(sel_start).max(0.0);
                let vis_end = px_of_byte(sel_end).min(inner_w);
                let sx = x + pad + vis_start;
                let sw = vis_end - vis_start;
                if sw > 0.0 {
                    self.rect(sx, y + 1.0, sw, h - 2.0, sel_color);
                }
            }
        }

        // ── Text ─────────────────────────────────────────────────────────────
        let text_y = y + (h - size) * 0.5;
        self.draw_text(font, visible_str, [x + pad, text_y], size, text_color);

        // ── Cursor ───────────────────────────────────────────────────────────
        if focused && cursor_visible && selection.is_none() {
            let cur_x = x + pad + px_of_byte(cursor_byte);
            self.rect(cur_x, y + 2.0, 1.5, h - 4.0, [1.0, 1.0, 1.0, 0.9]);
        }

        self.pop_clip();
    }

    pub fn push_clip(&mut self, rect: ferrous_ui_core::Rect) {
        let new_rect = if let Some(current) = self.current_scissor {
            current.intersect(&rect)
        } else {
            rect
        };
        self.scissor_stack.push(new_rect);
        self.current_scissor = Some(new_rect);
        self.ensure_segment();
    }

    pub fn pop_clip(&mut self) {
        self.scissor_stack.pop();
        self.current_scissor = self.scissor_stack.last().copied();
        self.ensure_segment();
    }
}

impl ToBatches for ferrous_ui_core::RenderCommand {
    #[cfg(feature = "text")]
    fn to_batches(&self, batch: &mut GuiBatch, font: Option<&ferrous_assets::Font>) {
        use ferrous_ui_core::RenderCommand;
        match self {
            RenderCommand::Quad {
                rect,
                color,
                radii,
                flags,
            } => {
                batch.push_quad(GuiQuad {
                    pos: [rect.x, rect.y],
                    size: [rect.width, rect.height],
                    uv0: [0.0, 0.0],
                    uv1: [1.0, 1.0],
                    color: *color,
                    radii: *radii,
                    tex_index: 0,
                    flags: *flags,
                });
                batch.update_last_segment();
            }
            #[cfg(feature = "assets")]
            RenderCommand::Image {
                rect,
                texture,
                uv0,
                uv1,
                color,
            } => {
                let idx = batch.reserve_texture_slot(texture.clone());
                batch.push_quad(GuiQuad {
                    pos: [rect.x, rect.y],
                    size: [rect.width, rect.height],
                    uv0: *uv0,
                    uv1: *uv1,
                    color: *color,
                    radii: [0.0; 4],
                    tex_index: idx,
                    flags: TEXTURED_BIT,
                });
                batch.update_last_segment();
            }
            RenderCommand::Text {
                rect,
                text,
                color,
                font_size,
                align,
            } => {
                if let Some(font) = font {
                    // Measure actual text width using real font metrics
                    let text_w = GuiBatch::measure_text(font, text, *font_size);
                    // Visual height of a glyph: font_size is the cap-height reference
                    let text_h = *font_size;
                    let x = align.resolve_x(rect.x, rect.width, text_w, 4.0);
                    let y = align.resolve_y(rect.y, rect.height, text_h, 4.0);
                    batch.draw_text(font, text, [x, y], *font_size, *color);
                }
            }
            RenderCommand::PushClip { rect } => {
                batch.push_clip(*rect);
            }
            RenderCommand::PopClip => {
                batch.pop_clip();
            }
            RenderCommand::GradientQuad {
                rect,
                background,
                radii,
                raster_resolution: _,
            } => {
                // Decompose into N vertical strips. Each strip carries:
                //   uv0.x = normalised left-edge offset of the strip inside the full rect
                //   uv0.y = 0.0
                //   uv1   = (full_rect_w, full_rect_h)  ← used by shader SDF
                //   radii = full rect radii (shader uses uv1 for the distance test)
                //   flags = GRADIENT_BIT
                const N: u32 = 64;
                let strip_w = rect.width / N as f32;
                let full_w = rect.width;
                let full_h = rect.height;
                for i in 0..N {
                    let u = (i as f32 + 0.5) / N as f32;
                    let color = background.sample(u, 0.5);
                    // Normalised left-edge of this strip within the full rect
                    let strip_offset_norm = i as f32 / N as f32;
                    batch.push_quad(GuiQuad {
                        pos: [rect.x + i as f32 * strip_w, rect.y],
                        size: [strip_w + 0.5, full_h],
                        uv0: [strip_offset_norm, 0.0],
                        uv1: [full_w, full_h],
                        color,
                        radii: *radii,
                        tex_index: 0,
                        flags: GRADIENT_BIT,
                    });
                }
                batch.update_last_segment();
            }
            #[cfg(not(feature = "assets"))]
            RenderCommand::Image { .. } => {}
        }
    }

    #[cfg(not(feature = "text"))]
    fn to_batches(&self, quad_batch: &mut GuiBatch) {
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
            RenderCommand::GradientQuad {
                rect,
                background,
                radii,
                raster_resolution: _,
            } => {
                const N: u32 = 64;
                let strip_w = rect.width / N as f32;
                let full_w = rect.width;
                let full_h = rect.height;
                for i in 0..N {
                    let u = (i as f32 + 0.5) / N as f32;
                    let color = background.sample(u, 0.5);
                    let strip_offset_norm = i as f32 / N as f32;
                    quad_batch.push_quad(GuiQuad {
                        pos: [rect.x + i as f32 * strip_w, rect.y],
                        size: [strip_w + 0.5, full_h],
                        uv0: [strip_offset_norm, 0.0],
                        uv1: [full_w, full_h],
                        color,
                        radii: *radii,
                        tex_index: 0,
                        flags: GRADIENT_BIT,
                    });
                }
            }
        }
    }
}
