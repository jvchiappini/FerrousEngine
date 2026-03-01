<!--
Guide to hooking new ferrous_core scene element types into the
renderer's sync_world mechanism.
-->

# Extending `sync_world`

`sync_world` is the bridge between the logical scene managed by
`ferrous_core::scene::World` and the GPU-side `RenderObject` list owned
by `Renderer`.  This document explains how it works and how to extend it
when you add a new kind of `Element` to `ferrous_core`.

## How sync_world works today

The free function lives in `scene/world_sync.rs`:

```rust
pub fn sync_world(
    world:        &World,
    objects:      &mut Vec<RenderObject>,
    device:       &wgpu::Device,
    queue:        &wgpu::Queue,
    model_layout: &wgpu::BindGroupLayout,
)
```

It performs a three-step reconciliation:

1. **Removals** — any `RenderObject` whose ID is no longer present in
   `world.elements` is dropped (and its GPU buffers freed).
2. **Additions** — any element in `world.elements` that has no
   corresponding `RenderObject` spawns one:
   - A model uniform buffer is allocated.
   - A bind-group (group 1) is created from `model_layout`.
   - The mesh is cloned (cheap — Arc clone) from the element's mesh
     handle.
3. **Updates** — every surviving object writes its current transform to
   the GPU via `RenderObject::update_transform(queue, matrix)`.

`Renderer::sync_world` calls this function each frame (or whenever the
scene is dirty) and stores the result in `self.objects`.

## `RenderObject` structure

```rust
pub struct RenderObject {
    pub id:               u64,
    pub mesh:             Mesh,
    pub model_buffer:     Arc<wgpu::Buffer>,
    pub model_bind_group: Arc<wgpu::BindGroup>,
}
```

- **`id`** — matches the `Element::id` from `ferrous_core`.  Used for
  reconciliation.
- **`mesh`** — shared Arc; no GPU copy.
- **`model_buffer`** — `UNIFORM | COPY_DST`, holds a 4×4 f32 matrix.
- **`model_bind_group`** — bound to group 1 in the world pipeline shader.

## Adding support for a new element type

### Case 1 — New element with a `Mesh` already attached

If your new element type attaches a `Mesh` (the same `ferrous_renderer::Mesh`
type) to the `Element`, `sync_world` already handles it — no changes
required.  Ensure your element's mesh is set before the next
`sync_world` call.

### Case 2 — New element with a different geometry representation

Suppose you add a `SpriteElement` that stores a 2-D quad defined by
`[f32; 4]` rather than a `Mesh`.  You need to:

1. **Generate a `Mesh` at sync time** — convert the sprite rect into a
   `Mesh` (two triangles) inside `sync_world` before creating the
   `RenderObject`.

2. **Cache the generated mesh** — to avoid reallocating GPU buffers
   every frame, maintain a `HashMap<u64, Mesh>` (keyed by element ID)
   alongside `objects`.

Here is a sketch of the extended `sync_world`:

```rust
pub fn sync_world_extended(
    world:        &World,
    objects:      &mut Vec<RenderObject>,
    mesh_cache:   &mut HashMap<u64, Mesh>,
    device:       &wgpu::Device,
    queue:        &wgpu::Queue,
    model_layout: &wgpu::BindGroupLayout,
) {
    // Remove stale entries
    let live_ids: HashSet<u64> = world.elements.keys().copied().collect();
    objects.retain(|o| live_ids.contains(&o.id));
    mesh_cache.retain(|id, _| live_ids.contains(id));

    for (id, element) in &world.elements {
        match element.kind {
            ElementKind::Mesh(ref mesh) => {
                // existing path
                if !objects.iter().any(|o| o.id == *id) {
                    objects.push(RenderObject::new(device, mesh.clone(), model_layout, element.transform));
                } else {
                    if let Some(obj) = objects.iter_mut().find(|o| o.id == *id) {
                        obj.update_transform(queue, element.transform);
                    }
                }
            }
            ElementKind::Sprite(ref sprite) => {
                let mesh = mesh_cache.entry(*id)
                    .or_insert_with(|| build_sprite_mesh(device, sprite));
                if !objects.iter().any(|o| o.id == *id) {
                    objects.push(RenderObject::new(device, mesh.clone(), model_layout, element.transform));
                } else {
                    if let Some(obj) = objects.iter_mut().find(|o| o.id == *id) {
                        obj.update_transform(queue, element.transform);
                    }
                }
            }
        }
    }
}
```

### Case 3 — New element rendered by a completely different pass

If your new element should not go through `WorldPass` at all (e.g. a
particle system rendered by a dedicated `ParticlePass`), do **not** add
it to the `objects` list.  Instead:

1. Maintain a separate GPU data structure (e.g. `Vec<ParticleRenderData>`)
   alongside `Renderer::objects`.
2. Add the data to `FramePacket` — either by extending the struct itself
   or by carrying it through a `Box<dyn Any>` side-channel on the packet.
3. Consume it in `ParticlePass::prepare` / `execute`.

## Modifying `FramePacket` for new data

If your new pass needs data that is not in `FramePacket` today, extend
it in `graph/frame_packet.rs`:

```rust
pub struct FramePacket {
    // existing fields …
    pub particle_data: Vec<ParticleDrawCommand>,   // new
}
```

Then populate it in `Renderer::build_packet` and consume it in your
custom pass.  No other code needs to change.

## Thread safety note

`sync_world` is called synchronously on the main thread.  All
`RenderObject` buffers are created on the `wgpu::Device` which is
`Send + Sync`, but the actual uploads happen via `queue.write_buffer`
which must be called from the thread that owns the queue.  Do not move
`sync_world` work onto a background thread unless you use a staging
buffer strategy.

## Checklist

- [ ] New `ElementKind` variant defined in `ferrous_core`
- [ ] `sync_world` (or your extended version) handles the new variant
- [ ] Mesh generation is cached if it is expensive
- [ ] `FramePacket` extended if the new element needs a dedicated pass
- [ ] `cargo check --workspace` passes
