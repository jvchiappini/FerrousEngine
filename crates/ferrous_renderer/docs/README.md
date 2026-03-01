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
├── render_target.md       # off-screen rendering, MSAA, pixel readback
└── extending/
    ├── custom_pass.md     # step-by-step guide to writing a custom pass
    ├── new_pipeline.md    # adding a new wgpu render pipeline
    └── world_sync.md      # hooking new scene element types into sync_world
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

You can replace or extend this list with `Renderer::add_pass` /
`Renderer::clear_passes`.  See `render_pass.md` for full details.

## Key public types

| Type | Where defined | Purpose |
|------|---------------|---------|
| `Renderer` | `lib.rs` | top-level entry point |
| `RenderPass` | `graph/pass_trait.rs` | trait for custom passes |
| `FramePacket` | `graph/frame_packet.rs` | per-frame CPU data bundle |
| `Viewport` | `graph/frame_packet.rs` | scissor/viewport rectangle |
| `Mesh` | `geometry/mesh.rs` | GPU vertex + index buffers |
| `Vertex` | `geometry/vertex.rs` | interleaved position + colour |
| `Camera` | *(re-export from ferrous_core)* | view + projection state |
| `Controller` | *(re-export from ferrous_core)* | key bindings + motion config |
| `GpuCamera` | `camera/uniform.rs` | GPU-side camera uniform |
| `RenderTarget` | `render_target/target.rs` | colour + depth target, MSAA-aware |
| `RenderObject` | `scene/object.rs` | per-instance GPU data |

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
- `extending/` — practical guides for common extension scenarios.
