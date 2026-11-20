# Performance: instruments, GPU passes and intermediate targets

This page explains how FerrousEngine measures a frame, where the GPU time
goes, how the MSAA policy affects memory, and how the opt-in resolution knobs
change the intermediate render targets.  It is written for readers profiling
their own application; it does not describe a specific consumer.

The three instruments are independent and can be enabled separately:

| Instrument | Entry point | Granularity |
|---|---|---|
| Frame trace | `FERROUS_TRACE_FRAMES=1` | CPU stages, draw calls, GPU pass labels, percentiles |
| GPU timings | `Renderer::enable_gpu_timings` | Timestamp queries per instrumented pass |
| Memory report | `Renderer::memory_report` | Registered GPU/CPU bytes by subsystem and label |

## Reading the frame trace

`ferrous_core::frame_trace` samples every frame when `FERROUS_TRACE_FRAMES=1`
(`crates/ferrous_core/src/frame_trace.rs:44`) and writes one JSON line per
frame.  The runner turns GPU timings on together with the trace
(`crates/ferrous_engine/src/graphics.rs:351`), so the trace includes both CPU
stages (`app update`, `ui layout`, `batch build`, `prepare/upload`, `encode`,
`submit`, `present/wait`) and a `gpu_passes_ms` map.

The trace records:

- `frames`, `presented`, `gpu_submits`, `dropped_frames`;
- `frame_ms_median`, `frame_ms_p90`, `frame_ms_p99`;
- `gpu_frame` counters (`draw_calls`, `bind_group_binds`, `buffer_binds`,
  `pipeline_binds`, `scissor_changes`, `ui_segments`, `gpu_passes`);
- `gpu_passes_ms`, the median GPU milliseconds per instrumented pass;
- `vram_mb`, sampled from the memory registry.

Use `p99` and the per-pass split together.  A high CPU stage with a flat GPU
map points at the CPU; a high `gpu_passes_ms` entry points at that pass.

## Per-pass GPU timing

`crates/ferrous_renderer/src/gpu_timing.rs` owns a timestamp ring.  Each
instrumented pass receives a `PassTiming` whose `writes()` fragment is placed
in `RenderPassDescriptor::timestamp_writes`.  The ring resolves the queries at
the end of the frame (`GpuTimings::end_frame`) and starts a **non-blocking**
map after the submit (`GpuTimings::after_submit`).  `GpuTimings::poll` consumes
the map on a later frame, so the CPU never waits for the GPU.  That is why a
pass label can be missing for one or two frames after the trace starts, and why
the reported value is a median over the sampled window.

When the adapter does not expose `TIMESTAMP_QUERY`, `GpuTimings::supported`
is false and `gpu_timings_summary()` reports `unsupported`; rendering continues
normally.

### Pass budget

`gpu_timing::MAX_PASSES` is **24** (48 timestamp queries).  The budget has to
cover the worst layered Pure2D frame, not just the fast path:

- `Pure2D Main Pass` or `Pure2D Layered Prefix Pass` / `Pure2D Layered Main Pass`;
- `Pure2D Plan Pass` and `Pure2D Backdrop Composite Pass` (one per backdrop
  segment of a plan);
- `Pure2D Mask Pass`, `Pure2D Blur Effect Pass` (two per effect) and
  `Pure2D Effect Composite Pass` (one per effect);
- `GUI Render Pass`.

`GpuTimings::pass` checks the budget at
`crates/ferrous_renderer/src/gpu_timing.rs:191`; once it is exhausted it returns
`None` and only that pass loses its timing.  A frame is never dropped because
of the budget.  Earlier the budget was 8, which the main and GUI passes used up
before any layered pass was reached.

> Instrumented labels in the layered path are only possible when the pass
> descriptor passes `timestamp_writes`.  The layered prefix, plan, backdrop,
> mask, blur and effect-composite descriptors currently pass `None`; the labels
> above are the intended names once those descriptors are wired to
> `GpuTimings::pass`.

## MSAA policy

The sample count comes from `AppConfig::sample_count`
(`crates/ferrous_engine/src/builder.rs:89`), whose default is `1`
(`crates/ferrous_engine/src/builder.rs:337`).  `with_msaa(n)`
(`crates/ferrous_engine/src/builder.rs:553`) or `[renderer] msaa` in
`ferrous.toml` overrides it.  The runner forwards the value to
`Renderer::new_with_mode` (`crates/ferrous_engine/src/graphics.rs:654`), which
stores it and uses it for the HDR/world attachments.

The important distinction:

- **MSAA is a Full3D cost.**  The HDR world pass renders into `Rgba16Float`
  attachments whose sample count is the configured value.  With
  `sample_count > 1`, `HdrTexture` creates one multisampled color texture and
  one single-sample resolve texture
  (`crates/ferrous_renderer/src/render_target/hdr.rs:26`).
- **2D modes use `sample_count = 1`.**  `Flat2D` and `Pure2D` render straight
  to the swapchain surface and do not create an HDR target.  The Pure2D
  renderer is built with an explicit sample count of `1`
  (`crates/ferrous_renderer/src/renderer_core.rs:1273`), and the direct
  surface path makes MSAA incompatible with presentation.  An application that
  only uses `RendererMode::Pure2D` and never calls `with_msaa` therefore has
  **no 4x MSAA HDR target at all**.

### Byte arithmetic

For a `w x h` target:

```text
HDR color (Rgba16Float, 8 bytes/texel) = w * h * 8 * samples
depth     (Depth32Float, 4 bytes/texel) = w * h * 4 * samples
resolve   (only when samples > 1)      = w * h * 8
```

At 1600x900 (1,440,000 texels):

| Configuration | Color | Depth | Resolve | Total |
|---|---:|---:|---:|---:|
| Full3D HDR, 1x | 11.52 MB | 5.76 MB | — | 17.28 MB |
| Full3D HDR, 4x | 46.08 MB | 23.04 MB | 11.52 MB | 80.64 MB |
| Pure2D surface target, 1x (4-byte format) | 5.76 MB | 5.76 MB | — | 11.52 MB |

Enabling 4x MSAA in a mode that uses the HDR path multiplies the color and
depth attachments by four and adds a full-size resolve.  In the 2D modes the
sample count stays one, so this cost does not appear.

## Intermediate resolution

`Render2dConfig` (`crates/ferrous_renderer/src/render_config.rs`) exposes three
opt-in knobs.  All defaults reproduce previous pixels exactly.

| Field | Default | Effect |
|---|---|---|
| `layer_resolution_scale` | `1.0` | Offscreen layer/effect texture size as a multiplier of the surface |
| `ui_depth` | `true` | Allocate/use the GUI depth attachment for Early-Z |
| `effect_scissor` | `false` | Restrict blur/effect/mask passes to the active layer rectangle |

Build them with `with_layer_resolution_scale`, `with_ui_depth` and
`with_effect_scissor`, then hand the config back through
`Renderer::set_render_2d_config` (or `RenderContext::set_render_2d_config`).

`scaled_extent(width, height, scale)` is the single implementation of the
scale.  It treats a non-finite or non-positive scale as `1.0`, rounds each axis
and clamps it to at least one texel:

```text
scaled_extent(1600, 900, 0.5) -> (800, 450)
scaled_extent(10, 10, 0.01)   -> (1, 1)
scaled_extent(10, 10, NAN)    -> (10, 10)
```

The layer pool, effect scratch targets, backdrop snapshots and the layer
viewport should all use the same scaled extent.  `1.0` is an exact identity,
so the default layered path allocates at surface size.

### Lazy swapchain target

`RenderTarget::new` allocates a color resolve plus a depth attachment eagerly.
A `Pure2D` application that renders straight to the surface never reads its
internal target, so those attachments are dead weight.  `RenderTarget::lazy`
returns a `LazyRenderTarget` (`crates/ferrous_renderer/src/render_target/target.rs`)
that allocates nothing until `ensure_color_depth(device, width, height, format,
samples)` is called.  Call it only when the frame actually renders to
`RenderDest::Target`; a view/surface frame skips the allocation.  The eager
constructor remains for callers that need the attachments at construction.

## P5 baseline

Measured on a Pure2D application (`sample_count = 1`, no `with_msaa`) at
1600x900.  Each frame encodes two passes and the GPU time stays small; the
large VRAM numbers come from textures, not from per-pass work.

| Scenario | Encoded passes | Timestamped GPU time | VRAM |
|---|---|---:|---:|
| Idle 2-D | 2 | not sampled (first frames) | 24.1 MB |
| Editor UI | 2 | 0.43 ms Pure2D + 0.43 ms GUI (hover) | 35.8 MB idle / 37.7 MB hover |
| Heavy layered composition | 2 | 0.28 ms GUI; layered Pure2D pass not timestamped | 67.3 MB |

Every pass in the baseline is at or below about 2 ms; the heavy-UI sample is
the only one that crosses it (`Pure2D Main Pass` 1.89 ms + GUI 0.17 ms =
2.06 ms).  The two-pass count is constant; what changes is how much of it is
timestamped.

The 2-D-only memory breaks down as follows:

- **GUI depth**: 5.76 MB at 1600x900 (`Depth32Float`, one sample).  It is a
  full-surface texture allocated by the GUI renderer and used for Early-Z.
- **Eager `RenderTarget`**: 11.52 MB, i.e. 5.76 MB color resolve plus 5.76 MB
  depth.  In `Pure2D` this target is not sampled, which is exactly what the
  lazy constructor avoids.
- **Layer/effect targets**: allocated at full surface size.  The heavy
  composition sample reports 34.56 MB of `2d:layer textures`, which dominates
  its 67.3 MB total.

Lowering `layer_resolution_scale` reduces the last item quadratically, and the
lazy `RenderTarget` removes the second when no internal-target frame runs.  The
GUI depth remains unless `ui_depth` is disabled; the visual result is unchanged
for painter-ordered UI because the depth attachment only participates in
Early-Z.

## Reading the memory report

`Renderer::memory_report` (`crates/ferrous_renderer/src/renderer_core.rs:2104`)
returns a `ferrous_core::memory::Report`.  `gpu_bytes` is the sum of every
registered GPU allocation, `by_subsystem` groups them and `largest` lists the
24 biggest entries.  Labels are stable, so re-registering a label replaces its
size instead of adding a duplicate.

Use the report to answer two questions:

1. **Is an attachment missing from the inventory?**  Every creation site should
   register its bytes and every pool drop should unregister them.
2. **Did a knob reduce memory?**  Compare the `render_target`, `renderer_2d`
   and `gui` subsystem totals before and after changing
   `layer_resolution_scale`, `ui_depth` or the lazy target.

The numbers are requested allocation sizes, not driver residency or
fragmentation.  They are the right tool for spotting growth and regressions,
not for predicting process private bytes.
