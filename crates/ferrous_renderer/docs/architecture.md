<!--
Detailed description of the ferrous_renderer module tree, internal data
flow, and the FramePacket lifecycle.  Read this before writing any
renderer extension.
-->

# Architecture

This document describes how the crate is structured, how data flows
through a single frame, and why the design choices were made.

## Module tree

```
ferrous_renderer/src/
├── lib.rs                    thin orchestrator; declares modules, defines Renderer
├── context.rs                re-exports EngineContext from ferrous_core
│
├── resources/                low-level GPU allocation helpers
│   ├── mod.rs
│   ├── buffer.rs             create_uniform / create_vertex / create_index / update_uniform
│   ├── model_buffer.rs       ModelBuffer – dynamic-uniform buffer (legacy/manual objects)
│   ├── instance_buffer.rs    InstanceBuffer – storage buffer for instanced World entities
│   └── texture.rs            create_render_texture / default_view / RenderTextureDesc
│
├── geometry/                 CPU + GPU geometry types
│   ├── mod.rs
│   ├── vertex.rs             Vertex { position: [f32;3], color: [f32;3] }
│   ├── mesh.rs               Mesh – Arc-wrapped vertex + index buffers
│   └── primitives/
│       ├── mod.rs
│       └── cube.rs           24-vertex, 36-index coloured cube
│
├── camera/                   view and projection management
│   ├── mod.rs                re-exports GpuCamera, OrbitState
│   ├── uniform.rs            GpuCamera – buffer + bind_group + sync()
│   └── controller.rs         OrbitState – yaw/pitch accumulator, reads Controller
│
├── pipeline/                 wgpu render pipeline construction
│   ├── mod.rs
│   ├── layout.rs             PipelineLayouts – camera BGL (group 0) + model BGL + instance BGL (group 1)
│   ├── world.rs              WorldPipeline – compiles assets/shaders/base.wgsl
│   ├── instancing.rs         InstancingPipeline – compiles assets/shaders/instanced.wgsl
│   ├── gizmo.rs              GizmoPipeline – LineList, depth_compare: Always, no depth write
│   └── compute.rs            ComputePipeline – generic wrapper for wgpu compute pipelines
│
├── render_target/            colour + depth targets with MSAA support
│   ├── mod.rs
│   ├── color.rs              ColorTarget – resolve texture + optional MSAA texture
│   ├── depth.rs              DepthTarget – Depth32Float, sample_count-aware
│   └── target.rs             RenderTarget – composed target, resize(), accessors
│
├── scene/                    bridge between ferrous_core::World and GPU objects
│   ├── mod.rs
│   ├── object.rs             RenderObject – mesh + matrix + aabb + slot
│   ├── gizmo.rs              GizmoDraw – transform + mode + highlights + GizmoStyle clone
│   └── world_sync.rs         sync_world() free function
│
├── graph/                    render-graph abstractions
│   ├── mod.rs
│   ├── pass_trait.rs         RenderPass trait – the primary extension point
│   └── frame_packet.rs       FramePacket, DrawCommand, InstancedDrawCommand, CameraPacket, Viewport
│
└── passes/                   built-in pass implementations
    ├── mod.rs
    ├── world_pass.rs         WorldPass – instanced path + legacy path
    ├── ui_pass.rs            UiPass – composites ferrous_gui output
    └── compute_pass.rs       ComputePass – generic compute shader dispatch via RenderPass

assets/shaders/
├── base.wgsl               per-object model matrix via dynamic uniform (group 1)
├── instanced.wgsl          instanced: reads instances[instance_index] from storage buffer
├── gizmo.wgsl              coloured line segments; only group 0 (camera) needed
├── gui.wgsl                2D quad rendering
└── text.wgsl               glyph / SDF text rendering
```

## Bind-group layout conventions

The crate uses two fixed bind-group slots for 3-D rendering:

| Group | Contents | Pipeline | Frequency |
|-------|----------|----------|-----------|
| 0 | Camera uniform (`CameraUniform` — view-projection matrix) | both | once per frame |
| 1 | Model uniform (4×4 transform, **dynamic offset**) | `WorldPipeline` / legacy path | once per object |
| 1 | Instance storage buffer (`array<mat4x4<f32>>`) | `InstancingPipeline` | once per mesh group |

**Instanced path** (`WorldPipeline` → `InstancingPipeline`):
All World entities that share the same vertex buffer are grouped into
one `InstancedDrawCommand`.  Their matrices are written contiguously
into `InstanceBuffer` and the shader reads `instances[instance_index]`.
Result: **1 `draw_indexed` call per unique mesh**, regardless of count.

**Legacy path** (`WorldPipeline`):
Manually-spawned objects (via `renderer.add_object(...)`) still use the
dynamic-uniform `ModelBuffer`.  One `draw_indexed` per object.

Custom pipelines must respect group 0 (camera) to be compatible with
`PipelineLayouts`.  See `extending/new_pipeline.md` for details.

## Frame lifecycle

A complete frame proceeds in four stages.

### Stage 1 — Input and world update (application code)

```
Renderer::handle_input(input, dt)
    └── OrbitState::update(camera, input, dt)
            reads: camera.controller.speed
                   camera.controller.mouse_sensitivity
                   camera.controller.orbit_distance
                   camera.controller.direction(input)   ← key bindings

Renderer::sync_world(world, ctx)
    └── scene::sync_world(world, objects, device, queue, model_layout)
            spawns / removes / updates RenderObjects
            InstanceBuffer::write_slice(queue, base, &[Mat4])
                writes contiguous matrix slice for World entities
```

### Stage 2 — Packet construction

`Renderer::build_base_packet` translates the live Rust types into a plain
`FramePacket`.  No GPU work happens here — only Arc clones, matrix
copies, and grouping by mesh.

World entities are grouped by vertex-buffer pointer.  Each group writes
its matrices into `InstanceBuffer` and emits one `InstancedDrawCommand`.
Manually-spawned objects are still emitted as individual `DrawCommand`s.

```rust
pub struct FramePacket {
    pub viewport:          Option<Viewport>,
    pub camera:            CameraPacket,
    /// Legacy per-object draw calls (manually-spawned, dynamic-uniform path).
    pub scene_objects:     Vec<DrawCommand>,
    /// Instanced draw calls for World entities — one per unique mesh.
    pub instanced_objects: Vec<InstancedDrawCommand>,
    /// Objects which request double‑sided rendering (culling disabled)
    /// carry a flag in both command types; the `WorldPass` uses that flag
    /// to pick a pipeline variant with `cull_mode = None`.  Instanced groups
    /// are split on the flag so mixed batches never occur.
    // ... open-ended extras map
}
```

`InstancedDrawCommand` carries `first_instance` and `instance_count`.
`WorldPass` emits `draw_indexed(0..N, 0, first..first+count)` for each.

### Stage 3 — Prepare

Each `RenderPass::prepare` is called in registration order.  Passes
upload any data they need from the packet:

```rust
fn prepare(
    &mut self,
    device:  &wgpu::Device,
    queue:   &wgpu::Queue,
    packet:  &FramePacket,
)
```

`WorldPass` uses this to call `GpuCamera::sync`, writing the current
view-projection matrix to the GPU camera uniform buffer.

`UiPass` uploads the `GuiBatch` (if any) through `GuiRenderer::prepare`.

### Stage 4 — Execute

A single `CommandEncoder` is created.  Each pass calls `execute` in turn
to record its render pass into the encoder:

```rust
fn execute(
    &self,
    device:        &wgpu::Device,
    queue:         &wgpu::Queue,
    encoder:       &mut wgpu::CommandEncoder,
    color_view:    &wgpu::TextureView,
    resolve_target: Option<&wgpu::TextureView>,
    depth_view:    &wgpu::TextureView,
    packet:        &FramePacket,
)
```

After all passes have recorded, the encoder is finished and submitted to
`queue.submit`.  For MSAA targets `color_view` is the multi-sample
attachment and `resolve_target` is the single-sample resolve texture;
for non-MSAA targets `resolve_target` is `None` and `color_view` is the
final destination.

## Data-flow diagram

```
Application
    │
    ├─ handle_input ──► OrbitState ──► Camera (yaw/pitch/matrices)
    │
    ├─ sync_world   ──► RenderObject per Element  (Arc<Buffer> mesh + matrix)
    │
    └─ render_to_view / render_to_target
            │
            ▼
       build_base_packet
            │  group World entities by vertex_buffer pointer
            │  write matrices → InstanceBuffer (queue.write_buffer)
            │  emit InstancedDrawCommand per unique mesh
            │  emit DrawCommand per manual/legacy object
            ▼
       FramePacket { instanced_objects, scene_objects, camera, … }
            │
            ▼
       for pass in passes:
           pass.prepare(device, queue, &packet)  ←── uploads uniforms
            │
            ▼
       CommandEncoder::new
       for pass in passes:
           pass.execute(encoder, views, &packet)
            │   WorldPass ──► instanced path:
            │                   bind InstancingPipeline
            │                   bind InstanceBuffer at group 1
            │                   draw_indexed(0..N, 0, first..first+count)  ← 1 call per mesh
            │              ──► legacy path:
            │                   bind WorldPipeline
            │                   for each DrawCommand: dynamic offset → draw_indexed
            │   execute_gizmo_pass (inline in Renderer):
            │                   build CPU vertex buffer from gizmo_draws
            │                   shafts + arrowheads (style.show_arrows)
            │                   plane squares (style.show_planes)
            │                   bind GizmoPipeline (LineList, depth: Always)
            │                   draw(0..vertex_count)
            │                   gizmo_draws.clear()
            │
            ▼
       queue.submit(encoder.finish())
```

## Compute passes

`ComputePass` implements the standard `RenderPass` trait but opens a
`wgpu::ComputePass` inside `execute` instead of a render pass.  This
means compute workloads slot into the same ordered graph as rasterisation
passes with no special handling required.

Typical use-cases:

- **Raymarching / SDF rendering** — dispatch a fullscreen compute shader
  that writes directly to a storage texture.
- **Particle simulation** — update positions and velocities on the GPU
  every frame before the world pass reads them.
- **Voxel data generation** — build a density field on the GPU and hand
  the buffer to a mesh extraction pass.

Because compute shaders do not use the camera/model bind-group slots,
you supply your own `BindGroupLayout`s when constructing
`ComputePipeline`.  Dispatch dimensions are configured at construction
time and can be changed dynamically via `set_workgroup_count`.

See `extending/compute_pipeline.md` for a step-by-step worked example.

## Design rationale

**Why `FramePacket`?**
Separating data gathering from GPU recording keeps passes stateless and
composable.  A pass only sees the packet; it does not reach back into
`Renderer` internals.  This also makes it easy to serialise or replay a
frame for debugging.

**Why Arc everywhere?**
Meshes and bind-groups are shared between the logical scene
(`RenderObject`) and the `DrawCommand` in the packet without copying.
The Arc overhead is negligible compared to GPU round-trips.

**Why two stages (prepare + execute)?**
`prepare` can safely write to the queue (staging buffer uploads) before
the command encoder is opened.  Once `execute` begins no further queue
writes should occur until submission.  This mirrors the two-phase
contract used by `wgpu`'s own examples and avoids borrow conflicts.

**Why does `ComputePass` implement `RenderPass`?**
Unifying raster and compute passes under a single trait keeps the graph
orderable and extensible without a second registration mechanism.  A
compute pass simply ignores the colour/depth view arguments in `execute`
and opens a `wgpu::ComputePass` on the same encoder.
