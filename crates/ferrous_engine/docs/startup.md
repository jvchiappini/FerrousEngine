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
