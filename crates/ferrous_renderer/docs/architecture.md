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
│   ├── layout.rs             PipelineLayouts – camera BGL (group 0) + model BGL (group 1)
│   └── world.rs              WorldPipeline – compiles assets/shaders/base.wgsl
│
├── render_target/            colour + depth targets with MSAA support
│   ├── mod.rs
│   ├── color.rs              ColorTarget – resolve texture + optional MSAA texture
│   ├── depth.rs              DepthTarget – Depth32Float, sample_count-aware
│   └── target.rs             RenderTarget – composed target, resize(), accessors
│
├── scene/                    bridge between ferrous_core::World and GPU objects
│   ├── mod.rs
│   ├── object.rs             RenderObject – mesh + model_buffer + model_bind_group
│   └── world_sync.rs         sync_world() free function
│
├── graph/                    render-graph abstractions
│   ├── mod.rs
│   ├── pass_trait.rs         RenderPass trait – the primary extension point
│   └── frame_packet.rs       FramePacket, DrawCommand, CameraPacket, Viewport
│
└── passes/                   built-in pass implementations
    ├── mod.rs
    ├── world_pass.rs         WorldPass – clears + draws scene_objects
    └── ui_pass.rs            UiPass – composites ferrous_gui output
```

## Bind-group layout conventions

The crate uses two fixed bind-group slots for 3-D rendering:

| Group | Contents | Frequency |
|-------|----------|-----------|
| 0 | Camera uniform (`CameraUniform` — view-projection matrix) | once per frame |
| 1 | Model uniform (4×4 transform matrix for one instance) | once per object |

Custom pipelines must respect these slots to be compatible with
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
```

### Stage 2 — Packet construction

`Renderer::build_packet` translates the live Rust types into a plain
`FramePacket`.  No GPU work happens here — only Arc clones and matrix
copies.

```rust
pub struct FramePacket {
    pub viewport:      Option<Viewport>,
    pub camera:        CameraPacket,
    pub scene_objects: Vec<DrawCommand>,
    pub ui_batch:      Option<GuiBatch>,
    pub text_batch:    Option<TextBatch>,
}
```

`CameraPacket` carries a snapshot of the view-projection matrix and the
`Arc<BindGroup>` that was already uploaded in the previous `sync` step.
`DrawCommand` carries Arc pointers to the vertex buffer, index buffer,
and model bind-group for one object.

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
    ├─ sync_world   ──► RenderObject per Element  (Arc<Buffer> mesh + model)
    │
    └─ render_to_view / render_to_target
            │
            ▼
       build_packet ──► FramePacket (pure CPU snapshot)
            │
            ▼
       for pass in passes:
           pass.prepare(device, queue, &packet)  ←── uploads uniforms
            │
            ▼
       CommandEncoder::new
       for pass in passes:
           pass.execute(encoder, views, &packet) ←── records draw calls
            │
            ▼
       queue.submit(encoder.finish())
```

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
