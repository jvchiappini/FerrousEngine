<!--
Reference for the RenderPass trait — the primary extension point of
ferrous_renderer.  Covers the full contract, both built-in
implementations, and the rules an implementor must follow.
-->

# RenderPass trait

`RenderPass` is the single trait you implement to inject any GPU work
into the renderer's frame loop.  Shadow maps, post-processing, outline
rendering, particle systems — all of these can be expressed as one or
more custom passes registered with `Renderer::add_pass`.

The trait is defined in `graph/pass_trait.rs` and re-exported from the
crate root.

## Trait definition

```rust
pub trait RenderPass: std::any::Any {
    /// Human-readable identifier used for debugging and lookup.
    fn name(&self) -> &str;

    // --- downcasting support ---
    fn as_any(&self)     -> &dyn std::any::Any;
    fn as_any_mut(&mut self) -> &mut std::any::Any;

    /// Upload any per-frame GPU data (uniforms, staging copies, …).
    /// Called before the CommandEncoder is opened.
    fn prepare(
        &mut self,
        device:  &wgpu::Device,
        queue:   &wgpu::Queue,
        packet:  &FramePacket,
    );

    /// Record draw calls into the provided encoder.
    /// Called after prepare() for all passes; the encoder is already open.
    fn execute(
        &self,
        device:         &wgpu::Device,
        queue:          &wgpu::Queue,
        encoder:        &mut wgpu::CommandEncoder,
        color_view:     &wgpu::TextureView,
        resolve_target: Option<&wgpu::TextureView>,
        depth_view:     &wgpu::TextureView,
        packet:         &FramePacket,
    );
}
```

## Contract

| Method | When called | Allowed GPU operations |
|--------|-------------|------------------------|
| `prepare` | Before encoder creation, in pass order | `queue.write_buffer`, staging uploads, buffer creation |
| `execute` | After all `prepare` calls, encoder already open | `encoder.begin_render_pass`, `encoder.copy_*` |

**Do not** call `queue.write_buffer` inside `execute`; the encoder is
already borrowing the queue context.  Move all uploads to `prepare`.

**Do not** call `encoder.finish()` inside `execute`; the outer loop
handles submission after all passes have recorded.

## Pass ordering

Passes are executed in the order they were registered.  The default
list constructed by `Renderer::new` is:

1. `WorldPass` — clears the colour and depth attachments, then draws
   all 3-D scene objects.
2. `UiPass` — composites the GUI and text layers using
   `LoadOp::Load` so it does not overwrite the 3-D content.

When you add a custom pass via `Renderer::add_pass` it is appended
after `UiPass`.  To insert a pass before the UI layer (e.g. a
post-process effect that must run before the HUD), call
`Renderer::clear_passes`, re-register `WorldPass` and your custom pass
manually, then re-register `UiPass`.

## Downcast API

Because passes are stored as `Box<dyn RenderPass>` the crate provides
downcast helpers via the `Any` super-trait.  You must implement both
`as_any` and `as_any_mut` on your type:

```rust
fn as_any(&self)     -> &dyn std::any::Any { self }
fn as_any_mut(&mut self) -> &mut std::any::Any { self }
```

Then to retrieve a concrete reference at runtime:

```rust
if let Some(pass) = renderer
    .passes()
    .iter()
    .find(|p| p.name() == "my_outline_pass")
{
    if let Some(concrete) = pass.as_any().downcast_ref::<OutlinePass>() {
        // access concrete fields
    }
}
```

## Built-in passes

### `WorldPass`

Located in `passes/world_pass.rs`.

```rust
pub struct WorldPass {
    pipeline:          Arc<wgpu::RenderPipeline>,
    camera_bind_group: Arc<wgpu::BindGroup>,
    clear_color:       wgpu::Color,
}
```

- **`prepare`** — calls `GpuCamera::sync` to upload the current
  view-projection matrix.
- **`execute`** — opens a render pass with `LoadOp::Clear`, sets the
  viewport and scissor rectangle from `packet.viewport`, then iterates
  `packet.scene_objects` and issues one draw call per `DrawCommand`.

The clear colour defaults to `wgpu::Color { r: 0.1, g: 0.1, b: 0.1, a: 1.0 }`.

### `UiPass`

Located in `passes/ui_pass.rs`.

```rust
pub struct UiPass {
    renderer: RefCell<GuiRenderer>,
}
```

- **`prepare`** — calls `GuiRenderer::prepare` with the `ui_batch` and
  `text_batch` from the packet (skips if both are `None`).
- **`execute`** — opens a render pass with `LoadOp::Load` and calls
  `GuiRenderer::render`.  The pass is a no-op if there is nothing to
  draw.

## Minimal custom pass example

```rust
use ferrous_renderer::{RenderPass, FramePacket};
use std::any::Any;

pub struct WireframePass {
    pipeline: Arc<wgpu::RenderPipeline>,
}

impl RenderPass for WireframePass {
    fn name(&self) -> &str { "wireframe_pass" }

    fn as_any(&self)         -> &dyn Any { self }
    fn as_any_mut(&mut self) -> &mut dyn Any { self }

    fn prepare(
        &mut self,
        _device: &wgpu::Device,
        _queue:  &wgpu::Queue,
        _packet: &FramePacket,
    ) {
        // nothing to upload for a simple wireframe pass
    }

    fn execute(
        &self,
        _device:         &wgpu::Device,
        _queue:          &wgpu::Queue,
        encoder:         &mut wgpu::CommandEncoder,
        color_view:      &wgpu::TextureView,
        resolve_target:  Option<&wgpu::TextureView>,
        depth_view:      &wgpu::TextureView,
        packet:          &FramePacket,
    ) {
        let mut rp = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("wireframe"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view:           color_view,
                resolve_target,
                ops: wgpu::Operations {
                    load:  wgpu::LoadOp::Load,  // keep previous content
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                view: depth_view,
                depth_ops: Some(wgpu::Operations {
                    load:  wgpu::LoadOp::Load,
                    store: wgpu::StoreOp::Store,
                }),
                stencil_ops: None,
            }),
            ..Default::default()
        });

        rp.set_pipeline(&self.pipeline);
        for cmd in &packet.scene_objects {
            rp.set_bind_group(1, &cmd.model_bind_group, &[]);
            rp.set_vertex_buffer(0, cmd.vertex_buffer.slice(..));
            rp.set_index_buffer(cmd.index_buffer.slice(..), cmd.index_format);
            rp.draw_indexed(0..cmd.index_count, 0, 0..1);
        }
    }
}
```

Register it after building the renderer:

```rust
renderer.add_pass(Box::new(WireframePass { pipeline }));
```

For a complete guide — including pipeline construction and shader
authoring — see `extending/custom_pass.md`.
