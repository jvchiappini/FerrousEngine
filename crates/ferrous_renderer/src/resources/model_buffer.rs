/// Dynamic uniform buffer for per-object model matrices.
///
/// ## Why this exists
///
/// The naïve approach allocates one `wgpu::Buffer` + one `wgpu::BindGroup`
/// per `RenderObject` and calls `set_bind_group(1, ...)` once per draw call.
/// With N objects that means N GPU-API calls just to switch the model matrix.
///
/// This type consolidates **all** model matrices into a single buffer.
/// `WorldPass` binds it once and supplies a byte offset per draw call:
///
/// ```text
/// rpass.set_bind_group(1, &model_buf.bind_group, &[offset]);  // once per object
/// ```
///
/// Because the bind group itself never changes, the GPU driver can avoid
/// expensive descriptor-table flushes between draw calls.
///
/// ## Alignment
///
/// wgpu requires each dynamic-offset element to be aligned to
/// `min_uniform_buffer_offset_alignment` (typically 256 bytes on desktop
/// hardware, 64 bytes on some mobile GPUs).  Each matrix slot is therefore
/// `align_up(64, alignment)` bytes, even though only 64 bytes are used.
use std::sync::Arc;

use wgpu::util::DeviceExt;

/// One aligned slot = 64 bytes of matrix + padding to reach `stride`.
const MAT4_SIZE: u64 = 64; // 4×4 × f32

/// A growable GPU buffer holding one `mat4x4<f32>` per slot, aligned to the
/// device's `min_uniform_buffer_offset_alignment`.
pub struct ModelBuffer {
    /// The raw GPU buffer.
    pub buffer: wgpu::Buffer,
    /// Single bind group that references the whole buffer with a dynamic offset.
    pub bind_group: Arc<wgpu::BindGroup>,
    /// Byte stride between consecutive matrix slots (≥ 64, multiple of alignment).
    pub stride: u32,
    /// Current capacity in number of matrix slots.
    capacity: usize,
}

impl ModelBuffer {
    /// Creates a `ModelBuffer` that can hold at least `initial_capacity` objects.
    ///
    /// `layout` must be the model bind-group layout with `has_dynamic_offset: true`.
    pub fn new(
        device: &wgpu::Device,
        layout: &wgpu::BindGroupLayout,
        initial_capacity: usize,
    ) -> Self {
        let alignment = device.limits().min_uniform_buffer_offset_alignment;
        let stride = align_up(MAT4_SIZE as u32, alignment);

        let capacity = initial_capacity.max(1);
        let buf = Self::create_buffer(device, capacity, stride);
        let bind_group = Self::create_bind_group(device, layout, &buf, stride);

        Self {
            buffer: buf,
            bind_group: Arc::new(bind_group),
            stride,
            capacity,
        }
    }

    /// Returns the byte offset of slot `index` within the buffer.
    #[inline]
    pub fn offset(&self, index: usize) -> u32 {
        (index as u32).wrapping_mul(self.stride)
    }

    /// Writes `matrix` into slot `index`.
    ///
    /// Panics if `index >= capacity`.
    #[inline]
    pub fn write(&self, queue: &wgpu::Queue, index: usize, matrix: &glam::Mat4) {
        debug_assert!(index < self.capacity, "ModelBuffer slot out of range");
        queue.write_buffer(
            &self.buffer,
            self.offset(index) as u64,
            bytemuck::cast_slice(&[matrix.to_cols_array()]),
        );
    }

    /// Ensures the buffer can hold at least `needed` objects.
    ///
    /// If the current capacity is insufficient the buffer and its bind group
    /// are **reallocated** (capacity doubles until sufficient).  Callers must
    /// re-record any commands that reference the old bind group.
    pub fn ensure_capacity(
        &mut self,
        device: &wgpu::Device,
        layout: &wgpu::BindGroupLayout,
        needed: usize,
    ) {
        if needed <= self.capacity {
            return;
        }
        // Double until large enough.
        let mut new_cap = self.capacity;
        while new_cap < needed {
            new_cap *= 2;
        }
        self.buffer = Self::create_buffer(device, new_cap, self.stride);
        self.bind_group = Arc::new(Self::create_bind_group(device, layout, &self.buffer, self.stride));
        self.capacity = new_cap;
    }

    // ── Private helpers ──────────────────────────────────────────────────────

    fn create_buffer(device: &wgpu::Device, capacity: usize, stride: u32) -> wgpu::Buffer {
        let size = capacity as u64 * stride as u64;
        // Fill with identity matrices so uninitialised slots are harmless.
        let identity = glam::Mat4::IDENTITY.to_cols_array();
        let mut data = vec![0u8; size as usize];
        for slot in 0..capacity {
            let off = slot * stride as usize;
            data[off..off + 64].copy_from_slice(bytemuck::cast_slice(&identity));
        }
        device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("ModelBuffer"),
            contents: &data,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        })
    }

    fn create_bind_group(
        device: &wgpu::Device,
        layout: &wgpu::BindGroupLayout,
        buffer: &wgpu::Buffer,
        _stride: u32,
    ) -> wgpu::BindGroup {
        device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("ModelBuffer BindGroup"),
            layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                    buffer,
                    offset: 0,
                    // Size of one slot (the dynamic portion the shader sees).
                    size: wgpu::BufferSize::new(MAT4_SIZE),
                }),
            }],
        })
    }
}

/// Round `value` up to the next multiple of `alignment` (which must be a
/// power of two).
#[inline]
fn align_up(value: u32, alignment: u32) -> u32 {
    (value + alignment - 1) & !(alignment - 1)
}
