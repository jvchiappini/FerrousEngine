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
        self.width = new_width;
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

impl RenderTarget {
    /// Creates a [`LazyRenderTarget`] for a swapchain-sized destination.
    ///
    /// No GPU texture is created here; the caller must call
    /// [`LazyRenderTarget::ensure_color_depth`] before rendering into it.  The
    /// eager [`RenderTarget::new`] remains available for callers that need the
    /// attachments immediately.
    #[must_use]
    pub fn lazy(width: u32, height: u32) -> LazyRenderTarget {
        LazyRenderTarget::new(width, height)
    }
}

/// A swapchain-sized render target whose color and depth attachments are
/// allocated on first use.
///
/// A `Pure2D` application that renders straight to the surface never samples
/// its own [`RenderTarget`]; allocating a full-size color resolve + depth pair
/// at construction costs VRAM (at 1600x900, a 4-byte color resolve plus a
/// `Depth32Float` depth is 11.52 MB) even when the internal target is never
/// rendered to.  This wrapper holds only the logical size until
/// [`Self::ensure_color_depth`] is called, so the texture memory follows actual
/// use.
///
/// The allocations match [`RenderTarget::new`] byte for byte, so switching to
/// the lazy wrapper does not change pixels, formats, sample counts or the
/// memory inventory labels once the target is materialized.
pub struct LazyRenderTarget {
    /// Logical width the target will be created at.
    pub width: u32,
    /// Logical height the target will be created at.
    pub height: u32,
    color: Option<ColorTarget>,
    depth: Option<DepthTarget>,
}

impl LazyRenderTarget {
    /// Creates a lazy target.  Allocates no GPU memory.
    #[must_use]
    pub fn new(width: u32, height: u32) -> Self {
        Self {
            width,
            height,
            color: None,
            depth: None,
        }
    }

    /// Creates the color resolve and depth attachments if they are missing or
    /// no longer match `(width, height, format, sample_count)`.
    ///
    /// Returns `true` when it allocated (or reallocated) the pair.  Repeated
    /// calls with the same parameters are a no-op, so a frame path can call it
    /// unconditionally without recreating textures.  The `sample_count` must
    /// match between color and depth (wgpu requires it for the render pass).
    pub fn ensure_color_depth(
        &mut self,
        device: &wgpu::Device,
        width: u32,
        height: u32,
        format: wgpu::TextureFormat,
        sample_count: u32,
    ) -> bool {
        let matches = match (&self.color, &self.depth) {
            (Some(color), Some(depth)) => {
                self.width == width
                    && self.height == height
                    && color.format == format
                    && color.sample_count == sample_count
                    && depth.sample_count == sample_count
            }
            _ => false,
        };
        if matches {
            return false;
        }
        self.width = width;
        self.height = height;
        self.color = Some(ColorTarget::new(
            device,
            width,
            height,
            format,
            sample_count,
        ));
        self.depth = Some(DepthTarget::new(device, width, height, sample_count));
        true
    }

    /// Whether both attachments are currently allocated.
    #[inline]
    #[must_use]
    pub fn is_allocated(&self) -> bool {
        self.color.is_some() && self.depth.is_some()
    }

    /// The allocated color attachment, if any.
    #[inline]
    #[must_use]
    pub fn color(&self) -> Option<&ColorTarget> {
        self.color.as_ref()
    }

    /// The allocated depth attachment, if any.
    #[inline]
    #[must_use]
    pub fn depth(&self) -> Option<&DepthTarget> {
        self.depth.as_ref()
    }

    /// The color attachment's `(render_view, resolve_target)` pair.
    #[inline]
    #[must_use]
    pub fn color_views(&self) -> Option<(&wgpu::TextureView, Option<&wgpu::TextureView>)> {
        self.color.as_ref().map(ColorTarget::attachment_views)
    }

    /// The color resolve view, if allocated.
    #[inline]
    #[must_use]
    pub fn color_view(&self) -> Option<&wgpu::TextureView> {
        self.color.as_ref().map(|color| &color.view)
    }

    /// The color resolve texture, if allocated.
    #[inline]
    #[must_use]
    pub fn color_texture(&self) -> Option<&wgpu::Texture> {
        self.color.as_ref().map(|color| &color.texture)
    }

    /// The depth view, if allocated.
    #[inline]
    #[must_use]
    pub fn depth_view(&self) -> Option<&wgpu::TextureView> {
        self.depth.as_ref().map(|depth| &depth.view)
    }

    /// Updates the logical size, reallocating only when attachments already
    /// exist and the size actually changed.
    ///
    /// A target that has not been materialized simply records the new size; the
    /// next [`Self::ensure_color_depth`] allocates at that size.
    pub fn resize(&mut self, device: &wgpu::Device, width: u32, height: u32) {
        if width == self.width && height == self.height {
            return;
        }
        self.width = width;
        self.height = height;
        if let Some(color) = self.color.as_mut() {
            color.resize(device, width, height);
        }
        if let Some(depth) = self.depth.as_mut() {
            depth.resize(device, width, height);
        }
    }

    /// Drops both attachments, returning the target to the unallocated state.
    pub fn release(&mut self) {
        self.color = None;
        self.depth = None;
    }
}
