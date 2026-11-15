# Textured triangle meshes (2D)

`Renderer::draw_textured_mesh_2d` (and the same call on `RenderContext` and
`headless::Pure2dHarness`) draws an arbitrary indexed triangle list sampled
from one registered sprite texture.  It is the generic primitive behind
deformed images and video: the engine knows nothing about lattices, bones or
image nodes, it receives vertices and indices.

```rust
let vertices = [
    MeshVertex2d::new([-0.5, -0.5], [0.0, 0.0]),
    MeshVertex2d::new([ 0.5, -0.5], [1.0, 0.0]),
    MeshVertex2d::new([-0.5,  0.5], [0.0, 1.0]),
    MeshVertex2d::new([ 0.5,  0.5], [1.0, 1.0]),
];
let indices = [0, 1, 2, 2, 1, 3];
renderer.draw_textured_mesh_2d(
    texture_id,
    &vertices,
    &indices,
    Mat4::from_scale_rotation_translation(Vec3::new(320.0, 220.0, 1.0), Quat::IDENTITY, Vec3::ZERO),
    ferrous_2d::render::BlendMode2d::Normal,
);
```

## Contract

- **Texture ids** are the same registry sprites use (`register_texture_2d`,
  `register_sprite_texture`, `_linear`, `_premultiplied`).  In-place updates
  (`update_sprite_texture`) and mipmaps are visible on the next frame.
- **Vertices** carry a local position (`[x, y, z]`, usually `z = 0`), a UV and
  an alpha.  The draw transform maps them to world space; its z translation is
  the painter-order depth, exactly like a sprite's transform.  Positions in
  `[-0.5, 0.5]` and UVs in `[0, 1]` match a sprite of the same box pixel for
  pixel (the documented tolerance is a few units on the shared triangle edge
  because the two triangulations cut the quad along different diagonals).
- **Indices** are a `u32` `TriangleList`.  An empty mesh, an index count that
  is not a multiple of three, or an out-of-range index is rejected (the call
  returns `false`) and nothing is drawn.  Degenerate triangles are accepted and
  rasterise nothing; nothing panics.
- **Alpha** is premultiplied on output like every Pure2D pipeline.  Straight
  texels are premultiplied in the shader; textures registered as premultiplied
  only take the vertex coverage, matching the sprite shader.
- **Painter order** is the draw's transform z.  Meshes interleave with shapes
  and sprites in the same order, work inside layers and masked layers, and obey
  the per-draw `BlendMode2d`.

## Batching and buffers

Meshes that share a texture and blend mode and are adjacent in painter order
collapse into one indexed draw call: the renderer appends their vertices to one
upload and rebases their indices.  The GPU vertex and index buffers are reused
across frames and only grow (to the next power of two), so a steady frame
performs no allocation.

## Example and tests

`cargo run -p ferrous_renderer --example textured_mesh_2d` writes a flat sheet
next to a bent one to `brain/textured_mesh_2d.png`.
`tests/pure2d_meshes.rs` covers sprite equivalence, a bent-mesh golden, UV
cropping, in-place texture updates, layers, masks, batching and the
empty/degenerate cases.
