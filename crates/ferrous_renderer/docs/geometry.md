<!--
Reference for the geometry subsystem: Vertex, Mesh, built-in primitives,
and how to supply custom geometry to the renderer.
-->

# Geometry

The `geometry` module provides the `Vertex` and `Mesh` types that the
renderer uses for all 3-D drawing.  A small set of built-in primitives
is included; you can also construct meshes from arbitrary vertex and
index data.

## `Vertex`

`Vertex` is the only vertex format understood by the built-in world
pipeline.

```rust
#[repr(C)]
#[derive(Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
pub struct Vertex {
    pub position: [f32; 3],
    pub color:    [f32; 3],
}
```

- **`position`** — world-space XYZ coordinates.
- **`color`** — linear-light RGB; alpha is not stored per-vertex (the
  pipeline's blend state is opaque).

`Vertex::layout()` returns a `wgpu::VertexBufferLayout` describing this
format with `stepMode: Vertex` and two attributes at shader locations
0 and 1.  Pass it when building a pipeline:

```rust
let layout = Vertex::layout();
// use in wgpu::RenderPipelineDescriptor::vertex.buffers
```

## `Mesh`

`Mesh` wraps a pair of GPU buffers behind `Arc` so that multiple
`RenderObject`s can share the same geometry without copying.

```rust
pub struct Mesh {
    pub vertex_buffer: Arc<wgpu::Buffer>,
    pub index_buffer:  Arc<wgpu::Buffer>,
    pub index_count:   u32,
    pub index_format:  wgpu::IndexFormat,   // Uint16 or Uint32
}
```

`Mesh` is `Clone` — cloning is cheap because it only increments the
reference counts.

### Construction

Use the helper functions in `resources::buffer` to allocate the GPU
buffers, then compose a `Mesh`:

```rust
use ferrous_renderer::resources::buffer::{create_vertex, create_index};
use ferrous_renderer::geometry::{Vertex, Mesh};

let vertices: Vec<Vertex> = vec![
    Vertex { position: [ 0.0,  0.5, 0.0], color: [1.0, 0.0, 0.0] },
    Vertex { position: [-0.5, -0.5, 0.0], color: [0.0, 1.0, 0.0] },
    Vertex { position: [ 0.5, -0.5, 0.0], color: [0.0, 0.0, 1.0] },
];
let indices: Vec<u16> = vec![0, 1, 2];

let mesh = Mesh {
    vertex_buffer: Arc::new(create_vertex(&ctx.device, &vertices)),
    index_buffer:  Arc::new(create_index(&ctx.device, &indices)),
    index_count:   indices.len() as u32,
    index_format:  wgpu::IndexFormat::Uint16,
};
```

Use `IndexFormat::Uint32` for meshes that exceed 65 535 vertices.

## Built-in primitives

### Cube

`geometry::primitives::cube::create_cube(device)` returns a `Mesh`
representing a unit cube centred at the origin.

- **24 vertices** — 4 per face, with per-face flat colours.
- **36 indices** — two triangles per face, `Uint16`.
- Face colours: red, green, blue, yellow, cyan, magenta.

```rust
use ferrous_renderer::geometry::primitives::cube::create_cube;

let cube_mesh = create_cube(&ctx.device);
```

## Using meshes with `RenderObject`

A `RenderObject` pairs a `Mesh` with a per-instance model transform
buffer and bind-group.  Create one explicitly or let `sync_world` manage
the lifecycle automatically (see `extending/world_sync.md`).

```rust
use ferrous_renderer::scene::object::RenderObject;

let obj = RenderObject::new(
    &ctx.device,
    mesh,
    &pipeline_layouts.model,
    glam::Mat4::IDENTITY,
);
```

`RenderObject::update_transform(queue, matrix)` writes a new transform
without reallocating the buffer.

## Shader vertex input

The world shader (`assets/shaders/base.wgsl`) expects the following
input layout, which matches `Vertex::layout()`:

```wgsl
struct VertexInput {
    @location(0) position: vec3<f32>,
    @location(1) color:    vec3<f32>,
};
```

Any custom pipeline drawing `Mesh` geometry must use the same attribute
locations.

## Adding a new primitive

1. Create a file under `geometry/primitives/my_shape.rs`.
2. Fill a `Vec<Vertex>` and a `Vec<u16>` (or `Vec<u32>`).
3. Call `create_vertex` / `create_index` to allocate the buffers.
4. Return a `Mesh`.
5. `pub use` from `geometry/primitives/mod.rs`.

See `cube.rs` as a reference implementation — it is intentionally short
and easy to follow.
