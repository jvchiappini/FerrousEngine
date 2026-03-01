/// Helpers for creating `wgpu::Texture` objects with common descriptor
/// patterns, reducing boilerplate inside the render-target and asset modules.

/// Descriptor for a 2-D render-attachment texture.
pub struct RenderTextureDesc<'a> {
    pub label: &'a str,
    pub width: u32,
    pub height: u32,
    pub format: wgpu::TextureFormat,
    /// MSAA sample count (1 = no MSAA).
    pub sample_count: u32,
    pub usage: wgpu::TextureUsages,
}

/// Creates a 2-D texture from a [`RenderTextureDesc`].
pub fn create_render_texture(device: &wgpu::Device, desc: &RenderTextureDesc<'_>) -> wgpu::Texture {
    device.create_texture(&wgpu::TextureDescriptor {
        label: Some(desc.label),
        size: wgpu::Extent3d {
            width: desc.width,
            height: desc.height,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: desc.sample_count,
        dimension: wgpu::TextureDimension::D2,
        format: desc.format,
        usage: desc.usage,
        view_formats: &[],
    })
}

/// Creates a default `TextureView` for a texture (all mips, all layers).
#[inline]
pub fn default_view(texture: &wgpu::Texture) -> wgpu::TextureView {
    texture.create_view(&wgpu::TextureViewDescriptor::default())
}
