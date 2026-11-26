# Measured comparison (Phase 9C)

Release, 1280x800 logical, vsync, five runs per app (6 s), one 30 s idle tail,
cursor parked away from the window. The shared `[profile.release]` is
`opt-level = 3`, `lto = "fat"`, `codegen-units = 1`, `strip = true`,
`panic = "abort"`; the opt-`s` sizes come from the same profile with
`opt-level = "s"`. Raw JSON: `final-9c/`, `final-9c-egui/`, `idle-30-9c/`,
`hover-9c/`, `button-9c/`, `typing-9c/`.

## Idle and startup

| Library | Startup ms (median [range]) | Idle CPU % | 30 s tail % | Frames | RSS MB | Binary 3/s MB | GPU MB |
|---|---:|---:|---:|---:|---:|---:|---:|
| GUIMaker | 560.8 [560.0..1108.7] | 0.681 | **0.218** | **1** | 226.5 | 7.997 / 6.025 | 102.13 |
| egui 0.29 glow | **256.5** [256.1..263.7] | **0.000** | **0.055** | 4 | **141.5** | 3.511 / **3.258** | **59.52** |
| egui 0.29 wgpu | 560.2 [559.9..584.9] | 0.913 | 0.160 | 38 | 356.2 | 6.926 / 5.651 | 82.10 |
| iced 0.13 | 762.8 [761.9..1191.1] | 0.977 | 0.327 | 1 | 229.9 | 6.918 / 5.330 | 73.69 |
| Slint 1.7 | 306.4 [255.6..432.6] | 0.342 | **0.055** | 1 | 125.7 | **3.158 / 2.783** | 63.47 |
| ImGui (imgui-rs 0.12 + wgpu 29) | 765.9 [762.8..1090.3] | 0.977 | 0.272 | 1 | 190.3 | 5.186 / 3.813 | 282.50 |
| WebView2 (wry + winit) | 460.6 [458.7..542.0] | 0.347 | 0.274 | 346 | 24.8* | 0.914 (no profile) | n/a |

\* The WebView2 process RSS excludes its Chromium child processes, so it is not
comparable with the in-process libraries.

## Damage-driven input (Lane A)

The body sweep now gives **1 frame and 0.341 % CPU** (689 moves), against
675 frames and 10.672 % before. `Ui::pointer_move_changes`
(`crates/guimaker/src/frame.rs`) and `GuiHost::on_window_event`
(`crates/guimaker_gallery/src/host.rs`); the unit test
`a_pointer_move_only_repaints_when_the_target_changes` covers a static-body
sweep (0 changes) and a target change (1).

## Button hover (1 run, 6 s, pointer parked on the bench button)

| Library | Frames | CPU % |
|---|---:|---:|
| GUIMaker | **111** | 0.67 |
| egui glow | 4 | 0.65 |
| egui wgpu | 42 | 1.82 |
| iced | 1 | 2.69 |
| Slint | 1 | 0.34 |
| ImGui | 1 | 0.67 |
| WebView2 | 345 | 0.35 |

GUIMaker's 111 frames are a **gap**: the hovered button keeps `Ui::animating()`
true for the parking window, so the host keeps asking for frames. The colour
transition is 90 ms; the extra frames are not a colour fade. Not fixed in
Phase 9C (the measured reason is recorded; the fix is to make the settled
visual report not-animating or to gate the host on damage).

## Typing at 10 cps for 5 s

The same text input is in every app. The harness clicks it and sends 50 Unicode
characters through `SendInput`. Native apps: egui, iced, Slint, ImGui and
WebView2 received the 50; iced/Slint reported 1 frame (their inputs did not
repaint in the sampled window), ImGui 8, egui 5, WebView2 531. **GUIMaker
received 0** in the final run: the click focus did not land on the field (the
app got the key events in one earlier run, 198 frames, but `bench_focused`
stayed false). The `GuiApp::on_text` hook and the field are in place
(`host.rs`, `bench/guimaker_compare/guimaker/src/main.rs`), but the injected
focus is not reliable. Not fixed; the measured reason is the activation/focus
of the hidden-then-shown window.

## Backends (Lane B)

| Phase (ms) | Vulkan | DX12 FXC | DX12 DXC |
|---|---:|---:|---:|
| surface configured | 445.03 | 385.06 | 390.64 |
| renderer built | **491.99** | 9578.36 | 7323.19 |
| first frame | **498.18** | 9603.46 | 7346.21 |

DXC cuts the DX12 renderer by 2255 ms (-23.5%) but it is still 7.32 s. GL is
rejected: `no usable wgpu adapter: ... lacks Features(TEXTURE_BINDING_ARRAY |
SAMPLED_TEXTURE_AND_STORAGE_BUFFER_ARRAY_NON_UNIFORM_INDEXING)`. G-0012 keeps
Vulkan as the Flat2D default. Lazy 2D pipelines are blocked on the eager
`Renderer2d` construction (`out/reports/phase15/p9c-lane-b-notes.md`).

## Where GUIMaker is not the lightest, and the measured floor

- **Startup:** 560.8 vs Slint 306.4 / egui glow 256.5 ms. The Vulkan floor is
  instance 192.9 + surface 188.4 + device 36.2 + renderer 47.0 ms; DX12 DXC is
  7.3 s and GL has no usable adapter. The floor stands after the alternatives.
- **30 s tail:** 0.218 vs 0.055 %. Per-thread: top thread 0.203 s/30 s
  (0.68 % of a core), UI thread 0.0 s; the residual is the driver threads.
- **RSS:** 226.5 vs 125.7/141.5 MB. `MemoryHints::MemoryUsage` is the Flat2D
  default (A/B 228.6 vs 354.7 MB); the rest is the Vulkan/wgpu driver baseline
  plus 8.00 MB of owned GPU resources (atlas 5.31, GUI 2.33, 2D 0.37). The
  60 MB target is not met.
- **Binary:** 7.997/6.025 vs Slint 3.158/2.783 MB. The engine + renderer are
  not feature-gated for Flat2D (the 3D code is still compiled); this is the
  remaining size work.
- **GPU memory:** 102.13 vs egui 59.52 / Slint 63.47 MB; the driver baseline and
  the 1600x1000 physical window.
- **Button hover:** 111 frames vs 1-4; a GUIMaker repaint gap (above).
- **Typing:** GUIMaker focus not landing; the native apps work.
