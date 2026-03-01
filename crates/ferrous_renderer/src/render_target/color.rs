/// Manages a single-sample color texture and an optional MSAA texture that
/// resolves into it.
///
/// The **resolve target** (single-sample) is always created even when MSAA is
/// active because it is used as a `TEXTURE_BINDING` (e.g. the viewport widget
/// shows this texture in the editor).
use crate::resources::texture::{self, RenderTextureDesc};

pub struct ColorTarget {
    /// Single-sample "presentable" texture — also the MSAA resolve target.
    pub texture: wgpu::Texture,
    pub view: wgpu::TextureView,
    /// Multisampled texture (present only when `sample_count > 1`).
    pub msaa_texture: Option<wgpu::Texture>,
    pub msaa_view: Option<wgpu::TextureView>,
    pub format: wgpu::TextureFormat,
    pub sample_count: u32,
}

impl ColorTarget {
    pub fn new(
        device: &wgpu::Device,
        width: u32,
        height: u32,
        format: wgpu::TextureFormat,
        sample_count: u32,
    ) -> Self {
        let (texture, view) = Self::make_resolve(device, width, height, format);
        let (msaa_texture, msaa_view) = Self::make_msaa(device, width, height, format, sample_count);

        Self { texture, view, msaa_texture, msaa_view, format, sample_count }
    }

    pub fn resize(&mut self, device: &wgpu::Device, width: u32, height: u32) {
        let (t, v) = Self::make_resolve(device, width, height, self.format);
        self.texture = t;
        self.view    = v;

        let (mt, mv) = Self::make_msaa(device, width, height, self.format, self.sample_count);
        self.msaa_texture = mt;
        self.msaa_view    = mv;
    }

    /// Returns `(render_view, resolve_target)` — the correct pair to pass to
    /// a `RenderPassColorAttachment`.
    pub fn attachment_views(&self) -> (&wgpu::TextureView, Option<&wgpu::TextureView>) {
        if self.sample_count > 1 {
            (self.msaa_view.as_ref().unwrap(), Some(&self.view))
        } else {
            (&self.view, None)
        }
    }

    // ── Private helpers ───────────────────────────────────────────────────

    fn make_resolve(
        device: &wgpu::Device,
        width: u32,
        height: u32,
        format: wgpu::TextureFormat,
    ) -> (wgpu::Texture, wgpu::TextureView) {
        let tex = texture::create_render_texture(device, &RenderTextureDesc {
            label: "Color Resolve Texture",
            width,
            height,
            format,
            sample_count: 1,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT
                | wgpu::TextureUsages::TEXTURE_BINDING
                | wgpu::TextureUsages::COPY_SRC,
        });
        let view = texture::default_view(&tex);
        (tex, view)
    }

    fn make_msaa(
        device: &wgpu::Device,
        width: u32,
        height: u32,
        format: wgpu::TextureFormat,
        sample_count: u32,
    ) -> (Option<wgpu::Texture>, Option<wgpu::TextureView>) {
        if sample_count <= 1 {
            return (None, None);
        }
        let tex = texture::create_render_texture(device, &RenderTextureDesc {
            label: "Color MSAA Texture",
            width,
            height,
            format,
            sample_count,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
        });
        let view = texture::default_view(&tex);
        (Some(tex), Some(view))
    }
}
