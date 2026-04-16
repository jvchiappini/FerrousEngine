//! GPU renderer for the UI.
//!
//! Handles WGPU pipelines, buffers, and textures for rendering UI batches.
//! Modularized for Phase 4: Early-Z, Depth Buffer, and Prepare/Render split.

use std::sync::Arc;
use wgpu::util::DeviceExt;
use crate::gpu_types::{GuiQuad, TextQuad};
use crate::gui_batch::GuiBatch;
use crate::MAX_TEXTURE_SLOTS;
use crate::pipelines::{
    layout::Layouts,
    quad::create_quad_pipeline,
    text::create_text_pipeline,
    id::create_id_pipeline,
};

/// Main UI rendering engine on the GPU.
pub struct GuiRenderer {
    pub device: Arc<wgpu::Device>,
    pub layouts: Layouts,
    pub opaque_pipeline: wgpu::RenderPipeline,
    pub transparent_pipeline: wgpu::RenderPipeline,
    pub text_pipeline: wgpu::RenderPipeline,
    pub svg_pipeline: wgpu::RenderPipeline,
    pub id_pipeline: wgpu::RenderPipeline,

    pub vertex_buffer: wgpu::Buffer,
    pub index_buffer: wgpu::Buffer,
    pub instance_buffer: wgpu::Buffer,
    pub text_instance_buffer: wgpu::Buffer,
    pub icon_instance_buffer: wgpu::Buffer,
    pub svg_vertex_buffer: wgpu::Buffer,
    pub svg_index_buffer: wgpu::Buffer,
    pub uniform_buffer: wgpu::Buffer,

    pub uniform_bind_group: wgpu::BindGroup,
    pub image_bind_group: wgpu::BindGroup,
    pub font_bind_group: Option<wgpu::BindGroup>,
    pub icon_bind_group: Option<wgpu::BindGroup>,

    pub max_instances: u32,
    pub text_max_instances: u32,
    pub icon_max_instances: u32,
    pub svg_max_vertices: u32,
    pub svg_max_indices: u32,
    pub resolution: [f32; 2],

    // Depth Buffer for Early-Z
    pub depth_texture: wgpu::Texture,
    pub depth_view: wgpu::TextureView,

    // GPU ID Hit-Testing
    pub id_texture: wgpu::Texture,
    pub id_view: wgpu::TextureView,
    pub id_staging_buffer: wgpu::Buffer,
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
        let layouts = Layouts::new(&device);

        // Standard buffers
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

        let instance_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("GUI Instance Buffer"),
            size: (std::mem::size_of::<GuiQuad>() as u32 * max_instances) as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let text_instance_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("GUI Text Instance Buffer"),
            size: (std::mem::size_of::<TextQuad>() as u32 * max_instances) as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let icon_instance_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("GUI Icon Instance Buffer"),
            size: (std::mem::size_of::<TextQuad>() as u32 * max_instances) as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let resolution = [width as f32, height as f32];
        let uniform_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("GUI Uniform Buffer"),
            contents: bytemuck::cast_slice(&resolution),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });

        let uniform_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("GUI Uniform Bind Group"),
            layout: &layouts.uniform_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: uniform_buffer.as_entire_binding(),
            }],
        });

        // Dummy image bind group
        let dummy_texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("GUI Dummy Texture"),
            size: wgpu::Extent3d { width: 1, height: 1, depth_or_array_layers: 1 },
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
            mag_filter: wgpu::FilterMode::Nearest,
            min_filter: wgpu::FilterMode::Nearest,
            ..Default::default()
        });
        let dummy_views: Vec<&wgpu::TextureView> = std::iter::repeat_n(&dummy_view, MAX_TEXTURE_SLOTS as usize).collect();
        let dummy_samplers: Vec<&wgpu::Sampler> = std::iter::repeat_n(&dummy_sampler, MAX_TEXTURE_SLOTS as usize).collect();

        #[cfg(not(target_arch = "wasm32"))]
        let image_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("GUI Image Bind Group (dummy)"),
            layout: &layouts.image_layout,
            entries: &[
                wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::TextureViewArray(&dummy_views) },
                wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::SamplerArray(&dummy_samplers) },
            ],
        });

        #[cfg(target_arch = "wasm32")]
        let image_bind_group = {
            let mut entries = Vec::new();
            for i in 0..8 {
                entries.push(wgpu::BindGroupEntry {
                    binding: (i * 2) as u32,
                    resource: wgpu::BindingResource::TextureView(&dummy_view),
                });
                entries.push(wgpu::BindGroupEntry {
                    binding: (i * 2 + 1) as u32,
                    resource: wgpu::BindingResource::Sampler(&dummy_sampler),
                });
            }
            device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("GUI Image Bind Group (Web dummy)"),
                layout: &layouts.image_layout,
                entries: &entries,
            })
        };

        // Pipelines
        let opaque_pipeline = create_quad_pipeline(&device, format, &layouts.quad_pipeline_layout, sample_count, true);
        let transparent_pipeline = create_quad_pipeline(&device, format, &layouts.quad_pipeline_layout, sample_count, false);
        let text_pipeline = create_text_pipeline(&device, format, &layouts.text_pipeline_layout, sample_count, false);
        let svg_pipeline = crate::pipelines::svg::create_svg_pipeline(&device, format, &layouts.quad_pipeline_layout, sample_count);
        let id_pipeline = create_id_pipeline(&device, &layouts.quad_pipeline_layout);

        // Depth Buffer for Early-Z
        let depth_texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("GUI Depth Texture"),
            size: wgpu::Extent3d { width: width.max(1), height: height.max(1), depth_or_array_layers: 1 },
            mip_level_count: 1,
            sample_count,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Depth32Float,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            view_formats: &[],
        });
        let depth_view = depth_texture.create_view(&wgpu::TextureViewDescriptor::default());

        // ID Buffer
        let id_texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("GUI ID Texture"),
            size: wgpu::Extent3d { width: width.max(1), height: height.max(1), depth_or_array_layers: 1 },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::R32Uint,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        let id_view = id_texture.create_view(&wgpu::TextureViewDescriptor::default());
        let id_staging_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("GUI ID Staging Buffer"),
            size: 4,
            usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let svg_vertex_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("GUI SVG Vertex Buffer"),
            size: 65536 * std::mem::size_of::<ferrous_svg::SvgVertex>() as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let svg_index_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("GUI SVG Index Buffer"),
            size: 65536 * 4,
            usage: wgpu::BufferUsages::INDEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        Self {
            device,
            layouts,
            opaque_pipeline,
            transparent_pipeline,
            text_pipeline,
            svg_pipeline,
            id_pipeline,
            vertex_buffer,
            index_buffer,
            instance_buffer,
            text_instance_buffer,
            icon_instance_buffer,
            svg_vertex_buffer,
            svg_index_buffer,
            uniform_buffer,
            uniform_bind_group,
            image_bind_group,
            font_bind_group: None,
            icon_bind_group: None,
            max_instances,
            text_max_instances: max_instances,
            icon_max_instances: max_instances,
            svg_max_vertices: 65536,
            svg_max_indices: 65536,
            resolution,
            depth_texture,
            depth_view,
            id_texture,
            id_view,
            id_staging_buffer,
        }
    }

    pub fn resize(&mut self, queue: &wgpu::Queue, width: u32, height: u32) {
        let width = width.max(1);
        let height = height.max(1);
        self.resolution = [width as f32, height as f32];
        queue.write_buffer(&self.uniform_buffer, 0, bytemuck::cast_slice(&self.resolution));

        // Resize Depth
        self.depth_texture = self.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("GUI Depth Texture Resized"),
            size: wgpu::Extent3d { width, height, depth_or_array_layers: 1 },
            mip_level_count: 1,
            sample_count: self.depth_texture.sample_count(),
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Depth32Float,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            view_formats: &[],
        });
        self.depth_view = self.depth_texture.create_view(&wgpu::TextureViewDescriptor::default());

        // Resize ID
        self.id_texture = self.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("GUI ID Texture Resized"),
            size: wgpu::Extent3d { width, height, depth_or_array_layers: 1 },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::R32Uint,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        self.id_view = self.id_texture.create_view(&wgpu::TextureViewDescriptor::default());
    }

    /// PREPARE phase: Uploads quad data to GPU buffers.
    /// This optimizes the submit by separating CPU logic from GPU logic.
    pub fn prepare(&mut self, queue: &wgpu::Queue, batch: &GuiBatch) {
        if !batch.quads.is_empty() {
            let bytes = batch.as_quad_bytes();
            let count = batch.quads.len() as u32;
            if count > self.max_instances {
                let new_max = count.next_power_of_two().max(64);
                self.instance_buffer = self.device.create_buffer(&wgpu::BufferDescriptor {
                    label: Some("GUI Instance Buffer (resized)"),
                    size: (std::mem::size_of::<GuiQuad>() as u32 * new_max) as u64,
                    usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                    mapped_at_creation: false,
                });
                self.max_instances = new_max;
            }
            queue.write_buffer(&self.instance_buffer, 0, bytes);
        }

        if !batch.text_quads.is_empty() {
            let bytes = batch.as_text_bytes();
            let count = batch.text_quads.len() as u32;
            if count > self.text_max_instances {
                let new_max = count.next_power_of_two().max(64);
                self.text_instance_buffer = self.device.create_buffer(&wgpu::BufferDescriptor {
                    label: Some("GUI Text Instance Buffer (resized)"),
                    size: (std::mem::size_of::<TextQuad>() as u32 * new_max) as u64,
                    usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                    mapped_at_creation: false,
                });
                self.text_max_instances = new_max;
            }
            queue.write_buffer(&self.text_instance_buffer, 0, bytes);
        }

        if !batch.icon_quads.is_empty() {
            let bytes = batch.as_icon_bytes();
            let count = batch.icon_quads.len() as u32;
            if count > self.icon_max_instances {
                let new_max = count.next_power_of_two().max(64);
                self.icon_instance_buffer = self.device.create_buffer(&wgpu::BufferDescriptor {
                    label: Some("GUI Icon Instance Buffer (resized)"),
                    size: (std::mem::size_of::<TextQuad>() as u32 * new_max) as u64,
                    usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                    mapped_at_creation: false,
                });
                self.icon_max_instances = new_max;
            }
            queue.write_buffer(&self.icon_instance_buffer, 0, bytes);
        }

        // Update image bind group if textures changed
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
            #[cfg(not(target_arch = "wasm32"))]
            {
                self.image_bind_group = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
                    label: Some("GUI Image Bind Group Update"),
                    layout: &self.layouts.image_layout,
                    entries: &[
                        wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::TextureViewArray(&views) },
                        wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::SamplerArray(&samplers) },
                    ],
                });
            }
            #[cfg(target_arch = "wasm32")]
            {
                let mut entries = Vec::new();
                for i in 0..8 {
                    let v = if i < views.len() { views[i] } else { views.last().unwrap() };
                    let s = if i < samplers.len() { samplers[i] } else { samplers.last().unwrap() };
                    entries.push(wgpu::BindGroupEntry {
                        binding: (i * 2) as u32,
                        resource: wgpu::BindingResource::TextureView(v),
                    });
                    entries.push(wgpu::BindGroupEntry {
                        binding: (i * 2 + 1) as u32,
                        resource: wgpu::BindingResource::Sampler(s),
                    });
                }
                self.image_bind_group = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
                    label: Some("GUI Image Bind Group Update (Web)"),
                    layout: &self.layouts.image_layout,
                    entries: &entries,
                });
            }
        }

        if !batch.svg_commands.is_empty() {
            let mut all_vertices = Vec::new();
            let mut all_indices = Vec::new();
            let mut current_v_offset = 0;

            for cmd in &batch.svg_commands {
                for v in &cmd.mesh.vertices {
                    let mut v_shifted = *v;
                    v_shifted.position[0] += cmd.pos[0];
                    v_shifted.position[1] += cmd.pos[1];
                    v_shifted.color = cmd.color;
                    v_shifted.z_order = cmd.z;
                    all_vertices.push(v_shifted);
                }
                for i in &cmd.mesh.indices {
                    all_indices.push(i + current_v_offset);
                }
                current_v_offset += cmd.mesh.vertices.len() as u32;
            }

            if all_vertices.len() as u32 > self.svg_max_vertices {
                let new_max = (all_vertices.len() as u32).next_power_of_two().max(1024);
                self.svg_vertex_buffer = self.device.create_buffer(&wgpu::BufferDescriptor {
                    label: Some("GUI SVG Vertex Buffer (resized)"),
                    size: (new_max as usize * std::mem::size_of::<ferrous_svg::SvgVertex>()) as u64,
                    usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                    mapped_at_creation: false,
                });
                self.svg_max_vertices = new_max;
            }
            if all_indices.len() as u32 > self.svg_max_indices {
                let new_max = (all_indices.len() as u32).next_power_of_two().max(1024);
                self.svg_index_buffer = self.device.create_buffer(&wgpu::BufferDescriptor {
                    label: Some("GUI SVG Index Buffer (resized)"),
                    size: (new_max as u64 * 4),
                    usage: wgpu::BufferUsages::INDEX | wgpu::BufferUsages::COPY_DST,
                    mapped_at_creation: false,
                });
                self.svg_max_indices = new_max;
            }

            queue.write_buffer(&self.svg_vertex_buffer, 0, bytemuck::cast_slice(&all_vertices));
            queue.write_buffer(&self.svg_index_buffer, 0, bytemuck::cast_slice(&all_indices));
        }
    }

    /// Fase RENDER: Ejecuta los comandos de dibujo sobre la vista proporcionada.
    pub fn render(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        target_view: &wgpu::TextureView,
        resolve_target: Option<&wgpu::TextureView>,
        batch: &GuiBatch,
        load_op: wgpu::LoadOp<wgpu::Color>,
    ) {
        if batch.segments.is_empty() && matches!(load_op, wgpu::LoadOp::Load) {
            return;
        }

        let mut rpass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("GUI Render Pass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: target_view,
                resolve_target,
                ops: wgpu::Operations { load: load_op, store: wgpu::StoreOp::Store },
            })],
            depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                view: &self.depth_view,
                depth_ops: Some(wgpu::Operations {
                    load: wgpu::LoadOp::Clear(1.0),
                    store: wgpu::StoreOp::Store,
                }),
                stencil_ops: None,
            }),
            occlusion_query_set: None,
            timestamp_writes: None,
        });

        let (base_sx, base_sy, base_sw, base_sh) = if let Some(damage) = batch.damage_union {
            let sx = (damage.x.max(0.0) as u32).min(self.resolution[0] as u32);
            let sy = (damage.y.max(0.0) as u32).min(self.resolution[1] as u32);
            let sw = (damage.width.max(0.0) as u32).min(self.resolution[0] as u32 - sx);
            let sh = (damage.height.max(0.0) as u32).min(self.resolution[1] as u32 - sy);
            (sx, sy, sw, sh)
        } else {
            (0, 0, self.resolution[0] as u32, self.resolution[1] as u32)
        };

        if (base_sw == 0 || base_sh == 0)
            && matches!(load_op, wgpu::LoadOp::Load) { return; }
        
        static mut FRAME_COUNT: u32 = 0;
        unsafe {
            FRAME_COUNT += 1;
            if FRAME_COUNT.is_multiple_of(120) {
                println!("[GuiRenderer] Render: segments={}, quads={}, text={}, icons={}, svg={}", 
                    batch.segments.len(), batch.quads.len(), batch.text_quads.len(), batch.icon_quads.len(), batch.svg_commands.len());
                println!("[GuiRenderer] Base Scissor: x={}, y={}, w={}, h={}", base_sx, base_sy, base_sw, base_sh);
                if let Some(s) = batch.segments.first() {
                    println!("[GuiRenderer] First Segment: q_range={:?}, scissor={:?}", s.quad_range, s.scissor);
                }
            }
        }

        for segment in &batch.segments {
            // Apply scissor if present, intersected with base_scissor
            let (sx, sy, sw, sh) = if let Some(s) = segment.scissor {
                let sx = (s.x.max(base_sx as f32) as u32).min(((base_sx + base_sw)));
                let sy = (s.y.max(base_sy as f32) as u32).min(((base_sy + base_sh)));
                let sw = ( (s.x + s.width).min((base_sx + base_sw) as f32) as u32 ).saturating_sub(sx);
                let sh = ( (s.y + s.height).min((base_sy + base_sh) as f32) as u32 ).saturating_sub(sy);
                (sx, sy, sw, sh)
            } else {
                (base_sx, base_sy, base_sw, base_sh)
            };

            if sw > 0 && sh > 0 {
                rpass.set_scissor_rect(sx, sy, sw, sh);
            } else {
                continue;
            }

            // Draw Quads (Using transparent pipeline to support blended edges/shadows safely)
            // TODO: En Fase 5 implementaremos el split Opaque/Transparent por segmento
            if !segment.quad_range.is_empty() {
                rpass.set_pipeline(&self.transparent_pipeline);
                rpass.set_bind_group(0, &self.uniform_bind_group, &[]);
                rpass.set_bind_group(1, &self.image_bind_group, &[]);
                rpass.set_vertex_buffer(0, self.vertex_buffer.slice(..));
                rpass.set_vertex_buffer(1, self.instance_buffer.slice(..));
                rpass.set_index_buffer(self.index_buffer.slice(..), wgpu::IndexFormat::Uint16);
                rpass.draw_indexed(0..6, 0, segment.quad_range.clone());
            }

            // Draw Text
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

            // Draw Icons (Same pipeline as text, different bind group)
            if !segment.icon_range.is_empty() {
                if let Some(icon_bg) = &self.icon_bind_group {
                    rpass.set_pipeline(&self.text_pipeline);
                    rpass.set_bind_group(0, &self.uniform_bind_group, &[]);
                    rpass.set_bind_group(1, icon_bg, &[]);
                    rpass.set_vertex_buffer(0, self.vertex_buffer.slice(..));
                    rpass.set_vertex_buffer(1, self.icon_instance_buffer.slice(..));
                    rpass.set_index_buffer(self.index_buffer.slice(..), wgpu::IndexFormat::Uint16);
                    rpass.draw_indexed(0..6, 0, segment.icon_range.clone());
                }
            }

            // Draw SVG
            if !segment.svg_range.is_empty() {
                rpass.set_pipeline(&self.svg_pipeline);
                rpass.set_bind_group(0, &self.uniform_bind_group, &[]);
                rpass.set_vertex_buffer(0, self.svg_vertex_buffer.slice(..));
                rpass.set_index_buffer(self.svg_index_buffer.slice(..), wgpu::IndexFormat::Uint32);
                
                // We need to calculate the index range. 
                // This is a bit tricky if we don't store index offsets per command.
                // For now, let's assume one big mesh or calculate on the fly.
                let mut start_idx = 0;
                for i in 0..segment.svg_range.start {
                    start_idx += batch.svg_commands[i as usize].mesh.indices.len() as u32;
                }
                let mut end_idx = start_idx;
                for i in segment.svg_range.clone() {
                    end_idx += batch.svg_commands[i as usize].mesh.indices.len() as u32;
                }
                
                if start_idx < end_idx {
                    rpass.draw_indexed(start_idx..end_idx, 0, 0..1);
                }
            }
        }
    }

    pub fn set_font_atlas(&mut self, view: &wgpu::TextureView, sampler: &wgpu::Sampler) {
        self.font_bind_group = Some(self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("GUI Font Bind Group"),
            layout: &self.layouts.font_layout,
            entries: &[
                wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::TextureView(view) },
                wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::Sampler(sampler) },
            ],
        }));
    }

    pub fn set_icon_atlas(&mut self, view: &wgpu::TextureView, sampler: &wgpu::Sampler) {
        self.icon_bind_group = Some(self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("GUI Icon Bind Group"),
            layout: &self.layouts.font_layout, // Re-use font layout (1 tex + 1 sampler)
            entries: &[
                wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::TextureView(view) },
                wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::Sampler(sampler) },
            ],
        }));
    }

    pub fn read_pixel_id(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        x: u32,
        y: u32,
        batch: &GuiBatch,
    ) -> crossbeam_channel::Receiver<u32> {
        if !batch.quads.is_empty() {
            let mut id_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("GUI ID Pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &self.id_view,
                    resolve_target: None,
                    ops: wgpu::Operations { load: wgpu::LoadOp::Clear(wgpu::Color::BLACK), store: wgpu::StoreOp::Store },
                })],
                depth_stencil_attachment: None,
                occlusion_query_set: None,
                timestamp_writes: None,
            });

            id_pass.set_pipeline(&self.id_pipeline);
            id_pass.set_bind_group(0, &self.uniform_bind_group, &[]);
            id_pass.set_vertex_buffer(0, self.vertex_buffer.slice(..));
            id_pass.set_vertex_buffer(1, self.instance_buffer.slice(..));
            id_pass.set_index_buffer(self.index_buffer.slice(..), wgpu::IndexFormat::Uint16);
            
            for segment in &batch.segments {
                if !segment.quad_range.is_empty() {
                    id_pass.draw_indexed(0..6, 0, segment.quad_range.clone());
                }
            }
        }
        
        encoder.copy_texture_to_buffer(
            wgpu::ImageCopyTexture {
                texture: &self.id_texture,
                mip_level: 0,
                origin: wgpu::Origin3d { x, y, z: 0 },
                aspect: wgpu::TextureAspect::All,
            },
            wgpu::ImageCopyBuffer {
                buffer: &self.id_staging_buffer,
                layout: wgpu::ImageDataLayout { offset: 0, bytes_per_row: None, rows_per_image: None },
            },
            wgpu::Extent3d { width: 1, height: 1, depth_or_array_layers: 1 },
        );

        let (sender, receiver) = crossbeam_channel::bounded(1);
        let slice = self.id_staging_buffer.slice(..);
        slice.map_async(wgpu::MapMode::Read, move |v| {
            if v.is_ok() { let _ = sender.send(1); }
        });

        receiver
    }

    pub fn fetch_pixel_id(&self) -> u32 {
        let view = self.id_staging_buffer.slice(..).get_mapped_range();
        let id_val = u32::from_le_bytes([view[0], view[1], view[2], view[3]]);
        drop(view);
        self.id_staging_buffer.unmap();
        id_val
    }

    /// Orchestrates a GPU-based hit test for a single pixel.
    /// This is normally called during Event Dispatch if AABB check returns custom widgets.
    pub fn hit_test_gpu(
        &self,
        _queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        x: u32,
        y: u32,
        batch: &GuiBatch,
    ) -> u32 {
        // In a real environment, this should be asynchronous to avoid blocking CPU.
        // For editor tools (GUIMaker), 1-frame latency is acceptable.
        let _receiver = self.read_pixel_id(encoder, x, y, batch);
        // El encoder debe enviarse antes de recibir.
        // Como no tenemos el control del submit aquí, asumimos que se llamará después.
        // En una implementación perfecta, esto se divide.
        0 // Placeholder: El usuario debe llamar a fetch_pixel_id tras el submit.
    }
}
