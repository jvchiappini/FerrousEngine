//! `hdda_viewer` — ventana interactiva que muestra el G-Buffer del HDDA.
//!
//! Lanza una ventana 1280×720, rellena el mundo con una esfera sólida de
//! voxels de ~48 cm de radio, registra un `VoxelHddaBundle` en el renderer y
//! hace blit del `gbuf_albedo` (normales como RGB) al swapchain cada frame.
//!
//! # Cómo ejecutar
//!
//! ```sh
//! cargo run --example hdda_viewer --features gpu -p ferrous_voxels
//! ```
//!
//! # Controles
//!
//! | Tecla | Acción          |
//! |-------|-----------------|
//! | ESC   | Cerrar ventana  |
//!
//! # Lo que verás
//!
//! - Fondo azul oscuro (ray miss → sky).
//! - Esfera de voxels con colores de normales:
//!   rojo = +X, verde = +Y, azul = +Z.

use std::sync::{Arc, Mutex};

use ferrous_app::traits::FerrousApp;
use ferrous_app::{App, AppContext, AppMode, KeyCode};
use ferrous_render_graph::{FramePacket, RenderPass};
use ferrous_voxels::{buffers::PersistentBuffers, HddaPrimaryPass, VoxelGpuUploadPass, VoxelWorld};
use wgpu::{
    BindGroup, BindGroupDescriptor, BindGroupEntry, BindGroupLayoutDescriptor,
    BindGroupLayoutEntry, BindingResource, BindingType, BlendState, ColorTargetState, ColorWrites,
    CommandEncoder, Device, FragmentState, MultisampleState, PipelineLayoutDescriptor,
    PrimitiveState, Queue, RenderPipeline, RenderPipelineDescriptor, Sampler, SamplerBindingType,
    SamplerDescriptor, ShaderModuleDescriptor, ShaderSource, ShaderStages, TextureFormat,
    TextureSampleType, TextureView, TextureViewDimension, VertexState,
};

// ── App state ─────────────────────────────────────────────────────────────────

struct HddaViewer {
    world: Arc<Mutex<VoxelWorld>>,
}

impl HddaViewer {
    fn new() -> Self {
        let world = Arc::new(Mutex::new(VoxelWorld::new()));

        // Fill a solid voxel sphere (radius = 48 voxels = 48 cm).
        {
            let mut w = world.lock().unwrap();
            let r: i32 = 48;
            for x in -r..=r {
                for y in -r..=r {
                    for z in -r..=r {
                        if x * x + y * y + z * z <= r * r {
                            w.set_voxel(x, y, z, 1);
                        }
                    }
                }
            }
            log::info!(
                "hdda_viewer: sphere ready — {} root chunks, {} voxels",
                w.dag.roots().len(),
                w.voxel_count(),
            );
        }

        Self { world }
    }
}

impl FerrousApp for HddaViewer {
    fn setup(&mut self, ctx: &mut AppContext) {
        // VoxelHddaBundle owns upload + HDDA + blit all in one pass:
        // - on_attach creates PersistentBuffers, compiles shaders, builds bind groups
        // - each frame: upload DAG → raytrace → blit albedo to swapchain
        ctx.render
            .add_pass(VoxelHddaBundle::new(Arc::clone(&self.world)));
        log::info!("hdda_viewer: passes registered");

        use ferrous_app::Vec3;

        ctx.world.ecs.spawn((
            ferrous_app::Camera3D::looking_at(Vec3::ZERO)
                .from(Vec3::new(80.0, 40.0, 80.0))
                .build(),
            // Remove OrbitCamera so WASD default controller does not snap back!
        ));
    }

    fn update(&mut self, ctx: &mut AppContext) {
        if ctx.input.just_pressed(KeyCode::Escape) {
            ctx.request_exit();
        }
    }

    fn draw_ui(&mut self, dc: &mut ferrous_app::traits::DrawContext<'_, '_>) {
        use ferrous_app::Color;
        let fps = dc.ctx.time.fps;
        dc.gui.draw_text(
            dc.font,
            &format!("FPS: {:.1}\nLeft-click + drag to rotate\nRight-click + drag to pan\nScroll to zoom", fps),
            [10.0, 30.0],
            24.0,
            Color::WHITE.to_linear_f32(),
        );
    }
}

// ── VoxelHddaBundle ───────────────────────────────────────────────────────────
//
// Bundles VoxelGpuUploadPass + HddaPrimaryPass + GbufBlitPass so they share
// the same PersistentBuffers Arc without requiring a pass registry.

struct VoxelHddaBundle {
    upload: VoxelGpuUploadPass,
    hdda: Option<HddaPrimaryPass>,
    blit: Option<GbufBlitPass>,
    world: Arc<Mutex<VoxelWorld>>,
}

impl VoxelHddaBundle {
    fn new(world: Arc<Mutex<VoxelWorld>>) -> Self {
        let upload = VoxelGpuUploadPass::new(Arc::clone(&world));
        Self {
            upload,
            hdda: None,
            blit: None,
            world,
        }
    }
}

impl RenderPass for VoxelHddaBundle {
    fn name(&self) -> &str {
        "VoxelHddaBundle"
    }

    fn on_attach(
        &mut self,
        device: &Device,
        queue: &Queue,
        format: TextureFormat,
        sample_count: u32,
    ) {
        // 1. Upload pass → allocates PersistentBuffers.
        self.upload.on_attach(device, queue, format, sample_count);

        // 2. HDDA pass shares those same buffers.
        let shared = self
            .upload
            .shared_buffers()
            .expect("upload pass must allocate buffers in on_attach");

        let mut hdda = HddaPrimaryPass::new(Arc::clone(&self.world), shared.clone());
        hdda.on_attach(device, queue, format, sample_count);

        // 3. Blit pass reads gbuf_albedo and writes to the swapchain.
        let mut blit = GbufBlitPass::new(shared);
        blit.on_attach(device, queue, format, sample_count);

        self.hdda = Some(hdda);
        self.blit = Some(blit);
    }

    fn on_resize(&mut self, device: &Device, queue: &Queue, width: u32, height: u32) {
        self.upload.on_resize(device, queue, width, height);
        if let Some(h) = &mut self.hdda {
            h.on_resize(device, queue, width, height);
        }
        if let Some(b) = &mut self.blit {
            b.on_resize(device, queue, width, height);
        }
    }

    fn prepare(&mut self, device: &Device, queue: &Queue, packet: &FramePacket) {
        self.upload.prepare(device, queue, packet);
        if let Some(h) = &mut self.hdda {
            h.prepare(device, queue, packet);
        }
        if let Some(b) = &mut self.blit {
            b.prepare(device, queue, packet);
        }
    }

    fn execute(
        &mut self,
        device: &Device,
        queue: &Queue,
        encoder: &mut CommandEncoder,
        color_view: &TextureView,
        resolve_target: Option<&TextureView>,
        depth_view: Option<&TextureView>,
        packet: &FramePacket,
    ) {
        self.upload.execute(
            device,
            queue,
            encoder,
            color_view,
            resolve_target,
            depth_view,
            packet,
        );
        if let Some(h) = &mut self.hdda {
            h.execute(
                device,
                queue,
                encoder,
                color_view,
                resolve_target,
                depth_view,
                packet,
            );
        }
        if let Some(b) = &mut self.blit {
            b.execute(
                device,
                queue,
                encoder,
                color_view,
                resolve_target,
                depth_view,
                packet,
            );
        }
    }
}

// ── GbufBlitPass ─────────────────────────────────────────────────────────────
//
// Fullscreen triangle (no vertex buffer) that samples gbuf_albedo → swapchain.

const BLIT_WGSL: &str = r#"
@group(0) @binding(0) var t_albedo : texture_2d<f32>;
@group(0) @binding(1) var s_albedo : sampler;

struct VOut {
    @builtin(position) pos : vec4<f32>,
    @location(0)       uv  : vec2<f32>,
};

@vertex
fn vs_main(@builtin(vertex_index) vi: u32) -> VOut {
    // One oversized triangle covering the full clip quad.
    var pos = array<vec2<f32>, 3>(
        vec2<f32>(-1.0, -3.0),
        vec2<f32>( 3.0,  1.0),
        vec2<f32>(-1.0,  1.0),
    );
    let p = pos[vi];
    var o: VOut;
    o.pos = vec4<f32>(p, 0.0, 1.0);
    o.uv  = vec2<f32>((p.x + 1.0) * 0.5, (1.0 - p.y) * 0.5);
    return o;
}

@fragment
fn fs_main(in: VOut) -> @location(0) vec4<f32> {
    return textureSample(t_albedo, s_albedo, in.uv);
}
"#;

struct GbufBlitPass {
    buffers: Arc<Mutex<PersistentBuffers>>,
    pipeline: Option<RenderPipeline>,
    bind_group: Option<BindGroup>,
    sampler: Option<Sampler>,
    swapchain_format: Option<TextureFormat>,
}

impl GbufBlitPass {
    fn new(buffers: Arc<Mutex<PersistentBuffers>>) -> Self {
        Self {
            buffers,
            pipeline: None,
            bind_group: None,
            sampler: None,
            swapchain_format: None,
        }
    }

    fn rebuild(&mut self, device: &Device, format: TextureFormat) {
        let sampler = device.create_sampler(&SamplerDescriptor {
            label: Some("blit_sampler"),
            mag_filter: wgpu::FilterMode::Nearest,
            min_filter: wgpu::FilterMode::Nearest,
            ..Default::default()
        });

        let bgl = device.create_bind_group_layout(&BindGroupLayoutDescriptor {
            label: Some("blit_bgl"),
            entries: &[
                BindGroupLayoutEntry {
                    binding: 0,
                    visibility: ShaderStages::FRAGMENT,
                    ty: BindingType::Texture {
                        multisampled: false,
                        view_dimension: TextureViewDimension::D2,
                        sample_type: TextureSampleType::Float { filterable: false },
                    },
                    count: None,
                },
                BindGroupLayoutEntry {
                    binding: 1,
                    visibility: ShaderStages::FRAGMENT,
                    ty: BindingType::Sampler(SamplerBindingType::NonFiltering),
                    count: None,
                },
            ],
        });

        let buffers = self.buffers.lock().unwrap();
        let bind_group = device.create_bind_group(&BindGroupDescriptor {
            label: Some("blit_bg"),
            layout: &bgl,
            entries: &[
                BindGroupEntry {
                    binding: 0,
                    resource: BindingResource::TextureView(&buffers.gbuf_albedo.view),
                },
                BindGroupEntry {
                    binding: 1,
                    resource: BindingResource::Sampler(&sampler),
                },
            ],
        });
        drop(buffers);

        let shader = device.create_shader_module(ShaderModuleDescriptor {
            label: Some("blit.wgsl"),
            source: ShaderSource::Wgsl(BLIT_WGSL.into()),
        });
        let layout = device.create_pipeline_layout(&PipelineLayoutDescriptor {
            label: Some("blit_layout"),
            bind_group_layouts: &[&bgl],
            push_constant_ranges: &[],
        });
        let pipeline = device.create_render_pipeline(&RenderPipelineDescriptor {
            label: Some("blit_pipeline"),
            layout: Some(&layout),
            vertex: VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                buffers: &[],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            },
            fragment: Some(FragmentState {
                module: &shader,
                entry_point: Some("fs_main"),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                targets: &[Some(ColorTargetState {
                    format,
                    blend: Some(BlendState::REPLACE),
                    write_mask: ColorWrites::ALL,
                })],
            }),
            primitive: PrimitiveState::default(),
            depth_stencil: None,
            multisample: MultisampleState::default(),
            multiview: None,
            cache: None,
        });

        self.sampler = Some(sampler);
        self.bind_group = Some(bind_group);
        self.pipeline = Some(pipeline);
        self.swapchain_format = Some(format);
    }
}

impl RenderPass for GbufBlitPass {
    fn name(&self) -> &str {
        "GbufBlitPass"
    }

    fn on_attach(&mut self, device: &Device, _queue: &Queue, format: TextureFormat, _s: u32) {
        self.rebuild(device, format);
    }

    fn on_resize(&mut self, device: &Device, _queue: &Queue, _w: u32, _h: u32) {
        // Texture views were recreated by HddaPrimaryPass::on_resize — rebuild bind group.
        if let Some(fmt) = self.swapchain_format {
            self.rebuild(device, fmt);
        }
    }

    fn prepare(&mut self, _d: &Device, _q: &Queue, _p: &FramePacket) {}

    fn execute(
        &mut self,
        _device: &Device,
        _queue: &Queue,
        encoder: &mut CommandEncoder,
        color_view: &TextureView,
        _resolve: Option<&TextureView>,
        _depth: Option<&TextureView>,
        _packet: &FramePacket,
    ) {
        let (Some(pipeline), Some(bg)) = (self.pipeline.as_ref(), self.bind_group.as_ref()) else {
            return;
        };

        let mut rpass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("GbufBlit"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: color_view,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color {
                        r: 0.0,
                        g: 0.05,
                        b: 0.1,
                        a: 1.0,
                    }),
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
        });
        rpass.set_pipeline(pipeline);
        rpass.set_bind_group(0, bg, &[]);
        rpass.draw(0..3, 0..1);
    }
}

// ── main ──────────────────────────────────────────────────────────────────────

fn main() {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();
    App::new(HddaViewer::new())
        .with_title("HDDA Voxel Viewer — Phase 3 (normal colours)")
        .with_size(1280, 720)
        .with_mode(AppMode::Game3D)
        .run();
}
