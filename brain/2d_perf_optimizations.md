# FerrousEngine — 2D Performance + Configurability Pass

> Branch: `perf/2d-optimize` (cut from `main`). Scope: runtime instrumentation so
> Koncreta can log/visualise metrics and A/B compare optimizations, plus the
> concrete optimizations from the 2D audit backlog (H3, H8, M4, M18).
>
> All numbers are **hardware-independent** (draw calls, bytes/frame,
> allocations/frame) except `cpu_ms`, which is a same-machine relative delta on
> the lavapipe/llvmpipe software rasteriser and is noisy — never an absolute fps
> claim.

## 1. What was added

### 1.1 Per-frame instrumentation (public API)

`ferrous_renderer` now exposes the live Pure2D counters and wall-clock timing:

```rust
// ferrous_renderer (re-exported from ferrous_engine)
pub struct Frame2dCounters {
    pub draw_calls: u32,
    pub shape_draw_calls: u32,
    pub sprite_draw_calls: u32,
    pub bytes_uploaded: u64,
    pub instances_uploaded: u32,
    pub buffer_reallocations: u32,
}
pub type Frame2dStats = Frame2dCounters;

impl Renderer {
    pub fn frame_2d_stats(&self) -> ferrous_2d::render::Frame2dCounters;
    pub fn frame_ms(&self) -> f32;
    pub fn frame_ms_avg(&self) -> f32;   // EMA, 60-frame time constant
    pub fn backend_name(&self) -> &'static str; // "vulkan" | "webgpu" | "gl" | ...
    pub fn record_frame_time(&mut self, ms: f32);
}
```

`ferrous_engine::RenderContext` (all additive; callable every frame, no
allocation):

```rust
impl RenderContext<'_> {
    pub fn frame_2d_stats(&self) -> ferrous_renderer::Frame2dCounters;
    pub fn frame_ms(&self) -> f32;
    pub fn frame_ms_avg(&self) -> f32;
    pub fn backend_name(&self) -> &'static str;
    pub fn render_2d_config(&self) -> ferrous_renderer::Render2dConfig;
    pub fn set_render_2d_config(&mut self, config: ferrous_renderer::Render2dConfig);
}
```

- The counters are reset at the start of every `render_internal` and accumulated
  by the Pure2D `prepare_*`/`render_*` calls. `RenderStats::draw_calls` (the 3D
  path) is left untouched; 2D consumers use `frame_2d_stats()`.
- `frame_ms` is measured in `crates/ferrous_engine/src/runner/frame.rs` around
  the whole frame (update + draw + present), recorded on every return path.
- `backend_name()` is derived from `EngineContext.backend`.
- `ferrous_engine` re-exports `Antialiasing2d`, `Frame2dCounters`, `Frame2dStats`
  and `Render2dConfig`, so a consumer only needs to depend on `ferrous_engine`.

### 1.2 Runtime configuration (public API)

```rust
// ferrous_renderer
pub enum Antialiasing2d { Off, Fxaa, Smaa }   // Default = Off

impl Antialiasing2d {
    pub fn to_mode(self) -> crate::passes::AntialiasingMode;
    pub fn from_mode(mode: crate::passes::AntialiasingMode) -> Self;
}

pub struct Render2dConfig {
    pub antialiasing: Antialiasing2d,   // default Off  (M4)
    pub ui_bind_group_cache: bool,      // default true (H8)
    pub scratch_reuse: bool,            // default true (H8/H3)
}
```

Consumers A/B compare by copying the config, mutating a field and handing it
back:

```rust
let mut cfg = ctx.render.render_2d_config();
cfg.scratch_reuse = false;                    // measure the allocating path
cfg.antialiasing = Antialiasing2d::Smaa;
ctx.render.set_render_2d_config(cfg);
```

`set_render_2d_config` mirrors `antialiasing` into the internal
`AntialiasingPass` and forwards the other two flags to the UI pass and the two
`Renderer2d` instances. No pipelines are rebuilt.

## 2. Concrete optimizations

### 2.1 Batching coalescing (H3) — implemented

The Pure2D path previously required **equal `z` and contiguous indices** to merge
a run, so any per-element `z` collapsed coalescing to one draw per primitive
(measured 4096 shapes → 4096 draws).

New algorithm (in `renderer_core::render_internal`, Pure2D branch):

1. Build a painter-order command list `(z, DrawCommand2d)` from the batchers
   **without mutating them**, tie-broken by the command itself (shape index,
   then sprite texture id / flat offset) — a deterministic total order, so
   `sort_unstable_by` is used and **allocates nothing** (fixes the latent M5
   concern too).
2. Split the stream into maximal **consecutive same-key** runs (key = shape
   pipeline, or sprite texture). `z` may vary inside a run.
3. Copy each run's instances into a reusable, painter-ordered scratch upload
   (`shape_upload` / `sprite_upload`), so a run is a contiguous GPU range.
4. Upload the coalesced buffers with the new `prepare_shape_slice` /
   `prepare_sprite_slice` (one `write_buffer` each) and emit one instanced draw
   per run, in painter order.

Painter order is preserved **exactly** (runs are emitted in order and each run's
instances are drawn in the same order they were sorted into), so shape/sprite
interleaving is unchanged. This is "grouping by (pipeline, texture) with an order
index instead of requiring contiguous equal-`z`".

**Deliberate limit:** same-texture sprites that are *interleaved* with other
textures in painter order cannot be merged without reordering cross-texture
alpha blending. That would silently change the visual result, so it is not done.
The benchmark's `pure2d_sprites` interleaves textures every sprite and therefore
still reports `n` draws; the fix for that scenario is the sprite atlas (H5).
Consecutive same-texture runs do coalesce (guarded by
`distinct_z_same_texture_sprites_coalesce`).

### 2.2 Scratch reuse (H8/H3) — implemented

- `Renderer2d` (`ferrous_2d`): the flattened sprite staging `Vec` is reused
  (`clear()` + `extend`) behind `set_scratch_reuse`; the allocating path is kept
  for A/B.
- `Renderer` (`ferrous_renderer`): `draw_list`, `sprite_flat`, `shape_upload`,
  `sprite_upload` and `coalesced` scratch buffers are reused across frames, gated
  by `Render2dConfig::scratch_reuse`. The old path is a fresh `Vec` per frame.
- New slice uploads `Renderer2d::prepare_shape_slice` /
  `prepare_sprite_slice` let the Pure2D path upload a caller-ordered buffer.

### 2.3 UI image bind-group cache (H8) — implemented

`GuiRenderer::prepare` used to rebuild `image_bind_group` (plus two scratch
`Vec`s) on **every frame with textures**. It is now keyed on the texture set
(`view` pointer, `sampler` pointer, `Arc<Texture2d>` pointer, in order) and only
rebuilt when that key changes. Gated by `image_bind_group_cache` (default true).
Enabled by the `assets` feature (the texture path itself is `assets`-gated).

### 2.4 AA policy (M4) — implemented

- Default antialiasing changed from `Fxaa` (over linear HDR, on top of the SDF
  analytic AA) to `Off`: no redundant post-AA stack. SDF shapes keep their
  analytic in-shader AA; MSAA via `with_msaa(n)` is unaffected.
- `Antialiasing2d::{to,from}_mode` keeps the small consumer enum in sync with the
  internal `AntialiasingMode` (which carries FXAA params).
- Pure2D has no post-AA (it does not create the HDR scene texture); this is
  documented and the config knob is a no-op there, which is the consistent
  policy.

### 2.5 Pipeline cache (M18 remainder) — implemented

- `EngineContext.pipeline_cache` is now `Option<Arc<wgpu::PipelineCache>>` so
  sub-renderers can clone/share it.
- Threaded into: the UI pipelines (`create_quad/text/svg/id_pipeline`,
  `GuiRenderer::new`/`update_output_config`) and the AA pass
  (`AntialiasingPass::on_attach`). The two `Renderer2d` instances already used
  `new_with_cache`.

## 3. Measured deltas (same machine, lavapipe/Vulkan)

`cargo bench -p ferrous_renderer --bench pure2d_headless -- --sample-count 10`,
before = `main`, after = this branch.

### Hardware-independent (primary signal)

| bench | draw calls before → after | `alloc:` bytes/frame before → after | alloc count |
|---|---|---|---|
| shapes n=16 | 16 → **1** | 7.454 KB → 7.070 KB | 56 → 55 |
| shapes n=256 | 256 → **1** | 13.21 KB → 7.070 KB | 56 → 55 |
| shapes n=4096 | 4096 → **1** | 105.3 KB (+655.2 KB grow) → **7.082 KB** | 56 → 55 |
| sprites n=64, t=1 | 64 → **1** | 15.77 KB → 7.070 KB | 57 → 55 |
| sprites n=512, t=8 | 512 → 512 (interleaved) | 76.85 KB (+89.92 KB grow) → 7.224 KB | 57 → 55 |
| sprites n=2048, t=64 | 2048 → 2048 (interleaved) | 286.9 KB (+360.2 KB grow) → 8.468 KB | 57 → 55 |

`bytes_uploaded` is unchanged (the same instance data is uploaded; only its
layout/coalescing changed). `buffer_reallocations` unchanged (amortized).

### Wall-clock `cpu_ms` (relative only — noisy on the shared VPS)

Median of 10 samples: shapes n=4096 11.58 ms → 4.04 ms; sprites n=64 1.80 ms →
0.88 ms; sprites n=2048 15.77 ms → 10.25 ms. Load average was ~2 on 4 vCPUs and
varied between runs, so treat these as indicative only.

## 4. Deferred / not done

- **True cross-texture sprite grouping** (would make `pure2d_sprites 512/8` →
  8 draws): unsafe without changing painter order. Correct alternatives are the
  sprite atlas (H5, already on the backlog) or an order-independent transparency
  scheme. Left unimplemented on purpose.
- **Full3D 2D overlay unification** (H2): the overlay still uses the monolithic
  `render_shapes` path; out of scope for this pass.
- **wasm32 runtime / real swapchain verification**: not runnable on this VPS.
  `cargo check`/`build`/tests are native lavapipe only.

## 5. Verification

- `cargo check --workspace` ✅ `cargo build --workspace` ✅
- `cargo test -p ferrous_renderer --test pure2d_headless -- --test-threads=1`
  → **9/9 pass** (added: stats API, distinct-`z` coalescing, sprite-run
  coalescing, config A/B).
- `cargo test -p ferrous_renderer --lib` ✅ (config unit tests).
- `cargo run -p ferrous_renderer --example cad_capture` ✅ sRGB capture still
  reports 0.5 → 128 (non-sRGB) / 187 (sRGB).

## 6. Migration notes (consumer-visible)

1. **Default AA changed from FXAA to Off** (visual, not API). To restore the old
   look call `ctx.render.set_antialiasing(AntialiasingMode::Fxaa(FxaaParams::default()))`
   or set `cfg.antialiasing = Antialiasing2d::Fxaa`. Rationale: the SDF shapes
   already do analytic AA and FXAA was running over linear HDR (M4).
2. **`EngineContext.pipeline_cache` type changed**
   `Option<wgpu::PipelineCache>` → `Option<Arc<wgpu::PipelineCache>>`. Migrate:
   use `.as_deref()` where a `Option<&wgpu::PipelineCache>` is expected, or
   `.clone()` to share. Only relevant to code that consumes `ferrous_gpu`
   directly.
3. **`ferrous_ui_render::GuiRenderer::new` gained a trailing parameter**
   `pipeline_cache: Option<Arc<wgpu::PipelineCache>>`. Pass `None` for the old
   behaviour (or `context.pipeline_cache.clone()` to participate in shader
   cache reuse). `update_output_config` is unchanged (the renderer stores the
   cache).
4. **`AntialiasingPass::on_attach` gained `cache: Option<&wgpu::PipelineCache>`.**
   Pass `None` for the old behaviour.
5. **Additive, no action required:** `Renderer::frame_2d_stats` / `frame_ms` /
   `frame_ms_avg` / `backend_name` / `render_2d_config` / `set_render_2d_config`;
   `RenderContext` equivalents; `Renderer2d::{prepare_shape_slice,
   prepare_sprite_slice, set_scratch_reuse, scratch_reuse}`;
   `GuiRenderer::{set_image_bind_group_cache, image_bind_group_cache}`.
6. **Pure2D draw submission changed** (coalesced painter-order runs). Output is
   pixel-identical for existing scenes (asserted by the headless pixel tests);
   only the number/order of `draw` calls changed. Consumers relying on an exact
   draw-call count should use `frame_2d_stats()` instead of inferring it.
