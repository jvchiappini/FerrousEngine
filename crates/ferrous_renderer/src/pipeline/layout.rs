/// Shared `wgpu::BindGroupLayout` objects that are used by multiple pipeline
/// stages.  Centralising them here means every pass that needs, for example,
/// a camera bind group can use the *same* layout without re-creating it.
use std::sync::Arc;

/// All bind-group layouts used by the built-in renderer pipelines.
///
/// Layouts are created once and shared via `Arc` so individual passes can
/// hold a reference without owning the whole `PipelineLayouts` struct.
#[derive(Clone)]
pub struct PipelineLayouts {
    /// group(0) — camera view-projection matrix (one `UNIFORM` buffer at binding 0)
    pub camera: Arc<wgpu::BindGroupLayout>,
    /// group(1) — per-object model matrix via a **dynamic** uniform buffer.
    ///
    /// `has_dynamic_offset: true` lets us bind a single large buffer once and
    /// supply a different byte offset per draw call — eliminating N bind-group
    /// switches and reducing CPU-side wgpu overhead from O(N) to O(1).
    pub model: Arc<wgpu::BindGroupLayout>,
}

impl PipelineLayouts {
    pub fn new(device: &wgpu::Device) -> Self {
        let static_uniform_entry = |binding: u32| wgpu::BindGroupLayoutEntry {
            binding,
            visibility: wgpu::ShaderStages::VERTEX,
            ty: wgpu::BindingType::Buffer {
                ty: wgpu::BufferBindingType::Uniform,
                has_dynamic_offset: false,
                min_binding_size: None,
            },
            count: None,
        };

        let camera = Arc::new(device.create_bind_group_layout(
            &wgpu::BindGroupLayoutDescriptor {
                label: Some("Layout: Camera"),
                entries: &[static_uniform_entry(0)],
            },
        ));

        // Model layout uses a dynamic offset so all per-object matrices live
        // in one buffer and we only switch the offset, not the bind group.
        let model = Arc::new(device.create_bind_group_layout(
            &wgpu::BindGroupLayoutDescriptor {
                label: Some("Layout: Model (dynamic)"),
                entries: &[wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::VERTEX,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: true,
                        // Each element is a mat4x4<f32> = 64 bytes.
                        min_binding_size: wgpu::BufferSize::new(64),
                    },
                    count: None,
                }],
            },
        ));

        Self { camera, model }
    }
}
