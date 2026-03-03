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
    pub fn update_shadow(&mut self, device: &Device, layout: &wgpu::BindGroupLayout, shadow: &crate::resources::ShadowResources) {
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
