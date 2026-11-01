<!--
Documentation home for the `ferrous_renderer` crate.
This folder contains reference material, design notes, and worked
examples for every public subsystem.  Start here, then follow the links
to whichever area you need.
-->

# ferrous_renderer documentation

`ferrous_renderer` is the GPU rendering layer of FerrousEngine.  It
wraps `wgpu` behind a high-level, modular interface that can drive both
a game window **and** render to an off-screen texture simultaneously.
The crate is intentionally decoupled from windowing: it receives an
`EngineContext` (device + queue) and a target surface and handles
everything else internally.

## Documentation map

```
docs/
├── README.md              # this file – overview and quick start
├── architecture.md        # module tree, data-flow, FramePacket lifecycle
├── render_pass.md         # RenderPass trait – the primary extension point
├── camera.md              # GpuCamera, OrbitState, Controller configuration
├── geometry.md            # Vertex, Mesh, built-in primitives, custom geometry
├── materials.md           # material & texture system (slots, registry, shaders)
├── render_target.md       # off-screen rendering, MSAA, pixel readback
├── gizmo.md               # GizmoDraw, GizmoPipeline, execute_gizmo_pass(), GizmoStyle
└── extending/
    ├── custom_pass.md         # step-by-step guide to writing a custom raster pass
    ├── new_pipeline.md        # adding a new wgpu render pipeline
    ├── compute_pipeline.md    # adding a compute shader pass (raymarching, particles, …)
    └── world_sync.md          # hooking new scene element types into sync_world
```
    └── world_sync.md          # hooking new scene element types into sync_world
```

## Quick start

Add the crate to your workspace:

```toml
[dependencies]
ferrous_renderer = { path = "../ferrous_renderer" }
```

Create a `Renderer` in your application setup and drive it each frame:

```rust
use ferrous_renderer::{Renderer, FramePacket, Viewport};

// --- setup (once) ---
let mut renderer = Renderer::new(&ctx, surface_width, surface_height);

// Optional: upload a font atlas for text rendering
renderer.set_font_atlas(&ctx, &atlas_bytes, atlas_width, atlas_height);

// --- per-frame ---
let output = surface.get_current_texture()?;
let view   = output.texture.create_view(&Default::default());

renderer.handle_input(&input_state, delta_time);
renderer.sync_world(&world, &ctx);
renderer.render_to_view(&ctx, &view);

output.present();
```

## Architecture at a glance

The renderer is structured around a **render graph** consisting of
ordered passes.  Each frame follows this contract:

1. `build_packet` — converts the live scene into a `FramePacket` (pure
   CPU data).
2. `prepare` — every active `RenderPass` uploads GPU data it needs
   (uniforms, staging buffers, …).
3. `execute` — a `CommandEncoder` is recorded; each pass appends its
   draw calls.
4. `queue.submit` — the finished command buffer is submitted.

The default pass list is:

| Order | Pass | Responsibility |
|-------|------|----------------|
| 0 | `WorldPass` | clears the frame, draws 3-D scene objects |
| 1 | `UiPass` | composites the GUI and text layers on top |

Compute workloads are added manually via `Renderer::add_pass` using
`ComputePass`.  Register them before `WorldPass` when their output is
consumed by raster passes in the same frame.

You can replace or extend this list with `Renderer::add_pass` /
`Renderer::clear_passes`.  See `render_pass.md` for full details.

## Key public types

| Type | Where defined | Purpose |
|------|---------------|--------|
| `Renderer` | `lib.rs` | top-level entry point |
| `RenderPass` | `graph/pass_trait.rs` | trait for custom passes (raster and compute) |
| `FramePacket` | `graph/frame_packet.rs` | per-frame CPU data bundle |
| `Viewport` | `graph/frame_packet.rs` | scissor/viewport rectangle |
| `Mesh` | `geometry/mesh.rs` | GPU vertex + index buffers |
| `Vertex` | `geometry/vertex.rs` | interleaved position + colour |
| `Camera` | *(re-export from ferrous_core)* | view + projection state |
| `Controller` | *(re-export from ferrous_core)* | key bindings + motion config |
| `GpuCamera` | `camera/uniform.rs` | GPU-side camera uniform |
| `RenderTarget` | `render_target/target.rs` | colour + depth target, MSAA-aware |
| `ComputePipeline` | `pipeline/compute.rs` | generic wgpu compute pipeline wrapper |
| `ComputePass` | `passes/compute_pass.rs` | compute shader dispatch via the render graph |
| `RenderObject` | `scene/object.rs` | per-instance GPU data |
| `GizmoDraw` | `scene/gizmo.rs` | per-frame gizmo draw descriptor (transform + style) |
| `GizmoPipeline` | `pipeline/gizmo.rs` | LineList pipeline, depth Always, no depth write |
| `GizmoStyle` | *(re-export from ferrous_core)* | full visual config for a gizmo instance |

## Feature highlights

- **Render to texture** — `Renderer::render_to_target` writes into the
  internal `RenderTarget` instead of a swap-chain view.  The resolved
  single-sample texture is accessible via
  `renderer.render_target().color_texture()` for use as a `GuiQuad`
  background or for CPU readback.
- **MSAA x4** — depth and colour targets are created with four samples
  by default; a resolve pass writes the final image to a single-sample
  texture.
- **Configurable camera** — speed, mouse sensitivity, orbit distance,
  and arbitrary key bindings are all runtime-configurable.  See
  `camera.md` for details.
- **Custom render passes** — implement the `RenderPass` trait to inject
  any GPU work (shadow maps, post-processing, outlines, …) into the
  frame.  See `extending/custom_pass.md` for a worked example.

## Further reading

- `architecture.md` — understand the complete data flow before writing
  any extensions.
- `render_pass.md` — full trait reference and contract.
- `camera.md` — configure movement speed, key mappings, and sensitivity.
- `geometry.md` — create meshes from code or load them from assets.
- `render_target.md` — off-screen rendering and MSAA details.
- `gizmo.md` — GizmoDraw, GizmoPipeline, vertex generation, and GizmoStyle reference.
- `extending/` — practical guides for common extension scenarios.

## Headless Pure2D readback

`ferrous_renderer::headless::Pure2dHarness` drives the Pure2D path without a
window and returns frames through a typed, overlapping readback ring.

### Ring state machine

`N` staging buffers (default 3, minimum 2) move through:

```
Free -> Copying { submission } -> Ready -> Free
                              \-> Failed -> Free   (map error)
```

- `render_frame` submits and returns; it never blocks on the GPU.
  `render_frame_blocking` keeps the old "GPU done" timing.
- `submit_readback` records the copy, submits it and calls `map_async`
  immediately, so the copy of frame N overlaps frame N+1. It returns the
  harness-assigned frame index (starting at 0) or `ReadbackError::Busy` when
  every slot is in flight (take one frame first; nothing changes on Busy).
- `take_readback` polls only for the oldest submission
  (`Maintain::WaitForSubmissionIndex`), copies the padded rows out, unmaps and
  frees the slot. A map failure is reported exactly once as
  `ReadbackError::MapFailed { message, frame_index }`, the slot is freed, and
  the next call returns the next frame.

### Exporter loop

```rust,no_run
use ferrous_renderer::headless::{Pure2dHarness, DEFAULT_FORMAT};

let mut h = Pure2dHarness::new(1920, 1080, DEFAULT_FORMAT).unwrap();
let mut frame = Vec::new();
let mut expected = 0u64;
for i in 0..300u64 {
    h.begin_frame();
    // ... draw the scene for frame i ...
    h.render_frame();
    if let Some(index) = h.readback_frame(&mut frame).unwrap() {
        assert_eq!(index, expected);
        expected += 1;
        // encode/write `frame` (tightly packed RGBA8, row 0 = top)
    }
}
h.flush_readbacks(|index, bytes| {
    assert_eq!(index, expected);
    expected += 1;
    let _ = bytes;
})
.unwrap();
```

Steady state allocates no staging buffers: the ring is created once and
reused. `readback_slots_allocated` is a monotonic high-water mark.