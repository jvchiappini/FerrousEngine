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
/// Create a dynamic texture and reserve a material slot for it.
/// Returns the slot index.
fn create_texture_from_rgba(&mut self, w: u32, h: u32, data: &[u8]) -> usize;

/// Create a material with a given base colour.  The optional texture slot
/// parameter is currently ignored (always uses the default white texture).
fn create_material(&mut self, base_color: [f32;4], texture_slot: Option<usize>) -> usize;

/// Change the material of an existing object.
fn set_object_material(&mut self, id: u64, material_slot: usize);

/// Change the material of a world object (by world index).
fn set_world_object_material(&mut self, index: usize, material_slot: usize);
```

All material creation automatically updates the `WorldPass`'s internal
bind-group table so that newly registered slots are available to the
shaders immediately.

## Shader support

Both `base.wgsl` and `instanced.wgsl` were extended to accept per-vertex
UV coordinates and a group‑2 material binding (uniform + sampler +
texture).  The vertex types now include a `uv: [f32;2]` field and the
cube/quad primitives supply sensible coordinates.

## Example: paint cube face

```rust
let texture_slot = renderer.create_texture_from_rgba(6, 1, &[
    /* six horizontal texels */
]);
let material = renderer.create_material([1.0,1.0,1.0,1.0], Some(texture_slot));
renderer.set_object_material(obj_id, material);
```

For now the texture is treated as a simple 6×1 strip where each cube face
maps to a distinct region, but the system is flexible enough to support
more complex atlases or PBR workflows going forward.
