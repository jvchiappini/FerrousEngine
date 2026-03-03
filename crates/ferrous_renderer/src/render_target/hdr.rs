/// HDR off-screen render target.
///
/// Wraps a single `Rgba16Float` texture that is used as the intermediate
/// render destination for the world pass.  Values in this texture may exceed
/// 1.0, which is exactly what we want for physically-correct lighting.
///
/// The post-process pass reads this texture via the supplied `sampler` and
/// applies ACES tone mapping + gamma correction before writing to the
/// swapchain surface.
use crate::resources::texture::{self, RenderTextureDesc};

pub struct HdrTexture {
    pub texture: wgpu::Texture,
    pub view: wgpu::TextureView,
    pub sampler: wgpu::Sampler,
    pub width: u32,
    pub height: u32,
}

impl HdrTexture {
    pub const FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba16Float;

    pub fn new(device: &wgpu::Device, width: u32, height: u32) -> Self {
        let texture = texture::create_render_texture(
            device,
            &RenderTextureDesc {
                label: "HDR Texture",
                width,
                height,
                format: Self::FORMAT,
                sample_count: 1,
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT
                    | wgpu::TextureUsages::TEXTURE_BINDING,
            },
        );
        let view = texture::default_view(&texture);

        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("HDR Sampler"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::FilterMode::Nearest,
            ..Default::default()
        });

        Self {
            texture,
            view,
            sampler,
            width,
            height,
        }
    }

    /// Recreates the texture when the window is resized.  No-op if dimensions
    /// are unchanged.
    pub fn resize(&mut self, device: &wgpu::Device, width: u32, height: u32) {
        if self.width == width && self.height == height {
            return;
        }
        *self = Self::new(device, width, height);
    }
}
