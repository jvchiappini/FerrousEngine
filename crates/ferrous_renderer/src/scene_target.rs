//! A generic off-screen target for the 2D renderer.
//!
//! [`SceneTarget2d`] is the reusable colour target an application draws a 2D
//! scene into and then reads back (screenshots, thumbnails, editor viewports,
//! golden comparisons). It wraps the headless [`OffscreenTarget`], which owns
//! the texture, the persistent frame staging buffer and the 1x1 pixel staging
//! buffer, so repeated reads create no GPU resources.
//!
//! The target is format-agnostic at creation time, but the read helpers accept
//! the two 8-bit RGBA formats [`OffscreenTarget`] supports
//! (`Rgba8Unorm` / `Rgba8UnormSrgb`). [`SceneTarget2d::DEFAULT_FORMAT`] matches
//! the native swapchain so pixels match what a window presents.
//!
//! ```no_run
//! # use ferrous_renderer::{Renderer, SceneTarget2d};
//! # use ferrous_renderer::headless::DEFAULT_FORMAT;
//! # use ferrous_2d::render::ShapeInstance;
//! # use glam::Vec2;
//! # fn run(renderer: &mut Renderer, device: &wgpu::Device, queue: &wgpu::Queue) {
//! let mut target = SceneTarget2d::new(device, 640, 480, DEFAULT_FORMAT);
//! target.render_with(renderer, device, queue, |renderer| {
//!     renderer.draw_2d_shape(ShapeInstance::rect(
//!         Vec2::ZERO,
//!         Vec2::splat(64.0),
//!         [1.0, 0.0, 0.0, 1.0],
//!     ));
//! });
//! let mut pixels = Vec::new();
//! target.read_rgba8_into(device, queue, &mut pixels).unwrap();
//! # }
//! ```

use crate::headless::{OffscreenTarget, ReadbackError};
use crate::renderer_core::Renderer;

/// An off-screen RGBA8 colour target for the 2D renderer.
pub struct SceneTarget2d {
    target: OffscreenTarget,
}

impl SceneTarget2d {
    /// The sRGB swapchain-compatible default format.
    pub const DEFAULT_FORMAT: wgpu::TextureFormat = crate::headless::DEFAULT_FORMAT;

    /// Creates the target and its persistent staging buffers. `width` and
    /// `height` are clamped to at least one texel.
    #[must_use]
    pub fn new(
        device: &wgpu::Device,
        width: u32,
        height: u32,
        format: wgpu::TextureFormat,
    ) -> Self {
        Self {
            target: OffscreenTarget::new(device, width.max(1), height.max(1), format),
        }
    }

    /// Target width in texels.
    #[must_use]
    pub fn width(&self) -> u32 {
        self.target.width
    }

    /// Target height in texels.
    #[must_use]
    pub fn height(&self) -> u32 {
        self.target.height
    }

    /// Target texture format.
    #[must_use]
    pub fn format(&self) -> wgpu::TextureFormat {
        self.target.format
    }

    /// The rendered texture.
    #[must_use]
    pub fn texture(&self) -> &wgpu::Texture {
        &self.target.texture
    }

    /// The default view of [`Self::texture`].
    #[must_use]
    pub fn view(&self) -> &wgpu::TextureView {
        &self.target.view
    }

    /// Resizes the target. A same-size call is a no-op (no GPU allocation).
    pub fn resize(&mut self, device: &wgpu::Device, width: u32, height: u32) {
        let width = width.max(1);
        let height = height.max(1);
        if width == self.target.width && height == self.target.height {
            return;
        }
        self.target = OffscreenTarget::new(device, width, height, self.target.format);
    }

    /// Renders the 2D command stream already queued on `renderer` into the
    /// target and submits it.
    ///
    /// Call [`Renderer::clear_2d`] before queueing the frame (or use
    /// [`Self::render_with`], which does it for you).
    pub fn render(&mut self, renderer: &mut Renderer, device: &wgpu::Device, queue: &wgpu::Queue) {
        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("2D Scene Target Encoder"),
        });
        renderer.render_to_texture_view(
            &mut encoder,
            &self.target.texture,
            &self.target.view,
            None,
        );
        queue.submit(Some(encoder.finish()));
    }

    /// Clears the 2D batchers, runs `draw`, then renders and submits.
    pub fn render_with(
        &mut self,
        renderer: &mut Renderer,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        draw: impl FnOnce(&mut Renderer),
    ) {
        renderer.clear_2d();
        draw(renderer);
        self.render(renderer, device, queue);
    }

    /// Copies the target into the persistent staging buffer and writes tightly
    /// packed RGBA8 bytes into `out` (reusing its allocation).
    pub fn read_rgba8_into(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        out: &mut Vec<u8>,
    ) -> Result<(), ReadbackError> {
        self.target.read_rgba8_into(device, queue, out)
    }

    /// Reads one texel as `[r, g, b, a]`.
    pub fn read_pixel_into(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        x: u32,
        y: u32,
    ) -> Result<[u8; 4], ReadbackError> {
        self.target.read_pixel_into(device, queue, x, y)
    }
}
