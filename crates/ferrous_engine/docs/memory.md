# GPU memory: hints, inventory and budgets

This page is the reference for how FerrousEngine asks the driver to allocate
device memory, how every engine-owned GPU resource is counted, and how to read
the resulting inventory.  It covers the memory-hints policy, the registry
labels, the per-resolution budgets and the steady-state test that guards the
inventory.

## Allocation hints

wgpu 23 passes a `wgpu::MemoryHints` value to `DeviceDescriptor`.  With the
default `MemoryHints::Performance`, wgpu-hal's Vulkan backend keeps 128 MB
free-list chunks per memory type so that large, frequently reused resources get
cheap sub-allocations.  A UI or editor process allocates mostly small buffers
and textures, so those chunks are reserved but almost entirely unused.  The same
build with `MemoryHints::MemoryUsage` uses small sub-allocation blocks instead.
Measured on the start screen with an NVIDIA Vulkan user-mode driver: 465 MB
private bytes at idle, of which roughly 126 MB is the instance, adapter and
device themselves and roughly 300 MB came from constructing the Flat2D
renderer.  `MemoryUsage` ended the same build at about 265 MB private (down 38 %)
and halved the working set.  This is the single largest memory win on this
layer.

The engine's policy is deliberately mode-driven rather than global:

- `AppConfig::memory_hints` is `None` by default
  (`crates/ferrous_engine/src/builder.rs:153`).
- `AppConfig::effective_memory_hints` resolves that `None`: `Flat2D` (and the
  Pure2D renderer it maps to) selects `MemoryUsage`, while `Game3D` keeps
  `Performance` (`crates/ferrous_engine/src/config.rs:55`).
- An explicit value always wins, either programmatically through
  `AppConfig::with_memory_hints` (`crates/ferrous_engine/src/config.rs:67`) or
  `App::with_memory_hints` (`crates/ferrous_engine/src/builder.rs:659`), or from
  `ferrous.toml` with `[renderer] memory_hints = "memory_usage" | "performance"`
  (`crates/ferrous_engine/src/config.rs:123`, applied at
  `crates/ferrous_engine/src/config.rs:201`).
- The runner copies the resolved value into `GpuInit`
  (`crates/ferrous_engine/src/graphics.rs:83`) and forwards it when the device
  is created (`crates/ferrous_engine/src/graphics.rs:465`).
- `EngineContext::from_adapter_with` is the only function that forwards hints to
  `DeviceDescriptor` (`crates/ferrous_gpu/src/context.rs:154`).
  `EngineContext::from_adapter` and `from_adapter_with_cache` keep the historical
  `Performance` default (`crates/ferrous_gpu/src/context.rs:121` and
  `crates/ferrous_gpu/src/context.rs:137`), so headless harnesses, benchmarks and
  examples keep their previous behaviour unless they opt in.

Rationale: editor and 2-D apps pay the performance policy's 128 MB chunks
per memory type for allocations that are mostly a few kilobytes.  Games keep
`Performance` because their buffers and textures are large enough for the bigger
sub-allocations to pay off.

## Reading `memory_report`

`Renderer::memory_report` (`crates/ferrous_renderer/src/renderer_core.rs:2060`)
returns a snapshot of `ferrous_core::memory::Report`:

| Field | Meaning |
|---|---|
| `gpu_bytes` | Sum of every registered GPU resource. |
| `cpu_bytes` | Sum of registered CPU-side copies (atlas pixels, decoded images). |
| `total_bytes` | `gpu_bytes + cpu_bytes`, saturating. |
| `by_subsystem` | Totals per `Subsystem`, largest first. |
| `largest` | The 24 biggest entries, descending. |

The registry is explicit: subsystems call
`memory::register_gpu` (`crates/ferrous_core/src/memory.rs:148`) at resource
creation and on resize, and `memory::unregister_gpu`
(`crates/ferrous_core/src/memory.rs:159`) when a pooled allocation is dropped.
The label is the identity, so registering the same label again replaces the
previous byte count instead of adding a second entry.
`memory::report` builds the snapshot (`crates/ferrous_core/src/memory.rs:178`).
The numbers are requested sizes, not driver bookkeeping, residency or
fragmentation; they are the right tool for spotting growth and missing
registrations, not for predicting private bytes exactly.

## Inventory labels

`Subsystem` names are stable (`crates/ferrous_core/src/memory.rs:30`):
`renderer_2d`, `gui`, `renderer`, `render_target`, `fonts`, `assets`, `engine`
and `other`.

The renderer registers four groups:

| Group | Labels | Subsystem | Registered at |
|---|---|---|---|
| Swapchain attachments | `rt:color resolve`, `rt:color msaa`, `rt:depth` | `render_target` | `crates/ferrous_renderer/src/renderer_core.rs:2075`, refreshed by `Renderer::resize` (`crates/ferrous_renderer/src/renderer_core.rs:1632`) |
| HDR attachments | `3d:hdr resolve`, `3d:hdr msaa` | `render_target` | `crates/ferrous_renderer/src/renderer_core.rs:2105`, built by `ensure_three_d` (`crates/ferrous_renderer/src/renderer_core.rs:1436`) |
| 3-D group | `3d:shadow map`, `3d:instance buffer`, `3d:shadow instance buffer`, `3d:prepass normal-depth resolve`, `3d:prepass normal-depth msaa`, `3d:prepass depth`, `3d:ssao raw`, `3d:ssao blurred`, `3d:ssao intermediate`, `3d:ssao kernel`, `3d:ssao params`, `3d:ssao noise`, `3d:particles storage`, `3d:particles emitter`, `3d:bloom chain`, `3d:aa output`, `3d:aa smaa edge`, `3d:aa smaa blend`, `3d:aa dummy` | `renderer` | `crates/ferrous_renderer/src/renderer_core.rs:2105`, built by `ensure_three_d` (`crates/ferrous_renderer/src/renderer_core.rs:1436`); instance buffers are refreshed after a scene sync (`crates/ferrous_renderer/src/renderer_core.rs:2966`) |
| Pure2D layer pools | `2d:layer textures`, `2d:layer effect targets`, `2d:layer backdrops`, `2d:effect uniform pool`, `2d:composite instances` | `renderer_2d` | `crates/ferrous_renderer/src/renderer_core.rs:2245`, created per layer index on first use and reused afterwards |
| Existing 2-D, GUI, font and asset resources | `2d:*`, `gui:*`, `font:*`, `2d:texture <id>` | their owning crate | their renderers |

The 3-D group is fully deferred: a `Flat2D`/`Pure2D` renderer never registers
any `3d:` label until `ensure_three_d` runs.  Skinning and gizmos have no
persistent GPU allocation of their own (they create vertex buffers per frame),
so they have no inventory entry.  The bloom chain exists only after the first
resize; before that the pass has no bloom textures and its entry is absent.

The layer pools are aggregated instead of registering one label per index.  The
aggregate is recomputed from the live textures, so a second frame with three
layers overwrites the same three labels rather than counting the pool twice.
`Renderer::unregister_layer_memory` removes the pool entries when the pool is
invalidated on resize (`crates/ferrous_renderer/src/renderer_core.rs:2306`);
the composite buffer and effect uniform pool survive a resize, so their entries
stay valid.

## Bytes at 1600x900, MSAA 4

The table is the requested size for the same scene at a 1600x900 target with
4x MSAA.  The surface format is Bgra8UnormSrgb (4 bytes per texel).

| Label | Bytes | Notes |
|---|---:|---|
| `rt:color resolve` | 5,760,000 | full size, 1 sample |
| `rt:color msaa` | 23,040,000 | full size, 4 samples |
| `rt:depth` | 23,040,000 | Depth32Float, 4 samples |
| `3d:hdr resolve` | 11,520,000 | Rgba16Float |
| `3d:hdr msaa` | 46,080,000 | Rgba16Float, 4 samples |
| `3d:prepass normal-depth resolve` | 11,520,000 | Rgba16Float |
| `3d:prepass normal-depth msaa` | 46,080,000 | Rgba16Float, 4 samples |
| `3d:prepass depth` | 23,040,000 | private Depth32Float, 4 samples |
| `3d:shadow map` | 4,194,304 | 1024x1024 Depth32Float |
| `3d:ssao raw` | 1,440,000 | half resolution, Rgba8Unorm |
| `3d:ssao blurred` | 1,440,000 | half resolution, Rgba8Unorm |
| `3d:ssao intermediate` | 1,440,000 | half resolution, Rgba8Unorm |
| `3d:ssao kernel` | 1,024 | 64 vec4 samples |
| `3d:ssao params` | 176 | uniform |
| `3d:ssao noise` | 64 | 4x4 Rgba8Unorm |
| `3d:instance buffer` | 65,536 minimum | 1024 slots x 64 bytes, grows with the scene |
| `3d:shadow instance buffer` | 65,536 minimum | same growth rule |
| `3d:particles storage` | 32,000,000 | 1,000,000 particles x 32 bytes |
| `3d:particles emitter` | 64 | uniform |
| `3d:bloom chain` | 6,715,200 | 5 levels plus accumulation, after the first resize |
| `3d:aa output` | 11,520,000 | Rgba16Float |
| `3d:aa smaa edge` | 5,760,000 | Rgba8Unorm |
| `3d:aa smaa blend` | 5,760,000 | Rgba8Unorm |
| `3d:aa dummy` | 8 | 1x1 Rgba16Float |
| `2d:layer textures` | 11,520,000 per layer | Rgba16Float full-size pool |
| `2d:layer effect targets` | 23,040,000 per layer | two full-size scratch textures |
| `2d:layer backdrops` | 11,520,000 per snapshot | only for backdrop-aware blends |
| `2d:composite instances` | grows in powers of two | 16 instances minimum |

## Budgets

On the measured NVIDIA Vulkan driver, instance, adapter and device creation
account for about 126 MB private and are not reducible by the engine.  The
start-screen budget is therefore stated on top of that:

- Flat2D/editor with `MemoryUsage`: target under 150 MB of renderer-attributable
  private bytes, which keeps an idle start screen under roughly 300 MB private
  (the measured build lands near 265 MB).
- Full3D at 1600x900, MSAA 4: the attachments above plus the 3-D group dominate.
  The particles alone are 30.5 MiB, the HDR and prepass MSAA attachments are
  about 44 MiB each, and the swapchain attachment set is another 49 MiB.  Budget
  these as fixed costs when sizing a 3-D scene; instance buffers then scale with
  the entity count.
- Pure2D layers are the main variable cost: every simultaneously open layer
  costs one `2d:layer textures` entry, and a layer with effects or a mask adds
  two effect targets.  Reuse layers instead of opening many per frame.

## The flat-inventory test

`crates/ferrous_renderer/tests/memory_flat.rs:113` builds a headless
`Pure2dHarness` and draws a non-trivial scene (interleaved shapes, optional
Roboto text, one offscreen layer with a blur effect and two overlapping
rectangles).  It renders 30 warm-up frames
(`crates/ferrous_renderer/tests/memory_flat.rs:20`), takes a snapshot, renders
30 more (`crates/ferrous_renderer/tests/memory_flat.rs:21`) and asserts that
`gpu_bytes`, `cpu_bytes` and `total_bytes` changed by at most 256 bytes
(`crates/ferrous_renderer/tests/memory_flat.rs:32`).

The 256-byte tolerance exists because every pool is established during warm-up,
so the steady-state delta is zero; the slack only covers a single late 64-byte
uniform or buffer registration.  A leak of one 32-byte entry per frame already
exceeds it across the 30 measured frames (960 bytes).  The test additionally
asserts that the layer texture counter did not change, which catches a leaked
256x256 Rgba16Float layer (512 KiB) immediately.  The test skips with a printed
reason when no adapter is available, and it never opens a window.

When adding a GPU resource, register it where it is created and overwrite the
same label on resize; do not register inside the per-frame draw path.  If a
pool can be dropped, unregister its labels at the same place the pool is
invalidated.
