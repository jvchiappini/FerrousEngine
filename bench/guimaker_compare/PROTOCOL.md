# GUIMaker comparison protocol (Phase 9D)

This file is the contract. `run_compare.py` enforces it; a result that
violates any rule below is not a measurement and must not be reported.

The question is narrow: **is GUIMaker the lightest UI stack for the same start
screen, on this OS, with the same pixels?** "Lighter" means fewer presents,
less CPU, less memory and less GPU memory while doing the same visible work.
An app that draws less is not lighter; its pixel diff will say so.

---

## 1. External measurement (non-negotiable)

1. **Startup** is process creation to the app window's **first present**. It is
   measured outside the process by PresentMon or by an ETW capture, never from
   the app's own trace.
2. **Frames** are presented frames reported by the OS. The apps' `frames N`
   lines are kept only as `frames_self`, a cross-check, and are never the
   reported count.
3. **No file polling.** The harness does not poll the apps' stdout/stderr while
   they run. Output is read once after exit. Window readiness is found with
   `FindWindowW` + `GetWindowThreadProcessId`, not by watching files.
4. **PresentMon is never installed by the harness.** The harness discovers it
   on `PATH` (or `--presentmon PATH`, or `PRESENTMON_PATH`). If it is absent:
   - do **not** install it silently; the protocol requires asking the user;
   - fall back to the built-in Windows ETW capture: `wpr.exe -start <profile>
     -filemode`, run the app, `wpr.exe -stop <etl>`, then
     `tracerpt <etl> -o <csv> -of CSV -y`.
5. `wpr` needs an elevated shell. If it is not available or not elevated, the
   row is marked `source_warning` and either the run is rejected (default) or
   `--allow-self-report` is used explicitly for a non-comparable diagnostic.
6. When PresentMon is the present source, an ETW `wpr` capture is still started
   for **context switches** unless `--no-wake-etw` is passed. `wakeups_source`
   records which source produced the numbers.

## 2. Identical conditions

- **DPI:** every app follows the monitor. Nothing forces `1.5`. The GUIMaker
  app receives `BENCH_DPI_SCALE` (the primary monitor's effective DPI) because
  the engine's `App::with_size` takes physical pixels; its window is
  `1280 x 800` *logical*. Every other app uses a `1280 x 800` logical window
  and the monitor's own scale factor.
- **Present mode:** FIFO / vsync, one sample (`multisampling == 0`), an 8-bit
  swapchain, and a 2-image queue where the API exposes it
  (`desired_maximum_frame_latency = 2`). GUIMaker, ImGui and the floors ask for
  an sRGB format explicitly; eframe/iced pick their surface format internally
  and are recorded as such (both are 8-bit; egui applies its own gamma
  handling). `WGPU_BACKEND` selects the tier.
- **Fonts:** the main table loads the **same Roboto bytes**
  (`assets/fonts/Roboto-Regular.ttf`) in every app:
  - GUIMaker: `App::with_font_bytes`;
  - egui / egui-wgpu: `FontDefinitions` with `FontData::from_static`;
  - iced: `.font(ROBOTO).default_font(Font::with_name("Roboto"))`;
  - Slint: runtime registration through `i-slint-common`'s shared fontdb, with
    `default-font-family: "Roboto"`;
  - ImGui: `FontSource::TtfData`.
  RSS is reported at equal fonts. A **separate** secondary measurement
  (`--fonts default`) reports each library's own default CJK/emoji font RSS;
  in that mode every app also draws the fixed sample `漢字 🎬 ✨` above the
  bench strip, so the fallback glyphs are actually loaded. That table is never
  mixed with the equal-font table.
- **Versions:** recorded in `results/versions.json` and in every row's
  `versions` field. They are read from this workspace's `Cargo.lock`/manifests.

### Versions on disk today

| App | Stack | Version |
|---|---|---|
| guimaker | ferrous_engine / wgpu / winit | 0.1.0 (workspace) / 23.0.1 / 0.30.13 |
| egui (glow) | eframe / egui / glow / winit | 0.29.1 / 0.29.1 / 0.13.1 / 0.30.13 |
| egui-wgpu | eframe / egui-wgpu / wgpu / winit | 0.29.1 / 0.29.1 / 22.1.0 / 0.30.13 |
| iced | iced / iced_wgpu / wgpu / winit | 0.13.1 / 0.13.5 / 22.1.0 / 0.30.13 |
| Slint | slint / femtovg / winit | 1.7.2 / 1.7.2 / 0.30.13 |
| ImGui | imgui / imgui-wgpu / imgui-winit-support / wgpu / winit | 0.12.0 / 0.28.0 / 0.13.0 / 29.0.4 / 0.30.13 |
| floor-wgpu-* | wgpu / winit | 29.0.4 / 0.30.13 |
| floor-glow | glow / glutin / glutin-winit / winit | 0.13.1 / 0.32.3 / 0.5.0 / 0.30.13 |

## 3. Same pixels

- Capture **only** the app's own window with `PrintWindow(hwnd,
  PW_RENDERFULLCONTENT)`. Never capture the desktop and never crop another
  window.
- Diff every capture against the **GUIMaker** capture for the same scenario
  (one reference per scenario/tier, reused across runs). Report
  `pixel_diff.mean_abs_channel_diff`, `changed_pixel_ratio`, and
  `comparable`. A size mismatch is `comparable: false` with a note.
- A row whose pixel diff says the app drew less is not evidence of a lighter
  stack. Fix the app or report the difference.
- `--save-captures` writes the raw `.bmp` files; they are diagnostics, not
  inputs, and nothing is deleted.

## 4. Backend tiers

One table per tier, always including the raw-API floor app:

| Tier | Apps | Raw-API floor |
|---|---|---|
| Vulkan/wgpu | GUIMaker, egui-wgpu, iced, ImGui | `floor-wgpu-vulkan` |
| DX12/wgpu | GUIMaker, egui-wgpu, iced, ImGui | `floor-wgpu-dx12` |
| GL | GUIMaker, egui/glow, Slint/femtovg | `floor-glow` |

`floor-wgpu-gl` is also built and can be run as the wgpu-on-GL floor. A final
table, **"each library's best backend on this OS"**, is computed from the idle
runs: lowest external frames, then lowest idle CPU.

The floors are tiny binaries in this workspace that only clear the window and
idle. They use the same CLI (`--seconds`, `--fonts`) and print the same
`startup_ms`, `bench_ready`, `frames`, `backend` lines. They are skip-listed in
interactive scenarios because they have no controls.

## 5. CPU, wakeups and thread names

- CPU percent is sampled from `GetProcessTimes` deltas (never from self-report).
- **Wakeups/second per thread** come from ETW context-switch events
  (`Kernel-Thread` / CSwitch) counted per TID and divided by the capture
  duration. If no ETW source is available, `wakeups_per_s` is `null` and
  `wakeups_source` says why; psutil-style percentages alone are not enough.
- **Every top thread is named.** The harness resolves:
  1. the Win32 thread description (`GetThreadDescription`); if empty,
  2. the thread's start address (`NtQueryInformationThread`,
     `ThreadQuerySetWin32StartAddress`) mapped to its module through
     `EnumProcessModulesEx` + `GetModuleInformation` + `GetModuleBaseNameW`.
  The display is e.g. `tid 8104 nvwgf2umx.dll (driver)` or the thread name.
  `tail_top_thread` names the top thread of the idle-30 tail.
- GPU memory uses **one source for every app**: the PDH
  `\GPU Process Memory(pid_<pid>_*)\Dedicated Usage` + `Local Usage` counters
  (`gpu_source = "pdh-gpu-process-memory"`).

## 6. Scenarios

Each scenario runs **5 times**; report **median and range**, never one sample.
App order alternates (forward on odd runs, reversed on even runs).

| Scenario | Length | Interaction |
|---|---|---|
| `idle6` | 6 s | none after the first frame |
| `idle30` | 30 s | none; the last 5 s are the idle tail |
| `hover_body` | 6 s | pointer sweeps the body |
| `hover_button` | 6 s | pointer parks on the bench button |
| `typing` | 10 s | a **real click** on the editable field, then 10 chars/s |
| `resize` | 8 s | programmatic 1000x700 ↔ 1280x800 toggles |
| `menu` | 8 s | click the bench menu open, then click it closed |

Every app (including GUIMaker) exposes the same editable field, hoverable
button and toggleable menu at the same logical coordinates:
field `x=40 w=320`, button `x=380 w=140`, menu `x=540 w=140`, strip top at
`height-76`. The GUIMaker field is `TextFieldModel::view_with_focus(..)` fed by
`GuiApp::on_text`; egui uses a focused `TextEdit`, iced a `text_input`, Slint a
`TextInput`, ImGui an `InputText`.

`idle6`/`idle30` run on every tier (floors included). Interactive scenarios run
on one tier (`--interactive-tier`, default Vulkan) so the tables stay
comparable.

## 7. Host prerequisites

- **Ask the user to free the host before measuring.** The harness prints this
  instruction and refuses to run without `--confirmed-host-free`.
- Close browsers, chat, video, other GPU apps; leave only the terminal.
- **C: must stay above 5 GB free.** The harness stops if it is below; it does
  **not** delete anything.
- Do not install PresentMon without asking the user.

## 8. Results schema

Backward compatible with `results/schema.json`, plus:

| Field | Meaning |
|---|---|
| `floor` | this row is a raw-API floor app |
| `scenario`, `tier` | scenario and backend tier |
| `backend`, `present_mode` | backend and present mode actually requested |
| `present_source` | `presentmon`, `wpr` or `none` |
| `frames_external`, `frames_self`, `frames_source` | OS presents vs self-report |
| `startup_ms`, `startup_ms_self`, `startup_source` | external startup + cross-check |
| `threads`, `thread_names`, `top_threads_cpu` | named per-thread CPU + wakeups |
| `tail_top_thread` | named top thread of the idle-30 tail |
| `wakeups_per_s`, `wakeups_source` | ETW context-switch rate |
| `gpu_bytes`, `gpu_source` | one GPU-memory source for every app |
| `pixel_diff`, `capture_ok` | PrintWindow capture diff against GUIMaker |
| `versions` | exact library versions |
| `fonts` | `roboto` (main table) or `default` (secondary table) |
| `session_id`, `measured_at` | the measurement session and row timestamp |
| `reason` | measured explanation added to every row after the run |
| `winner` | the idle-6 s tier winner (non-floor) used to write the reason |

## 8.1 Generated performance section

`render_readme.py` (and `run_compare.py --write-readme`) rewrites only the
marked block in both READMEs:

```text
<!-- PHASE9D-PERFORMANCE:START -->
...
<!-- PHASE9D-PERFORMANCE:END -->
```

- Targets: the repository root `README.md` and `crates/guimaker/README.md`.
  Nothing outside the markers is touched (the backend-choice section stays).
- It reads only the JSON under `results/`, never hand-typed numbers. It uses
  the newest Phase 9D `session_id` and ignores the older Phase 9B/9C shapes.
- It requires a non-empty measured `reason` on every row and fails loudly when
  one is missing. `run_compare.py` annotates every row after measuring.
- It never omits an app: missing apps are listed as `not measured`, and extra
  libraries found in the JSON are appended to the tables.
- The block contains the per-tier tables with the raw-API floor row, the floor
  table, the best-backend table, the interactive tables, hardware/OS/driver,
  library versions, the measurement date, a link to this protocol, and the
  exact reproduce command.
- `--check` renders in memory and exits 1 if a README would change, so CI can
  detect a stale section.

## 9. Commands

From `bench/guimaker_compare`:

```powershell
cargo build --release --workspace
python run_compare.py --dry-run
# after the user frees the host and confirms:
python run_compare.py --confirmed-host-free --runs 5 --scenario idle6 --scenario idle30 --tier vulkan --tier dx12 --tier gl
python run_compare.py --confirmed-host-free --runs 5 --scenario hover_body --scenario hover_button --scenario typing --scenario resize --scenario menu --interactive-tier vulkan
python run_compare.py --confirmed-host-free --runs 5 --scenario idle6 --fonts default
# after the measurement: refresh both READMEs
python run_compare.py --render-only
# or do it in the same invocation:
python run_compare.py --confirmed-host-free ... --write-readme
# CI: fail if either README is stale
python render_readme.py --check
```

PresentMon discovery: `--presentmon "C:\path\PresentMon.exe"` or
`PRESENTMON_PATH`. If absent, use an elevated terminal so `wpr` can capture.

## 10. Known risks / limitations (recorded, not hidden)

- `wpr` requires administrator rights; a non-elevated shell yields no external
  source and the run is rejected by default.
- `tracerpt` CSV column names vary by Windows build. Present counting and
  context-switch counting are best-effort in the wpr path; PresentMon is the
  preferred path for presents.
- `PrintWindow(PW_RENDERFULLCONTENT)` can return black for some
  DirectComposition/GPU windows on older Windows builds. `capture_ok` and
  `pixel_diff.comparable` record it; a black capture is not a pass.
- Slint 1.7 has no public runtime font API; this bench reaches the shared
  fontdb through `i-slint-common` (pinned to the same `=1.7.2`). If that
  breaks, the Slint row is marked and the equal-font table must not include it.
- iced 0.13 exposes the wgpu backend/present mode only through environment
  variables (`WGPU_BACKEND`, `ICED_PRESENT_MODE`); the harness sets both.
- eframe (egui-wgpu) chooses `Bgra8Unorm` rather than `Bgra8UnormSrgb` for its
  surface; it is 8-bit and egui applies gamma correction in its shader. GUIMaker,
  ImGui and the floors use an explicit sRGB format. This is recorded rather than
  hidden; do not compare screenshots between the two formats without the pixel
  diff.
- The GUIMaker window size is physical because the engine's `with_size` is
  physical; `BENCH_DPI_SCALE` is what makes it 1280x800 logical on this host.
