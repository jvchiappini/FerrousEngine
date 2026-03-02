//! Basic texture asset helpers.  This module provides a thin wrapper around
//! [`wgpu::Texture`] to simplify loading images from disk or from raw bytes.

use anyhow::Result;
use std::path::Path;

/// 2-D GPU texture with view and sampler.
pub struct Texture2d {
    pub texture: wgpu::Texture,
    pub view: wgpu::TextureView,
    pub sampler: wgpu::Sampler,
}

impl Texture2d {
    /// Create a texture from raw RGBA8 data.  Caller must ensure `data` has
    /// exactly `width * height * 4` bytes.
    pub fn from_rgba8(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        width: u32,
        height: u32,
        data: &[u8],
    ) -> Self {
        assert_eq!(data.len(), (width * height * 4) as usize);
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("Texture2d"),
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8UnormSrgb,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        queue.write_texture(
            wgpu::ImageCopyTexture {
                texture: &texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            data,
            wgpu::ImageDataLayout {
                offset: 0,
                bytes_per_row: Some(4 * width),
                rows_per_image: Some(height),
            },
            wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
        );
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("Texture2d sampler"),
            address_mode_u: wgpu::AddressMode::Repeat,
            address_mode_v: wgpu::AddressMode::Repeat,
            address_mode_w: wgpu::AddressMode::Repeat,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::FilterMode::Nearest,
            ..Default::default()
        });
        Texture2d { texture, view, sampler }
    }

    /// Load an image file from disk (any format supported by the `image`
    /// crate) and upload it to a GPU texture.
    pub fn from_file<P: AsRef<Path>>(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        path: P,
    ) -> Result<Self> {
        let img = image::open(path)?;
        let rgba = img.to_rgba8();
        let (width, height) = rgba.dimensions();
        Ok(Self::from_rgba8(device, queue, width, height, &rgba))
    }
}
