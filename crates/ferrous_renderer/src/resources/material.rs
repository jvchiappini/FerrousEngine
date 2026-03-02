use std::sync::Arc;

use wgpu::util::DeviceExt;

/// Simple GPU-backed texture with associated view and sampler.
#[derive(Clone)]
pub struct Texture {
    pub texture: Arc<wgpu::Texture>,
    pub view: Arc<wgpu::TextureView>,
    pub sampler: Arc<wgpu::Sampler>,
}

impl Texture {
    /// Create a texture from raw RGBA8 data.  `data` must be `width*height*4`
    /// bytes long.  The resulting texture is immediately uploaded to the GPU.
    pub fn from_rgba8(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        width: u32,
        height: u32,
        data: &[u8],
    ) -> Self {
        assert_eq!(data.len(), (width * height * 4) as usize);
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("Texture::from_rgba8"),
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
                // wgpu versions vary; historically these fields were NonZero,
                // but the compiler errors indicate they are plain u32.  we
                // supply plain values to remain compatible.
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
            label: Some("Texture sampler"),
            address_mode_u: wgpu::AddressMode::Repeat,
            address_mode_v: wgpu::AddressMode::Repeat,
            address_mode_w: wgpu::AddressMode::Repeat,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::FilterMode::Nearest,
            ..Default::default()
        });

        Self {
            texture: Arc::new(texture),
            view: Arc::new(view),
            sampler: Arc::new(sampler),
        }
    }
}

/// GPU representation of a material.  Stores a uniform buffer and an
/// associated bind group; the latter can be bound directly during rendering.
#[derive(Clone)]
pub struct Material {
    pub bind_group: Arc<wgpu::BindGroup>,
    /// keep the uniform buffer alive
    _buffer: Arc<wgpu::Buffer>,
}

#[repr(C)]
#[derive(Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
struct MaterialUniform {
    base_color: [f32; 4],
    use_texture: u32,
    _pad: [u32; 3],
}

impl Material {
    /// Create a material given a base color and optional texture.  If
    /// `texture` is `None` a fallback white texture must be provided by the
    /// caller (see `Renderer::default_white_texture`).
    pub fn new(
        device: &wgpu::Device,
        layouts: &crate::pipeline::PipelineLayouts,
        _queue: &wgpu::Queue,
        base_color: [f32; 4],
        texture: &Texture,
    ) -> Self {
        let uniform = MaterialUniform {
            base_color,
            use_texture: 1,
            _pad: [0; 3],
        };
        let buf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("MaterialUniform"),
            contents: bytemuck::cast_slice(&[uniform]),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });

        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Material BindGroup"),
            layout: &layouts.material,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: buf.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&texture.sampler),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::TextureView(&texture.view),
                },
            ],
        });

        Self {
            bind_group: Arc::new(bind_group),
            _buffer: Arc::new(buf),
        }
    }
}
