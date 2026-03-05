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
    /// Minimal layout used exclusively by the shadow pass (group 1).
    ///
    /// Contains only binding 0 (directional light uniform).  This avoids
    /// binding the shadow map texture as a `RESOURCE` while the same texture
    /// is simultaneously used as `DEPTH_STENCIL_WRITE` — which wgpu forbids
    /// as conflicting exclusive usages within a render pass.
    pub shadow_lights: Arc<wgpu::BindGroupLayout>,
    /// group(3) for cel/outline passes.
    ///
    /// Bindings:
    ///   0  — directional light uniform (same as `lights`)
    ///   10 — `CelParams` { toon_levels, outline_width, _pad×2 }
    pub cel_lights: Arc<wgpu::BindGroupLayout>,
    /// group(3) for outline pass.
    ///
    /// Bindings:
    ///   0  — directional light uniform
    ///   10 — `CelParams`
    ///   11 — `OutlineColor` { vec4<f32> }
    pub outline_lights: Arc<wgpu::BindGroupLayout>,
    /// group(3) for the flat-shaded pass.
    ///
    /// Bindings:
    ///   0  — directional light uniform (same dir-light struct as PBR)
    pub flat_lights: Arc<wgpu::BindGroupLayout>,

    // ── Phase 11: GPU-driven culling layouts ──────────────────────────────────
    /// Layout for the cull compute shader — group(0).
    ///
    /// Bindings:
    ///   0  — `instances: array<InstanceCullData>` (storage, read-only)
    ///         Each entry holds the model matrix + local AABB + command index.
    pub cull_instances: Arc<wgpu::BindGroupLayout>,

    /// Layout for the cull compute shader — group(1).
    ///
    /// Bindings:
    ///   0  — `draw_cmds: array<DrawIndexedIndirect>` (storage, read-write)
    ///         The cull shader increments `instance_count` in the matching slot.
    ///   1  — `counters: array<atomic<u32>>` (storage, read-write)
    ///         Per-batch write cursor so each visible instance gets a unique index.
    pub cull_indirect: Arc<wgpu::BindGroupLayout>,

    /// Layout for the cull compute shader — group(2).
    ///
    /// Bindings:
    ///   0  — `out_instances: array<mat4x4<f32>>` (storage, read-write)
    ///         Compacted visible instance matrices for the render pass.
    pub cull_out_instances: Arc<wgpu::BindGroupLayout>,

    /// Layout for the cull compute shader — group(3).
    ///
    /// Bindings:
    ///   0  — `params: CullParams` (uniform): frustum planes + instance count.
    pub cull_params: Arc<wgpu::BindGroupLayout>,
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
                        // directional light is now read in both vertex (shadow
                        // coordinate computation) and fragment stages.
                        visibility: wgpu::ShaderStages::VERTEX | wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Buffer {
                            ty: wgpu::BufferBindingType::Uniform,
                            has_dynamic_offset: false,
                            // size of DirectionalLightUniform after adding
                            // light_view_proj (96 bytes)
                            min_binding_size: wgpu::BufferSize::new(96),
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
                    // binding 5: point lights storage buffer (read-only)
                    wgpu::BindGroupLayoutEntry {
                        binding: 5,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Buffer {
                            ty: wgpu::BufferBindingType::Storage { read_only: true },
                            has_dynamic_offset: false,
                            min_binding_size: None,
                        },
                        count: None,
                    },
                    // binding 6: comparison sampler for shadow map
                    wgpu::BindGroupLayoutEntry {
                        binding: 6,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Comparison),
                        count: None,
                    },
                    // binding 7: shadow depth texture
                    wgpu::BindGroupLayoutEntry {
                        binding: 7,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Texture {
                            multisampled: false,
                            view_dimension: wgpu::TextureViewDimension::D2,
                            sample_type: wgpu::TextureSampleType::Depth,
                        },
                        count: None,
                    },
                    // binding 8: SSAO (blurred, half-resolution R8Unorm)
                    wgpu::BindGroupLayoutEntry {
                        binding: 8,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Texture {
                            multisampled: false,
                            view_dimension: wgpu::TextureViewDimension::D2,
                            sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        },
                        count: None,
                    },
                    // binding 9: SSAO sampler
                    wgpu::BindGroupLayoutEntry {
                        binding: 9,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                        count: None,
                    },
                ],
            }),
        );

        // Minimal layout for the shadow pass: only the directional light
        // uniform at binding 0.  The shadow shaders only read `light_view_proj`
        // from this buffer — they never sample the shadow map texture.
        // Using this layout prevents wgpu from seeing the shadow-map texture
        // bound as both DEPTH_STENCIL_WRITE (depth attachment) and RESOURCE
        // (sampled texture) in the same render-pass scope.
        let shadow_lights = Arc::new(device.create_bind_group_layout(
            &wgpu::BindGroupLayoutDescriptor {
                label: Some("Layout: Shadow Lights (dir light only)"),
                entries: &[wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::VERTEX,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: wgpu::BufferSize::new(96),
                    },
                    count: None,
                }],
            },
        ));

        // ── cel_lights layout ─────────────────────────────────────────────────
        // group(3) for the cel-shaded pass: dir-light (0) + CelParams (10).
        let dir_light_entry = wgpu::BindGroupLayoutEntry {
            binding: 0,
            visibility: wgpu::ShaderStages::VERTEX | wgpu::ShaderStages::FRAGMENT,
            ty: wgpu::BindingType::Buffer {
                ty: wgpu::BufferBindingType::Uniform,
                has_dynamic_offset: false,
                min_binding_size: wgpu::BufferSize::new(96),
            },
            count: None,
        };
        let cel_params_entry = wgpu::BindGroupLayoutEntry {
            binding: 10,
            visibility: wgpu::ShaderStages::VERTEX | wgpu::ShaderStages::FRAGMENT,
            ty: wgpu::BindingType::Buffer {
                ty: wgpu::BufferBindingType::Uniform,
                has_dynamic_offset: false,
                // CelParams: toon_levels(4) + outline_width(4) + 2×pad(8) = 16 bytes
                min_binding_size: wgpu::BufferSize::new(16),
            },
            count: None,
        };
        let cel_lights = Arc::new(device.create_bind_group_layout(
            &wgpu::BindGroupLayoutDescriptor {
                label: Some("Layout: Cel Lights"),
                entries: &[dir_light_entry.clone(), cel_params_entry.clone()],
            },
        ));

        // ── outline_lights layout ─────────────────────────────────────────────
        // group(3) for outline pass: dir-light (0) + CelParams (10) + OutlineColor (11).
        let outline_color_entry = wgpu::BindGroupLayoutEntry {
            binding: 11,
            visibility: wgpu::ShaderStages::FRAGMENT,
            ty: wgpu::BindingType::Buffer {
                ty: wgpu::BufferBindingType::Uniform,
                has_dynamic_offset: false,
                // OutlineColor: vec4<f32> = 16 bytes
                min_binding_size: wgpu::BufferSize::new(16),
            },
            count: None,
        };
        let outline_lights = Arc::new(device.create_bind_group_layout(
            &wgpu::BindGroupLayoutDescriptor {
                label: Some("Layout: Outline Lights"),
                entries: &[
                    dir_light_entry.clone(),
                    cel_params_entry.clone(),
                    outline_color_entry,
                ],
            },
        ));

        // ── flat_lights layout ────────────────────────────────────────────────
        // group(3) for the flat-shaded pass: only the dir-light uniform (0).
        let flat_lights = Arc::new(device.create_bind_group_layout(
            &wgpu::BindGroupLayoutDescriptor {
                label: Some("Layout: Flat Lights"),
                entries: &[dir_light_entry],
            },
        ));

        Self {
            camera,
            model,
            instance,
            material,
            lights,
            shadow_lights,
            cel_lights,
            outline_lights,
            flat_lights,
            cull_instances: Self::make_cull_instances(device),
            cull_indirect: Self::make_cull_indirect(device),
            cull_out_instances: Self::make_cull_out_instances(device),
            cull_params: Self::make_cull_params(device),
        }
    }

    // ── Phase 11: cull layout helpers ─────────────────────────────────────────

    fn make_cull_instances(device: &wgpu::Device) -> Arc<wgpu::BindGroupLayout> {
        Arc::new(
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("Layout: Cull Instances (RO storage)"),
                entries: &[wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Storage { read_only: true },
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                }],
            }),
        )
    }

    fn make_cull_indirect(device: &wgpu::Device) -> Arc<wgpu::BindGroupLayout> {
        Arc::new(
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("Layout: Cull Indirect (RW storage)"),
                entries: &[
                    // binding 0: draw commands (RW)
                    wgpu::BindGroupLayoutEntry {
                        binding: 0,
                        visibility: wgpu::ShaderStages::COMPUTE,
                        ty: wgpu::BindingType::Buffer {
                            ty: wgpu::BufferBindingType::Storage { read_only: false },
                            has_dynamic_offset: false,
                            min_binding_size: None,
                        },
                        count: None,
                    },
                    // binding 1: per-batch write counters (atomic RW)
                    wgpu::BindGroupLayoutEntry {
                        binding: 1,
                        visibility: wgpu::ShaderStages::COMPUTE,
                        ty: wgpu::BindingType::Buffer {
                            ty: wgpu::BufferBindingType::Storage { read_only: false },
                            has_dynamic_offset: false,
                            min_binding_size: None,
                        },
                        count: None,
                    },
                ],
            }),
        )
    }

    fn make_cull_out_instances(device: &wgpu::Device) -> Arc<wgpu::BindGroupLayout> {
        Arc::new(
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("Layout: Cull Out Instances (RW storage)"),
                entries: &[wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Storage { read_only: false },
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                }],
            }),
        )
    }

    fn make_cull_params(device: &wgpu::Device) -> Arc<wgpu::BindGroupLayout> {
        Arc::new(
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("Layout: Cull Params (uniform)"),
                entries: &[wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        // CullParams: 6 planes × 16 bytes + instance_count (4) + pad (12) = 112 bytes
                        min_binding_size: wgpu::BufferSize::new(112),
                    },
                    count: None,
                }],
            }),
        )
    }
}
