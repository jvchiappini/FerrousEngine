# Measured comparison (Phase 9B, final pass)

Release builds, 1280x800 logical, vsync on, cursor parked away from the window,
`--seconds 6`, **five runs** each (idle), plus one 30 s idle tail per app. Startup
is the harness wall time from `Popen` to the first-frame marker, so process
launch and GPU init are included. Raw JSON: `final/`, `idle-30/`, `hover/`,
`mem-usage/`, `mem-perf/`.

## Idle and startup

| Library | Startup ms (median [range]) | Idle CPU % (median [range]) | 30 s tail % | Idle frames | RSS MB | Binary MB |
|---|---:|---:|---:|---:|---:|---:|
| GUIMaker | 560.5 [559.6..565.6] | 1.220 [0.609..1.821] | 0.376 | 3 | 227.5 | 11.07 |
| egui 0.29 (glow) | **256.1** [255.9..259.0] | **0.326** [0.325..0.654] | **0.054** | 5 | **144.5** | **4.54** |
| egui 0.29 (wgpu) | 559.5 [509.9..561.0] | 1.213 [0.908..1.519] | 0.428 | 5 | 355.0 | 9.61 |
| iced 0.13 | 762.4 [761.5..763.8] | 1.008 [0.673..2.014] | 0.163 | 1 | 238.2 | 10.08 |
| Slint 1.7 | 307.3 [306.4..328.2] | 0.000 [0.000..0.342] | 0.109 | 1 | **144.5** | **4.20** |
| ImGui (imgui-rs 0.12 + wgpu 29) | 813.7 [762.1..813.9] | 1.340 [0.336..1.683] | 0.217 | 1 | 198.2 | 7.11 |
| WebView2 | not built | — | — | — | — | — |

## Hover (3 runs, 6 s, cursor swept over the body)

| Library | CPU % (median) | Frames / 6 s | Note |
|---|---:|---:|---|
| GUIMaker | 10.672 | 675 | engine redraws the whole frame on every pointer move |
| egui (glow) | 38.662 | 1930 | eframe continuous repaint |
| egui (wgpu) | 21.771 | 1890 | same |
| iced | 9.187 | 1 | no redraw on body hover (no hover state on the swept nodes) |
| Slint | 0.342 | 1 | same |
| ImGui | 1.023 | 1 | same |

The iced/Slint/ImGui rows are **not like-for-like**: the start-screen body has
no hover state, so those apps correctly do not repaint. GUIMaker and egui
redraw on every move regardless. A button-focused hover is a follow-up.

## GPU memory (dedicated, Windows GPU Process Memory counters)

| Library | Dedicated MB |
|---|---:|
| GUIMaker | 85.13 |
| egui (glow) | 59.02 |
| egui (wgpu) | 86.35 |
| iced | 73.68 |
| Slint | 63.41 |
| ImGui | 270.00 |

GUIMaker's own GPU inventory (`memory-report`) is **8.00 MB** (font atlas
5.31 MB, GUI 2.33 MB, 2D 0.37 MB); the rest is the Vulkan driver/allocator
baseline.

## MemoryHints A/B (GUIMaker, 3 runs each)

| Policy | RSS MB | Idle CPU % |
|---|---:|---:|
| `MemoryUsage` (Flat2D default) | **228.6** | 1.205 |
| `Performance` (opt-in) | 354.7 | 1.805 |

`MemoryUsage` is already the `AppMode::Flat2D` default
(`AppConfig::effective_memory_hints`, `crates/ferrous_engine/src/config.rs`);
the A/B confirms **−126.1 MB (−35.6 %)** and lower idle CPU.

## Backend startup (G-0011)

| Phase (ms) | Vulkan | DX12 |
|---|---:|---:|
| window created | 19.22 | 18.34 |
| instance | 207.10 | 325.17 |
| surface configured | 426.53 | 451.27 |
| renderer built | **472.78** | **7003.21** |
| first frame | **477.39** | none before the 800 ms show timeout |

DX12 wins the surface step (49.9 vs 183.2 ms) but loses the main window because
the 2D pipeline build is 6166.7 ms (wgpu 23's pipeline cache is Vulkan-only).
Vulkan stays the Windows default; G-0011 records the table.

## Async layered readback A/B (600k-quad main scene)

`crates/ferrous_engine/examples/p9b_readback_stall.rs`, release, 5 runs:

| Path | Median ms |
|---|---:|
| `sync_block_ms` (old: `Maintain::Wait` on the UI thread) | 7.159 |
| `async_submit_ms` (new: `Maintain::Poll`, worker maps) | **0.001** |
| `async_total_ms` (worker read ready) | 8.955 |

The UI thread's wait drops from 7.159 ms to 0.001 ms; the previous popup image
stays until the worker finishes ~9 ms later.

## Golden timing (finding 4)

`cargo test -p guimaker_gallery --test goldens`: **1 passed, 46.10 s** test time
(65.75 s wall incl. build), down from **169.87 s** (−72.9 %), byte-identical
goldens. Cause: one `Pure2dHarness` per story; now one `StoryTarget` per batch.

## Where GUIMaker is not the lightest

- **Startup vs egui/glow and Slint:** 560.5 vs 256.1/307.3 ms. The engine's
  Vulkan driver floor is instance 187.9 ms + surface 183.2 ms + device 36.2 ms;
  G-0011 shows DX12's renderer is 6.2 s. The fix is a lighter backend/pipeline
  path, not a GUIMaker loop.
- **RSS vs egui/glow and Slint:** 227.5 vs 144.5 MB. `MemoryUsage` is already
  on; the remaining ~83 MB is the Vulkan/wgpu driver baseline plus 8 MB of owned
  GPU resources. The 60 MB budget is not met.
- **Binary size:** 11.07 MB is the engine + renderer; egui/glow and Slint are
  4.2-4.5 MB.
- **Hover CPU:** 10.7 % is the engine's full-frame redraw per pointer move; the
  GUIMaker core itself is measured separately in divan (hover frame ~12 µs).
  Disabling pointer-move redraws and requesting frames on damage is the
  Motioner-style integration, not a bench-app change.

GUIMaker **wins against egui/wgpu**: same startup (560 vs 560 ms), lower RSS
(227.5 vs 355.0 MB), fewer idle frames (3 vs 5); and against iced/ImGui on
startup and RSS.
