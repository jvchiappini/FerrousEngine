//! Persistent GPU buffers for the voxel renderer.
//!
//! `PersistentBuffers` owns the long-lived `wgpu::Buffer` objects that the
//! compute passes bind every frame.  Buffers grow automatically when the DAG
//! expands — old buffers are dropped and new, larger ones are allocated.
//!
//! # Buffer slots
//!
//! | Binding | Name             | Usage                                   |
//! |---------|------------------|-----------------------------------------|
//! | 0       | `node_buf`       | All `GpuDagNode`s (per-level packed)    |
//! | 1       | `root_buf`       | `GpuChunkRoot` table (live chunks)      |
//! | 2       | `occupancy_buf`  | Per-level occupancy bitmasks (future)   |
//! | staging | `staging_buf`    | CPU-writable staging for `node_buf`     |
//!
//! The staging buffer is `MAP_WRITE | COPY_SRC`.  `node_buf` is
//! `STORAGE | COPY_DST`.  `root_buf` is `STORAGE | COPY_DST`.
//! Copies from staging → device-local happen inside `VoxelGpuUploadPass::execute`.

use std::sync::Arc;

use wgpu::{Buffer, BufferDescriptor, BufferUsages, Device, Texture, TextureView};

use crate::dag::gpu_types::{GpuChunkRoot, GpuDagNode};

/// Initial capacity in nodes for the storage buffer.  Doubles on overflow.
const INITIAL_NODE_CAPACITY: u64 = 4096;
/// Initial capacity in chunk-root entries.
const INITIAL_ROOT_CAPACITY: u64 = 256;

// ── G-Buffer texture helpers ──────────────────────────────────────────────────

/// One G-Buffer render target managed by `PersistentBuffers`.
///
/// Used as a `STORAGE_BINDING` output from compute shaders and a
/// `TEXTURE_BINDING` input to subsequent passes.
pub struct GBufferTexture {
    /// Underlying texture (resized on `on_resize`).
    pub texture: Texture,
    /// View used by shaders.
    pub view: TextureView,
    /// Current width in pixels.
    pub width: u32,
    /// Current height in pixels.
    pub height: u32,
}

impl GBufferTexture {
    fn new(
        device: &Device,
        width: u32,
        height: u32,
        format: wgpu::TextureFormat,
        label: &str,
    ) -> Self {
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some(label),
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format,
            usage: wgpu::TextureUsages::STORAGE_BINDING
                | wgpu::TextureUsages::TEXTURE_BINDING
                | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        Self {
            texture,
            view,
            width,
            height,
        }
    }

    /// Recreate the texture at the new resolution (drops the old one).
    pub fn resize(
        &mut self,
        device: &Device,
        width: u32,
        height: u32,
        format: wgpu::TextureFormat,
        label: &str,
    ) {
        if self.width == width && self.height == height {
            return;
        }
        *self = Self::new(device, width, height, format, label);
    }
}

// ── PersistentBuffers ─────────────────────────────────────────────────────────

/// Owns the GPU-side buffers that back the voxel HashDAG.
///
/// Created once during `VoxelGpuUploadPass::on_attach` and kept alive for the
/// entire lifetime of the renderer.  All compute passes receive a shared
/// reference via `Arc`.
pub struct PersistentBuffers {
    // ── Staging (CPU → GPU) ──────────────────────────────────────────────────
    /// CPU-writable staging buffer for node data.
    pub staging_node: Buffer,
    /// Staging buffer for the root-chunk table.
    pub staging_root: Buffer,

    // ── Device-local DAG SSBOs (GPU read-only in shaders) ────────────────────
    /// Storage buffer read by all voxel compute shaders: `array<GpuDagNode>`.
    ///
    /// Layout: `[level-0 nodes | level-1 nodes | … | level-12 nodes]`.
    /// The shader uses `level_offsets.base[L]` to index into the correct slice.
    pub node_buf: Arc<Buffer>,

    /// Flat array of live `GpuChunkRoot` entries, sorted by (cx, cy, cz).
    pub root_buf: Arc<Buffer>,

    // ── G-Buffer (written by HDDA pass, read by lighting passes) ─────────────
    /// `Rgba32Float` — (world_x, world_y, world_z, depth).
    pub gbuf_pos: GBufferTexture,
    /// `Rgba8Snorm` — (nx, ny, nz, hit_flag: 1.0 = hit, 0.0 = sky).
    pub gbuf_norm: GBufferTexture,
    /// `Rgba8Unorm` — (r, g, b, material_id_normalised).
    pub gbuf_albedo: GBufferTexture,

    // ── Uniform buffers (small, written every frame in prepare()) ────────────
    /// Camera data: view_proj inverse, eye position, resolution. 128 bytes.
    pub camera_buf: Buffer,
    /// Per-level base indices into `node_buf`.  `[u32; 13]` + `[u32; 3]` pad = 64 bytes.
    pub level_offsets_buf: Buffer,

    // ── Capacities ───────────────────────────────────────────────────────────
    /// Current capacity of `node_buf` / `staging_node` in `GpuDagNode` units.
    pub node_capacity: u64,
    /// Current capacity of `root_buf` / `staging_root` in `GpuChunkRoot` units.
    pub root_capacity: u64,
}

impl PersistentBuffers {
    /// G-Buffer position texture format: `(world_x, world_y, world_z, depth)`.
    pub const GBUF_POS_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba32Float;
    /// G-Buffer normal texture format: `(nx, ny, nz, hit)` snorm.
    pub const GBUF_NORM_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8Snorm;
    /// G-Buffer albedo texture format.
    pub const GBUF_ALBEDO_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8Unorm;
    /// Camera uniform size in bytes (see `GpuCameraUniform` in `hdda_pass.rs`).
    pub const CAMERA_UNIFORM_SIZE: u64 = 128;
    /// Level-offsets uniform: 5 × vec4<u32> = 80 bytes (16-byte aligned for WGSL uniform).
    pub const LEVEL_OFFSETS_SIZE: u64 = 80;

    /// Allocate all GPU buffers with the default initial capacities.
    ///
    /// G-Buffer textures are created at `(width, height)`.  Call
    /// [`PersistentBuffers::resize`] when the window resizes.
    pub fn new(device: &Device, width: u32, height: u32) -> Self {
        let node_capacity = INITIAL_NODE_CAPACITY;
        let root_capacity = INITIAL_ROOT_CAPACITY;

        let node_bytes = node_capacity * std::mem::size_of::<GpuDagNode>() as u64;
        let root_bytes = root_capacity * std::mem::size_of::<GpuChunkRoot>() as u64;

        let staging_node = device.create_buffer(&BufferDescriptor {
            label: Some("voxels::staging_node"),
            size: node_bytes,
            usage: BufferUsages::COPY_SRC | BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let staging_root = device.create_buffer(&BufferDescriptor {
            label: Some("voxels::staging_root"),
            size: root_bytes,
            usage: BufferUsages::COPY_SRC | BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let node_buf = Arc::new(device.create_buffer(&BufferDescriptor {
            label: Some("voxels::node_buf"),
            size: node_bytes,
            usage: BufferUsages::STORAGE | BufferUsages::COPY_DST,
            mapped_at_creation: false,
        }));
        let root_buf = Arc::new(device.create_buffer(&BufferDescriptor {
            label: Some("voxels::root_buf"),
            size: root_bytes,
            usage: BufferUsages::STORAGE | BufferUsages::COPY_DST,
            mapped_at_creation: false,
        }));

        let gbuf_pos = GBufferTexture::new(
            device,
            width,
            height,
            Self::GBUF_POS_FORMAT,
            "voxels::gbuf_pos",
        );
        let gbuf_norm = GBufferTexture::new(
            device,
            width,
            height,
            Self::GBUF_NORM_FORMAT,
            "voxels::gbuf_norm",
        );
        let gbuf_albedo = GBufferTexture::new(
            device,
            width,
            height,
            Self::GBUF_ALBEDO_FORMAT,
            "voxels::gbuf_albedo",
        );

        let camera_buf = device.create_buffer(&BufferDescriptor {
            label: Some("voxels::camera_uniform"),
            size: Self::CAMERA_UNIFORM_SIZE,
            usage: BufferUsages::UNIFORM | BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let level_offsets_buf = device.create_buffer(&BufferDescriptor {
            label: Some("voxels::level_offsets_uniform"),
            size: Self::LEVEL_OFFSETS_SIZE,
            usage: BufferUsages::UNIFORM | BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        Self {
            staging_node,
            staging_root,
            node_buf,
            root_buf,
            gbuf_pos,
            gbuf_norm,
            gbuf_albedo,
            camera_buf,
            level_offsets_buf,
            node_capacity,
            root_capacity,
        }
    }

    /// Recreate all resolution-dependent resources at the new `(width, height)`.
    ///
    /// Call from `RenderPass::on_resize`.  After this, any bind groups that
    /// reference G-Buffer views must be rebuilt.
    pub fn resize(&mut self, device: &Device, width: u32, height: u32) {
        self.gbuf_pos.resize(
            device,
            width,
            height,
            Self::GBUF_POS_FORMAT,
            "voxels::gbuf_pos",
        );
        self.gbuf_norm.resize(
            device,
            width,
            height,
            Self::GBUF_NORM_FORMAT,
            "voxels::gbuf_norm",
        );
        self.gbuf_albedo.resize(
            device,
            width,
            height,
            Self::GBUF_ALBEDO_FORMAT,
            "voxels::gbuf_albedo",
        );
    }

    /// Ensure the node buffers can hold at least `required` nodes.
    ///
    /// If the current capacity is insufficient, drops the old buffers and
    /// allocates new ones with the next power-of-two size ≥ `required`.
    /// Returns `true` if reallocation occurred (callers must rebuild bind
    /// groups).
    pub fn ensure_node_capacity(&mut self, device: &Device, required: u64) -> bool {
        if required <= self.node_capacity {
            return false;
        }
        let mut new_cap = self.node_capacity;
        while new_cap < required {
            new_cap *= 2;
        }
        let node_bytes = new_cap * std::mem::size_of::<GpuDagNode>() as u64;

        self.staging_node = device.create_buffer(&BufferDescriptor {
            label: Some("voxels::staging_node"),
            size: node_bytes,
            usage: BufferUsages::COPY_SRC | BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        self.node_buf = Arc::new(device.create_buffer(&BufferDescriptor {
            label: Some("voxels::node_buf"),
            size: node_bytes,
            usage: BufferUsages::STORAGE | BufferUsages::COPY_DST,
            mapped_at_creation: false,
        }));
        self.node_capacity = new_cap;
        true
    }

    /// Ensure the root buffers can hold at least `required` roots.
    ///
    /// Analogous to [`ensure_node_capacity`].
    pub fn ensure_root_capacity(&mut self, device: &Device, required: u64) -> bool {
        if required <= self.root_capacity {
            return false;
        }
        let mut new_cap = self.root_capacity;
        while new_cap < required {
            new_cap *= 2;
        }
        let root_bytes = new_cap * std::mem::size_of::<GpuChunkRoot>() as u64;

        self.staging_root = device.create_buffer(&BufferDescriptor {
            label: Some("voxels::staging_root"),
            size: root_bytes,
            usage: BufferUsages::COPY_SRC | BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        self.root_buf = Arc::new(device.create_buffer(&BufferDescriptor {
            label: Some("voxels::root_buf"),
            size: root_bytes,
            usage: BufferUsages::STORAGE | BufferUsages::COPY_DST,
            mapped_at_creation: false,
        }));
        self.root_capacity = new_cap;
        true
    }

    /// Total GPU memory in use (staging + device-local, approximate).
    pub fn total_gpu_bytes(&self) -> u64 {
        let node_bytes = self.node_capacity * std::mem::size_of::<GpuDagNode>() as u64;
        let root_bytes = self.root_capacity * std::mem::size_of::<GpuChunkRoot>() as u64;
        // × 2 because each lives in both staging and device-local buffers
        (node_bytes + root_bytes) * 2
    }
}
