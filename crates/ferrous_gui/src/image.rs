use crate::constraint::Constraint;
use crate::{RenderCommand, Widget};

/// A simple widget that displays a bitmap or icon.
///
/// This widget only exists when the `assets` feature is enabled because it
/// stores a reference to a [`ferrous_assets::Texture2d`].  The texture is
/// expected to be uploaded and managed by the application; the GUI merely
/// keeps an `Arc` handle so that lifetimes are easy.
#[cfg(feature = "assets")]
pub struct Image {
    pub rect: [f32; 4],
    pub texture: std::sync::Arc<ferrous_assets::Texture2d>,
    /// sub-rectangle within the texture, in normalized coordinates
    pub uv0: [f32; 2],
    pub uv1: [f32; 2],
    /// tint colour applied to the sampled pixels (default white)
    pub color: [f32; 4],
    pub constraint: Option<Constraint>,
}

#[cfg(feature = "assets")]
impl Image {
    pub fn new(
        rect: [f32; 4],
        texture: std::sync::Arc<ferrous_assets::Texture2d>,
    ) -> Self {
        Self {
            rect,
            texture,
            uv0: [0.0, 0.0],
            uv1: [1.0, 1.0],
            color: [1.0, 1.0, 1.0, 1.0],
            constraint: None,
        }
    }

    pub fn with_uv(mut self, uv0: [f32; 2], uv1: [f32; 2]) -> Self {
        self.uv0 = uv0;
        self.uv1 = uv1;
        self
    }

    pub fn with_color(mut self, color: [f32; 4]) -> Self {
        self.color = color;
        self
    }

    pub fn with_constraint(mut self, c: Constraint) -> Self {
        self.constraint = Some(c);
        self
    }

    /// Shortcut constructor that rasterizes an SVG file into a texture using
    /// the supplied device/queue.  This requires that the `ferrous_assets`
    /// crate is built with its `svg` feature enabled; the resulting texture
    /// will be uploaded at the specified dimensions.
    #[cfg(feature = "svg")]
    pub fn from_svg_file<P: AsRef<std::path::Path>>(
        rect: [f32; 4],
        path: P,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        width: u32,
        height: u32,
    ) -> anyhow::Result<Self> {
        let tex = std::sync::Arc::new(
            ferrous_assets::Texture2d::from_svg_file(device, queue, path, width, height)?,
        );
        Ok(Self {
            rect,
            texture: tex,
            uv0: [0.0, 0.0],
            uv1: [1.0, 1.0],
            color: [1.0, 1.0, 1.0, 1.0],
            constraint: None,
        })
    }
}

#[cfg(feature = "assets")]
impl Widget for Image {
    fn collect(&self, cmds: &mut Vec<RenderCommand>) {
        cmds.push(RenderCommand::Image {
            rect: crate::layout::Rect {
                x: self.rect[0],
                y: self.rect[1],
                width: self.rect[2],
                height: self.rect[3],
            },
            texture: self.texture.clone(),
            uv0: self.uv0,
            uv1: self.uv1,
            color: self.color,
        });
    }

    fn bounding_rect(&self) -> Option<[f32; 4]> {
        Some(self.rect)
    }

    fn apply_constraint(&mut self, container_w: f32, container_h: f32) {
        if let Some(c) = &self.constraint {
            c.apply_to_rect(&mut self.rect, container_w, container_h);
        }
    }
}
