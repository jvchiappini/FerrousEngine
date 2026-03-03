//! Material/texture registry for the renderer.
//!
//! This module encapsulates all logic related to creating GPU textures and
//! materials (base color + optional texture), keeping `lib.rs` slim and
//! focused on frame orchestration.  The registry exposes a simple slot-based
//! API that the rest of the renderer (and user code) can call.

use std::sync::Arc;

use crate::pipeline::PipelineLayouts;
use crate::resources::material::{
    MaterialUniformPbr, ALBEDO_TEX, AO_TEX, EMISSIVE_TEX, FLAG_ALPHA_MASK, MET_ROUGH_TEX,
    NORMAL_TEX,
};
use crate::resources::{Material, TextureHandle, TextureRegistry};

// material primitives (handle, descriptor, alpha mode) are defined in
// `ferrous_core` so that the core crate can carry them without depending on
// the renderer.  the renderer re-exports the same items from its own
// `lib.rs` so clients can continue to import from either crate.
use ferrous_core::scene::{AlphaMode, MaterialDescriptor, MaterialHandle, MATERIAL_DEFAULT};


/// Manages materials, texture handles and the resulting bind groups.  All
/// materials are built from a [`MaterialDescriptor`] and may refer to
/// textures by handle; the registry owns the underlying GPU textures so
/// resources stay alive for the lifetime of the registry.
#[derive(Clone)]
pub struct MaterialRegistry {
    /// copy of the pipeline layouts that are required for material bind
    /// groups.  stored here so the renderer itself doesn't need to hold a
    /// second copy.
    layouts: PipelineLayouts,

    /// texture registry used for all material slots.  frequently we'll need
    /// to resolve handles -> `Texture` during bind group creation.
    tex_registry: TextureRegistry,

    /// actual material objects; index corresponds to the handle value.
    materials: Vec<Material>,
    /// free list of material slots that have been explicitly released via
    /// [`MaterialRegistry::free`].  values are raw u32 indices.
    free_slots: Vec<u32>,
}

impl MaterialRegistry {
    /// Create a new registry, cloning the provided pipeline layouts and
    /// populating both the texture and material vectors with the required
    /// default entries.  After this call the `MATERIAL_DEFAULT` handle
    /// (0) is guaranteed to be valid and corresponds to a neutral PBR
    /// material using the white fallback texture.
    pub fn new(device: &wgpu::Device, queue: &wgpu::Queue, layouts: &PipelineLayouts) -> Self {
        let layouts = layouts.clone();

        // create texture registry with its three mandatory fallbacks
        let tex_registry = TextureRegistry::new(device, queue);

        // create a default material from a descriptor and push it into the
        // list so handle 0 is valid.
        let default_desc = MaterialDescriptor::default();
        let default_material =
            Material::from_descriptor(device, queue, &layouts, &default_desc, &tex_registry);

        let mut materials = Vec::new();
        materials.push(default_material);

        Self {
            layouts,
            tex_registry,
            materials,
            free_slots: Vec::new(),
        }
    }

    /// Convenience wrapper around [`TextureRegistry::register_rgba8`].
    /// Returns a handle that may later be used in a material descriptor.
    pub fn register_texture_rgba8(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        width: u32,
        height: u32,
        data: &[u8],
    ) -> TextureHandle {
        self.tex_registry
            .register_rgba8(device, queue, width, height, data)
    }

    /// Delegate to the underlying texture registry to free a texture slot.
    pub fn free_texture(&mut self, handle: TextureHandle) {
        self.tex_registry.free(handle);
    }

    /// Delegate to texture registry for hot-reload updates.
    pub fn update_texture_data(
        &mut self,
        queue: &wgpu::Queue,
        handle: TextureHandle,
        width: u32,
        height: u32,
        data: &[u8],
    ) {
        self.tex_registry
            .update_texture_data(queue, handle, width, height, data);
    }

    /// Allocate a new material from the provided descriptor.  The descriptor
    /// may reference texture handles previously obtained with
    /// [`register_texture_rgba8`].
    ///
    /// Returns a [`MaterialHandle`] that can be passed back to the renderer
    /// when object geometry is submitted.
    pub fn create(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        desc: &MaterialDescriptor,
    ) -> MaterialHandle {
        let material =
            Material::from_descriptor(device, queue, &self.layouts, desc, &self.tex_registry);
        if let Some(slot) = self.free_slots.pop() {
            let idx = slot as usize;
            // overwrite the placeholder default material previously written
            // by `free`.
            self.materials[idx] = material;
            MaterialHandle(slot)
        } else {
            let idx = self.materials.len() as u32;
            self.materials.push(material);
            MaterialHandle(idx)
        }
    }

    /// Update the scalar parameters/flags of an existing material.  The
    /// bind group itself is *not* rebuilt (textures are assumed to remain
    /// the same); only the uniform buffer is rewritten.  This allows clients
    /// to cheaply tweak colours or metallic/roughness values without
    /// reallocating GPU resources.
    pub fn update_params(
        &mut self,
        queue: &wgpu::Queue,
        handle: MaterialHandle,
        desc: &MaterialDescriptor,
    ) {
        let mat = &mut self.materials[handle.0 as usize];
        // pack uniform exactly as in `from_descriptor` so that the size
        // stays stable and shaders continue to match.
        let mut uniform = MaterialUniformPbr::default();
        uniform.base_color = desc.base_color;
        uniform.emissive = [
            desc.emissive[0],
            desc.emissive[1],
            desc.emissive[2],
            desc.emissive_strength,
        ];
        uniform.metallic_roughness = [desc.metallic, desc.roughness, desc.ao_strength, 0.0];
        uniform.normal_ao = [desc.normal_scale, 0.0, 0.0, 0.0];

        let mut flags = 0;
        if desc.albedo_tex.is_some() {
            flags |= ALBEDO_TEX;
        }
        if desc.normal_tex.is_some() {
            flags |= NORMAL_TEX;
        }
        if desc.metallic_roughness_tex.is_some() {
            flags |= MET_ROUGH_TEX;
        }
        if desc.emissive_tex.is_some() {
            flags |= EMISSIVE_TEX;
        }
        if desc.ao_tex.is_some() {
            flags |= AO_TEX;
        }
        // mask handling
        let mut alpha_cutoff = 0.0;
        if let AlphaMode::Mask { cutoff } = desc.alpha_mode {
            flags |= FLAG_ALPHA_MASK;
            alpha_cutoff = cutoff;
        }
        uniform.flags = flags;
        uniform.alpha_cutoff = alpha_cutoff;
        // (rest of uniform already written above)

        queue.write_buffer(&mat.buffer, 0, bytemuck::cast_slice(&[uniform]));
        mat.alpha_mode = desc.alpha_mode.clone();
        mat.double_sided = desc.double_sided;
    }

    /// Returns a vector of all bind groups in slot order.  Used by passes to
    /// refresh their local copy of the material table.
    pub fn bind_group_table(&self) -> Vec<Arc<wgpu::BindGroup>> {
        self.materials
            .iter()
            .map(|m| m.bind_group.clone())
            .collect()
    }

    /// Retrieve the rendering flags associated with a material.  These are
    /// stored in the [`Material`] itself so that the renderer can decide
    /// which pipeline variant to use.
    pub fn get_render_flags(&self, handle: MaterialHandle) -> (&AlphaMode, bool) {
        let mat = &self.materials[handle.0 as usize];
        (&mat.alpha_mode, mat.double_sided)
    }
}

impl MaterialRegistry {
    /// Free a material slot previously returned by [`create`].  the slot is
    /// replaced with a clone of the default material (slot 0) so that any
    /// stray draws referencing the old handle will simply sample a white
    /// material rather than triggering a GPU error.  freed indices are
    /// pushed onto the free list and may be reused by subsequent
    /// [`create`] calls.
    pub fn free(&mut self, handle: MaterialHandle) {
        if handle == MATERIAL_DEFAULT {
            return;
        }
        let idx = handle.0 as usize;
        if idx < self.materials.len() {
            // copy default material into this slot
            self.materials[idx] = self.materials[0].clone();
            self.free_slots.push(handle.0);
        }
    }
}
