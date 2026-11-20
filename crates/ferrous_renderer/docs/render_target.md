<!--
Reference for RenderTarget — off-screen rendering, MSAA, and pixel readback.
-->

# RenderTarget

`RenderTarget` is the compositor-aware colour + depth target used by the
renderer for all draw output.  It supports 4x MSAA out of the box and
exposes the resolved single-sample colour texture for use as a GUI quad
background or for CPU-side pixel readback.

The renderer stores a [`RenderTargetHandle`](#rendertargethandle-lazy-target)
instead of a bare `RenderTarget`, so the 2-D modes can defer the allocation.

## Types


### `RenderTarget`

Defined in `render_target/target.rs` and re-exported from the crate root.

```rust
pub struct RenderTarget {
    color: ColorTarget,
    depth: DepthTarget,
}
```

Key methods:

| Method | Return type | Description |
|--------|-------------|-------------|
| `new(device, width, height, sample_count)` | `Self` | Allocates colour + depth textures |
| `resize(device, width, height)` | `()` | Destroys and recreates all textures |
| `color_views()` | `(&TextureView, Option<&TextureView>)` | MSAA attachment + resolve target |
| `depth_view()` | `&TextureView` | Depth-stencil attachment |
| `color_texture()` | `&wgpu::Texture` | Resolved single-sample texture |
| `color_view()` | `&TextureView` | View of the resolved texture |
| `sample_count()` | `u32` | MSAA sample count |

### `ColorTarget`

```rust
pub struct ColorTarget {
    resolve:      wgpu::Texture,      // sample_count = 1, TEXTURE_BINDING | COPY_SRC
    resolve_view: wgpu::TextureView,
    msaa:         Option<wgpu::Texture>,       // sample_count > 1
    msaa_view:    Option<wgpu::TextureView>,
    format:       wgpu::TextureFormat,         // Bgra8UnormSrgb
}
```

When `sample_count > 1` the render pass writes to `msaa` and resolves
into `resolve` automatically.  When `sample_count == 1` only `resolve`
exists and `msaa` is `None`.

`attachment_views()` returns the correct `(attachment, resolve_target)`
pair for a `RenderPassColorAttachment` in either configuration.

### `DepthTarget`

```rust
pub struct DepthTarget {
    texture: wgpu::Texture,   // Depth32Float
    view:    wgpu::TextureView,
    // sample_count matches the ColorTarget
}
```

The depth format is always `Depth32Float`.  The sample count must match
the colour target — `RenderTarget::new` ensures this.

## RenderTargetHandle (lazy target)

`Renderer::render_target` is a `RenderTargetHandle`:

```rust
pub struct RenderTargetHandle { /* eager RenderTarget or LazyRenderTarget */ }

impl RenderTargetHandle {
    pub fn new_eager(device, width, height, format, sample_count) -> Self;
    pub fn new_lazy(width, height, format, sample_count) -> Self;
    pub fn ensure_allocated(&mut self, device) -> bool;
    pub fn is_allocated(&self) -> bool;
    pub fn is_lazy(&self) -> bool;
    pub fn color(&self) -> Option<&ColorTarget>;
    pub fn depth(&self) -> Option<&DepthTarget>;
    pub fn color_view(&self) -> Option<&wgpu::TextureView>;
    pub fn color_texture(&self) -> Option<&wgpu::Texture>;
    pub fn depth_view(&self) -> Option<&wgpu::TextureView>;
    pub fn resize(&mut self, device, width, height);
    pub fn release(&mut self);
}
```

`Full3D` constructs the eager form, because the depth attachment and the colour
resolve are used every frame.  `Flat2D`/`Pure2D` present straight to the
surface, so they construct the lazy form: at 1600x900 the colour resolve
(5.76 MB) plus the `Depth32Float` depth (5.76 MB) would be 11.52 MB of unused
VRAM.

`ensure_allocated` is called by `render_to_internal_target` (and by
`ensure_three_d` before the 3-D passes).  Until then:

- `is_allocated()` is `false`,
- every accessor returns `None`,
- the memory inventory has no `rt:color resolve`, `rt:color msaa` or `rt:depth`
  entry.

Once materialized the allocation, formats, sample counts and inventory labels
match `RenderTarget::new` byte for byte.  `resize` only records the new logical
size while the target is unallocated; `release` drops the attachments and
keeps the logical size.

```rust
// 2-D: no allocation at construction.
assert!(!renderer.render_target.is_allocated());
// ... first render into the internal target ...
renderer.render_to_internal_target(&mut encoder, None);
assert!(renderer.render_target.is_allocated());
```

## MSAA

The `Renderer` creates its eager `RenderTarget` with the configured sample
count (4 by default in `Full3D`):

```rust
let render_target = RenderTarget::new(&ctx.device, width, height, 4);
```

Internally this allocates:

| Texture | Usage flags | Samples |
|---------|-------------|---------|
| MSAA colour | `RENDER_ATTACHMENT` | 4 |
| Resolve colour | `RENDER_ATTACHMENT \| TEXTURE_BINDING \| COPY_SRC` | 1 |
| Depth | `RENDER_ATTACHMENT` | 4 |

On execute the render pass fills the MSAA texture and resolves into the
single-sample texture automatically via `resolve_target`.  The resolved
texture is the one you read back or display in the UI.

To disable MSAA, pass `sample_count = 1`.  The MSAA texture is not
allocated and `color_views()` returns `(resolve_view, None)`.

## Rendering to a window view

```rust
// swap-chain view provided by winit
renderer.render_to_view(&ctx, &surface_view);
```

This uses the internal `RenderTarget` for all intermediate work and
copies/resolves into `surface_view` at the end of `WorldPass::execute`.

## Rendering to an off-screen texture

```rust
renderer.render_to_target(&ctx);

// Access the resolved texture afterwards (allocated by the Target render)
let texture = renderer.render_target.color_texture().expect("allocated");
```

The resolved texture has `TEXTURE_BINDING | COPY_SRC` usage.  You can:

- **Display it in a `ViewportWidget`** — create a `wgpu::TextureView`
  from it and pass it to `GuiRenderer` as a sampled texture for a quad.
- **Read it back to CPU** — copy to a staging buffer (see below).

## CPU pixel readback

The resolved texture has `COPY_SRC` usage.  Copy it to a mapped buffer
to read pixels on the CPU:

```rust
let texture = renderer.render_target.color_texture().expect("allocated");
let width   = renderer.width();
let height  = renderer.height();

// bytes per row must be a multiple of 256
let align        = wgpu::COPY_BYTES_PER_ROW_ALIGNMENT;
let bytes_per_px = 4u32; // Bgra8UnormSrgb
let unpadded     = width * bytes_per_px;
let padded       = (unpadded + align - 1) / align * align;

let staging = ctx.device.create_buffer(&wgpu::BufferDescriptor {
    label:              Some("readback"),
    size:               (padded * height) as u64,
    usage:              wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
    mapped_at_creation: false,
});

let mut encoder = ctx.device.create_command_encoder(&Default::default());
encoder.copy_texture_to_buffer(
    texture.as_image_copy(),
    wgpu::ImageCopyBuffer {
        buffer: &staging,
        layout: wgpu::ImageDataLayout {
            offset:         0,
            bytes_per_row:  Some(padded),
            rows_per_image: Some(height),
        },
    },
    wgpu::Extent3d { width, height, depth_or_array_layers: 1 },
);
ctx.queue.submit([encoder.finish()]);

// Map and read
let slice = staging.slice(..);
slice.map_async(wgpu::MapMode::Read, |_| {});
ctx.device.poll(wgpu::Maintain::Wait);
let data: Vec<u8> = slice.get_mapped_range().to_vec();
```

Remember to strip the row padding before interpreting `data`.

## Resize

Call `Renderer::resize(ctx, width, height)` when the window is resized;
this forwards to `RenderTarget::resize` which drops and recreates all
textures at the new dimensions.  All `TextureView` references obtained
before the resize are invalid afterwards — do not hold onto them across
frames.
