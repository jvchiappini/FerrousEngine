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
    /// group(1) — per-object model matrix (one `UNIFORM` buffer at binding 0)
    pub model: Arc<wgpu::BindGroupLayout>,
}

impl PipelineLayouts {
    pub fn new(device: &wgpu::Device) -> Self {
        let uniform_entry = |binding: u32| wgpu::BindGroupLayoutEntry {
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
                entries: &[uniform_entry(0)],
            },
        ));

        let model = Arc::new(device.create_bind_group_layout(
            &wgpu::BindGroupLayoutDescriptor {
                label: Some("Layout: Model"),
                entries: &[uniform_entry(0)],
            },
        ));

        Self { camera, model }
    }
}
