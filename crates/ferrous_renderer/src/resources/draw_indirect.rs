#![cfg(feature = "gpu-driven")]

/// GPU-side indirect draw command buffer for GPU-driven rendering.
///
/// Stores a contiguous array of `wgpu::util::DrawIndexedIndirect` (20 bytes each).
/// The buffer is created with `INDIRECT | STORAGE | COPY_DST` usage so that:
///
/// * The compute cull shader can write to it as a storage buffer (RW).
/// * The render pass can use it as an indirect draw source (`draw_indexed_indirect`).
/// * The CPU can reset command counts each frame via `write_buffer`.
///
/// ## Frame lifecycle
///
/// ```text
/// // 1. CPU writes per-instance data (matrices + AABBs) to InstanceCullBuffer.
/// // 2. CullPass compute shader reads instances, tests frustum, writes to DrawIndirectBuffer.
/// // 3. WorldPass calls draw_indexed_indirect for each mesh batch.
/// ```
use std::sync::Arc;

use wgpu::util::DeviceExt;

/// One `DrawIndexedIndirect` entry, matching the GPU struct layout.
///
/// This mirrors `wgpu::util::DrawIndexedIndirect` exactly (20 bytes, no padding).
#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
pub struct GpuDrawIndexedIndirect {
    /// Total index count for this draw (from the original mesh).
    pub index_count: u32,
    /// Number of visible instances — written by the cull shader.
    pub instance_count: u32,
    /// Byte offset into the index buffer where this mesh starts.
    pub first_index: u32,
    /// Added to each index before reading from the vertex buffer.
    pub base_vertex: i32,
    /// First instance slot in the instance storage buffer for this mesh batch.
    pub first_instance: u32,
}

/// Minimum number of command slots to allocate.
const MIN_CAPACITY: usize = 64;
/// Bytes per draw command (5 × u32 = 20 bytes).
const CMD_BYTES: u64 = 20;

/// Buffer holding [`GpuDrawIndexedIndirect`] commands, written by the GPU cull compute
/// shader and consumed by `draw_indexed_indirect` in the render pass.
pub struct DrawIndirectBuffer {
    /// Raw GPU buffer (`INDIRECT | STORAGE | COPY_DST`).
    pub buffer: Arc<wgpu::Buffer>,
    /// Bind group exposing the buffer as a read-write storage binding (for cull shader).
    pub rw_bind_group: Arc<wgpu::BindGroup>,
    /// Current allocated capacity (number of draw command slots).
    pub capacity: usize,
}

impl DrawIndirectBuffer {
    /// Creates a `DrawIndirectBuffer` with at least `initial_capacity` slots.
    ///
    /// `rw_layout` must be the `cull_indirect` bind-group layout from
    /// [`crate::pipeline::PipelineLayouts`], which exposes binding 0 as a
    /// read-write storage buffer.
    pub fn new(
        device: &wgpu::Device,
        rw_layout: &wgpu::BindGroupLayout,
        initial_capacity: usize,
    ) -> Self {
        let capacity = initial_capacity.max(MIN_CAPACITY);
        let buffer = Arc::new(Self::create_buffer(device, capacity));
        let rw_bind_group = Arc::new(Self::create_bind_group(device, rw_layout, &buffer));
        Self {
            buffer,
            rw_bind_group,
            capacity,
        }
    }

    /// Ensures the buffer can hold at least `needed` command slots.
    ///
    /// Reallocates both the buffer and the bind group if the capacity is exceeded.
    pub fn reserve(
        &mut self,
        device: &wgpu::Device,
        rw_layout: &wgpu::BindGroupLayout,
        needed: usize,
    ) {
        if needed <= self.capacity {
            return;
        }
        let mut new_cap = self.capacity;
        while new_cap < needed {
            new_cap *= 2;
        }
        let buf = Arc::new(Self::create_buffer(device, new_cap));
        self.rw_bind_group = Arc::new(Self::create_bind_group(device, rw_layout, &buf));
        self.buffer = buf;
        self.capacity = new_cap;
    }

    /// Writes an initial set of commands to the buffer (CPU-side template).
    ///
    /// The cull shader later overwrites `instance_count` for each visible batch.
    /// This method is called each frame before the cull dispatch to reset counts to zero.
    pub fn write_templates(&self, queue: &wgpu::Queue, commands: &[GpuDrawIndexedIndirect]) {
        if commands.is_empty() {
            return;
        }
        debug_assert!(
            commands.len() <= self.capacity,
            "DrawIndirectBuffer: template count exceeds capacity"
        );
        queue.write_buffer(&self.buffer, 0, bytemuck::cast_slice(commands));
    }

    /// Returns the byte offset into the buffer for command at `index`.
    #[inline]
    pub fn byte_offset(index: usize) -> u64 {
        index as u64 * CMD_BYTES
    }

    // ── Private helpers ──────────────────────────────────────────────────────

    fn create_buffer(device: &wgpu::Device, capacity: usize) -> wgpu::Buffer {
        let data = vec![0u8; capacity * CMD_BYTES as usize];
        device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("DrawIndirectBuffer"),
            contents: &data,
            usage: wgpu::BufferUsages::INDIRECT
                | wgpu::BufferUsages::STORAGE
                | wgpu::BufferUsages::COPY_DST,
        })
    }

    fn create_bind_group(
        device: &wgpu::Device,
        layout: &wgpu::BindGroupLayout,
        buffer: &wgpu::Buffer,
    ) -> wgpu::BindGroup {
        device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("DrawIndirectBuffer RW BindGroup"),
            layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: buffer.as_entire_binding(),
            }],
        })
    }
}

// ── Per-instance data for the cull shader ─────────────────────────────────────

/// Per-instance culling data uploaded to the GPU each frame.
///
/// The cull compute shader reads this to decide whether each instance is visible.
/// Layout matches the WGSL struct in `cull.wgsl`.
#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
pub struct InstanceCullData {
    /// World-space model matrix (columns 0-3).
    pub model: [[f32; 4]; 4],
    /// AABB center in local space (xyz) + command index (w).
    /// The command index is which slot in the `DrawIndirectBuffer` this
    /// instance belongs to — the cull shader uses it to atomically increment
    /// `instance_count` in the right draw command.
    pub aabb_center_cmd: [f32; 4],
    /// AABB half-extents in local space (xyz) + padding (w).
    pub aabb_half_extents_pad: [f32; 4],
}

impl InstanceCullData {
    /// Creates an `InstanceCullData` entry.
    ///
    /// `cmd_index` is the index into the `DrawIndirectBuffer` for this
    /// instance's mesh batch.
    pub fn new(
        model: glam::Mat4,
        aabb_center: glam::Vec3,
        aabb_half_extents: glam::Vec3,
        cmd_index: u32,
    ) -> Self {
        Self {
            model: model.to_cols_array_2d(),
            aabb_center_cmd: [
                aabb_center.x,
                aabb_center.y,
                aabb_center.z,
                f32::from_bits(cmd_index),
            ],
            aabb_half_extents_pad: [
                aabb_half_extents.x,
                aabb_half_extents.y,
                aabb_half_extents.z,
                0.0,
            ],
        }
    }
}

/// GPU buffer holding an array of [`InstanceCullData`] (read by the cull shader).
///
/// Created with `STORAGE | COPY_DST`.
pub struct InstanceCullBuffer {
    pub buffer: Arc<wgpu::Buffer>,
    /// Bind group exposing the buffer as a read-only storage binding.
    pub ro_bind_group: Arc<wgpu::BindGroup>,
    capacity: usize,
}

const CULL_ENTRY_BYTES: u64 = std::mem::size_of::<InstanceCullData>() as u64;

impl InstanceCullBuffer {
    pub fn new(
        device: &wgpu::Device,
        ro_layout: &wgpu::BindGroupLayout,
        initial_capacity: usize,
    ) -> Self {
        let capacity = initial_capacity.max(64);
        let buffer = Arc::new(Self::alloc(device, capacity));
        let ro_bind_group = Arc::new(Self::make_bg(device, ro_layout, &buffer));
        Self {
            buffer,
            ro_bind_group,
            capacity,
        }
    }

    pub fn reserve(
        &mut self,
        device: &wgpu::Device,
        ro_layout: &wgpu::BindGroupLayout,
        needed: usize,
    ) {
        if needed <= self.capacity {
            return;
        }
        let mut cap = self.capacity;
        while cap < needed {
            cap *= 2;
        }
        let buf = Arc::new(Self::alloc(device, cap));
        self.ro_bind_group = Arc::new(Self::make_bg(device, ro_layout, &buf));
        self.buffer = buf;
        self.capacity = cap;
    }

    pub fn write(&self, queue: &wgpu::Queue, data: &[InstanceCullData]) {
        if data.is_empty() {
            return;
        }
        queue.write_buffer(&self.buffer, 0, bytemuck::cast_slice(data));
    }

    fn alloc(device: &wgpu::Device, capacity: usize) -> wgpu::Buffer {
        let data = vec![0u8; capacity * CULL_ENTRY_BYTES as usize];
        device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("InstanceCullBuffer"),
            contents: &data,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
        })
    }

    fn make_bg(
        device: &wgpu::Device,
        layout: &wgpu::BindGroupLayout,
        buffer: &wgpu::Buffer,
    ) -> wgpu::BindGroup {
        device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("InstanceCullBuffer BindGroup"),
            layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: buffer.as_entire_binding(),
            }],
        })
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gpu_draw_indexed_indirect_is_20_bytes() {
        assert_eq!(std::mem::size_of::<GpuDrawIndexedIndirect>(), 20);
    }

    #[test]
    fn gpu_draw_indexed_indirect_zeroed() {
        let cmd = GpuDrawIndexedIndirect::zeroed();
        assert_eq!(cmd.index_count, 0);
        assert_eq!(cmd.instance_count, 0);
    }

    #[test]
    fn byte_offset_is_correct() {
        assert_eq!(DrawIndirectBuffer::byte_offset(0), 0);
        assert_eq!(DrawIndirectBuffer::byte_offset(1), 20);
        assert_eq!(DrawIndirectBuffer::byte_offset(5), 100);
    }

    #[test]
    fn instance_cull_data_size() {
        // 16*4 bytes for model (mat4) + 4*4 for center+cmd + 4*4 for extents+pad = 96 bytes
        assert_eq!(std::mem::size_of::<InstanceCullData>(), 96);
    }

    #[test]
    fn instance_cull_data_new_stores_cmd_index() {
        let data =
            InstanceCullData::new(glam::Mat4::IDENTITY, glam::Vec3::ZERO, glam::Vec3::ONE, 42);
        // w component of aabb_center_cmd stores cmd_index as raw bits
        assert_eq!(data.aabb_center_cmd[3].to_bits(), 42u32);
    }
}
