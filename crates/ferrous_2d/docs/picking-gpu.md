# GPU ID-buffer picking

`ferrous_2d::picking::GpuPicker` is the on-demand alternative to the spatial
index in [picking.md](picking.md). It renders the pickable items into a small
`R32Uint` target with the same geometry, clips, tolerance and `(z, insertion)`
order the CPU queries use, reads the pixels back and decodes them with the
`IdBuffer` encoding.

It is a native feature: the readback blocks on `map_async`, which the browser
event loop cannot do. On wasm use the CPU queries.

```rust
use ferrous_2d::picking::{GpuPicker, PickItem, PickScene, PickShape};
use glam::Vec2;

let mut scene = PickScene::new();
scene.add(PickItem::new(7, PickShape::rounded_rect(Vec2::new(240.0, 120.0), 12.0)));
scene.rebuild();

let picker = GpuPicker::new(&device);
if let Some(hit) = picker.pick_gpu(&device, &queue, &scene, Vec2::new(10.0, 10.0), 0.0)? {
    println!("selected {}", hit.id);
}
```

The picker owns its pipeline, instance/point/clip buffers and the `R32Uint`
target. The target and its staging buffer are allocated on first use and reused
while the requested size does not change; repeated picks of one region create no
GPU resources.

## API

| Method | Target | Result |
|---|---|---|
| `pick_gpu(point, tolerance)` | 1x1 | topmost `PickHit` or `None` |
| `pick_all_gpu(point, tolerance, out)` | 1x1 per pass | every hit, topmost first |
| `region_gpu(area, tolerance, out)` | region, snapped to whole pixels | unique items that cover a pixel |
| `marquee_gpu(...)` | alias of `region_gpu` | GPU `MarqueeMode::Intersect` |
| `target_size()`, `id_texture_bytes()` | - | allocation/last-region diagnostics |

`GpuPicker::new_with_cache(device, Some(&cache))` threads a shared pipeline
cache, like the rest of the 2D pipelines.

`pick_all_gpu` re-runs the 1x1 pass with a decreasing rank cutoff (one pass per
hit), because the id buffer stores one id per pixel. It is for on-demand tools,
not for a per-move path.

## Encoding

`IdBuffer` is the CPU reference. `IdBuffer::encode(id)` writes the id into the
RGB bytes with alpha 255; an `R32Uint` target has no alpha channel, so the GPU
path stores `id + 1` and reserves `0` for "clear"
(`IdBuffer::encode_u32` / `IdBuffer::decode_u32`). Consequences:

- `PickId` values must be unique per scene and must not be `u32::MAX`.
- Id `0` is valid (it becomes `1`); the clear value is `0`.
- The GPU target itself is not an RGBA8 image. Read it with `GpuPicker`, not by
  treating it as `IdBuffer::decode` input.

## Semantics and limits

- Point and region picks sample **pixel centres**, exactly like
  `IdBuffer::rasterize`. A region is snapped up to whole pixels, so the result is
  "items that cover at least one pixel".
- `region_gpu` is `MarqueeMode::Intersect`; `Contain` needs the outline and stays
  on the CPU.
- `Image` alpha thresholds are not sampled: the GPU tests the image bounds only.
  Use `PickScene::pick` when the alpha mask is significant.
- There is no GPU lasso, `contain`, `nearest_point` or `nearest_edge`; those
  stay on the spatial index.
- Only `visible`, `pickable` items with `opacity > 0.0` are rendered, matching
  the CPU filters.
- The exact fragment tests are the CPU ones: rounded-rect SDF, circle, non-zero
  / even-odd path fill, polyline stroke with caps, text bounds and clip chains.
  The depth buffer uses the `(z, insertion)` rank, so the topmost item wins even
  if the driver rasterises instances out of order.

## When it wins

Measured with `cargo bench -p ferrous_2d --bench picking_gpu` on a Ryzen 5
8645HS + RTX 4050 Laptop, release, 10 000 shapes, median of 20-40 iterations.
The CPU columns are the same scene through the spatial index; the P4B report
numbers are reproduced (point 61 ns, marquee 5.4 us, contain 93 us, lasso
14.9 us, nearest 174 ns).

| Query | CPU index | GPU pass | Winner |
|---|---:|---:|---|
| point pick (per query) | 63 ns | 399 us | CPU, ~6 000x |
| marquee 200x200 (intersect) | 5.3 us | 738 us | CPU |
| contain / lasso / nearest | 91 us / 14.4 us / 196 ns | none | CPU |
| id buffer 64x64 | 117 us | 427 us | CPU |
| id buffer 128x128 | 472 us | 527 us | CPU |
| id buffer 256x256 | 2 006 us | 809 us | **GPU, 2.5x** |
| id buffer 512x512 | 8 295 us | 2 246 us | **GPU, 3.7x** |
| id buffer 1024x1024 | 33 158 us | 7 701 us | **GPU, 4.3x** |

The CPU "id buffer" column is `IdBuffer::rasterize` (one indexed pick per
pixel), i.e. the reference cost of the query a bulk id pass replaces. The
crossover on this host is between 128x128 and 256x256 (roughly 65k sampled
pixels). On a software adapter the readback dominates and the crossover moves
much further out; on a discrete GPU it can move closer.

Use the GPU pass when:

- the region is large (a brush, a marquee snapshot, a whole-canvas id map);
- one pass over the scene is cheaper than tens of thousands of indexed picks;
- the ids are already needed as a buffer, not as a single answer.

Use the spatial index (the fallback) for every interactive query: hover, drag,
point selection, small marquees, lasso, containment and snapping. It is the
default, needs no GPU adapter and has zero allocations per query.

## The 2D scene target

The colour target used for screenshots, thumbnails and editor viewports is
`ferrous_renderer::SceneTarget2d`. It wraps the headless `OffscreenTarget`
(texture, persistent staging buffer, 1x1 pixel staging buffer) and renders the
2D command stream:

```rust
let mut target = SceneTarget2d::new(&device, 1280, 800, SceneTarget2d::DEFAULT_FORMAT);
target.render_with(&mut renderer, &device, &queue, |renderer| {
    // queue draw_2d_shape / draw_sprite_2d / draw_text_2d here
});
let mut pixels = Vec::new();
target.read_rgba8_into(&device, &queue, &mut pixels)?;
```

`resize` reuses the target when the size is unchanged; `read_pixel_into` is the
single-texel path. The id pass is a separate `R32Uint` target inside
`GpuPicker`, so a normal scene render does not pay for it.
