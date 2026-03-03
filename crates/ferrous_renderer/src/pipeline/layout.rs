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
    /// group(1) — instanced model matrices via a **storage** buffer.
    ///
    /// The shader indexes this array by `@builtin(instance_index)`, so a
    /// single `draw_indexed` with `instance_count > 1` renders all instances.
    pub instance: Arc<wgpu::BindGroupLayout>,
    /// group(2) — material parameters (uniform buffer) + optional texture.
    /// binding 0 = Material uniform, binding 1 = sampler, binding 2 = texture.
    pub material: Arc<wgpu::BindGroupLayout>,
    /// group(3) — directional light uniform buffer (one `UNIFORM` buffer at binding 0)
    pub lights: Arc<wgpu::BindGroupLayout>,
}

impl PipelineLayouts {
    pub fn new(device: &wgpu::Device) -> Self {
        let _static_uniform_entry = |binding: u32| wgpu::BindGroupLayoutEntry {
            binding,
            visibility: wgpu::ShaderStages::VERTEX,
            ty: wgpu::BindingType::Buffer {
                ty: wgpu::BufferBindingType::Uniform,
                has_dynamic_offset: false,
                min_binding_size: None,
            },
            count: None,
        };

        let camera = Arc::new(
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("Layout: Camera"),
                entries: &[wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    // eye_pos is read in the fragment shader for PBR view vector.
                    visibility: wgpu::ShaderStages::VERTEX | wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                }],
            }),
        );

        // Model layout uses a dynamic offset so all per-object matrices live
        // in one buffer and we only switch the offset, not the bind group.
        let model = Arc::new(
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("Layout: Model (dynamic)"),
                entries: &[wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::VERTEX,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: true,
                        // Each element is now two mat4x4<f32> = 128 bytes
                        // (model matrix + normal matrix).
                        min_binding_size: wgpu::BufferSize::new(128),
                    },
                    count: None,
                }],
            }),
        );

        // Instance layout: read-only storage buffer holding array<mat4x4<f32>>.
        // Indexed by instance_index in the vertex shader.
        let instance = Arc::new(device.create_bind_group_layout(
            &wgpu::BindGroupLayoutDescriptor {
                label: Some("Layout: Instances (storage)"),
                entries: &[wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::VERTEX,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Storage { read_only: true },
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                }],
            },
        ));

        // material layout: uniform + sampler + up to five textures
        let material = Arc::new(device.create_bind_group_layout(
            &wgpu::BindGroupLayoutDescriptor {
                label: Some("Layout: Material"),
                entries: &[
                    // binding 0: uniform buffer (MaterialUniformPbr)
                    wgpu::BindGroupLayoutEntry {
                        binding: 0,
                        visibility: wgpu::ShaderStages::VERTEX | wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Buffer {
                            ty: wgpu::BufferBindingType::Uniform,
                            has_dynamic_offset: false,
                            // 80 bytes for our PBR material struct
                            // struct now 96 bytes due to alignment/padding
                            min_binding_size: wgpu::BufferSize::new(96),
                        },
                        count: None,
                    },
                    // binding 1: sampler (filtering)
                    wgpu::BindGroupLayoutEntry {
                        binding: 1,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                        count: None,
                    },
                    // bindings 2-6: up to five texture2d slots
                    // albedo, normal, metallic/roughness, emissive, ao
                    wgpu::BindGroupLayoutEntry {
                        binding: 2,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Texture {
                            multisampled: false,
                            view_dimension: wgpu::TextureViewDimension::D2,
                            sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        },
                        count: None,
                    },
                    wgpu::BindGroupLayoutEntry {
                        binding: 3,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Texture {
                            multisampled: false,
                            view_dimension: wgpu::TextureViewDimension::D2,
                            sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        },
                        count: None,
                    },
                    wgpu::BindGroupLayoutEntry {
                        binding: 4,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Texture {
                            multisampled: false,
                            view_dimension: wgpu::TextureViewDimension::D2,
                            sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        },
                        count: None,
                    },
                    wgpu::BindGroupLayoutEntry {
                        binding: 5,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Texture {
                            multisampled: false,
                            view_dimension: wgpu::TextureViewDimension::D2,
                            sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        },
                        count: None,
                    },
                    wgpu::BindGroupLayoutEntry {
                        binding: 6,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Texture {
                            multisampled: false,
                            view_dimension: wgpu::TextureViewDimension::D2,
                            sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        },
                        count: None,
                    },
                ],
            },
        ));

        // lights layout: directional light + IBL resources (Phase 10)
        let lights = Arc::new(
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("Layout: Lights/Environment"),
                entries: &[
                    // binding 0: directional light uniform
                    wgpu::BindGroupLayoutEntry {
                        binding: 0,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Buffer {
                            ty: wgpu::BufferBindingType::Uniform,
                            has_dynamic_offset: false,
                            min_binding_size: wgpu::BufferSize::new(32),
                        },
                        count: None,
                    },
                    // binding 1: sampler for environment maps
                    wgpu::BindGroupLayoutEntry {
                        binding: 1,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                        count: None,
                    },
                    // binding 2: irradiance cubemap
                    wgpu::BindGroupLayoutEntry {
                        binding: 2,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Texture {
                            multisampled: false,
                            view_dimension: wgpu::TextureViewDimension::Cube,
                            sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        },
                        count: None,
                    },
                    // binding 3: prefiltered environment cubemap
                    wgpu::BindGroupLayoutEntry {
                        binding: 3,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Texture {
                            multisampled: false,
                            view_dimension: wgpu::TextureViewDimension::Cube,
                            sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        },
                        count: None,
                    },
                    // binding 4: BRDF integration LUT (2D)
                    wgpu::BindGroupLayoutEntry {
                        binding: 4,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Texture {
                            multisampled: false,
                            view_dimension: wgpu::TextureViewDimension::D2,
                            sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        },
                        count: None,
                    },
                ],
            }),
        );

        Self {
            camera,
            model,
            instance,
            material,
            lights,
        }
    }
}
