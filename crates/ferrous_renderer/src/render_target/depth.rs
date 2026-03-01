/// A depth-stencil texture.
///
/// The sample count **must** match the color attachment's sample count;
/// otherwise the GPU validation layer will reject the render pass.
use crate::resources::texture::{self, RenderTextureDesc};

pub struct DepthTarget {
    pub texture: wgpu::Texture,
    pub view: wgpu::TextureView,
    pub sample_count: u32,
}

impl DepthTarget {
    pub const FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Depth32Float;

    pub fn new(device: &wgpu::Device, width: u32, height: u32, sample_count: u32) -> Self {
        let (texture, view) = Self::make(device, width, height, sample_count);
        Self { texture, view, sample_count }
    }

    pub fn resize(&mut self, device: &wgpu::Device, width: u32, height: u32) {
        let (t, v) = Self::make(device, width, height, self.sample_count);
        self.texture = t;
        self.view    = v;
    }

    fn make(
        device: &wgpu::Device,
        width: u32,
        height: u32,
        sample_count: u32,
    ) -> (wgpu::Texture, wgpu::TextureView) {
        let tex = texture::create_render_texture(device, &RenderTextureDesc {
            label: "Depth Texture",
            width,
            height,
            format: Self::FORMAT,
            sample_count,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
        });
        let view = texture::default_view(&tex);
        (tex, view)
    }
}
