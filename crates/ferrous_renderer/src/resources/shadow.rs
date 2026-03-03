use std::sync::Arc;

/// Simple container for the shadow map texture + comparison sampler.
///
/// The texture is created with a 2048×2048 `Depth32Float` format and can be
/// bound both as a render attachment (for the shadow pass) and as a
/// sampled texture (for later depth comparisons when applying the shadow map).
pub struct ShadowResources {
    pub texture: Arc<wgpu::Texture>,
    pub view: Arc<wgpu::TextureView>,
    pub sampler: Arc<wgpu::Sampler>,
}

impl ShadowResources {
    pub fn new(device: &wgpu::Device) -> Self {
        let size = wgpu::Extent3d {
            width: 2048,
            height: 2048,
            depth_or_array_layers: 1,
        };

        let desc = wgpu::TextureDescriptor {
            label: Some("Shadow Map"),
            size,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Depth32Float,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        };

        let texture = Arc::new(device.create_texture(&desc));
        let view = Arc::new(texture.create_view(&wgpu::TextureViewDescriptor::default()));

        let sampler = Arc::new(device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("Shadow Map Sampler"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            compare: Some(wgpu::CompareFunction::Less),
            ..Default::default()
        }));

        ShadowResources {
            texture,
            view,
            sampler,
        }
    }
}
