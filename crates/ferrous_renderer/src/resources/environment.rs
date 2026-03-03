use std::sync::Arc;

use wgpu::{Device, Queue};

use crate::resources::light::DirectionalLightUniform;

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
            ],
        }));

        Environment {
            bind_group,
            light_uniform,
            light_buffer,
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
}
