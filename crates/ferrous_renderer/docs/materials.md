# Materials & Textures

This document describes the new modular material system added to the
renderer.  Its implementation lives in `src/materials.rs` so that
`lib.rs` can stay focused on frame orchestration and the render graph.

## Concepts

* **MaterialSlot** – opaque index (`usize`) used by `RenderObject`.
* **Texture** – thin GPU wrapper (`resources::Texture`) created from
  raw bytes.  Textures are owned by the registry; the slot corresponds
  directly to a material so textures and materials are co-located.
* **Material** – encapsulates a base colour and a bind group pointing at a
  texture/sampler.  The slot of a material is its index in the registry.
* **MaterialRegistry** – manages all of the above and exposes creation
  helpers.

## Public API (in `Renderer`)

```rust
/// Create a dynamic texture from raw RGBA8 data and return a
fn register_texture(&mut self, w: u32, h: u32, data: &[u8]) -> TextureHandle;
fn create_texture_from_rgba(&mut self, w: u32, h: u32, data: &[u8]) -> TextureHandle;

/// release a texture slot so that the GPU memory may be reclaimed and the
/// index reused.  built-in fallbacks are immune to this call.
fn free_texture(&mut self, handle: TextureHandle);

/// overwrite the bytes of an existing texture.  used for hot–reloading
/// image assets at runtime; any materials referencing the handle will
/// automatically see the new pixels.
fn update_texture_data(&mut self, handle: TextureHandle, w: u32, h: u32, data: &[u8]);

fn create_material(&mut self, desc: &MaterialDescriptor) -> MaterialHandle;
fn create_material(&mut self, desc: MaterialDescriptor) -> usize;

/// destroy a material slot.  the slot will revert to the default material
/// and its index will be recycled on subsequent creations.
fn free_material(&mut self, handle: MaterialHandle);

/// Change the material of an existing object.
fn set_object_material(&mut self, id: u64, material_slot: usize);

/// Change the material of a world object (by world index).
fn set_world_object_material(&mut self, index: usize, material_slot: usize);
```

All material creation automatically updates the `WorldPass`'s internal
let material = renderer.create_material(&desc);
shaders immediately.

## Shader support

Both `base.wgsl` and `instanced.wgsl` were extended to accept per-vertex
UV coordinates and a group‑2 material binding (uniform + sampler +
texture).  The vertex types now include a `uv: [f32;2]` field and the
cube/quad primitives supply sensible coordinates.

## Example: paint cube face

```rust
use ferrous_renderer::materials::{MaterialDescriptor, AlphaMode};

let texture_handle = renderer.create_texture_from_rgba(6, 1, &[
  /* six horizontal texels */
]);
let mut desc = MaterialDescriptor::default();
desc.albedo_tex = Some(texture_handle);
let material = renderer.create_material(desc);
renderer.set_object_material(obj_id, material);
```

For now the texture is treated as a simple 6×1 strip where each cube face
maps to a distinct region, but the system is flexible enough to support
more complex atlases or PBR workflows going forward.
