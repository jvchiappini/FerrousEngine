/// Contiguous STORAGE buffer holding one `mat4x4<f32>` per instance.
///
/// Unlike [`super::ModelBuffer`] (which uses a *dynamic-uniform* buffer with
/// large per-slot alignment), this buffer is a plain STORAGE buffer where
/// every slot is exactly 64 bytes.  The shader indexes it by
/// `@builtin(instance_index)`, so a single `draw_indexed(0..N, 0, base..base+count)`
/// call renders `count` instances with no extra CPU work per object.
///
/// ## Typical usage (per frame)
///
/// ```text
/// // 1. tell the buffer how many instances we need this frame
/// instance_buf.reserve(device, layout, total_instances);
///
/// // 2. write matrices in contiguous groups (one group per unique mesh)
/// instance_buf.write_slice(queue, base_slot, &matrices);
///
/// // 3. build InstancedDrawCommand { first_instance: base_slot, instance_count: n, … }
/// ```
use std::sync::Arc;

use wgpu::util::DeviceExt;

/// Size of one mat4x4<f32> in bytes.
const MAT4_BYTES: u64 = 64;

/// Minimum number of slots to allocate.
const MIN_CAPACITY: usize = 64;

pub struct InstanceBuffer {
    /// Raw GPU buffer (`STORAGE | COPY_DST`).
    pub buffer: wgpu::Buffer,
    /// Bind group that exposes the whole buffer as a read-only storage binding.
    pub bind_group: Arc<wgpu::BindGroup>,
    /// Current allocated capacity (number of mat4 slots).
    capacity: usize,
}

impl InstanceBuffer {
    /// Creates an `InstanceBuffer` with at least `initial_capacity` slots.
    ///
    /// `layout` must be the instance bind-group layout created by
    /// [`super::super::pipeline::PipelineLayouts::instance`].
    pub fn new(
        device: &wgpu::Device,
        layout: &wgpu::BindGroupLayout,
        initial_capacity: usize,
    ) -> Self {
        let capacity = initial_capacity.max(MIN_CAPACITY);
        let buffer = Self::create_buffer(device, capacity);
        let bind_group = Arc::new(Self::create_bind_group(device, layout, &buffer));
        Self { buffer, bind_group, capacity }
    }

    /// Ensures the buffer can hold at least `needed` slots.
    ///
    /// If a reallocation happens the `bind_group` is also recreated.
    /// Callers must update any passes that hold a reference to the old bind
    /// group (the `Arc` pointer will differ).
    pub fn reserve(
        &mut self,
        device: &wgpu::Device,
        layout: &wgpu::BindGroupLayout,
        needed: usize,
    ) {
        if needed <= self.capacity {
            return;
        }
        let mut new_cap = self.capacity;
        while new_cap < needed {
            new_cap *= 2;
        }
        self.buffer = Self::create_buffer(device, new_cap);
        self.bind_group = Arc::new(Self::create_bind_group(device, layout, &self.buffer));
        self.capacity = new_cap;
    }

    /// Writes a contiguous slice of matrices starting at `base_slot`.
    ///
    /// Panics (debug) if `base_slot + matrices.len() > capacity`.
    pub fn write_slice(
        &self,
        queue: &wgpu::Queue,
        base_slot: usize,
        matrices: &[glam::Mat4],
    ) {
        if matrices.is_empty() {
            return;
        }
        debug_assert!(
            base_slot + matrices.len() <= self.capacity,
            "InstanceBuffer: write would exceed capacity"
        );
        let offset = base_slot as u64 * MAT4_BYTES;
        // Flatten to [f32; 16] per matrix and write in one call.
        let flat: Vec<f32> = matrices
            .iter()
            .flat_map(|m| m.to_cols_array())
            .collect();
        queue.write_buffer(&self.buffer, offset, bytemuck::cast_slice(&flat));
    }

    // ── Private helpers ──────────────────────────────────────────────────────

    fn create_buffer(device: &wgpu::Device, capacity: usize) -> wgpu::Buffer {
        let size = capacity as u64 * MAT4_BYTES;
        // Initialise with identity matrices so stale slots are harmless.
        let identity = glam::Mat4::IDENTITY.to_cols_array();
        let mut data = vec![0u8; size as usize];
        for i in 0..capacity {
            let off = i * MAT4_BYTES as usize;
            data[off..off + 64].copy_from_slice(bytemuck::cast_slice(&identity));
        }
        device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("InstanceBuffer"),
            contents: &data,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
        })
    }

    fn create_bind_group(
        device: &wgpu::Device,
        layout: &wgpu::BindGroupLayout,
        buffer: &wgpu::Buffer,
    ) -> wgpu::BindGroup {
        device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("InstanceBuffer BindGroup"),
            layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: buffer.as_entire_binding(),
            }],
        })
    }
}
