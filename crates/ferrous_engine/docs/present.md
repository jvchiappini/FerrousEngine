# Present modes and frame pacing

The swapchain decides when a finished frame reaches the display. Until prompt
18 P5 the engine mapped a single boolean: `vsync = true` selected
`wgpu::PresentMode::Fifo` and `vsync = false` selected
`wgpu::PresentMode::AutoNoVsync` (`crates/ferrous_engine/src/graphics.rs`).
The engine now exposes all four modes, keeps `vsync` as an alias, and adds an
environment override so a release binary can be measured without a rebuild.

## The four modes

| Mode | Tearing | Pacing | Pending frames | Use when |
|---|---|---|---|---|
| `Fifo` (default) | none | locked to the display refresh | one queued frame | normal desktop rendering; the safe choice |
| `FifoRelaxed` | possible when a frame is late | refresh rate; a late frame presents immediately instead of waiting a whole vblank | one queued frame | you want tear-free normal pacing but no half-rate drop on a missed deadline |
| `Mailbox` | none | uncapped; a new frame replaces the pending one | one pending frame | lowest latency at high frame rates; smooth without a hard cap |
| `AutoNoVsync` | possible (when it resolves to `Immediate`) | uncapped, GPU throughput | driver default | throughput benchmarks and competitive settings |

`FifoRelaxed` and `Mailbox` are optional in wgpu. `AutoNoVsync` is wgpu's
portable "no vsync" request: it picks the first mode the backend exposes out of
`Immediate`, `Mailbox` and `Fifo`, so on a backend without `Immediate` it can
be tear-free but uncapped.

## Precedence

Three sources choose the mode, in strictly decreasing priority:

1. **`FERROUS_PRESENT_MODE`** environment variable.
2. **`AppConfig::present_mode`**, set with
   `AppConfig::with_present_mode(mode)` or `App::with_present_mode(mode)`.
3. **`AppConfig::vsync`** alias, used only when neither of the above is set:
   `true` means `PresentMode::Fifo`, `false` means
   `PresentMode::AutoNoVsync`.

`AppConfig::effective_present_mode()` implements that order
(`crates/ferrous_engine/src/builder.rs`). `GpuInit::from_config` resolves it at
startup and `graphics.rs` only sees the result. Changing
`FERROUS_PRESENT_MODE` after launch has no effect; restart the application.

The precedence is what makes the environment override work in the release
Motioner binary: Motioner calls `with_vsync(true)`, so without an explicit
mode it stays on `Fifo`, but `FERROUS_PRESENT_MODE=mailbox` wins over the
`vsync` alias without a code change.

Accepted `FERROUS_PRESENT_MODE` values (case-insensitive, whitespace trimmed):

| Value | Mode |
|---|---|
| `fifo` | `PresentMode::Fifo` |
| `fifo_relaxed` (or `fiforelaxed`) | `PresentMode::FifoRelaxed` |
| `mailbox` | `PresentMode::Mailbox` |
| `auto_no_vsync` (or `autonovsync`) | `PresentMode::AutoNoVsync` |

An unknown value logs a warning that lists the accepted names and is then
ignored, so the next source in the precedence order stays in charge instead of
silently changing pacing.

`ferrous.toml` has no present-mode key yet. Use
`AppConfig::with_present_mode`, `App::with_present_mode` or the environment
variable; the existing `[engine] vsync` key still drives the alias.

## Capability check and fallback

At device initialisation the engine asks the surface what it supports:

```text
surface.get_capabilities(&adapter).present_modes
```

(`crates/ferrous_engine/src/graphics.rs`). If the requested mode is not in that
list, the surface is configured with `Fifo` and the engine logs a warning that
names both the requested mode and the supported list. `Fifo` is the only
present mode wgpu guarantees on every backend, so it is the fallback.

Practical consequences:

- `Mailbox` and `FifoRelaxed` requests on a backend that does not expose them
  run as `Fifo` and log the downgrade.
- The application does not need to branch per backend; it asks for the mode it
  wants and reads the warning when the result differs.
- `GraphicsState::resize` mutates the stored `SurfaceConfiguration` and calls
  `surface.configure` again, so the resolved mode survives resizes. A resize
  never re-probes or silently changes the mode.

## Platform notes

### Windows + Vulkan

Vulkan exposes `FIFO`, `FIFO_RELAXED`, `MAILBOX` and `IMMEDIATE`, so all four
engine modes are usually available. Two caveats matter on real drivers:

- On several NVIDIA/Vulkan configurations `MAILBOX` is still capped to the
  display refresh rate and measures like `Fifo`. This is a driver property,
  not an engine bug; the old code avoided Mailbox for that reason.
- `FIFO_RELAXED` only changes behaviour when the application misses a vblank;
  a fast application measures the same as `Fifo` until a frame is late.

### Windows + DX12

DX12's swapchain is built around the DXGI sync interval. wgpu maps `Fifo` to
sync interval 1 and `AutoNoVsync` to interval 0 with the tearing flag where the
adapter supports it. `Mailbox` and `FifoRelaxed` are normally absent from the
DX12 capability list, so those requests fall back to `Fifo` with a warning.

Tearing in windowed mode is also subject to Desktop Window Manager composition
(MPO, fullscreen optimisations, the window's fullscreen state). Compare runs
with the same window state: a windowed run and a fullscreen-borderless run are
not the same experiment.

### macOS / Metal, GL and Web

Metal exposes `Fifo`, `Mailbox` and `AutoNoVsync`; `FifoRelaxed` usually falls
back. GL exposes `Fifo` and `AutoNoVsync`; `Mailbox` and `FifoRelaxed` are
normally unavailable. The browser owns presentation on the web: the runner
currently forwards only `vsync`, and `FERROUS_PRESENT_MODE` is honoured only
when the host provides a process environment. `AppConfig::present_mode` reaches
the web path once the runner forwards it.

## Why `desired_maximum_frame_latency` stays at 2

`wgpu::SurfaceConfiguration` keeps `desired_maximum_frame_latency: 2` in both
the desktop and web paths. It is wgpu's default and it is intentional:

- A lower value (1) removes the spare queued frame, but it changes pacing
  rather than simply lowering latency: the CPU then blocks in
  `get_current_texture` on the previous frame.
- With no spare buffer, a slow frame can starve the swapchain and show up as
  stutter rather than as lower input latency.

There is deliberately no environment knob for this field. The present mode is
the pacing control this engine exposes; see `AppConfig` and `PresentMode` in
`builder.rs`.

## Measuring

`python scripts/perf_bench.py` drives the release Motioner binary, enables the
engine frame trace with `FERROUS_TRACE_FRAMES=1`, and reports `fps`, frames,
`frame_ms_median` / `frame_ms_p90` / `frame_ms_p99`, `dropped_frames`, GPU
submits and the per-stage medians (including `present/wait`).

Build the release binary first:

```powershell
cargo build --release -p motioner_app --bins
```

Then run the playback scenario once per mode. The three-run protocol is the
`--runs 3` flag; `perf_bench` discards one warm-up run by default and reports
medians across the measured runs:

```powershell
foreach ($mode in @("fifo", "fifo_relaxed", "mailbox", "auto_no_vsync")) {
    $env:FERROUS_PRESENT_MODE = $mode
    python scripts/perf_bench.py --scenario playback_1080p --allow-pointer --runs 3
}
Remove-Item Env:FERROUS_PRESENT_MODE
```

The same command on a POSIX shell:

```bash
for mode in fifo fifo_relaxed mailbox auto_no_vsync; do
    FERROUS_PRESENT_MODE="$mode" \
        python scripts/perf_bench.py --scenario playback_1080p --allow-pointer --runs 3
done
```

`--allow-pointer` is required by the hover scenarios and is harmless for
playback. Every invocation writes one timestamped JSON under
`out/reports/phase18/`, so the loop leaves one file per mode.

### Reading the result

The playback scenario is **application-paced**: Motioner schedules the next
playback frame 16 ms after the current one
(`crates/motioner_ui/src/app/mod.rs:546`), so every present mode runs at about
60 fps. The modes differ in how those 60 frames reach the display and in how
much the CPU blocks in `present`, not in the nominal `fps`. To see the
uncapped ceiling, use the probe in
`crates/ferrous_renderer/examples/present_pacing.rs`, which disables the CPU
frame cap and is not paced by Motioner's animation timer.

| Mode | Expected `fps` | Expected `frame_ms_p99` | What a surprising result means |
|---|---|---|---|
| `fifo` | about 60 (app-paced) | near the 16.7 ms budget; present/wait may absorb the remainder | much higher means the GPU or composition is already the bottleneck before present |
| `fifo_relaxed` | about 60 (app-paced) | can be lower on late frames than `fifo` | no difference from `fifo` until a frame misses its deadline |
| `mailbox` | about 60 (app-paced) | often lower because `present` does not wait for the queue | equal to `fifo` means the GPU is saturated or the driver caps Mailbox (common on NVIDIA/Vulkan) |
| `auto_no_vsync` | about 60 (app-paced) | lower and less uniform; `present` does not wait | tearing risk is highest; `fps` still follows the app's 16 ms timer |

- `frame_ms` is the whole frame (update + draw + present), so a lower
  `frame_ms` under Mailbox/AutoNoVsync is the removed present wait, not extra
  frames.
- `dropped_frames` is `max(0, expected_frames - frames)`, not a compositor
  dropped-present counter. It is a timeout/stall signal: 600 expected frames
  that all arrive report 0 even if a mode is capped.
- `presented` versus `gpu_submits` and `stages_ms["present/wait"]` show how
  long the CPU spent in the swapchain. A large `present/wait` under `Fifo` is
  the pacing acting as designed; a large `encode` or GPU pass time is a
  different bottleneck.
- Compare medians across the three runs on the same machine, display settings
  and window state. Do not mix windowed and fullscreen-borderless runs.

### Latency caveat

This machine has no external latency instrument (photodiode, LDAT or
high-speed camera), so **input-to-photon latency cannot be measured here**. Do
not infer it from `fps` or from `frame_ms`.

The measurement plan uses submitted-vs-presented pacing as the proxy:
`frame_ms()` is the CPU wall-clock duration of the last frame, and the
`present/wait` stage captures the time spent waiting for the swapchain. A mode
that reports a lower `frame_ms_p99` at the same `fps` is spending less time
blocked in `present`; that is a pacing claim, not an input-latency claim.

## Programmatic use

```rust
use ferrous_engine::{App, PresentMode};

App::new(MyApp)
    .with_present_mode(PresentMode::Mailbox)
    .run();
```

`AppConfig::with_present_mode` does the same for a hand-built `AppConfig`. In
both cases `FERROUS_PRESENT_MODE` still wins at runtime.
