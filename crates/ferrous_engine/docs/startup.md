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
| `engine: first frame presented` | the first `frame.present()` succeeded, before `ShowWindow` |
| `engine: window visible` | the window was shown carrying the first frame |
| `engine: window visible (timeout)` | the window was shown with the background because the first frame was late |
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
pipeline validation; GL in wgpu 23 is such a backend. See
[GL is rejected for Flat2D](#gl-is-rejected-for-flat2d) below.

### Choosing a backend from code

The automatic per-OS order above is only the **default**. A developer can pick
any backend the engine was compiled with:

```rust
use ferrous_engine::{App, Backend};

// One backend, readable shorthand:
App::new(MyApp).with_backend(Backend::Dx12).run();

// A custom ordered fallback list:
App::new(MyApp)
    .with_backends([wgpu::Backends::GL, wgpu::Backends::VULKAN])
    .run();
```

`Backend` is `Vulkan`, `Dx12`, `Gl`, `Metal`, `WebGpu`; `Backend::parse` reads
the same names as `WGPU_BACKEND`. At run time `WGPU_BACKEND` wins over both
methods, so a shipped choice can be overridden without a rebuild.

Every backend the engine is compiled with runs the same app. If the machine has
no adapter for the requested one, the engine stops with a named error that lists
the requested backends and how to change them (exit code 2), instead of a panic
or a silent fallback:

```text
FerrousEngine cannot start: no usable GPU backend among [GL]. Every requested
backend had no adapter or lacks TEXTURE_BINDING_ARRAY | ... . Install or update
the GPU driver, or choose another backend with WGPU_BACKEND (vulkan, dx12, gl)
or App::with_backend/with_backends.
```

A per-backend smoke run on this host (the app must reach its first frame):

```powershell
foreach ($b in "vulkan","dx12","gl") {
    $env:WGPU_BACKEND = $b
    $env:FERROUS_TRACE_STARTUP = "1"
    cargo run --release -p guimaker-compare-guimaker -- --seconds 4 2>&1 |
        Select-String "engine: first frame presented|no usable GPU backend"
}
Remove-Item Env:WGPU_BACKEND,Env:FERROUS_TRACE_STARTUP
```

GL reaches the first frame only after the non-bindless GUI image path lands
(prompt 15 Phase 9D Lane A); until then it stops with the named error above.

### Measuring the per-backend startup breakdown

The trace is the measurement tool for the backend decision (`G-0013`). Keep the
binary and the scenario identical across backends; change only `WGPU_BACKEND`
(and, for DX12, `FERROUS_DX12_COMPILER`). Read these marks and their deltas:

| Mark | What it isolates |
|---|---|
| `engine: entry` | process entry to `App::run` (loader/Defender are outside) |
| `engine: window created` | winit window creation, overlaps the GPU probe |
| `engine: instance` | `wgpu::Instance` creation for that one backend |
| `engine: adapter` | adapter enumeration and selection |
| `engine: device` | `request_device` and the pipeline cache load |
| `engine: surface configured` | swapchain creation/configure |
| `engine: renderer built` | every pipeline compile (the DX12 cost) |
| `engine: first frame presented` | the first full frame, the headline number |
| `engine: window visible` | the first composed frame the user sees |

`engine: renderer build (worker)` is the same `Renderer::new` span on the
worker. The start of the run to `engine: entry` is not traced; call
`startup::begin_from_env()` as the first statement of `main` if the loader cost
matters.

#### Cold and warm

`Renderer::new` is the phase that changes most between runs. Measure both:

- **Cold:** point the pipeline cache at a **fresh directory under `out/`**, e.g.
  `out/pipeline-cache-cold-vulkan-1`, for every cold run. wgpu's Vulkan cache is
  the only one that speeds this up in wgpu 23, so a cold run is the honest
  pipeline-build cost. Do **not** delete `%LOCALAPPDATA%\<app>\cache\pipelines`
  (or any existing cache): leave prior caches in place and give the cold run its
  own directory.
- **Warm:** immediately repeat the same binary with the same cache directory the
  cold run just populated. A warm run after a rebuild is not warm.

A harness that owns `AppConfig` sets it with
`PipelineCacheConfig { directory: Some(PathBuf::from("out/pipeline-cache-...")),
..PipelineCacheConfig::for_app("guimaker-compare-guimaker") }`; when the
compare app does not expose the directory, the default is
`%LOCALAPPDATA%\guimaker-compare-guimaker\cache\pipelines` and a cold run needs
a fresh OS account or a copied-out cache directory rather than a deletion.

#### Five runs, alternating

Run every backend five times, alternating cold then warm, and alternate the
backends as well so driver/Defender drift shows up:

```
vulkan cold, vulkan warm, dx12 cold, dx12 warm, gl cold, gl warm,
vulkan cold, vulkan warm, dx12 cold, dx12 warm, ...
```

Keep one process per launch and a clean environment per run (clear
`WGPU_BACKEND`, `FERROUS_DX12_COMPILER`, `VK_LOADER_*` before setting the next
run's values). Record the median and the min/max; a single run is not evidence.
The DX12 renderer build is longer than the 3 s bench watchdog, so pass
`--seconds 15` (as `G-0012` did). DX12 ignores wgpu's pipeline cache in wgpu 23,
so its warm run only benefits from the OS file cache; keep cold and warm labelled
correctly anyway.

#### Proving DXC actually loaded (DX12)

`FERROUS_DX12_COMPILER=dynamic_dxc` (or `dxc`, with the `dxc` feature) only
measures DXC if `dxcompiler.dll` and `dxil.dll` are found. wgpu-hal logs the
decision at debug level:

```powershell
$env:RUST_LOG = "wgpu_hal=debug"
$env:WGPU_BACKEND = "dx12"
$env:FERROUS_DX12_COMPILER = "dynamic_dxc"
$env:FERROUS_TRACE_STARTUP = "1"
cargo run --release -p guimaker-compare-guimaker --features ferrous_engine/dxc -- --seconds 15
```

A DXC run prints `Using DXC for shader compilation`; an FXC run prints
`Using FXC for shader compilation`. If `dxcompiler.dll` or `dxil.dll` is missing
wgpu logs `Failed to load dxcompiler.dll. Defaulting to FXC instead` (same for
`dxil.dll`) and then the FXC line, so that launch measured FXC. The application
must initialise a logger for these lines to appear (the compare apps do). A run
without the DXC line is an FXC sample, not a DXC sample.

#### Vulkan loader layers

`VK_LOADER_DEBUG=layer` makes the Vulkan loader print the layers it enumerates
and enables, which is how a measurement explains an instance/adapter outlier
caused by an implicit layer (Steam, RTSS, a vendor overlay).
`VK_LOADER_LAYERS_DISABLE=~implicit~` disables every implicit layer for one
run, so the same binary can be compared with and without them:

```powershell
$env:VK_LOADER_DEBUG = "layer"
$env:VK_LOADER_LAYERS_DISABLE = "~implicit~"   # diagnostic only
```

This is diagnostic only. The engine must **never** set `VK_LOADER_LAYERS_DISABLE`
(or any other loader switch) by default: a user's implicit layers are the user's
choice, and silently disabling them hides validation and tooling. Report the
before/after numbers; do not change the app's default environment.

### GL is rejected for Flat2D

`WGPU_BACKEND=gl` is accepted by the configuration path (it becomes a
one-element backend list), but it cannot run the Flat2D renderer today:

- The probe's adapter is checked by `adapter_is_usable` in
  `crates/ferrous_engine/src/graphics.rs:218-227` against
  `ENGINE_ADAPTER_FEATURES` (`graphics.rs:175-183`). With the default `assets`
  feature that set is
  `TEXTURE_BINDING_ARRAY | SAMPLED_TEXTURE_AND_STORAGE_BUFFER_ARRAY_NON_UNIFORM_INDEXING`.
- wgpu 23's GL adapter does not expose those features, so the probe logs
  `adapter ... cannot run the engine: missing ...` and the re-probe rejects it
  too; the run then stops with the named `no usable GPU backend` error (exit
  code 2, no panic).
- That is a capability limit of the GUI image path, not a bug in the backend
  request. Until the non-bindless image path lands (prompt 15 Phase 9D Lane A),
  GL is **not** a Flat2D startup measurement option; do not report a GL number
  before it does. Once it lands, `WGPU_BACKEND=gl` (or
  `App::with_backend(Backend::Gl)`) runs the same GUIMaker app and joins the GL
  tier.

To confirm the rejection on a machine:

```powershell
$env:WGPU_BACKEND = "gl"
$env:FERROUS_TRACE_STARTUP = "1"
cargo run --release -p guimaker-compare-guimaker -- --seconds 8 2>&1 |
    Select-String "adapter|no usable wgpu adapter"
```

### DX12 shader compiler (FXC vs DXC)

DX12 compiles its 2D pipelines with a shader compiler chosen when the
`wgpu::Instance` is created. wgpu 23 puts that choice on
`wgpu::InstanceDescriptor::dx12_shader_compiler` (**not** the device
descriptor), so the engine threads it into both the probe worker and the
event-loop re-probe. `AppConfig::dx12_compiler` (or
`App::with_dx12_compiler`) selects it from code; the
`FERROUS_DX12_COMPILER=fxc|dxc|dynamic_dxc` environment variable wins at
runtime.

- `fxc` (default): the legacy compiler, no extra DLLs, and the slow renderer
  build recorded by `G-0011`.
- `dxc` / `dynamic_dxc`: wgpu loads `dxcompiler.dll` and `dxil.dll` at runtime
  (standard DLL search rules) and falls back to Fxc with a logged warning when
  either is missing. Both values need the `dxc` cargo feature on
  `ferrous_engine` so the default build keeps Fxc; wgpu 23 needs no extra
  dependency (its default `dx12` feature already includes DXC support).
  `dynamic_dxc` is the future wgpu name for the same runtime-loaded choice.
  Put both DLLs next to the executable or on `PATH`; when either is missing
  wgpu logs a warning and compiles with Fxc, so that DXC run is really an FXC
  run.

A/B run (PowerShell, from the workspace root, one process per line):

```powershell
# DX12 + FXC (default build)
$env:WGPU_BACKEND = "dx12"; $env:FERROUS_DX12_COMPILER = "fxc"
$env:FERROUS_TRACE_STARTUP = "1"; $env:FERROUS_TRACE_STARTUP_JSON = "1"
cargo run --release -p guimaker-compare-guimaker -- --seconds 8

# DX12 + DXC (runtime-loaded dxcompiler.dll/dxil.dll)
$env:FERROUS_DX12_COMPILER = "dynamic_dxc"
cargo run --release -p guimaker-compare-guimaker --features ferrous_engine/dxc -- --seconds 8
```

Read `engine: renderer built` (the pipeline compile) and `engine: first frame
presented` from the trace. DX12 ignores wgpu's pipeline cache in wgpu 23, so a
warm run only benefits from the OS file cache; keep the two runs identical and
alternate them.

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
pays it. Selecting DXC (see
[DX12 shader compiler](#dx12-shader-compiler-fxc-vs-dxc)) is the lever for
that DX12 build cost; the backend order itself is unchanged.

### Show the window when it is ready

The window is created hidden and the first frame is prepared behind it. The
window is then shown right after that frame is presented, so the first composed
pixel is the application, not a white or black rectangle
(`AppConfig::show_window_when_ready`, default on).

The GPU state is split in two:

- `GraphicsState::init_device` (instance, adapter, device, swapchain) stays on
  the event-loop thread: DXGI swapchains are not usable from another thread
  (a surface created off the window thread fails `is_surface_supported`).
- `Renderer::new` — the part that takes seconds on a cold DX12 driver — runs on
  a worker (`engine: renderer build (worker)` in the trace). The event loop
  keeps running, so the window can still be shown if the driver is slow.

A hidden window does not receive `RedrawRequested` from winit, so the runner
renders the first frame itself as soon as the worker delivers
(`Runner::about_to_wait`), presents it while the window is hidden, and only then
calls `ShowWindow`. Windows 11's open fade is disabled for that window
(`DWMWA_TRANSITIONS_FORCEDISABLED`): otherwise the fade blends the frame with
whatever was on screen and the first visible moment would not be the frame.

**Safety timeout** (`AppConfig::window_show_timeout`, default 800 ms from the
start of the run; `FERROUS_SHOW_TIMEOUT_MS` overrides it): if the first frame
is not presented by then, the window is shown with
`AppConfig::background_color`. The colour is painted into the client area
*before* `ShowWindow` (class brush + `FillRect`), so even the first composition
is the app colour. The class brush is created once per colour for the life of
the process (`winit` registers its class without one; a fresh brush per window
would leak a GDI handle). On other platforms the window is simply shown; the
next clear covers it.

Input that arrives while the window is hidden is queued in order and replayed
after the first frame; closing works at any time.

`engine: first frame presented` is marked as soon as `frame.present()` returns,
before `ShowWindow`; `engine: window visible` is marked after the show. That
order is what lets the regression guard prove the window never appeared before
its frame.

Measured on the reference machine (Vulkan, 1600x900, 20 warm runs, S6): the
first frame is ready at **471 ms** and the window is visible at **491 ms**
median (496 ms p90); `Renderer::new` takes 28 ms on the worker. The previous
S3b value was 517 ms with the same mechanism. With
`FERROUS_SHOW_TIMEOUT_MS=50` the window appears at 460-472 ms (the event loop
cannot run while the device phase blocks it, ~430-455 ms) painted with the
start screen's background `#0A0B0D`; DX12's 7.4 s `Renderer::new` shows the
window at 837 ms (timeout 800) and stays responsive (`SendMessageTimeout`
answers while the shaders compile). A real desktop capture of the first
composed frame (Vulkan, forced timeout, DX12) matches a capture taken 1 s later
exactly; the capture taken at the instant `IsWindowVisible` turns true still
shows the previous screen because DWM has not composed the window yet (one
vsync).

#### Timeout limitation

The safety timeout is checked by the event loop. `GraphicsState::init_device`
(instance, adapter, device and swapchain) runs on the event-loop thread when
the window is created, so **the timeout cannot fire while that call blocks the
loop**: it fires as soon as the loop regains control. On the reference machine
that floor is ~430 ms warm, so a 50 ms timeout still shows the window at
~450-500 ms; a cold driver can block longer. `Renderer::new` does run on the
worker, so DX12's ~7.4 s shader compilation does not stop the event loop and
the window appears at the timeout with the background colour and stays
responsive.

### Budgets and the regression guard

The trace is the measurement tool; a budget turns it into a guard without
depending on the GPU driver floor:

- **Our critical path** is `engine: first frame presented` minus
  `engine: surface configured`. Instance/adapter/device/swapchain costs are
  excluded, so a slower driver does not fail the guard.
- **Our spans** are budgeted directly (`motioner: host`, `document load`,
  `canvas`, `paint composition`, `engine: renderer build (worker)`, ...).
- **The window must not be visible before its first frame**: the normal-path
  `engine: window visible` mark must not precede `engine: first frame
  presented`. The timeout path is the only accepted exception and has its own
  `engine: window visible (timeout)` mark.

Motioner ships `scripts/startup_budgets.json` (2x the S6 measured values, with
50 ms for the close-to-hidden budget) and
`python scripts/startup_bench.py --check` reads it, runs a handful of processes
and exits non-zero on a violation. It is deliberately outside the CI gate:
first-run Defender scans and GPU state make wall-clock code checks
unreliable in CI, but the reviewer can run it on the reference machine (see the
Motioner README and `docs/startup.md`).

### Keeping startup fast in an application

- Do CPU-only work before `App::run` (document load, font bakes, asset decode):
  the engine creates the window and starts the GPU at the same time.
- Keep the pre-window path to listing and cheap lookups; never read file
  contents the first screen does not show.
- Move unbounded work (ffprobe, thumbnails, audio device open, network) to
  workers and fill placeholders after the first frame.
- Build only the renderer mode the first screen needs (`Flat2D`); 3-D passes
  and their VRAM are created on first use.
- Opt into the font atlas cache and the pipeline cache.

### Power preference

`AppConfig::power_preference` selects the GPU (default
`HighPerformance`, the discrete one on a hybrid laptop). `WGPU_POWER_PREF=low`
picks the integrated GPU, `none` lets wgpu decide. Startup cost on the
integrated GPU is comparable: the swapchain is cheaper (~150 ms versus
~190 ms) while shader compilation is a little slower.