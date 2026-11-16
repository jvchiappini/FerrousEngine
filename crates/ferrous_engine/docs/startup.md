# Startup trace

The engine can record how long every step between the process start and the
first presented frame takes, and print it as one table.  The trace lives in
`ferrous_core::startup` (re-exported as `ferrous_engine::startup`), so it
needs no GPU and no window to compile.

## Enabling it

Two switches, one trace:

- `FERROUS_TRACE_STARTUP=1` (`true`, `yes` and `on` also work) enables it for
  any application without a rebuild.  An application that wants the whole of
  `main` measured calls `startup::begin_from_env()` as its first statement;
  the engine reuses that origin when `App::run` starts.
- `App::new(app).with_startup_trace(true)` enables the same trace from code
  (`AppConfig::startup_trace`).

`FERROUS_TRACE_STARTUP_JSON=1` additionally prints one machine-readable line
per report, which build and benchmarking tools parse.

## The marks the engine records

| Mark | Taken |
|---|---|
| `engine: entry` | `App::run` begins |
| `engine: event loop` | the winit event loop exists |
| `engine: window created` | the window exists, before any GPU work |
| `engine: instance` | `wgpu::Instance` created |
| `engine: adapter` | the adapter was selected |
| `engine: device` | device, queue and pipeline cache exist |
| `engine: surface configured` | the swapchain was configured |
| `engine: renderer built` | `Renderer::new` returned |
| `engine: fonts ready` | the engine UI font was baked and uploaded |
| `engine: app setup done` | `FerrousApp::setup` returned |
| `engine: first frame presented` | the first `frame.present()` succeeded |
| `engine: close requested` | the close button or `request_exit` |
| `engine: window hidden` | the window left the screen, before teardown |
| `engine: event loop exited` | `run_app` returned |
| `engine: teardown done` | the window and GPU state were dropped |

The delta between two consecutive marks is the phase that took that long:
`adapter - instance` is adapter enumeration, `device - adapter` is
`request_device`, `surface configured - device` is the swapchain, and so on.

## Adding your own marks

Marks are points, spans are durations; both are free when the trace is off.

```rust
use ferrous_engine::startup;

startup::mark("myapp: settings loaded");
{
    let _span = startup::span("myapp: decode images");
    // ...
}
// A computed label: the closure only runs when tracing is enabled.
let _span = startup::span_with(|| format!("myapp: bake {family}"));
```

Names are free-form; by convention the engine uses `engine: <phase>` and
applications use `<app>: <phase>`.

## Reports

At the first presented frame the engine writes one table to stderr:

```
startup trace (ms since trace start)
  mark                                         at       delta
  engine: entry                              0.02        0.02
  engine: event loop                        12.90       12.88
  engine: window created                    44.61       31.71
  engine: instance                         290.12      245.51
  engine: adapter                          540.31      250.19
  engine: device                           570.44       30.13
  engine: surface configured               762.10      191.66
  engine: renderer built                   827.02       64.92
  engine: fonts ready                      942.31      115.29
  engine: app setup done                  1620.80      678.49
  engine: first frame presented           1651.02       30.22
  total                                   1651.02
```

A second table is printed after teardown, so the closing path is measured
with the same marks (`close requested` → `window hidden` → `teardown done`).

With `FERROUS_TRACE_STARTUP_JSON=1` each table is followed by one line:

```
startup-trace {"event":"startup-trace","stage":"first-present","trace":{...}}
startup-close {"event":"startup-close","stage":"exit","trace":{...}}
```

The prefix identifies the line among the application's own logging; the
`trace` object carries `version`, `total_ms`, `marks[]` and `spans[]` with
times in milliseconds since the origin.

## Cost

- Disabled: every `mark`/`span` call is one relaxed atomic load and a branch.
  Nothing is allocated and `span_with` labels are not formatted.
- Enabled: one `Instant::now()` and one `String` per mark, plus a mutex push.
  That is noise next to the phases it measures, but do not trace per glyph.
- The report is formatted at the first present *after* the first-present mark
  is taken, so printing does not inflate the measured time.

## Limitations

- The origin is the first `startup::begin` call, not the OS process creation
  time. Calling `begin_from_env()` at the top of `main` makes the difference
  a few hundred microseconds; the loader and Defender scan (first run after a
  rebuild) are outside the trace.
- `mark` records the calling thread's clock; spans opened on worker threads
  are safe and appear in the same report.
- On wasm32 the trace compiles and records, but no teardown report is
  emitted because the browser owns the page lifetime.

## GPU startup policy

The engine starts the GPU by explicit phases and never pays for a backend it
does not use.

### Backend order

`AppConfig::backends` is an ordered list, one `wgpu::Instance` per entry. The
engine moves to the next entry only when the previous one has no adapter (or
an adapter that cannot present on the window). The default is Vulkan, then
DX12, then GL on Windows; Metal on macOS; Vulkan, then GL elsewhere.

On the reference machine the default order saves the DX12 and GL enumeration:
`Instance::new(Backends::all())` took ~245 ms in Vulkan plus ~250 ms more
enumerating DX12 and GL, while a Vulkan-only instance takes ~200 ms and its
adapter is selected in ~1 ms.

`WGPU_BACKEND` (e.g. `vulkan`, `dx12`, `gl`) replaces the whole list with
exactly those backends. A backend whose adapter lacks
`TEXTURE_BINDING_ARRAY | SAMPLED_TEXTURE_AND_STORAGE_BUFFER_ARRAY_NON_UNIFORM_INDEXING`
(the GUI image pass) is skipped with a warning instead of panicking later in
pipeline validation; GL in wgpu 23 is such a backend.

### Overlap

`App::run` starts two workers before the window exists:

- the GPU probe: instance creation and a surface-less adapter request (the
  adapter is re-checked against the real surface with
  `Adapter::is_surface_supported` and re-probed if it cannot present);
- the UI font bake: `BakedAtlas::build_cached` for the app's UI font, joined
  at upload.

The window is created in parallel, so window creation is not behind the
driver. Applications should start their own CPU-only work (document load,
font bakes, asset decode) before `App::run` for the same reason.

### Pipeline cache

`App::with_pipeline_cache(PipelineCacheConfig::for_app("MyApp"))` seeds the
wgpu pipeline cache from
`%LOCALAPPDATA%\<app>\cache\pipelines\<key>.bin` and writes it back after the
first frame and at exit. The key covers backend, vendor, device, driver and
driver version; a driver update starts from an empty cache. The file has a
small header (magic, format version, key) so a foreign or truncated file is
ignored; wgpu also runs with `fallback: true`, so bad bytes only cost a
recompile. Only backends that expose `PIPELINE_CACHE` use it (Vulkan in wgpu
23); DX12 and Metal ignore it.

On the reference machine (Vulkan, NVIDIA) the cache takes `Renderer::new`
from ~68 ms to ~48 ms. DX12 has no cache and takes ~7.3 s in `Renderer::new`
(shader compilation), so it stays a fallback: a machine with Vulkan never
pays it.

### Window with no white flash

The window is created hidden. On Windows the engine sets the window class
background brush to `AppConfig::background_color`, shows the window and forces
`WM_ERASEBKGND`, so the first pixel the user sees is the app background; winit
registers its class without a brush, which is what makes a fresh window white.
On other platforms the window is shown after the first clear-only frame is
presented (the engine presents one clear frame as soon as the surface is
configured, before the renderer and fonts exist).

Measured on the reference machine: the window is visible and correctly
coloured ~50 ms after process start, and the first presented frame replaces it
with the same background colour.

### Power preference

`AppConfig::power_preference` selects the GPU (default
`HighPerformance`, the discrete one on a hybrid laptop). `WGPU_POWER_PREF=low`
picks the integrated GPU, `none` lets wgpu decide. Startup cost on the
integrated GPU is comparable: the swapchain is cheaper (~150 ms versus
~190 ms) while shader compilation is a little slower.
### Launch snapshot

The GPU driver floor is ~430 ms per process and nothing about it can be
cached. An application can show its own last screen during that time:

- `App::with_launch_snapshot(LaunchSnapshot::for_app("MyApp").with_key(key))`
  opts in. `key` is the application's content key: its version plus a hash of
  everything the snapshotted screen shows. The engine adds the window size,
  scale factor, monitor, surface format and its own version.
- `ctx.request_launch_snapshot()` in `update` or `draw_ui` stores the frame
  about to be presented; `ctx.set_launch_snapshot_key(key)` updates the key
  when the content changes. One capture per key per run; the readback is the
  screenshot path (one frame, then a worker writes the file).
- At launch a worker reads the newest file before the window exists. A file
  whose identity does not match (content key, size, scale, monitor) is
  ignored, so a stale screen is never shown.

On Windows the snapshot is a child window of the app window (`WS_EX_TRANSPARENT
| WS_EX_NOACTIVATE`, `HTTRANSPARENT`) painted with `SetDIBitsToDevice`, created
before the window is shown and destroyed in the same message-loop turn as the
first present. A child window is used instead of subclassing winit's window
procedure: winit owns `GWLP_USERDATA`, and the child answers hit-testing with
`HTTRANSPARENT`, so clicks, keys, the wheel and the close button reach the app
(the runner queues input until the handoff). The GDI child and the wgpu
swapchain coexist: the snapshot stays visible through `surface.configure`, the
clear-only frame and a slow `Renderer::new` (verified under Vulkan and DX12,
including DX12's ~8 s shader compilation).

Input produced while the snapshot is up is queued in order and replayed right
after the first present; a close is honoured at once; a resize or DPI change
drops the snapshot and falls back to the background colour.

The file is a fixed header plus tight RGBA8: decoding is a copy, which is the
fastest option at 1600x900 (5.5 MiB; a lossless codec would trade decode time
for bytes). A foreign, truncated, oversized or corrupt file is ignored. On
platforms without a painter the engine shows the background colour instead.

Measured on the reference machine (Vulkan, 1600x900): snapshot visible at
51 ms median (54 ms p90) from process start; the handoff and the first GPU
frame land in the same instant at ~490 ms; a `preview` window with no snapshot
shows at ~54 ms and its first frame at ~1509 ms. Read 8.4 ms (worker, overlapped
with window creation), capture 0.9 ms, file write 6.8 ms (worker), file 5.5 MiB.
The switch is pixel-identical: a screen capture of the snapshot and of the
first GPU frame differ from the stored image in 0 of 720000 interior pixels
(the only differences are DWM's rounded window corners).