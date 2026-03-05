/// GPU-driven frustum culling pass — Phase 11.
///
/// `CullPass` is the CPU orchestration layer for the `cull.wgsl` compute
/// shader. It manages all GPU buffers and bind groups required by the cull
/// pipeline and implements the frame lifecycle:
///
/// 1. **`upload_instances`** — called by `Renderer::sync_world` to write per-frame
///    instance data (model matrices + AABBs + command indices) and the CPU-side
///    draw-command templates (index_count, first_index, first_instance).
///
/// 2. **`reset_counters`** — writes zeros to the per-batch atomic counters so
///    each batch starts from zero visible instances.
///
/// 3. **`dispatch`** — encodes the compute pass into the command encoder.
///
/// 4. **`patch_draw_commands`** — after the compute dispatch the CPU reads back
///    the `counters` buffer (via a staging buffer + map_async round-trip) and
///    patches `instance_count` in each `DrawIndexedIndirect` command so the
///    render pass uses the correct count.
///
/// ## GPU-driven render flow
///
/// ```text
/// sync_world → CullPass::upload_instances
///           → CullPass::reset_counters
/// do_render  → CullPass::dispatch          (compute encoder)
///           → (optional readback of counters for patching)
///           → WorldPass::execute (draw_indexed_indirect)
/// ```
use std::sync::Arc;

use bytemuck::Zeroable;
use wgpu::util::DeviceExt;

use crate::graph::{FramePacket, RenderPass};
use crate::pipeline::{GpuCullPipeline, PipelineLayouts};
use crate::resources::draw_indirect::{
    DrawIndirectBuffer, GpuDrawIndexedIndirect, InstanceCullBuffer, InstanceCullData,
};
use crate::scene::Frustum;

// ── CullParams GPU struct ────────────────────────────────────────────────────

/// Matches the `CullParams` struct in `cull.wgsl` (112 bytes).
#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
pub struct CullParamsUniform {
    /// Six view-frustum planes, each stored as `vec4<f32>(nx, ny, nz, d)`.
    pub planes: [[f32; 4]; 6],
    /// Total number of instances submitted this frame.
    pub instance_count: u32,
    pub _pad0: u32,
    pub _pad1: u32,
    pub _pad2: u32,
}

impl CullParamsUniform {
    /// Builds `CullParamsUniform` from an existing [`Frustum`] and instance count.
    ///
    /// The `Frustum` stores planes as `[Vec4; 6]` in the same layout the shader
    /// expects: normal in xyz, signed distance in w.
    pub fn from_frustum(frustum: &Frustum, instance_count: u32) -> Self {
        let mut planes = [[0.0f32; 4]; 6];
        for (i, p) in frustum.planes().iter().enumerate() {
            planes[i] = [p.x, p.y, p.z, p.w];
        }
        Self {
            planes,
            instance_count,
            _pad0: 0,
            _pad1: 0,
            _pad2: 0,
        }
    }
}

// ── CullPass ─────────────────────────────────────────────────────────────────

/// Per-frame GPU-driven culling pass.
///
/// Holds the compute pipeline and all GPU buffers needed to cull and compact
/// the instance list. Integrates with `WorldPass` via `DrawIndirectBuffer`.
pub struct CullPass {
    /// The compiled cull compute pipeline.
    pipeline: GpuCullPipeline,

    // ── Input buffers ────────────────────────────────────────────────────────
    /// Per-instance model matrices + AABBs + command indices.
    pub instance_cull_buf: InstanceCullBuffer,
    /// Read-only view of the draw command templates (index_count, first_index, etc.).
    /// The shader reads `first_instance` from this to know where to write.
    draw_cmds_ro_buf: Arc<wgpu::Buffer>,
    draw_cmds_ro_bg: Arc<wgpu::BindGroup>,
    /// Atomic per-batch write counters (zeroed each frame).
    counters_buf: Arc<wgpu::Buffer>,
    counters_bg: Arc<wgpu::BindGroup>, // combined bind group with draw_cmds_ro

    // ── Output buffers ───────────────────────────────────────────────────────
    /// Compacted visible instance matrices; consumed by the render pass.
    pub out_instance_buf: Arc<wgpu::Buffer>,
    pub out_instance_bg: Arc<wgpu::BindGroup>,

    // ── Indirect draw commands (written by CPU template, patched after dispatch)
    /// The actual indirect buffer consumed by `draw_indexed_indirect`.
    pub indirect_buf: DrawIndirectBuffer,

    // ── Uniform ─────────────────────────────────────────────────────────────
    params_buf: Arc<wgpu::Buffer>,
    params_bg: Arc<wgpu::BindGroup>,

    // ── Staging buffer for counter readback ─────────────────────────────────
    counter_staging: Arc<wgpu::Buffer>,

    // ── State ────────────────────────────────────────────────────────────────
    /// Number of mesh batches (draw commands) this frame.
    pub batch_count: usize,
    /// Number of total instances this frame.
    pub instance_count: u32,
    /// Capacity of out_instance_buf (number of mat4 slots).
    out_capacity: usize,
    layouts: PipelineLayouts,
}

const MAT4_BYTES: u64 = 64;
const MIN_INSTANCE_CAP: usize = 64;
const MIN_BATCH_CAP: usize = 16;

impl CullPass {
    /// Creates a new `CullPass` with minimum-capacity GPU buffers.
    pub fn new(device: &wgpu::Device, layouts: &PipelineLayouts) -> Self {
        let pipeline = GpuCullPipeline::new(device, layouts);

        // ── Instance cull buffer (RO) ────────────────────────────────────────
        let instance_cull_buf =
            InstanceCullBuffer::new(device, &layouts.cull_instances, MIN_INSTANCE_CAP);

        // ── Draw commands RO buffer (read by shader for first_instance) ──────
        let draw_cmds_ro_buf = Arc::new(create_zero_buffer(
            device,
            MIN_BATCH_CAP as u64 * 20, // 20 bytes per DrawIndexedIndirect
            wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
            "DrawCmds RO Buffer",
        ));

        // ── Atomic counters buffer ────────────────────────────────────────────
        // One u32 per batch; zeroed each frame before dispatch.
        let counters_buf = Arc::new(create_zero_buffer(
            device,
            MIN_BATCH_CAP as u64 * 4,
            wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
            "Cull Counters Buffer",
        ));

        // Combined bind group for group(1): draw_cmds_ro + counters.
        let counters_bg = Arc::new(device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Cull group(1): draw_cmds+counters"),
            layout: &layouts.cull_indirect,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: draw_cmds_ro_buf.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: counters_buf.as_entire_binding(),
                },
            ],
        }));
        // Keep a separate Arc for the draw_cmds_ro bind group (group(1) will
        // also contain counters — we share the combined BG via counters_bg).
        let draw_cmds_ro_bg = counters_bg.clone();

        // ── Output instances buffer (RW, consumed by render pass) ─────────────
        let out_capacity = MIN_INSTANCE_CAP;
        let out_instance_buf = Arc::new(create_zero_buffer(
            device,
            out_capacity as u64 * MAT4_BYTES,
            wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
            "Cull Out Instances",
        ));
        let out_instance_bg = Arc::new(device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Cull group(2): out_instances"),
            layout: &layouts.cull_out_instances,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: out_instance_buf.as_entire_binding(),
            }],
        }));

        // ── Indirect draw buffer (INDIRECT | STORAGE | COPY_DST) ─────────────
        let indirect_buf = DrawIndirectBuffer::new(device, &layouts.cull_indirect, MIN_BATCH_CAP);

        // ── CullParams uniform ────────────────────────────────────────────────
        let params_data = CullParamsUniform::zeroed();
        let params_buf = Arc::new(device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("CullParams Uniform"),
            contents: bytemuck::bytes_of(&params_data),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        }));
        let params_bg = Arc::new(device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Cull group(3): params"),
            layout: &layouts.cull_params,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: params_buf.as_entire_binding(),
            }],
        }));

        // ── Staging buffer for counter readback ───────────────────────────────
        let counter_staging = Arc::new(device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Cull Counter Staging"),
            size: MIN_BATCH_CAP as u64 * 4,
            usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        }));

        Self {
            pipeline,
            instance_cull_buf,
            draw_cmds_ro_buf,
            draw_cmds_ro_bg,
            counters_buf,
            counters_bg,
            out_instance_buf,
            out_instance_bg,
            indirect_buf,
            params_buf,
            params_bg,
            counter_staging,
            batch_count: 0,
            instance_count: 0,
            out_capacity,
            layouts: layouts.clone(),
        }
    }

    /// Uploads per-frame instance data and draw command templates.
    ///
    /// Called by `Renderer::sync_world` after building the instanced draw commands.
    ///
    /// `instances`     — one entry per entity; includes model matrix, AABB, and command index.
    /// `cmd_templates` — one entry per unique mesh batch (index_count, first_index, etc.).
    ///                   `instance_count` should be 0 — the cull shader fills it.
    pub fn upload_instances(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        instances: &[InstanceCullData],
        cmd_templates: &[GpuDrawIndexedIndirect],
    ) {
        self.instance_count = instances.len() as u32;
        self.batch_count = cmd_templates.len();

        // Grow instance cull buffer if needed.
        self.instance_cull_buf
            .reserve(device, &self.layouts.cull_instances, instances.len());
        self.instance_cull_buf.write(queue, instances);

        // Grow counters / draw_cmds_ro buffers if needed.
        self.maybe_grow_batch_buffers(device, cmd_templates.len());

        // Write draw command templates to the RO buffer (shader reads first_instance).
        queue.write_buffer(
            &self.draw_cmds_ro_buf,
            0,
            bytemuck::cast_slice(cmd_templates),
        );

        // Write the same templates to the actual indirect draw buffer.
        self.indirect_buf.write_templates(queue, cmd_templates);

        // Grow output instance buffer if needed.
        self.maybe_grow_out_instances(device, instances.len());
    }

    /// Zeros out the per-batch atomic counters. Must be called each frame
    /// before the cull dispatch so counts start from zero.
    pub fn reset_counters(&self, queue: &wgpu::Queue) {
        let zeros = vec![0u32; self.batch_count.max(1)];
        queue.write_buffer(&self.counters_buf, 0, bytemuck::cast_slice(&zeros));
    }

    /// Updates the CullParams uniform with the current frustum and instance count.
    pub fn update_params(
        &self,
        queue: &wgpu::Queue,
        frustum: &Frustum,
    ) {
        let params = CullParamsUniform::from_frustum(frustum, self.instance_count);
        queue.write_buffer(&self.params_buf, 0, bytemuck::bytes_of(&params));
    }

    /// Encodes a copy of `counters` into the staging buffer so the CPU can
    /// read back instance counts after the dispatch finishes.
    pub fn copy_counters_to_staging(&self, encoder: &mut wgpu::CommandEncoder) {
        let byte_size = (self.batch_count as u64 * 4).max(4);
        encoder.copy_buffer_to_buffer(
            &self.counters_buf,
            0,
            &self.counter_staging,
            0,
            byte_size,
        );
    }

    /// Patches the indirect draw buffer's `instance_count` fields using the
    /// CPU-readable counter values.
    ///
    /// This must be called *after* the GPU has finished executing the cull pass
    /// and the staging buffer has been mapped. Use `wgpu::Device::poll` with
    /// `Maintain::Wait` before calling this when synchronous readback is needed.
    ///
    /// In an async frame pipeline, call `map_async` on `counter_staging` and
    /// poll in the next frame — for this implementation we use the simpler
    /// synchronous path since we're targeting the initial Phase 11 milestone.
    pub fn patch_indirect_counts_sync(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
    ) {
        // Encode the copy in the current encoder (already done by
        // copy_counters_to_staging). This method just handles the patching
        // part using data from a previous frame's staging map — or skips if
        // batch_count == 0. The synchronous approach is: after the encoder
        // is submitted, map the staging buffer, read counts, write the
        // indirect buffer. Since this requires device.poll(), we separate it
        // into a helper called from the Renderer after the frame submit.
        let _ = (device, queue, encoder); // placeholder for async path
    }

    /// Performs the synchronous counter readback + indirect buffer patch.
    ///
    /// Call this AFTER submitting the frame encoder but BEFORE the next frame's
    /// draw_indexed_indirect call. Uses `device.poll(Maintain::Wait)` to block
    /// until the GPU copy is done.
    ///
    /// Returns the per-batch visible instance counts (for benchmarking).
    pub fn sync_patch_indirect(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
    ) -> Vec<u32> {
        if self.batch_count == 0 {
            return vec![];
        }

        // Map the staging buffer.
        let slice = self.counter_staging.slice(..self.batch_count as u64 * 4);
        let (tx, rx) = std::sync::mpsc::sync_channel(1);
        slice.map_async(wgpu::MapMode::Read, move |r| {
            let _ = tx.send(r);
        });
        device.poll(wgpu::Maintain::Wait);
        let _ = rx.recv().ok();

        let counts: Vec<u32> = {
            let view = slice.get_mapped_range();
            bytemuck::cast_slice(&view).to_vec()
        };
        self.counter_staging.unmap();

        // Patch the indirect draw buffer's instance_count fields.
        let _patched: Vec<GpuDrawIndexedIndirect> = Vec::with_capacity(self.batch_count);
        // We only know index_count etc. from the templates written in upload_instances.
        // Re-read from the buffer would require another staging copy. Instead,
        // we kept the templates in the CullPass on the CPU side via the
        // `upload_instances` call — but we didn't store them. For the initial
        // implementation we write only the 4-byte instance_count field at
        // the correct byte offset in the indirect buffer (offset 4 in each 20-byte cmd).
        for (i, &count) in counts.iter().enumerate() {
            let cmd_offset = i as u64 * 20 + 4; // offset of instance_count field
            queue.write_buffer(
                &self.indirect_buf.buffer,
                cmd_offset,
                bytemuck::bytes_of(&count),
            );
        }

        counts
    }

    /// Dispatches the cull compute shader using the pre-built bind groups.
    pub fn dispatch(&self, encoder: &mut wgpu::CommandEncoder) {
        if self.instance_count == 0 || self.batch_count == 0 {
            return;
        }
        self.pipeline.dispatch(
            encoder,
            [
                &self.instance_cull_buf.ro_bind_group,
                &self.counters_bg,
                &self.out_instance_bg,
                &self.params_bg,
            ],
            self.instance_count,
        );
    }

    // ── Private helpers ──────────────────────────────────────────────────────

    fn maybe_grow_batch_buffers(&mut self, device: &wgpu::Device, needed: usize) {
        let current_cap = self.counters_buf.size() as usize / 4;
        if needed <= current_cap {
            return;
        }
        let mut cap = current_cap.max(MIN_BATCH_CAP);
        while cap < needed {
            cap *= 2;
        }

        let new_counters = Arc::new(create_zero_buffer(
            device,
            cap as u64 * 4,
            wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
            "Cull Counters Buffer",
        ));
        let new_cmds_ro = Arc::new(create_zero_buffer(
            device,
            cap as u64 * 20,
            wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
            "DrawCmds RO Buffer",
        ));

        let bg = Arc::new(device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Cull group(1): draw_cmds+counters"),
            layout: &self.layouts.cull_indirect,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: new_cmds_ro.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: new_counters.as_entire_binding(),
                },
            ],
        }));

        self.counters_buf = new_counters;
        self.draw_cmds_ro_buf = new_cmds_ro;
        self.counters_bg = bg.clone();
        self.draw_cmds_ro_bg = bg;

        // Grow staging buffer too.
        self.counter_staging = Arc::new(device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Cull Counter Staging"),
            size: cap as u64 * 4,
            usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        }));
    }

    fn maybe_grow_out_instances(&mut self, device: &wgpu::Device, needed: usize) {
        if needed <= self.out_capacity {
            return;
        }
        let mut cap = self.out_capacity.max(MIN_INSTANCE_CAP);
        while cap < needed {
            cap *= 2;
        }
        let buf = Arc::new(create_zero_buffer(
            device,
            cap as u64 * MAT4_BYTES,
            wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
            "Cull Out Instances",
        ));
        let bg = Arc::new(device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Cull group(2): out_instances"),
            layout: &self.layouts.cull_out_instances,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: buf.as_entire_binding(),
            }],
        }));
        self.out_instance_buf = buf;
        self.out_instance_bg = bg;
        self.out_capacity = cap;
    }
}

// ── RenderPass impl ───────────────────────────────────────────────────────────

impl RenderPass for CullPass {
    fn name(&self) -> &str {
        "CullPass"
    }

    fn prepare(
        &mut self,
        _device: &wgpu::Device,
        _queue: &wgpu::Queue,
        _packet: &FramePacket,
    ) {
        // Params and instance data are updated externally (via update_params
        // and upload_instances). Nothing to do here.
    }

    fn execute(
        &mut self,
        _device: &wgpu::Device,
        _queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        _color_view: &wgpu::TextureView,
        _resolve_target: Option<&wgpu::TextureView>,
        _depth_view: Option<&wgpu::TextureView>,
        _packet: &FramePacket,
    ) {
        self.dispatch(encoder);
        // After the dispatch, the caller must:
        //   1. copy_counters_to_staging(encoder)
        //   2. submit the encoder
        //   3. call sync_patch_indirect(device, queue) to write instance counts
        //      into the indirect buffer before the render pass begins.
    }
}

// ── Utility ──────────────────────────────────────────────────────────────────

fn create_zero_buffer(
    device: &wgpu::Device,
    size: u64,
    usage: wgpu::BufferUsages,
    label: &str,
) -> wgpu::Buffer {
    device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some(label),
        contents: &vec![0u8; size as usize],
        usage,
    })
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cull_params_uniform_is_112_bytes() {
        assert_eq!(std::mem::size_of::<CullParamsUniform>(), 112);
    }

    #[test]
    fn cull_params_from_frustum_identity() {
        // Frustum::from_view_proj with identity should produce 6 planes.
        let vp = glam::Mat4::IDENTITY;
        let frustum = crate::scene::Frustum::from_view_proj(&vp);
        let params = CullParamsUniform::from_frustum(&frustum, 42);
        assert_eq!(params.instance_count, 42);
        // All plane arrays should be non-zero (frustum from identity produces
        // planes at ±1 in each axis).
        let has_nonzero = params.planes.iter().any(|p| p.iter().any(|&v| v != 0.0));
        assert!(has_nonzero, "Expected non-zero frustum planes");
    }
}
