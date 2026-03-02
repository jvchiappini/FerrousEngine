//! Material/texture registry for the renderer.
//!
//! This module encapsulates all logic related to creating GPU textures and
//! materials (base color + optional texture), keeping `lib.rs` slim and
//! focused on frame orchestration.  The registry exposes a simple slot-based
//! API that the rest of the renderer (and user code) can call.

use std::sync::Arc;

use crate::pipeline::PipelineLayouts;
use crate::resources::{Material, Texture};
use wgpu::util::DeviceExt;

/// Handle to a material slot.
pub type MaterialSlot = usize;

/// Manages a list of materials and their associated GPU resources.
pub struct MaterialRegistry {
    /// Uniform copy of the pipeline layouts required when creating new
    /// material bind groups.  Stored here so we don't need to keep layouts
    /// in `Renderer` itself.
    layouts: PipelineLayouts,

    /// Default 1×1 white texture.  Always lives at slot 0 when the registry
    /// is first created; clients can update it if necessary but it's the
    /// fallback for any material that doesn't specify a texture.
    default_texture: Texture,

    /// Vector of materials; index = slot.
    materials: Vec<Material>,
}

impl MaterialRegistry {
    /// Create a new registry, allocating the default white texture/material
    /// immediately so that slot 0 is always valid.
    pub fn new(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        layouts: &PipelineLayouts,
    ) -> Self {
        // store a clone of layouts for later use
        let layouts = layouts.clone();

        // build 1×1 white texture
        let white_data = [255u8, 255, 255, 255];
        let default_texture = Texture::from_rgba8(device, queue, 1, 1, &white_data);

        // create mutable vec and insert default material
        let mut materials = Vec::new();
        let default_material = Material::new(device, &layouts, queue, [1.0, 1.0, 1.0, 1.0], &default_texture);
        materials.push(default_material);

        Self {
            layouts,
            default_texture,
            materials,
        }
    }

    /// Returns a vector of all bind groups in slot order.  Used by passes to
    /// refresh their local copy of the material table.
    pub fn bind_group_table(&self) -> Vec<Arc<wgpu::BindGroup>> {
        self.materials.iter().map(|m| m.bind_group.clone()).collect()
    }

    /// Create a new texture from raw RGBA8 bytes and register an empty
    /// material that uses it.  The returned slot is where the material
    /// resides; to update color use [`create_material`] separately.
    pub fn create_texture_from_rgba(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        width: u32,
        height: u32,
        data: &[u8],
    ) -> MaterialSlot {
        let tex = Texture::from_rgba8(device, queue, width, height, data);
        let slot = self.materials.len();
        let material = Material::new(device, &self.layouts, queue, [1.0, 1.0, 1.0, 1.0], &tex);
        self.materials.push(material);
        slot
    }

    /// Create a material with a base colour and optional texture slot.
    /// If `texture_slot` is `None` the default white texture is used.
    pub fn create_material(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        base_color: [f32; 4],
        _texture_slot: Option<MaterialSlot>,
    ) -> MaterialSlot {
        // currently we always bind the default texture; texture_slot is
        // ignored.  a future enhancement can resolve the texture from the
        // given slot and clone its view/sampler.
        let slot = self.materials.len();
        let material = Material::new(device, &self.layouts, queue, base_color, &self.default_texture);
        self.materials.push(material);
        slot
    }
}
