# GUIMaker comparison bench (Phase 9B)

Standalone apps that build the **same start screen** with GUIMaker, egui, iced,
Slint, Dear ImGui (imgui-rs + wgpu) and, if time allows, WebView2. They are a
separate Cargo workspace: their dependencies never reach the engine or GUIMaker.

The purpose is a measured answer to "is GUIMaker the lightest UI stack?" —
startup, idle CPU and frames, hover/typing CPU, RSS, GPU memory and release
binary size, plus a feature table. Where GUIMaker is not the lightest, the
result is either a profile and a fix in the engine/GUIMaker or a precise
measured reason.

## The start screen (one spec, every app)

`1280 x 800` logical, DPI scale `1.5`, dark palette. The structure is the
`start_screen` recipe:

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

Each binary takes `--measure` and, after the first frame, samples for a fixed
window and prints one JSON object on stdout with the schema in
`results/schema.json`. The harness (`run_compare.py`, written with the
measurement lane) runs five release runs per app, keeps the raw JSON and
reports median and range.

## Measurement rules (do not relax)

- Release builds, on this machine, with the same vsync mode (Fifo), window
  size and DPI for every app.
- Five runs per app; report median and range, never a single sample.
- **Stop before measuring and ask the user to free the host.** Close other
  apps, then run the baseline perf sweep and the comparison. Resume only after
  the user confirms.
- Raw JSON goes to `results/`, one file per library and run, so a later phase
  can detect a regression.
- C: must stay above 5 GB; if it drops below, stop and report; delete nothing.

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

```json
{
  "library": "guimaker",
  "run": 1,
  "startup_ms": 0.0,
  "idle_cpu_pct": 0.0,
  "idle_frames": 0,
  "hover_cpu_pct": 0.0,
  "typing_cpu_pct": 0.0,
  "rss_bytes": 0,
  "gpu_bytes": 0,
  "binary_bytes": 0,
  "features": {
    "cjk": true,
    "emoji": true,
    "popups_outside_window": true,
    "per_monitor_dpi": true,
    "ime": true
  }
}
```

## Build and run (from this directory)

```powershell
cargo build --release --workspace
python run_compare.py --runs 5 --out results
```

## Status

**Prepared, not yet built or measured.** The engine/GUIMaker side of Phase 9B
(async layered readback, golden-time device reuse, the Phase 9 view fix) is in
the working tree. Per the Phase 9B rule, the baseline perf sweep and these
comparison runs wait for the user to free the host; no result in this folder
is a measurement yet.
