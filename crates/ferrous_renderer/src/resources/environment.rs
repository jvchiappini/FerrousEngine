use std::sync::Arc;

use wgpu::{Device, Queue};

use crate::resources::light::{
    DirectionalLightUniform, LightStorageHeader, PointLightUniform, MAX_POINT_LIGHTS,
};

/// Simple container for environment-related GPU resources.
///
/// At the moment we only support a "dummy" implementation used during
/// Phase 10 development; it creates 1x1 placeholder textures and a
/// bind group that matches the layout defined in `PipelineLayouts`.
pub struct Environment {
    /// Complete bind group bound at `group(3)` in the PBR shader.
    pub bind_group: Arc<wgpu::BindGroup>,
    /// Cached copy of the directional light uniform.
    pub light_uniform: DirectionalLightUniform,
    /// Buffer holding the directional light data.
    pub light_buffer: Arc<wgpu::Buffer>,

    // ── Point lights ──────────────────────────────────────────────────────
    /// GPU storage buffer: 16-byte header + array of PointLightUniform.
    pub point_light_buffer: Arc<wgpu::Buffer>,
    /// Number of PointLightUniform slots currently allocated in the buffer.
    pub point_light_capacity: usize,

    // ── IBL resources kept so we can rebuild the bind group on resize ─────
    sampler: Arc<wgpu::Sampler>,
    irradiance_view: Arc<wgpu::TextureView>,
    prefilter_view: Arc<wgpu::TextureView>,
    brdf_view: Arc<wgpu::TextureView>,
    shadow_sampler: Arc<wgpu::Sampler>,
    shadow_view: Arc<wgpu::TextureView>,
}

impl Environment {
    /// Construct an "empty" environment.  This creates a default
    /// directional light uniform and three placeholder textures (irradiance,
    /// prefilter cubemaps and a 2D BRDF LUT) so that the pipeline can bind
    /// something without crashing.
    ///
    /// `layout` is expected to be the bind-group-layout produced by
    /// `PipelineLayouts::lights` after Phase 10 modifications.
    pub fn new_dummy(device: &Device, queue: &Queue, layout: &wgpu::BindGroupLayout) -> Self {
        let light_uniform = DirectionalLightUniform::default();
        let light_buffer = crate::resources::buffer::create_uniform(
            device,
            "Directional Light Uniform",
            &light_uniform,
        );

        // sampler used for all environment textures
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("EnvSampler"),
            address_mode_u: wgpu::AddressMode::Repeat,
            address_mode_v: wgpu::AddressMode::Repeat,
            address_mode_w: wgpu::AddressMode::Repeat,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });

        // helper that writes the same 4-byte color to each layer of a cubemap
        let write_cube = |tex: &wgpu::Texture| {
            let pixel = [30u8, 30, 30, 255];
            for layer in 0..6 {
                queue.write_texture(
                    wgpu::ImageCopyTexture {
                        texture: tex,
                        mip_level: 0,
                        origin: wgpu::Origin3d {
                            x: 0,
                            y: 0,
                            z: layer,
                        },
                        aspect: wgpu::TextureAspect::All,
                    },
                    &pixel,
                    wgpu::ImageDataLayout {
                        offset: 0,
                        bytes_per_row: Some(4),
                        rows_per_image: Some(1),
                    },
                    wgpu::Extent3d {
                        width: 1,
                        height: 1,
                        depth_or_array_layers: 1,
                    },
                );
            }
        };

        // irradiance cubemap (1x1, six layers)
        let cube_desc = wgpu::TextureDescriptor {
            label: Some("DummyCube"),
            size: wgpu::Extent3d {
                width: 1,
                height: 1,
                depth_or_array_layers: 6,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2, // must be D2
            format: wgpu::TextureFormat::Rgba8Unorm,
            view_formats: &[],
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        };
        let irradiance_tex = device.create_texture(&cube_desc);
        let irradiance_view = irradiance_tex.create_view(&wgpu::TextureViewDescriptor {
            label: Some("IrradianceView"),
            dimension: Some(wgpu::TextureViewDimension::Cube),
            ..Default::default()
        });
        write_cube(&irradiance_tex);

        // prefilter cubemap (same layout)
        let prefilter_tex = device.create_texture(&cube_desc);
        let prefilter_view = prefilter_tex.create_view(&wgpu::TextureViewDescriptor {
            label: Some("PrefilterView"),
            dimension: Some(wgpu::TextureViewDimension::Cube),
            ..Default::default()
        });
        write_cube(&prefilter_tex);

        // BRDF LUT 2D texture 1x1
        let brdf_desc = wgpu::TextureDescriptor {
            label: Some("DummyBRDF"),
            size: wgpu::Extent3d {
                width: 1,
                height: 1,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            view_formats: &[],
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        };
        let brdf_tex = device.create_texture(&brdf_desc);
        let brdf_view = brdf_tex.create_view(&wgpu::TextureViewDescriptor::default());
        let brdf_pixel = [255u8, 0, 0, 255];
        queue.write_texture(
            wgpu::ImageCopyTexture {
                texture: &brdf_tex,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            &brdf_pixel,
            wgpu::ImageDataLayout {
                offset: 0,
                bytes_per_row: Some(4),
                rows_per_image: Some(1),
            },
            wgpu::Extent3d {
                width: 1,
                height: 1,
                depth_or_array_layers: 1,
            },
        );

        // dummy shadow resources (1x1 depth texture + comparison sampler)
        let shadow_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("DummyShadowSampler"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            compare: Some(wgpu::CompareFunction::LessEqual),
            ..Default::default()
        });
        let shadow_desc = wgpu::TextureDescriptor {
            label: Some("DummyShadow"),
            size: wgpu::Extent3d {
                width: 1,
                height: 1,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Depth32Float,
            view_formats: &[],
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        };
        let shadow_tex = device.create_texture(&shadow_desc);
        let shadow_view = shadow_tex.create_view(&wgpu::TextureViewDescriptor::default());

        // Initial point-light storage buffer: 8-light capacity, zero-initialised.
        let initial_pl_capacity: usize = 8;
        let point_light_buffer =
            Arc::new(Self::create_point_light_buffer(device, initial_pl_capacity));
        // Write a zeroed header so the shader sees count = 0.
        let zero_header = LightStorageHeader {
            count: 0,
            _pad: [0u32; 3],
        };
        queue.write_buffer(&point_light_buffer, 0, bytemuck::bytes_of(&zero_header));

        let bind_group = Arc::new(device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Environment Bind Group"),
            layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: light_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&sampler),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::TextureView(&irradiance_view),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: wgpu::BindingResource::TextureView(&prefilter_view),
                },
                wgpu::BindGroupEntry {
                    binding: 4,
                    resource: wgpu::BindingResource::TextureView(&brdf_view),
                },
                wgpu::BindGroupEntry {
                    binding: 5,
                    resource: point_light_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 6,
                    resource: wgpu::BindingResource::Sampler(&shadow_sampler),
                },
                wgpu::BindGroupEntry {
                    binding: 7,
                    resource: wgpu::BindingResource::TextureView(&shadow_view),
                },
            ],
        }));

        Environment {
            bind_group,
            light_uniform,
            light_buffer,
            point_light_buffer,
            point_light_capacity: initial_pl_capacity,
            sampler: Arc::new(sampler),
            irradiance_view: Arc::new(irradiance_view),
            prefilter_view: Arc::new(prefilter_view),
            brdf_view: Arc::new(brdf_view),
            shadow_sampler: Arc::new(shadow_sampler),
            shadow_view: Arc::new(shadow_view),
        }
    }

    /// Build an environment from a high‑dynamic range image file on disk.
    ///
    /// A sequence of GPU compute passes is executed only once: the equirect
    /// image is converted to a cube map, an irradiance map is convoluted,
    /// a prefiltered specular map is generated (all mip levels) and finally a
    /// 2D BRDF integration LUT is computed.  The resulting textures are bound
    /// into the same layout as `new_dummy` so the rest of the renderer is
    /// unaware the data came from an HDRI.
    pub fn from_hdri(
        device: &Device,
        queue: &Queue,
        layout: &wgpu::BindGroupLayout,
        hdr_path: &std::path::Path,
    ) -> anyhow::Result<Self> {
        use crate::pipeline::ComputePipeline;
        use ferrous_assets::Texture2d;

        // 1. load equirectangular HDR image as a 2D float texture
        let hdr = Texture2d::from_hdr(device, queue, hdr_path)?;

        // common sampler for all environment textures
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("EnvSampler"),
            address_mode_u: wgpu::AddressMode::Repeat,
            address_mode_v: wgpu::AddressMode::Repeat,
            address_mode_w: wgpu::AddressMode::Repeat,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });

        // 2. create the cubemap that will hold the converted environment
        let env_size: u32 = 1024;
        let mip_count = (env_size as f32).log2().floor() as u32 + 1;
        let cube_desc = wgpu::TextureDescriptor {
            label: Some("EnvCube"),
            size: wgpu::Extent3d {
                width: env_size,
                height: env_size,
                depth_or_array_layers: 6,
            },
            mip_level_count: mip_count,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba16Float,
            view_formats: &[],
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::STORAGE_BINDING,
        };
        let env_tex = device.create_texture(&cube_desc);
        // 2D-array view for compute writes (equirect->cube, irradiance, prefilter source)
        let env_array_view = env_tex.create_view(&wgpu::TextureViewDescriptor {
            label: Some("EnvArrayView"),
            dimension: Some(wgpu::TextureViewDimension::D2Array),
            base_mip_level: 0,
            mip_level_count: Some(1),
            ..Default::default()
        });

        // 3. irradiance map (low res)
        let irr_size: u32 = 32;
        let irr_desc = wgpu::TextureDescriptor {
            label: Some("IrradianceCube"),
            size: wgpu::Extent3d {
                width: irr_size,
                height: irr_size,
                depth_or_array_layers: 6,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba16Float,
            view_formats: &[],
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::STORAGE_BINDING,
        };
        let irr_tex = device.create_texture(&irr_desc);
        let irr_view = irr_tex.create_view(&wgpu::TextureViewDescriptor {
            label: Some("IrradianceView"),
            dimension: Some(wgpu::TextureViewDimension::D2Array),
            ..Default::default()
        });

        // 4. dedicated prefilter output cubemap.  We keep env_tex as a
        // read-only source and write each mip into a separate texture to
        // avoid the read/write aliasing that wgpu forbids in a single dispatch.
        let prefilter_tex = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("PrefilterCube"),
            size: wgpu::Extent3d {
                width: env_size,
                height: env_size,
                depth_or_array_layers: 6,
            },
            mip_level_count: mip_count,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba16Float,
            view_formats: &[],
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::STORAGE_BINDING,
        });
        let prefilter_cube_view = prefilter_tex.create_view(&wgpu::TextureViewDescriptor {
            label: Some("PrefilterCubeView"),
            dimension: Some(wgpu::TextureViewDimension::Cube),
            ..Default::default()
        });

        // 5. BRDF LUT texture
        let brdf_size: u32 = 512;
        let brdf_desc = wgpu::TextureDescriptor {
            label: Some("BRDF_LUT"),
            size: wgpu::Extent3d {
                width: brdf_size,
                height: brdf_size,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba16Float,
            view_formats: &[],
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::STORAGE_BINDING,
        };
        let brdf_tex = device.create_texture(&brdf_desc);
        let brdf_view = brdf_tex.create_view(&wgpu::TextureViewDescriptor::default());

        // helper to run a compute shader given its code and a bind group
        let run_compute = |pipeline: ComputePipeline,
                           bind_groups: &[&wgpu::BindGroup],
                           workgroups: (u32, u32, u32)| {
            let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("EnvComputeEncoder"),
            });
            {
                let mut cpass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                    label: Some("EnvComputePass"),
                    timestamp_writes: None,
                });
                cpass.set_pipeline(&pipeline.inner);
                for (i, bg) in bind_groups.iter().enumerate() {
                    // `bg` has type `&&wgpu::BindGroup` from the slice iterator.
                    // dereference twice to obtain `&wgpu::BindGroup`.
                    cpass.set_bind_group(i as u32, &**bg, &[]);
                }
                cpass.dispatch_workgroups(workgroups.0, workgroups.1, workgroups.2);
            }
            queue.submit(Some(encoder.finish()));
        };

        // --- equirect -> cube map ---
        let eq_bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("EqToCube Layout"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Texture {
                        multisampled: false,
                        view_dimension: wgpu::TextureViewDimension::D2,
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::StorageTexture {
                        access: wgpu::StorageTextureAccess::WriteOnly,
                        format: wgpu::TextureFormat::Rgba16Float,
                        view_dimension: wgpu::TextureViewDimension::D2Array,
                    },
                    count: None,
                },
            ],
        });
        let eq_pipeline = ComputePipeline::new(
            device,
            include_str!("../../../../assets/shaders/equirect_to_cubemap.wgsl"),
            &[&eq_bgl],
            "main",
            Some("EquirectToCube"),
        );
        let eq_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("EquirectBindGroup"),
            layout: &eq_bgl,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&hdr.view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&hdr.sampler),
                },
                // write into the 2D-array view
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::TextureView(&env_array_view),
                },
            ],
        });
        run_compute(
            eq_pipeline.clone(),
            &[&eq_bind_group],
            (env_size / 8, env_size / 8, 6),
        );

        // --- irradiance ---
        let irrad_bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("Irradiance Layout"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Texture {
                        multisampled: false,
                        view_dimension: wgpu::TextureViewDimension::D2Array,
                        sample_type: wgpu::TextureSampleType::Float { filterable: false },
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::StorageTexture {
                        access: wgpu::StorageTextureAccess::WriteOnly,
                        format: wgpu::TextureFormat::Rgba16Float,
                        view_dimension: wgpu::TextureViewDimension::D2Array,
                    },
                    count: None,
                },
            ],
        });
        let irrad_pipeline = ComputePipeline::new(
            device,
            include_str!("../../../../assets/shaders/irradiance.wgsl"),
            &[&irrad_bgl],
            "main",
            Some("Irradiance"),
        );
        let irrad_bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("IrradianceBindGroup"),
            layout: &irrad_bgl,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    // 2D-array view so the shader can textureLoad per-face
                    resource: wgpu::BindingResource::TextureView(&env_array_view),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::TextureView(&irr_view),
                },
            ],
        });
        run_compute(
            irrad_pipeline.clone(),
            &[&irrad_bind],
            (irr_size / 8, irr_size / 8, 6),
        );

        // --- prefilter for each mip level ---
        let prefilter_bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("Prefilter Layout"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    // Rgba32Float is not filterable on standard hardware; the
                    // shader uses textureLoad (integer coords) so filterable
                    // is not needed.
                    ty: wgpu::BindingType::Texture {
                        multisampled: false,
                        view_dimension: wgpu::TextureViewDimension::D2Array,
                        sample_type: wgpu::TextureSampleType::Float { filterable: false },
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::StorageTexture {
                        access: wgpu::StorageTextureAccess::WriteOnly,
                        format: wgpu::TextureFormat::Rgba16Float,
                        view_dimension: wgpu::TextureViewDimension::D2Array,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 3,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: wgpu::BufferSize::new(4),
                    },
                    count: None,
                },
            ],
        });
        let prefilter_pipeline = ComputePipeline::new(
            device,
            include_str!("../../../../assets/shaders/prefilter.wgsl"),
            &[&prefilter_bgl],
            "main",
            Some("Prefilter"),
        );
        // uniform buffer for roughness
        let rough_buf = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("RoughnessUBO"),
            size: 4,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        for mip in 0..mip_count {
            let mip_size = env_size >> mip;
            let roughness = if mip == 0 {
                0.0
            } else {
                (mip as f32) / ((mip_count - 1) as f32)
            };
            queue.write_buffer(&rough_buf, 0, bytemuck::bytes_of(&roughness));

            let view = prefilter_tex.create_view(&wgpu::TextureViewDescriptor {
                label: Some(&format!("PrefilterMip{}", mip)),
                dimension: Some(wgpu::TextureViewDimension::D2Array),
                base_mip_level: mip,
                mip_level_count: Some(1),
                ..Default::default()
            });

            let prefilter_bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some(&format!("PrefilterBG{}", mip)),
                layout: &prefilter_pipeline.inner.get_bind_group_layout(0),
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        // always sample from the base level array view
                        resource: wgpu::BindingResource::TextureView(&env_array_view),
                    },
                    wgpu::BindGroupEntry {
                        binding: 2,
                        resource: wgpu::BindingResource::TextureView(&view),
                    },
                    wgpu::BindGroupEntry {
                        binding: 3,
                        resource: rough_buf.as_entire_binding(),
                    },
                ],
            });

            run_compute(
                prefilter_pipeline.clone(),
                &[&prefilter_bind],
                ((mip_size / 8).max(1), (mip_size / 8).max(1), 6),
            );
        }

        // --- BRDF LUT ---
        let brdf_bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("BRDF Layout"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::COMPUTE,
                ty: wgpu::BindingType::StorageTexture {
                    access: wgpu::StorageTextureAccess::WriteOnly,
                    format: wgpu::TextureFormat::Rgba16Float,
                    view_dimension: wgpu::TextureViewDimension::D2,
                },
                count: None,
            }],
        });
        let brdf_pipeline = ComputePipeline::new(
            device,
            include_str!("../../../../assets/shaders/brdf.wgsl"),
            &[&brdf_bgl],
            "main",
            Some("BRDF_LUT"),
        );
        let brdf_bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("BRDFBindGroup"),
            layout: &brdf_bgl,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::TextureView(&brdf_view),
            }],
        });
        run_compute(
            brdf_pipeline,
            &[&brdf_bind],
            (brdf_size / 8, brdf_size / 8, 1),
        );

        // build rest of environment exactly like new_dummy but with actual textures
        // The irradiance texture was written through a D2Array view; create a Cube
        // view so the PBR shader can sample it as a cube map.
        let irr_cube_view = irr_tex.create_view(&wgpu::TextureViewDescriptor {
            label: Some("IrradianceCubeView"),
            dimension: Some(wgpu::TextureViewDimension::Cube),
            ..Default::default()
        });
        let light_uniform = DirectionalLightUniform::default();
        let light_buffer = crate::resources::buffer::create_uniform(
            device,
            "Directional Light Uniform",
            &light_uniform,
        );

        // point lights
        let initial_pl_capacity: usize = 8;
        let point_light_buffer =
            Arc::new(Self::create_point_light_buffer(device, initial_pl_capacity));
        let zero_header = LightStorageHeader {
            count: 0,
            _pad: [0u32; 3],
        };
        queue.write_buffer(&point_light_buffer, 0, bytemuck::bytes_of(&zero_header));

        // shadow resources same as dummy
        let shadow_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("DummyShadowSampler"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            compare: Some(wgpu::CompareFunction::LessEqual),
            ..Default::default()
        });
        let shadow_desc = wgpu::TextureDescriptor {
            label: Some("DummyShadow"),
            size: wgpu::Extent3d {
                width: 1,
                height: 1,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Depth32Float,
            view_formats: &[],
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        };
        let shadow_tex = device.create_texture(&shadow_desc);
        let shadow_view = shadow_tex.create_view(&wgpu::TextureViewDescriptor::default());

        let bind_group = Arc::new(device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Environment Bind Group"),
            layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: light_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&sampler),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::TextureView(&irr_cube_view),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: wgpu::BindingResource::TextureView(&prefilter_cube_view),
                },
                wgpu::BindGroupEntry {
                    binding: 4,
                    resource: wgpu::BindingResource::TextureView(&brdf_view),
                },
                wgpu::BindGroupEntry {
                    binding: 5,
                    resource: point_light_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 6,
                    resource: wgpu::BindingResource::Sampler(&shadow_sampler),
                },
                wgpu::BindGroupEntry {
                    binding: 7,
                    resource: wgpu::BindingResource::TextureView(&shadow_view),
                },
            ],
        }));

        Ok(Environment {
            bind_group,
            light_uniform,
            light_buffer,
            point_light_buffer,
            point_light_capacity: initial_pl_capacity,
            sampler: Arc::new(sampler),
            irradiance_view: Arc::new(irr_cube_view),
            prefilter_view: Arc::new(prefilter_cube_view),
            brdf_view: Arc::new(brdf_view),
            shadow_sampler: Arc::new(shadow_sampler),
            shadow_view: Arc::new(shadow_view),
        })
    }

    /// Update the directional light data stored in the environment.
    pub fn update_light(&mut self, queue: &Queue, uniform: DirectionalLightUniform) {
        self.light_uniform = uniform;
        queue.write_buffer(
            &self.light_buffer,
            0,
            bytemuck::bytes_of(&self.light_uniform),
        );
    }

    /// Update the shadow resources used by the bind group.
    ///
    /// `shadow` is typically the `ShadowResources` created by the world pass.
    /// After updating the stored handles we rebuild the bind group so the
    /// PBR pipeline can sample the real shadow map.
    pub fn update_shadow(
        &mut self,
        device: &Device,
        layout: &wgpu::BindGroupLayout,
        shadow: &crate::resources::ShadowResources,
    ) {
        self.shadow_sampler = Arc::clone(&shadow.sampler);
        self.shadow_view = Arc::clone(&shadow.view);
        self.rebuild_bind_group(device, layout);
    }

    /// Upload a list of `PointLightUniform` to the GPU storage buffer.
    ///
    /// If the list exceeds the current buffer capacity the buffer is
    /// recreated (doubling the capacity) and the bind group is rebuilt.
    /// The `layout` argument must be the same `BindGroupLayout` used to
    /// create this `Environment`.
    pub fn update_point_lights(
        &mut self,
        device: &Device,
        queue: &Queue,
        layout: &wgpu::BindGroupLayout,
        lights: &[PointLightUniform],
    ) {
        let count = lights.len().min(MAX_POINT_LIGHTS);

        // Resize (recreate) the buffer if it is too small.
        if count > self.point_light_capacity {
            let new_capacity = (count * 2).max(8);
            self.point_light_buffer =
                Arc::new(Self::create_point_light_buffer(device, new_capacity));
            self.point_light_capacity = new_capacity;
            // Rebuild the bind group so it references the new buffer.
            self.rebuild_bind_group(device, layout);
        }

        // Write header (count + padding).
        let header = LightStorageHeader {
            count: count as u32,
            _pad: [0u32; 3],
        };
        queue.write_buffer(&self.point_light_buffer, 0, bytemuck::bytes_of(&header));

        // Write the light array immediately after the 16-byte header.
        if count > 0 {
            queue.write_buffer(
                &self.point_light_buffer,
                16, // sizeof(LightStorageHeader)
                bytemuck::cast_slice(&lights[..count]),
            );
        }
    }

    // ─── Private helpers ─────────────────────────────────────────────────────

    /// Allocates a zero-initialised point-light storage buffer for
    /// `capacity` lights (16-byte header + `capacity * 32` bytes for lights).
    fn create_point_light_buffer(device: &Device, capacity: usize) -> wgpu::Buffer {
        let size = 16 + capacity * std::mem::size_of::<PointLightUniform>();
        device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("PointLight Storage Buffer"),
            size: size.max(32) as u64, // wgpu requires at least one binding unit
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        })
    }

    /// Rebuilds the bind group after the point-light buffer has been replaced.
    fn rebuild_bind_group(&mut self, device: &Device, layout: &wgpu::BindGroupLayout) {
        self.bind_group = Arc::new(device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Environment Bind Group"),
            layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: self.light_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&self.sampler),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::TextureView(&self.irradiance_view),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: wgpu::BindingResource::TextureView(&self.prefilter_view),
                },
                wgpu::BindGroupEntry {
                    binding: 4,
                    resource: wgpu::BindingResource::TextureView(&self.brdf_view),
                },
                wgpu::BindGroupEntry {
                    binding: 5,
                    resource: self.point_light_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 6,
                    resource: wgpu::BindingResource::Sampler(&self.shadow_sampler),
                },
                wgpu::BindGroupEntry {
                    binding: 7,
                    resource: wgpu::BindingResource::TextureView(&self.shadow_view),
                },
            ],
        }));
    }
}
