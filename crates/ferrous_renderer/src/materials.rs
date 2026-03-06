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

// ---------------------------------------------------------------------------
// Bindless support (Phase 13)
//
// When the `bindless` feature gate is enabled we will maintain a single GPU
// descriptor set containing an array of textures rather than creating one
// bind group per material.  This stub is intentionally minimal; future
// commits will flesh out the allocation logic and shader bindings.

#[cfg(feature = "bindless")]
#[derive(Clone)]
struct BindlessMaterials {
    // placeholder for the GPU bind group / texture array
    // e.g. `texture_array: wgpu::BindGroup`, `max_slots: u32`, etc.
    bind_group: Arc<wgpu::BindGroup>,
    count: u32,
}

#[cfg(feature = "bindless")]
impl BindlessMaterials {
    fn new(_device: &wgpu::Device, _queue: &wgpu::Queue) -> Self {
        // build a minimal bindless layout.  for the stub we keep **no
        // bindings** so that creating an empty bind group is valid.  a
        // future iteration will flesh out the actual texture-array binding.
        let layout = _device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("bindless_materials_layout"),
            entries: &[],
        });
        let bind_group = _device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("bindless_materials"),
            layout: &layout,
            entries: &[],
        });
        BindlessMaterials { bind_group: Arc::new(bind_group), count: 0 }
    }

    /// Reserve a new slot in the bindless table and return its index.
    fn add_material(&mut self, _device: &wgpu::Device, _queue: &wgpu::Queue, _material: &Material) -> u32 {
        let idx = self.count;
        self.count += 1;
        idx
    }

    fn bind_group(&self) -> Arc<wgpu::BindGroup> {
        self.bind_group.clone()
    }
}

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
    /// Optional bindless bookkeeping (Phase 13 roadmap).
    ///
    /// When the `bindless` feature is enabled we will allocate a single
    /// descriptor set/array that holds all material textures.  For now the
    /// implementation is a stub that simply falls back to the regular
    /// per-material bind groups; later phases will populate and consume
    /// this field.
    #[cfg(feature = "bindless")]
    bindless: Option<BindlessMaterials>,
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
            #[cfg(feature = "bindless")]
            bindless: Some(BindlessMaterials::new(device, queue)),
        }
    }

    /// Convenience wrapper around [`TextureRegistry::register_rgba8`].
    /// Returns a handle that may later be used in a material descriptor.
    /// Use this for **color** data (albedo, emissive) — the GPU will apply
    /// gamma correction automatically.
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

    /// Convenience wrapper around [`TextureRegistry::register_rgba8_linear`].
    /// Returns a handle that may later be used in a material descriptor.
    /// Use this for **non-color** data (normal maps, metallic-roughness, AO)
    /// where no gamma correction should be applied.
    pub fn register_texture_rgba8_linear(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        width: u32,
        height: u32,
        data: &[u8],
    ) -> TextureHandle {
        self.tex_registry
            .register_rgba8_linear(device, queue, width, height, data)
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
        #[cfg(feature = "bindless")]
        {
            // allocate slot in bindless table and append material for flag
            let handle = MaterialHandle(self.bindless.as_mut().unwrap().add_material(device, queue, &material));
            self.materials.push(material);
            handle
        }

        #[cfg(not(feature = "bindless"))]
        {
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
        #[cfg(feature = "bindless")]
        {
            vec![self.bindless.as_ref().unwrap().bind_group()]
        }
        #[cfg(not(feature = "bindless"))]
        {
            self.materials
                .iter()
                .map(|m| m.bind_group.clone())
                .collect()
        }
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

// simple smoke tests that exercise the public API, compiled with or without
// the `bindless` feature.  wgpu requires a real device/queue so we create
// one via `pollster`.
#[cfg(test)]
mod tests {
    use super::*;
    use crate::pipeline::PipelineLayouts;
    use pollster;

    #[test]
    fn registry_roundtrip() {
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
            backends: wgpu::Backends::all(),
            ..Default::default()
        });
        let adapter = pollster::block_on(
            instance.request_adapter(&wgpu::RequestAdapterOptions::default()),
        )
        .expect("adapter");
        let (device, queue) = pollster::block_on(
            adapter.request_device(&wgpu::DeviceDescriptor::default(), None),
        )
        .expect("device");
        let layouts = PipelineLayouts::new(&device);
        let mut reg = MaterialRegistry::new(&device, &queue, &layouts);
        let table = reg.bind_group_table();
        assert!(!table.is_empty());
    }
}
