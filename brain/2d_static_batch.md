# FerrousEngine — Retained (Static) 2D Shape Batches

> Branch: `perf/2d-retained` (cut from `main`). Step 2 of the 2D optimization
> pass: geometry that does not change every frame (walls, slabs, baked plans) is
> uploaded to a **persistent GPU buffer once** and re-drawn with one instanced
> draw call per batch, with **zero per-frame upload**.
>
> Hardware-independent numbers only. `cpu_ms` is a noisy same-machine relative
> delta on the software rasteriser — never an fps claim.

## 1. What was added

The immediate Pure2D path rebuilds and re-uploads its instance buffers every
frame (`queue.write_buffer`) even for geometry that never changes.  A
**retained static batch** owns a persistent `VERTEX | COPY_DST` instance buffer
containing `ShapeInstance`s.  It is uploaded at creation, drawn every frame with
one `draw(0..4, 0..count)` through the existing shape (SDF) pipeline, and only
re-uploaded when the consumer calls `update`.

Only **shapes** are retained (the SDF rect / circle / line / triangle /
`polygon_triangle` pipeline).  Textured sprites are a separate layer and remain
immediate.

## 2. Public API (exact)

### Handle

```rust
// ferrous_renderer::static_2d (re-exported from ferrous_engine)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Static2dBatchId { /* private: index, generation */ }

impl Static2dBatchId {
    pub fn index(self) -> u32;
    pub fn generation(self) -> u32;
}
```

The id carries a **generation**.  `destroy` frees the slot and bumps its
generation, so a handle obtained before the destroy can never address a
different batch once the slot is recycled.  `update`/`destroy` with a stale or
unknown id are safe no-ops and the stale batch does not draw.

### `Renderer` / `RenderContext`

```rust
impl Renderer {
    /// Upload `shapes` once; returns a handle.  `&[]` is allowed (draws 0).
    pub fn create_static_2d_batch(
        &mut self,
        shapes: &[ferrous_2d::render::ShapeInstance],
    ) -> Static2dBatchId;

    /// Replace the contents.  Reuses the GPU buffer when it fits, otherwise
    /// recreates it with amortized (next power of two) capacity.
    pub fn update_static_2d_batch(
        &mut self,
        id: Static2dBatchId,
        shapes: &[ferrous_2d::render::ShapeInstance],
    );

    /// Free the batch's GPU buffer and invalidate the handle.
    pub fn destroy_static_2d_batch(&mut self, id: Static2dBatchId);

    /// Number of live retained batches (including empty ones).
    pub fn static_2d_batch_count(&self) -> usize;

    /// Advanced escape hatch: draw all active batches into `view` in a new
    /// `LoadOp::Load` pass.  Not needed with the automatic Pure2D path.
    pub fn draw_static_2d_batches(
        &mut self,
        encoder: &mut wgpu::CommandEncoder,
        view: &wgpu::TextureView,
    );
}

// `ferrous_engine::RenderContext` mirrors create/update/destroy/draw_static_2d_batches
// with identical signatures (types via `ferrous_renderer::`).
```

### `ferrous_2d::render::Renderer2d`

```rust
impl Renderer2d {
    /// Draw a persistent shape buffer as one instanced draw call, recording a
    /// shape draw + the static-batch telemetry counters.  No upload.
    pub fn render_static_shapes(
        &self,
        render_pass: &mut wgpu::RenderPass<'_>,
        buffer: &wgpu::Buffer,
        count: u32,
    );
}
```

### Config & telemetry

```rust
// Render2dConfig (default static_batches = true)
pub static_batches: bool,
// read/set through the existing render_2d_config()/set_render_2d_config(cfg).

// Frame2dCounters (additive)
pub static_batches_drawn: u32,
pub static_instances_drawn: u32,
// plus headless::FrameReport mirrors.
// exposed via RenderContext::frame_2d_stats().
```

## 3. Semantics & resource behaviour

- **Buffer**: `VERTEX | COPY_DST`, `capacity = shapes.len().next_power_of_two()`
  with a 128-instance minimum; grows amortized.  `create` does exactly one
  `write_buffer`; `update` does one `write_buffer` only when the buffer is
  reused, or a recreate + upload when the slice no longer fits.  The buffer is
  never touched on a plain render frame.
- **Slot table**: `Vec<Option<Static2dBatch>>` with a free-list and per-slot
  generation.  Reused slots keep their index but get a new generation after a
  destroy, so ids are stable while alive and unforgeable across recycling.
- **Determinism**: active batches are drawn ordered by representative `z`
  (minimum `transform_c3[2]` of the batch), then slot index — stable across
  frames and independent of insertion order.

## 4. Interaction with the immediate path (z-order & H3 coalescing)

The Pure2D pass emits **all active static batches first**, then the immediate
painter-order coalesced stream (H3) on top.  Consequences:

- Within a static batch, slice order is painter order (draw it in the order you
  want); across batches, the representative-`z` order is respected.
- Static geometry is always drawn **behind** immediate geometry, regardless of
  absolute z.  This is the right contract for Koncreta's use case (static walls/
  slabs at the bottom, dynamic annotations/overlays on top) and it keeps the
  "1 draw call per batch" guarantee.
- If a consumer needs a static batch *in front of* immediate geometry, the
  options are: bake that geometry into the immediate path, or move to a real
  depth-tested 2D pass.  A per-batch z sorted against the immediate runs would
  require splitting the immediate coalesced run, sacrificing H3.  Documented as
  a trade-off, not implemented.
- Static batches are **not** considered by the H3 coalescing (each is its own
  buffer/draw).  They are not affected by `cull_2d` either (see §5).

## 5. Culling trade-off (deliberate)

Static batches are **not** CPU-culled: culling requires per-instance world AABBs
on the CPU, which means keeping a CPU copy of every batch — exactly the memory
the retained buffer is meant to avoid.

- **Chosen for this delivery (option a): draw the whole batch.**  The GPU clips
  off-screen geometry cheaply; the retained path still removes the per-frame CPU
  upload/transform work, which is the dominant cost on large plans.
- **Follow-up (option b): keep a CPU copy + a culled index buffer.**  Store the
  batch's `ShapeInstance`s on the CPU and, when the camera moves, rebuild a
  `u32` index buffer of the visible subset and issue an indexed instanced draw
  (`draw_indexed(0..4, 0, 0..visible)` with the index buffer bound as the
  per-instance index).  This trades RAM and a periodic CPU pass for fewer
  fragment/vertex invocations.  Not implemented; tracked below.

## 6. Measured deltas (same machine, lavapipe/Vulkan)

`cargo bench -p ferrous_renderer --bench pure2d_headless -- --sample-count 10`,
`pure2d_shapes` (immediate, re-uploads every frame) vs `pure2d_static_batch`
(retained, built once then rendered without `fill`):

| scene | draw_calls | instances_uploaded | bytes_uploaded/frame |
|---|---|---|---|
| `pure2d_shapes n=256` | 1 | 256 | 24 576 |
| `pure2d_static_batch n=256` | 1 | **0** | **0** |
| `pure2d_shapes n=4096` | 1 | 4096 | 393 216 |
| `pure2d_static_batch n=4096` | 1 | **0** | **0** |

Both report 1 draw call (the retained batch is one instanced draw; the immediate
path already coalesces to one).  The retained path's win is the eliminated
**393 216 bytes/frame** (and the CPU cost of rebuilding/uploading 4096 instances)
for a static plan.

## 7. Tests (`crates/ferrous_renderer/tests/pure2d_headless.rs`)

- `static_batch_matches_immediate_pixels_and_uploads_once`: a retained batch and
  the same shapes via `draw_2d_shape` produce **byte-identical pixels**; the
  retained frame reports `static_batches_drawn == 1`, `static_instances_drawn ==
  3`, `bytes_uploaded == 0`, `instances_uploaded == 0`, `shape_draw_calls == 1`.
- `static_batch_update_replaces_content_in_place`: `update` changes the pixels
  (red → green), grows past the 128 capacity (300 shapes) and shrinks back, all
  with `bytes_uploaded == 0` on the render frames.
- `static_batch_destroy_stops_drawing_and_invalidates_id`: after `destroy` the
  batch no longer draws (`draw_calls == 0`); stale `update`/`destroy` are safe
  no-ops; a recycled slot issues a new generation and a stale update cannot
  overwrite the live batch.
- `static_batch_has_no_per_frame_upload`: 3 steady-state frames each report 0
  upload / 0 instances / 0 realloc and `static_batches_drawn == 1`.
- `multiple_static_batches_draw_one_call_each`: 2 batches → 2 draws, 0 uploads;
  the `static_batches` config flag skips them without destroying them.

Full suite: **24/24 pass** (`--test-threads=1`).  `cargo check --workspace` and
`cargo build --workspace` green; `cad_capture` still 0.5 → 128/187, 1 draw call.

## 8. Migration notes

- **Additive, no action required:** `Static2dBatchId`, the four `Renderer`/
  `RenderContext` batch methods, `Renderer2d::render_static_shapes`,
  `Frame2dCounters::{static_batches_drawn, static_instances_drawn}`,
  `headless::FrameReport::{static_batches_drawn, static_instances_drawn}`.
- **`Render2dConfig` gained a field** (`static_batches`, default `true`).  A
  consumer building the struct with a full literal must add it; structs built
  with `..Default::default()` are unaffected.
- **No immediate-path behaviour change:** `draw_2d_shape` / `draw_2d_shapes` and
  the automatic draw are unchanged; only consumers that opt into
  `create_static_2d_batch` are affected.
- No Apéndice A signature changed (`draw_2d_shape(s)`, `ShapeInstance`, camera).
- Exports: `ferrous_renderer::Static2dBatchId`, re-exported as
  `ferrous_engine::Static2dBatchId`.

## 9. Not verifiable here

- Real-GPU/display and wasm32 runtime (lavapipe-only VPS).
- Absolute frame time (`cpu_ms` noisy under shared 4-vCPU load); only the
  hardware-independent counters above are reliable.
- Culling of retained batches (option b) is not implemented, so large fully
  off-screen static batches still cost their fragment work on the GPU.
- The Full3D overlay 2D path (`renderer_2d`) does not draw retained batches;
  only the live Pure2D path does (overlay unification is H2).
