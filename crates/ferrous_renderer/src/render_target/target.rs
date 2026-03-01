/// A complete render target composed of a [`ColorTarget`] and a [`DepthTarget`].
///
/// Rendering to a texture (viewport widget, off-screen effects) uses this
/// struct instead of the raw swapchain surface view.
use super::{color::ColorTarget, depth::DepthTarget};

pub struct RenderTarget {
    pub color: ColorTarget,
    pub depth: DepthTarget,
    pub width: u32,
    pub height: u32,
}

impl RenderTarget {
    pub fn new(
        device: &wgpu::Device,
        width: u32,
        height: u32,
        format: wgpu::TextureFormat,
        sample_count: u32,
    ) -> Self {
        Self {
            color: ColorTarget::new(device, width, height, format, sample_count),
            depth: DepthTarget::new(device, width, height, sample_count),
            width,
            height,
        }
    }

    /// Recreates all attachments when the resolution changes.
    ///
    /// Returns early (no GPU allocation) if the dimensions are identical.
    pub fn resize(&mut self, device: &wgpu::Device, new_width: u32, new_height: u32) {
        if new_width == self.width && new_height == self.height {
            return;
        }
        self.width  = new_width;
        self.height = new_height;
        self.color.resize(device, new_width, new_height);
        self.depth.resize(device, new_width, new_height);
    }

    /// Convenience accessor for the sample count (read from the color target).
    #[inline]
    pub fn sample_count(&self) -> u32 {
        self.color.sample_count
    }

    /// Returns the (render_view, resolve_target) pair for color attachments.
    #[inline]
    pub fn color_views(&self) -> (&wgpu::TextureView, Option<&wgpu::TextureView>) {
        self.color.attachment_views()
    }

    /// Returns the depth/stencil view.
    #[inline]
    pub fn depth_view(&self) -> &wgpu::TextureView {
        &self.depth.view
    }

    /// The single-sample resolve texture (used as a `TEXTURE_BINDING`, e.g.
    /// for displaying the viewport in an editor).
    #[inline]
    pub fn color_texture(&self) -> &wgpu::Texture {
        &self.color.texture
    }

    /// The single-sample resolve view.
    #[inline]
    pub fn color_view(&self) -> &wgpu::TextureView {
        &self.color.view
    }
}
