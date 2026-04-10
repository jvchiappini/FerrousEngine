//! `VoxelGpuUploadPass` — synchronise dirty `HashDAG` nodes to GPU storage buffers.
//!
//! # Integration
//!
//! ```rust,ignore
//! // In your app setup:
//! let world = Arc::new(Mutex::new(VoxelWorld::new()));
//! let pass = VoxelGpuUploadPass::new(Arc::clone(&world));
//! renderer.add_pass(Box::new(pass));
//! ```
//!
//! # Frame flow
//!
//! 1. **`prepare`** — calls `DagGpuSync::prepare` to convert dirty nodes into bytes,
//!    then writes them into the staging buffers via `queue.write_buffer`.
//! 2. **`execute`** — records `encoder.copy_buffer_to_buffer` commands to transfer
//!    from staging → `node_buf` / `root_buf` (device-local STORAGE buffers).
//!
//! The two-phase split is required because `write_buffer` cannot be called while a
//! `wgpu::RenderPass` is open.

use std::sync::{Arc, Mutex};

use wgpu::{CommandEncoder, Device, Queue, TextureView};

use ferrous_render_graph::{FramePacket, RenderPass};

use crate::{
    buffers::PersistentBuffers,
    dag::gpu_upload::{DagGpuSync, DagUploadBatch},
    world::voxel_edit::VoxelWorld,
};

// ── VoxelGpuUploadPass ────────────────────────────────────────────────────────

/// Render pass that keeps the GPU storage buffers in sync with the CPU `HashDAG`.
///
/// Holds:
/// - A shared reference to the `VoxelWorld` (locks only during `prepare`).
/// - The `PersistentBuffers` that own `node_buf` / `root_buf`.
/// - An optional pending `DagUploadBatch` produced in `prepare` and consumed in
///   `execute`.
pub struct VoxelGpuUploadPass {
    /// Shared voxel world.  The pass takes a `Mutex` lock only during `prepare`.
    world: Arc<Mutex<VoxelWorld>>,

    /// Persistent GPU buffers.  `None` until `on_attach` allocates them.
    /// Wrapped in `Arc<Mutex<>>` so `HddaPrimaryPass` can share the same instance.
    buffers: Option<Arc<Mutex<PersistentBuffers>>>,

    /// Bytes staged during `prepare` and consumed during `execute`.
    pending: Option<DagUploadBatch>,
}

impl VoxelGpuUploadPass {
    /// Create the pass.  GPU buffers are allocated in `on_attach`.
    pub fn new(world: Arc<Mutex<VoxelWorld>>) -> Self {
        Self {
            world,
            buffers: None,
            pending: None,
        }
    }

    /// Returns a cloned `Arc` to the shared GPU buffers.
    ///
    /// Returns `None` before `on_attach` has been called.
    /// Pass the returned `Arc` to `HddaPrimaryPass::new` to share the same
    /// `node_buf`, `root_buf`, and G-Buffer textures.
    ///
    /// ```rust,ignore
    /// let upload = VoxelGpuUploadPass::new(Arc::clone(&world));
    /// // register upload with the renderer first, then:
    /// let buffers = upload.shared_buffers().expect("on_attach not called yet");
    /// let hdda = HddaPrimaryPass::new(Arc::clone(&world), buffers);
    /// ```
    pub fn shared_buffers(&self) -> Option<Arc<Mutex<PersistentBuffers>>> {
        self.buffers.as_ref().map(Arc::clone)
    }
}

// ── RenderPass impl ───────────────────────────────────────────────────────────

impl RenderPass for VoxelGpuUploadPass {
    fn name(&self) -> &str {
        "VoxelGpuUploadPass"
    }

    fn on_attach(
        &mut self,
        device: &Device,
        _queue: &Queue,
        _format: wgpu::TextureFormat,
        _sample_count: u32,
    ) {
        // Start with 1×1 textures; on_resize will immediately resize to the
        // actual surface dimensions before the first frame.
        self.buffers = Some(Arc::new(Mutex::new(PersistentBuffers::new(device, 1, 1))));
        log::info!("VoxelGpuUploadPass: GPU buffers allocated");
    }

    fn on_resize(&mut self, device: &Device, _queue: &Queue, width: u32, height: u32) {
        if let Some(b) = self.buffers.as_ref() {
            b.lock().expect("PersistentBuffers mutex poisoned").resize(device, width, height);
        }
    }

    fn prepare(&mut self, device: &Device, queue: &Queue, _packet: &FramePacket) {
        let buffers_arc = match self.buffers.as_ref() {
            Some(b) => Arc::clone(b),
            None => {
                log::warn!("VoxelGpuUploadPass::prepare called before on_attach");
                return;
            }
        };
        let mut buffers = buffers_arc.lock().expect("PersistentBuffers mutex poisoned");

        // Lock the world only long enough to snapshot the dirty state.
        let batch = {
            let mut world = self.world.lock().expect("VoxelWorld mutex poisoned");
            let batch_opt = DagGpuSync::prepare(&world.dag, &*buffers);
            if batch_opt.is_some() {
                world.dag.take_dirty_nodes();
                world.dag.take_dirty_chunks();
                world.chunks.take_dirty();
            }
            batch_opt
        };

        let batch = match batch {
            Some(b) => b,
            None => return, // nothing dirty
        };

        // ── Grow buffers if needed ────────────────────────────────────────────
        let total_nodes = batch.offsets.total_nodes as u64;
        let root_count = (batch.root_bytes.len() / std::mem::size_of::<crate::dag::gpu_types::GpuChunkRoot>()) as u64;

        if batch.needs_node_realloc {
            let realloced = buffers.ensure_node_capacity(device, total_nodes);
            if realloced {
                log::debug!(
                    "VoxelGpuUploadPass: node_buf grown to {} nodes ({} KB)",
                    buffers.node_capacity,
                    buffers.node_capacity * 40 / 1024
                );
            }
        }

        if batch.needs_root_realloc {
            buffers.ensure_root_capacity(device, root_count);
        }

        // ── Write staging buffers via queue.write_buffer ──────────────────────
        if !batch.node_bytes.is_empty() {
            queue.write_buffer(&buffers.staging_node, 0, &batch.node_bytes);
        }
        if !batch.root_bytes.is_empty() {
            queue.write_buffer(&buffers.staging_root, 0, &batch.root_bytes);
        }

        // â”€â”€ Write level offsets uniform â”€â”€â”€â”€â”€
        let mut gpu_offsets = [[0u32; 4]; 5];
        for level in 0..13 {
            gpu_offsets[level / 4][level % 4] = batch.offsets.base[level];      
        }
        gpu_offsets[4][0] = batch.offsets.total_nodes;
        gpu_offsets[4][1] = (batch.root_bytes.len() / std::mem::size_of::<crate::dag::gpu_types::GpuChunkRoot>()) as u32;
        queue.write_buffer(&buffers.level_offsets_buf, 0, bytemuck::cast_slice(&gpu_offsets));        self.pending = Some(batch);
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
        let batch = match self.pending.take() {
            Some(b) => b,
            None => return,
        };

        let buffers_arc = match self.buffers.as_ref() {
            Some(b) => Arc::clone(b),
            None => return,
        };
        let buffers = buffers_arc.lock().expect("PersistentBuffers mutex poisoned");

        // ── Copy staging → device-local ───────────────────────────────────────
        let node_bytes = batch.node_bytes.len() as u64;
        if node_bytes > 0 {
            encoder.copy_buffer_to_buffer(
                &buffers.staging_node,
                0,
                &buffers.node_buf,
                0,
                node_bytes,
            );
        }

        let root_bytes = batch.root_bytes.len() as u64;
        if root_bytes > 0 {
            encoder.copy_buffer_to_buffer(
                &buffers.staging_root,
                0,
                &buffers.root_buf,
                0,
                root_bytes,
            );
        }
    }
}
