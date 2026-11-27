# GUIMaker comparison bench (Phase 9B)

Standalone apps that build the **same start screen** with GUIMaker, egui, iced,
Slint, Dear ImGui (imgui-rs + wgpu) and, if time allows, WebView2. They are a
separate Cargo workspace: their dependencies never reach the engine or GUIMaker.

The purpose is a measured answer to "is GUIMaker the lightest UI stack?" —
startup, idle CPU and frames, hover/typing CPU, RSS, GPU memory and release
binary size, plus a feature table. Where GUIMaker is not the lightest, the
result is either a profile and a fix in the engine/GUIMaker or a precise
measured reason.

**The enforceable rules are in `PROTOCOL.md`.** This README only describes the
apps. The harness measures presents from the OS (PresentMon, else `wpr` ETW),
captures each app's own window with `PrintWindow`, names every top thread, and
requires the raw-API floor apps (`floor-wgpu-vulkan`, `floor-wgpu-dx12`,
`floor-wgpu-gl`, `floor-glow`) in every backend tier.

## The start screen (one spec, every app)

`1280 x 800` logical, monitor DPI (nothing forces a scale), dark palette. The
structure is the `start_screen` recipe:

1. A wrapping header: brand "Motioner" + tagline "Code-driven motion graphics"
   on the left; "Settings" and "Docs" links on the right.
2. A divider.
3. A two-column body, 55 % recent projects / 45 % form:
   - "Recent projects" heading and five card rows: "Project N", "Edited
     recently", trailing "11:42".
   - "New project" heading, a two-line body, four labelled read-only fields
     (Name/Untitled, Size/1920 x 1080, Frame rate/60 fps, Engine/FerrousEngine)
     and Create (filled) + Browse (raised) actions.
4. A footer: "FerrousEngine 0.1" left, "Apache-2.0" right.

The colours are the GUIMaker dark theme: `#06080D` page, `#0F1217` cards,
`#0B0D13` input wells, `#181B20` raised, `#26292F` borders, `#F1F5FE` text,
`#ADB1B9` muted, `#71757C` faint, `#2750C7` accent.

Each binary takes `--seconds N` and `--fonts roboto|default`. The harness
(`run_compare.py`) runs five release runs per scenario, keeps the raw JSON and
reports median and range. Self-reported counters are cross-checks only.

## Measurement rules

See `PROTOCOL.md` for the complete list. In short: external presents, no file
polling, `PrintWindow` on the app's own HWND only, the same Roboto bytes and
monitor DPI everywhere, one table per backend tier with a clear-only floor,
five alternating runs, named threads, ETW wakeups, GPU memory from one source,
and **ask the user to free the host before measuring** (C: stays above 5 GB;
delete nothing).

## Feature table (fill from the apps, not from marketing)

| Feature | GUIMaker | egui | iced | Slint | ImGui | WebView2 |
|---|---|---|---|---|---|---|
| CJK with no setup | | | | | | |
| Emoji with no setup | | | | | | |
| Popups outside the window | | | | | | |
| Idle CPU | | | | | | |
| Idle frames | | | | | | |
| Per-monitor DPI | | | | | | |
| IME | | | | | | |

## Results schema

`results/schema.json` is the machine-readable schema. The Phase 9D additions
(external frames/startup, named threads, ETW wakeups, GPU source, pixel diff,
backend/present mode, versions, fonts) are listed in `PROTOCOL.md` section 8.
The old field names (`startup_ms`, `idle_cpu_pct`, `idle_frames`, `rss_bytes`,
`gpu_bytes`) are still written; `idle_frames` is an alias of the external
present count.

## Build and run (from this directory)

```powershell
cargo build --release --workspace
python run_compare.py --dry-run
# after the user frees the host:
python run_compare.py --confirmed-host-free --runs 5 --out results
# refresh the marked Performance sections in both READMEs from results/
python run_compare.py --render-only     # or pass --write-readme to the run above
```

## Status

**Prepared, not yet built or measured.** The engine/GUIMaker side of Phase 9B
(async layered readback, golden-time device reuse, the Phase 9 view fix) is in
the working tree. Per the Phase 9B rule, the baseline perf sweep and these
comparison runs wait for the user to free the host; no result in this folder
is a measurement yet.
