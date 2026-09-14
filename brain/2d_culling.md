# FerrousEngine — Pure2D Viewport Culling

> Branch: `perf/2d-culling` (cut from `main`). Implements audit item **H15**:
> automatic, runtime-toggleable CPU-side culling of 2D instances against the
> visible camera rect, with telemetry so a consumer (Koncreta) can log/visualise
> it and A/B compare.
>
> Hardware-independent numbers only (draw calls, instances, bytes). `cpu_ms` is a
> same-machine relative delta on the software rasteriser and is noisy — never an
> fps claim.

## 1. What was added

The Pure2D fast path (`crates/ferrous_renderer/src/renderer_core.rs`,
`render_internal`) used to upload and draw **every** batched instance. On large
floor plans with zoom/pan this wastes instance-buffer writes, upload bandwidth
and draw-call slots on geometry the camera cannot see.

Now, after building and sorting the painter-order `draw_list` and **before** the
coalescing run walk, the stream is compacted: every instance whose world-space
AABB does not intersect the visible camera rectangle (expanded by an AA-safe
margin) is removed and counted. Alloc-free: it uses `Vec::retain` on the reused
`draw_list` scratch buffer.

Because culling happens **before** the run walk, primitives that were interleaved
with culled ones become contiguous and coalesce, so culling lowers the draw-call
count too, not just bytes.

## 2. Public API (exact)

### Config (`crates/ferrous_renderer/src/render_config.rs`)

Two new fields on the existing `Render2dConfig`, read/set through the existing
`Renderer::render_2d_config()` / `set_render_2d_config(cfg)` (and the
`ferrous_engine::RenderContext` equivalents):

```rust
pub struct Render2dConfig {
    pub antialiasing: Antialiasing2d,   // unchanged
    pub ui_bind_group_cache: bool,      // unchanged
    pub scratch_reuse: bool,            // unchanged
    /// CPU-side viewport culling for the Pure2D path. Default `true`.
    pub cull_2d: bool,
    /// Extra world-space margin around the visible rect, in world units
    /// (pixels at Pure2D zoom 1). Default `4.0`.
    pub cull_margin: f32,
}
```

Effective margin = `max(cull_margin, 4.0 * world_units_per_pixel)`, i.e. the
caller's world-space margin plus an automatic ≥4-pixel floor that keeps the SDF
anti-aliasing band and sub-pixel line widening from popping at the viewport edge.

A/B compare at runtime:

```rust
let mut cfg = ctx.render.render_2d_config();
cfg.cull_2d = !cfg.cull_2d;          // measure culling on vs off
// cfg.cull_margin = 8.0;            // widen if a consumer sees edge pop
ctx.render.set_render_2d_config(cfg);
```

### Telemetry

`Frame2dCounters` (in `crates/ferrous_2d/src/render/renderer.rs`) gained an
additive field:

```rust
pub struct Frame2dCounters {
    // ... unchanged ...
    pub draw_calls: u32,
    pub shape_draw_calls: u32,
    pub sprite_draw_calls: u32,
    pub bytes_uploaded: u64,
    pub instances_uploaded: u32,
    pub buffer_reallocations: u32,
    /// Instances discarded by CPU-side viewport culling (shapes + sprites).
    /// `0` when `cull_2d == false`.
    pub culled_instances: u32,
}
```

It is populated by `Renderer2d::add_culled_instances(n)`, reset with the other
counters by `reset_frame_counters`, and surfaced by the existing
`RenderContext::frame_2d_stats()` (also on `Renderer::frame_2d_stats()`). The
headless harness mirrors it as `FrameReport::culled_instances`.

## 3. Visible-rect computation

`Renderer::pure2d_cull_rect()` inverts the camera's current view-projection and
unprojects the four NDC corners `(-1,-1) … (1,1)` at `z = 0`, taking min/max.
For the Pure2D orthographic camera this is exactly the world rect the camera can
see, and it stays correct for `set_ortho_size`, `set_camera_eye`, camera-target
pan and resize (1 world unit = 1 pixel, origin centred). The margin is then
subtracted/added on both axes.

`world_units_per_pixel` is derived the same way the shaders do:
`2 / (|view_proj[0][0]| * viewport_width)`.

## 4. AABB assumptions (conservative)

`shape_world_aabb` / `sprite_world_aabb` transform the **four corners of the
shader's canonical local quad** through the instance `Mat4`
(`world = c0*lx + c1*ly + c3`) and take their min/max:

| Instance | Canonical quad | Notes |
|---|---|---|
| rect / circle / rounded / outline / line | `[-0.5, 0.5]²` | `shape_2d.wgsl` re-centres the `[0,1]²` corners. Round/square line caps sit exactly on the quad edge (`geom_length = length + width`), so the bounds are tight. |
| triangle (`params.w > 1.5`) | `[0, 1]²` | `c0 = b-a`, `c1 = c-a`, `c3 = a`; the triangle is fully contained in the quad. |
| sprite | `[-0.5, 0.5]²` | `sprite_2d.wgsl` has no geometric expansion. |

The AABB is therefore conservative but not the tight SDF bounds: the fragment
shader's AA band (`filter_width = world_pixel_size * smoothing`) and the
sub-pixel line widening can extend a few pixels past it — which is exactly what
`cull_margin` (+ the 4-pixel floor) covers. **No constructor emits geometry
outside its canonical quad**, so nothing visible is ever culled provided the
margin covers the AA band.

## 5. Measured deltas (same machine, lavapipe/Vulkan)

`cargo bench -p ferrous_renderer --bench pure2d_headless -- --sample-count 10`.
The two culling benches share a 512×512 viewport, 512 visible sprites (texture 0)
and `n_off` off-screen sprites (texture 1) interleaved in painter order:

| case | draw_calls | instances_uploaded | bytes_uploaded | culled_instances |
|---|---|---|---|---|
| `pure2d_culling_off off=1024` | 1025 | 1536 | 172 032 | 0 |
| `pure2d_culling_on  off=1024` | **1** | **512** | **57 344** | 1024 |
| `pure2d_culling_off off=8192` | 1025 | 8704 | 974 848 | 0 |
| `pure2d_culling_on  off=8192` | **1** | **512** | **57 344** | 8192 |

Delta for 8192 off-screen sprites: **1025 → 1 draw calls**, **8704 → 512**
instances, **974 848 → 57 344 bytes/frame** (`buffer_reallocations` 1 → 0
because the upload no longer exceeds the initial 1024 capacity). The off-screen
set no longer affects the frame at all.

The pre-existing `pure2d_shapes` / `pure2d_sprites` benches explicitly disable
culling so they keep measuring the full extract/coalesce/upload path (their large
grids intentionally overflow the 512px viewport); they are unchanged:
shapes 4096 → 1 draw / 393 216 bytes; sprites 2048 → 2048 draws / 229 376 bytes.

## 6. Tests (`crates/ferrous_renderer/tests/pure2d_headless.rs`)

- `culling_preserves_visible_pixels_and_drops_offscreen_instances`: renders a
  scene with visible shapes/sprites + 64 off-screen shapes + 64 off-screen
  sprites with culling off and on; asserts the readback pixels are
  **byte-identical**, `culled_instances == 128`, `instances_uploaded` drops by
  128 and `bytes_uploaded` drops.
- `culling_reduces_draw_calls_for_interleaved_culled_texture_runs`: an
  off-screen texture interleaved with a visible one; culling off yields 16
  single-sprite runs, culling on collapses to 1 and reports 8 culled.

Full suite: **19/19 pass** (`--test-threads=1`). `cargo check --workspace` and
`cargo build --workspace` are green; `cad_capture` still reports
0.5 → 128 (non-sRGB) / 187 (sRGB) and 1 draw call.

## 7. Migration notes

- **Additive, no action required:** `Render2dConfig::{cull_2d, cull_margin}`,
  `Frame2dCounters::culled_instances`, `headless::FrameReport::culled_instances`,
  `Renderer2d::add_culled_instances`.
- **Behaviour change:** Pure2D culling is **on by default**. Visible output is
  unchanged (asserted by the byte-identical pixel test), but off-screen geometry
  is no longer uploaded/drawn, so `frame_2d_stats().instances_uploaded` /
  `bytes_uploaded` drop and `draw_calls` can drop. Consumers that inferred work
  from those counters should read `culled_instances` too. To restore the old
  behaviour: `cfg.cull_2d = false`.
- `Render2dConfig` is `#[derive(PartialEq)]` and constructed with
  `..Default::default()` in the tests; a consumer building it with a full struct
  literal must add the two new fields.

## 8. Not verifiable here

- Real-GPU/display verification and wasm32 runtime (this VPS is lavapipe-only).
- Absolute frame-time impact: `cpu_ms` is noisy under the shared 4-vCPU load;
  only the hardware-independent counters above are reliable.
- The 2D **Full3D overlay** path (`renderer_2d`) is not culled; only the live
  Pure2D path is. Overlay unification remains H2.
