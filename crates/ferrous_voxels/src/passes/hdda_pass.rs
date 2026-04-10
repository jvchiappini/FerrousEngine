//! `HddaPrimaryPass` — HDDA compute-shader raymarching into the G-Buffer.
//!
//! # Overview
//!
//! This pass dispatches `ceil(width/8) × ceil(height/8)` workgroups of the
//! `hdda_primary.wgsl` compute shader.  The shader marches a ray per pixel
//! through the HashDAG and writes:
//!
//! | G-Buffer texture | Format         | Contents                          |
//! |-----------------|----------------|-----------------------------------|
//! | `gbuf_pos`      | Rgba32Float    | (world_x, world_y, world_z, depth)|
//! | `gbuf_norm`     | Rgba8Snorm     | (nx, ny, nz, hit_flag)            |
//! | `gbuf_albedo`   | Rgba8Unorm     | (r, g, b, material_id / 255)      |
//!
//! For Phase 3 the albedo channel visualises the hit normal as an RGB colour —
//! proper material lookup is added in Phase 4.
//!
//! # Bind-group layout
//!
//! | Group | Binding | Resource                        |
//! |-------|---------|---------------------------------|
//! | 0     | 0       | `dag_nodes` SSBO                |
//! | 0     | 1       | `chunk_roots` SSBO              |
//! | 0     | 2       | `camera` uniform                |
//! | 0     | 3       | `level_offsets` uniform         |
//! | 1     | 0       | `gbuf_pos` storage texture      |
//! | 1     | 1       | `gbuf_norm` storage texture     |
//! | 1     | 2       | `gbuf_albedo` storage texture   |
//!
//! # Integration
//!
//! ```rust,ignore
//! // `VoxelGpuUploadPass` must be registered first — it owns `PersistentBuffers`.
//! let buffers = Arc::clone(upload_pass.shared_buffers());
//! let hdda = HddaPrimaryPass::new(buffers);
//! renderer.add_pass(Box::new(hdda));
//! ```

use std::sync::{Arc, Mutex};

use bytemuck::{Pod, Zeroable};
use wgpu::{
    BindGroup, BindGroupDescriptor, BindGroupEntry, BindGroupLayout, BindGroupLayoutDescriptor,
    BindGroupLayoutEntry, BindingResource, BindingType, BufferBindingType, CommandEncoder,
    ComputePassDescriptor, ComputePipeline, ComputePipelineDescriptor, Device,
    PipelineLayoutDescriptor, Queue, ShaderStages, StorageTextureAccess, TextureFormat,
    TextureView, TextureViewDimension,
};

use ferrous_render_graph::{FramePacket, RenderPass};

use crate::{buffers::PersistentBuffers, world::voxel_edit::VoxelWorld};

// ── GPU-side camera uniform (mirrors CameraUniform in hdda_primary.wgsl) ─────

/// Camera data uploaded every frame via `queue.write_buffer`.
///
/// Layout (128 bytes, `repr(C)`, all naturally aligned):
///
/// | Offset | Size | Field             |
/// |--------|------|-------------------|
/// | 0      | 64   | `inv_view_proj`   |
/// | 64     | 16   | `eye` (xyz + w)   |
/// | 80     | 16   | `resolution` (xyzw, zw unused) |
/// | 96     | 16   | `near_far` (x=near, y=far, zw pad) |
/// | 112    | 16   | padding           |
#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
pub struct GpuCameraUniform {
    /// Clip-to-world matrix (inverse view-projection), column-major.
    pub inv_view_proj: [[f32; 4]; 4],
    /// Eye position in world space.  `w` is unused (padding).
    pub eye: [f32; 4],
    /// `xy` = (width, height) in pixels.  `zw` = unused.
    pub resolution: [f32; 4],
    /// `x` = near, `y` = far.  `zw` = unused.
    pub near_far: [f32; 4],
    /// Padding to reach 128 bytes.
    pub _pad: [f32; 4],
}

const _: () = assert!(
    std::mem::size_of::<GpuCameraUniform>() == 128,
    "GpuCameraUniform must be exactly 128 bytes"
);

// ── GPU-side level-offsets uniform ────────────────────────────────────────────

/// Packed form of [`LevelOffsets`] suitable for `queue.write_buffer`.
///
/// 5 rows × 4 u32 = 80 bytes.  Matches `array<vec4<u32>, 5>` in WGSL.
/// Rows 0-3: level base offsets 0-15 (only 0-12 used).
/// Row 4: [total_nodes, 0, 0, 0].
#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct GpuLevelOffsets {
    /// 5 vec4 rows: rows[L/4][L%4] = first node index for level L.
    rows: [[u32; 4]; 5],
}

const _: () = assert!(
    std::mem::size_of::<GpuLevelOffsets>() == 80,
    "GpuLevelOffsets must be exactly 80 bytes"
);

// ── HddaPrimaryPass ───────────────────────────────────────────────────────────

/// Compute pass: HDDA raymarching → G-Buffer.
///
/// Requires a `PersistentBuffers` shared with `VoxelGpuUploadPass` so that the
/// G-Buffer textures are identical objects that later passes can sample.
#[allow(dead_code)] // world and pending_* fields used starting Phase 4
pub struct HddaPrimaryPass {
    /// Voxel world — used only to read camera data for now.
    /// Phase 4 will add per-frame camera from the scene.
    world: Arc<Mutex<VoxelWorld>>,

    /// Shared GPU buffers.  Created by `VoxelGpuUploadPass::on_attach`.
    buffers: Arc<Mutex<PersistentBuffers>>,

    /// Compiled compute pipeline.  `None` until `on_attach`.
    pipeline: Option<ComputePipeline>,

    /// Bind group layout 0: DAG SSBOs + uniforms.
    bgl_data: Option<BindGroupLayout>,
    /// Bind group layout 1: output storage textures.
    bgl_output: Option<BindGroupLayout>,

    /// Bind group 0.  Rebuilt on reallocation or after `on_resize`.
    bg_data: Option<BindGroup>,
    /// Bind group 1 (output textures).  Rebuilt on `on_resize`.
    bg_output: Option<BindGroup>,

    /// Current render resolution.
    width: u32,
    height: u32,

    /// Pending camera data to upload in `prepare`.
    pending_camera: Option<GpuCameraUniform>,
    /// Pending level offsets to upload in `prepare`.
    pending_offsets: Option<GpuLevelOffsets>,
}

impl HddaPrimaryPass {
    /// Create the pass.  GPU resources are allocated in `on_attach`.
    ///
    /// `buffers` must be the same `Arc<Mutex<PersistentBuffers>>` owned by
    /// the `VoxelGpuUploadPass` that runs before this pass each frame.
    pub fn new(world: Arc<Mutex<VoxelWorld>>, buffers: Arc<Mutex<PersistentBuffers>>) -> Self {
        Self {
            world,
            buffers,
            pipeline: None,
            bgl_data: None,
            bgl_output: None,
            bg_data: None,
            bg_output: None,
            width: 1,
            height: 1,
            pending_camera: None,
            pending_offsets: None,
        }
    }

    // ── Private helpers ───────────────────────────────────────────────────────

    /// Build the `BindGroupLayout` for group 0 (DAG SSBOs + uniforms).
    fn make_bgl_data(device: &Device) -> BindGroupLayout {
        device.create_bind_group_layout(&BindGroupLayoutDescriptor {
            label: Some("hdda::bgl_data"),
            entries: &[
                // binding 0: dag_nodes SSBO
                BindGroupLayoutEntry {
                    binding: 0,
                    visibility: ShaderStages::COMPUTE,
                    ty: BindingType::Buffer {
                        ty: BufferBindingType::Storage { read_only: true },
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                // binding 1: chunk_roots SSBO
                BindGroupLayoutEntry {
                    binding: 1,
                    visibility: ShaderStages::COMPUTE,
                    ty: BindingType::Buffer {
                        ty: BufferBindingType::Storage { read_only: true },
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                // binding 2: camera uniform
                BindGroupLayoutEntry {
                    binding: 2,
                    visibility: ShaderStages::COMPUTE,
                    ty: BindingType::Buffer {
                        ty: BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: wgpu::BufferSize::new(
                            PersistentBuffers::CAMERA_UNIFORM_SIZE,
                        ),
                    },
                    count: None,
                },
                // binding 3: level_offsets uniform
                BindGroupLayoutEntry {
                    binding: 3,
                    visibility: ShaderStages::COMPUTE,
                    ty: BindingType::Buffer {
                        ty: BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: wgpu::BufferSize::new(
                            PersistentBuffers::LEVEL_OFFSETS_SIZE,
                        ),
                    },
                    count: None,
                },
            ],
        })
    }

    /// Build the `BindGroupLayout` for group 1 (output storage textures).
    fn make_bgl_output(device: &Device) -> BindGroupLayout {
        device.create_bind_group_layout(&BindGroupLayoutDescriptor {
            label: Some("hdda::bgl_output"),
            entries: &[
                // binding 0: gbuf_pos  rgba32float write
                BindGroupLayoutEntry {
                    binding: 0,
                    visibility: ShaderStages::COMPUTE,
                    ty: BindingType::StorageTexture {
                        access: StorageTextureAccess::WriteOnly,
                        format: PersistentBuffers::GBUF_POS_FORMAT,
                        view_dimension: TextureViewDimension::D2,
                    },
                    count: None,
                },
                // binding 1: gbuf_norm  rgba8snorm write
                BindGroupLayoutEntry {
                    binding: 1,
                    visibility: ShaderStages::COMPUTE,
                    ty: BindingType::StorageTexture {
                        access: StorageTextureAccess::WriteOnly,
                        format: PersistentBuffers::GBUF_NORM_FORMAT,
                        view_dimension: TextureViewDimension::D2,
                    },
                    count: None,
                },
                // binding 2: gbuf_albedo  rgba8unorm write
                BindGroupLayoutEntry {
                    binding: 2,
                    visibility: ShaderStages::COMPUTE,
                    ty: BindingType::StorageTexture {
                        access: StorageTextureAccess::WriteOnly,
                        format: PersistentBuffers::GBUF_ALBEDO_FORMAT,
                        view_dimension: TextureViewDimension::D2,
                    },
                    count: None,
                },
            ],
        })
    }

    /// Rebuild bind group 0 from current buffer handles.
    fn rebuild_bg_data(&mut self, device: &Device) {
        let bgl = match &self.bgl_data {
            Some(b) => b,
            None => return,
        };
        let buffers = self
            .buffers
            .lock()
            .expect("PersistentBuffers mutex poisoned");
        self.bg_data = Some(device.create_bind_group(&BindGroupDescriptor {
            label: Some("hdda::bg_data"),
            layout: bgl,
            entries: &[
                BindGroupEntry {
                    binding: 0,
                    resource: buffers.node_buf.as_entire_binding(),
                },
                BindGroupEntry {
                    binding: 1,
                    resource: buffers.root_buf.as_entire_binding(),
                },
                BindGroupEntry {
                    binding: 2,
                    resource: buffers.camera_buf.as_entire_binding(),
                },
                BindGroupEntry {
                    binding: 3,
                    resource: buffers.level_offsets_buf.as_entire_binding(),
                },
            ],
        }));
    }

    /// Rebuild bind group 1 from current G-Buffer texture views.
    fn rebuild_bg_output(&mut self, device: &Device) {
        let bgl = match &self.bgl_output {
            Some(b) => b,
            None => return,
        };
        let buffers = self
            .buffers
            .lock()
            .expect("PersistentBuffers mutex poisoned");
        self.bg_output = Some(device.create_bind_group(&BindGroupDescriptor {
            label: Some("hdda::bg_output"),
            layout: bgl,
            entries: &[
                BindGroupEntry {
                    binding: 0,
                    resource: BindingResource::TextureView(&buffers.gbuf_pos.view),
                },
                BindGroupEntry {
                    binding: 1,
                    resource: BindingResource::TextureView(&buffers.gbuf_norm.view),
                },
                BindGroupEntry {
                    binding: 2,
                    resource: BindingResource::TextureView(&buffers.gbuf_albedo.view),
                },
            ],
        }));
    }
}

// ── RenderPass impl ───────────────────────────────────────────────────────────

impl RenderPass for HddaPrimaryPass {
    fn name(&self) -> &str {
        "HddaPrimaryPass"
    }

    fn on_attach(
        &mut self,
        device: &Device,
        _queue: &Queue,
        _format: TextureFormat,
        _sample_count: u32,
    ) {
        // ── Build bind group layouts ──────────────────────────────────────────
        let bgl_data = Self::make_bgl_data(device);
        let bgl_output = Self::make_bgl_output(device);

        // ── Pipeline layout ───────────────────────────────────────────────────
        let pipeline_layout = device.create_pipeline_layout(&PipelineLayoutDescriptor {
            label: Some("hdda::pipeline_layout"),
            bind_group_layouts: &[&bgl_data, &bgl_output],
            push_constant_ranges: &[],
        });

        // ── Compile WGSL shader ───────────────────────────────────────────────
        let shader_src = include_str!("../../../../assets/shaders/voxels/hdda_primary.wgsl");
        let shader_module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("hdda_primary.wgsl"),
            source: wgpu::ShaderSource::Wgsl(shader_src.into()),
        });

        let pipeline = device.create_compute_pipeline(&ComputePipelineDescriptor {
            label: Some("hdda::pipeline"),
            layout: Some(&pipeline_layout),
            module: &shader_module,
            entry_point: Some("main"),
            compilation_options: wgpu::PipelineCompilationOptions::default(),
            cache: None,
        });

        self.bgl_data = Some(bgl_data);
        self.bgl_output = Some(bgl_output);
        self.pipeline = Some(pipeline);

        // ── Build initial bind groups ─────────────────────────────────────────
        self.rebuild_bg_data(device);
        self.rebuild_bg_output(device);

        log::info!("HddaPrimaryPass: compute pipeline compiled");
    }

    fn on_resize(&mut self, device: &Device, _queue: &Queue, width: u32, height: u32) {
        self.width = width;
        self.height = height;

        // Resize G-Buffer textures inside shared PersistentBuffers.
        {
            let mut buffers = self
                .buffers
                .lock()
                .expect("PersistentBuffers mutex poisoned");
            buffers.resize(device, width, height);
        }

        // Texture views are recreated inside resize(); rebuild output bind group.
        self.rebuild_bg_output(device);

        log::debug!("HddaPrimaryPass: resized to {width}×{height}");
    }

    fn prepare(&mut self, device: &Device, queue: &Queue, packet: &FramePacket) {
        // ── Rebuild bind group to catch buffer reallocations ───────────────
        self.rebuild_bg_data(device);

        // ── Camera uniform ────────────────────────────────────────────────────
        let inv_view_proj = packet.camera.view_proj.inverse();
        let cam = GpuCameraUniform {
            inv_view_proj: inv_view_proj.to_cols_array_2d(),
            eye: [
                packet.camera.eye.x,
                packet.camera.eye.y,
                packet.camera.eye.z,
                0.0,
            ],
            resolution: [self.width as f32, self.height as f32, 0.0, 0.0],
            // TODO: derive near/far from camera if needed.
            near_far: [0.1, 10000.0, 0.0, 0.0],
            _pad: [0.0; 4],
        };

        let buffers = self
            .buffers
            .lock()
            .expect("PersistentBuffers mutex poisoned");
        queue.write_buffer(&buffers.camera_buf, 0, bytemuck::bytes_of(&cam));
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
        let pipeline = match &self.pipeline {
            Some(p) => p,
            None => return,
        };
        let bg_data = match &self.bg_data {
            Some(b) => b,
            None => return,
        };
        let bg_output = match &self.bg_output {
            Some(b) => b,
            None => return,
        };

        let wg_x = (self.width + 7) / 8;
        let wg_y = (self.height + 7) / 8;

        let mut cpass = encoder.begin_compute_pass(&ComputePassDescriptor {
            label: Some("HddaPrimaryPass"),
            timestamp_writes: None,
        });
        cpass.set_pipeline(pipeline);
        cpass.set_bind_group(0, bg_data, &[]);
        cpass.set_bind_group(1, bg_output, &[]);
        cpass.dispatch_workgroups(wg_x, wg_y, 1);
    }
}
