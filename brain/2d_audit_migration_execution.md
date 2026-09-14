# FerrousEngine — 2D Audit Migration Execution Log

> Central log for applying `brain/2d_audit_migration_notes.md` to the two real
> consumers. FerrousEngine is already migrated (Phase 3 merged to `main`).
> Order enforced: **Koncreta first, then GUIMaker**. No absolute-FPS targets;
> only same-machine relative deltas (software rasteriser / lavapipe on this VPS).

## Repositories and rollback points

| Repo | Path | Branch | Notes |
|---|---|---|---|
| FerrousEngine | `/opt/softwaremaker/workspaces/96a0b3c0-be13-4354-8674-72220ab22596` | `main` | source of truth; H1 evidence added |
| Koncreta | `/opt/softwaremaker/workspaces/8af62d16-b60c-4e7a-95d6-74deb2d46c2e` | `migrate/ferrous-2d-audit` | branched from `master` (`d40f45b`) |
| GUIMaker | `/opt/softwaremaker/workspaces/d3a95f70-92a6-48b3-a540-f319263da71e` | `migrate/ferrous-2d-audit` | branched from `main` (`9dcbc28`) |

Every branch is a clean rollback point. Nothing was force-pushed; the engine
`main` was only advanced with additive commits.

## 1. Koncreta (`core_viewer`)

`core_viewer` is a `cdylib` (Rust→WASM) plus a `core_viewer` lib. Its Ferrous
path-deps already pointed at the migrated engine checkout (absolute paths), so
there was no version to bump; the migration took effect on the next build.

### Commits (branch `migrate/ferrous-2d-audit`)

1. `30f8882` `chore(deps): align glam to 0.29 with FerrousEngine`
   - `core_viewer` declared `glam 0.32.1`; the engine uses `glam 0.29`. This
     forced a manual `Vec2 → Vec2` bridge (`to_fe`) at every call and risked
     silent type-split errors. Aligned to `0.29`.
   - Result: `Cargo.lock` now resolves `glam 0.29.3` (engine/API) plus
     `glam 0.30.10` transitively via `fontmesh` (internal to `ferrous_renderer`;
     never crosses the public 2D API). The `0.32.1` entry is gone.
2. `5e996dd` `refactor(render): adopt FerrousEngine 2D audit migration (C1, C4)`
   - **C1:** `ctx.render.enable_pure_2d(...)` replaces
     `renderer_mut().enable_pure_2d(...)`; the manual
     `renderer_mut().clear_2d()` in `update()` was removed because the engine
     runner now clears the 2D batchers at the start of every frame.
   - **C4:** both 2D bridges now call
     `ShapeInstance::line_with_cap_style(..., CapStyle)`; the legacy `u8`
     selector is mapped at the boundary (`0=Flat, 1=Round, 2=Square`). CAD flat
     joints use `CapStyle::Flat` explicitly instead of a magic `0`.

### Verification

- `cargo check` (native): **OK** (warnings only, pre-existing).
- `cargo check --target wasm32-unknown-unknown`: **OK**.
- `cargo test`: **142 passed, 0 failed**.
- H1 evidence: see §3.

## 2. GUIMaker

### 2.0 Working-tree state (blocker found and handled)

GUIMaker was **not** in a committable state: most of its library source was
**untracked** (`src/lib.rs`, `src/codegen/`, `src/builders/`, `src/plugin/`,
`src/scene/`, `src/template/`, `src/theme.rs`, ...) plus in-flight refactors to
panels and `main.rs`. The debt lived in those untracked files, so a clean
migration commit was impossible without first checkpointing the WIP.

Per explicit decision, the WIP was committed as a checkpoint:

1. `6f06b7c` `chore(wip): checkpoint in-progress UI refactor before ferrous migration`
   - Snapshot of the working tree exactly as found; no functional change.
   - This is a **prerequisite**, not part of the migration. It can be reverted
     independently (`git revert 6f06b7c`) if the WIP is later rebased/squashed.

### 2.1 Pre-existing debt (prerequisite, fixed)

2. `53b612e` `fix(app): update AppMode to current enum (pre-existing debt)`
   - `src/main.rs` used `AppMode::Desktop`; codegen emitted
     `AppMode::Desktop2D` (`src/codegen/mod.rs`,
     `src/codegen/template_generator.rs`). Neither variant exists; the current
     enum is `AppMode::{Game3D, Flat2D}`. Mapped to `Flat2D` (UI-only mode).
   - The binary did **not** compile before this change; this is GUIMaker's own
     debt, independent of the audit.
   - The `ferrous_app` → `ferrous_engine` codegen rename was **already applied**
     (no `ferrous_app` references remain).

### 2.2 Applying the migration (1, 2, 3, 6, 7 of Part 1)

- **Branch:** `migrate/ferrous-2d-audit` (created before the checkpoint).
- **Dependency bump:** none required — GUIMaker's manifest already uses
  relative path-deps (`../96a0b3c0-.../crates/...`) pointing at the migrated
  engine checkout.
- **C1 (facade + auto-clear):** not applicable. GUIMaker runs `AppMode::Flat2D`
  (UI-only); it does not call `enable_pure_2d`/`clear_2d`. Its only
  `renderer_mut()` use is to reach `device`/`queue` for texture upload in
  `setup`, which the migration does not change.
- **C2 (`from_svg_bytes`):** **GUIMaker does use it** (contrary to the original
  relevamiento) in `src/lib.rs:211/225/239` to load window icons at setup. The
  migration kept the signature and return value identical, so no change is
  needed; it still compiles. `from_svg_bytes_cached` would give nothing here
  (each SVG is loaded once).
- **C4 (`line_with_cap`/`CapStyle`):** not used anywhere in GUIMaker.
- **glam:** GUIMaker declares no `glam` of its own; it uses the engine's
  re-export. `Cargo.lock` resolves `glam 0.29.3` (+ `0.30.10` via `fontmesh`),
  i.e. no cross-boundary mismatch.
- **Tests:** GUIMaker has **no test suite** (`cargo test` → 0 tests in all
  targets). Nothing to run; reported instead of skipped silently.

### 2.3 Verification

- `cargo check`: **OK** for lib + bin (44 pre-existing warnings, no errors).

## 3. H1 colour-space evidence (FerrousEngine `brain/`)

Generated headlessly with `crates/ferrous_renderer/examples/cad_capture.rs`
(new; committed as `2648757`), which renders a representative architectural
floor plan (flat-capped walls, columns, dimension lines, a 0.5-grey probe
patch) through the real Pure2D harness twice:

| Target | Meaning | 0.5-grey probe |
|---|---|---|
| `rgba8unorm` | emulates the **pre-H1** non-sRGB production swapchain | `[128,128,128]` |
| `rgba8unorm_srgb` | **post-H1** production | `[187,187,187]` |

Files (for human review; the VPS has no display, so this is not confirmed
by eye here):
- `brain/2d_h1_cad_before_rgba8unorm.png`
- `brain/2d_h1_cad_after_rgba8unorm_srgb.png`

Because the engine is already migrated, "before" is reproduced via the target
format, not by reverting the engine. Draw calls were 1 for the whole plan
(equal-`z` coalescing), confirming there is no draw-call regression; this is
colour evidence only, with no absolute-FPS claim.

## 4. Discrepancies vs `2d_audit_migration_notes.md`

These are findings; the note was **not** silently patched.

1. **[API] `RenderContext::enable_pure_2d` colour type.** The notes state the
   facade has "same signatures as on `Renderer`". In fact
   `Renderer::enable_pure_2d` takes `wgpu::Color`, while
   `RenderContext::enable_pure_2d` takes the engine's own `ferrous_engine::Color`
   (converted internally via `Color::to_wgpu`). This caused the only compile
   error during the Koncreta migration. Koncreta now passes
   `ferrous_engine::Color`. Either the note or the facade signature should be
   reconciled — it is a legitimate migration-doc/API inconsistency.
2. **[Fact] GUIMaker and `from_svg_bytes`.** The relevamiento said GUIMaker does
   not use it; it does (`src/lib.rs`). Additive change, so no action, but the
   assumption was wrong.
3. **[Fact] GUIMaker `ferrous_app` reference.** Already absent/replaced by
   `ferrous_engine`; nothing to fix.
4. **[Fact] GUIMaker test suite.** The task said "run the whole test suite";
   GUIMaker has none.
5. **[Scope] Dependency bump.** Both consumers already pointed at the migrated
   engine via path-deps, so "bump the dependency" was a no-op for both. Worth
   noting because a relative-version consumer would have needed a real bump.

### Residual (not addressed by design)
- GUIMaker's generated templates still emit
  `use ferrous_gui::{GuiBatch, TextBatch, Ui};` in `src/codegen/plugin_generator.rs`.
  Generated projects may not compile if `ferrous_gui` no longer exports
  `Ui`/`TextBatch`. This is a pre-existing codegen-output issue, outside the
  two debt items asked for; left as an observation.
- The audit backlog not selected for Phase 3 (H2/H3/H5/H7/H8/H11/H13, M4, M6,
  M7, M9, C3, ...) remains open in `brain/2d_audit_master_plan.md`.

## 5. Final build/test status

| Repo | Command | Result |
|---|---|---|
| FerrousEngine | `cargo check --workspace` | OK |
| FerrousEngine | `cargo test -p ferrous_renderer --test pure2d_headless` | 6/6 pass |
| FerrousEngine | `cargo run -p ferrous_renderer --example cad_capture` | ran on lavapipe, PNGs written |
| Koncreta | `cargo check` (native) | OK |
| Koncreta | `cargo check --target wasm32-unknown-unknown` | OK |
| Koncreta | `cargo test` | 142/142 pass |
| GUIMaker | `cargo check` | OK (lib + bin) |
| GUIMaker | `cargo test` | 0 tests (no suite) |

## 6. How to roll back

- Koncreta: `git revert 5e996dd 30f8882` on `migrate/ferrous-2d-audit`, or
  reset to `master`.
- GUIMaker: `git revert 53b612e` (debt fix) and `git revert 6f06b7c` (WIP
  checkpoint), or reset to `main`.
- Engine H1 evidence: `git revert 2648757`.

## 7. Deployment (Koncreta live on the public gateway)

The migrated `core_viewer` WASM was built and deployed so the changes are
visible at the public URL:

**http://194.238.25.16:8888/app/**

Deployment steps (the project's own script):

```
cd /opt/softwaremaker/workspaces/8af62d16-b60c-4e7a-95d6-74deb2d46c2e
./scripts/rebuild-rust-app.sh
```

which does: `wasm-pack build --target web --out-dir pkg --no-opt` in
`packages/core_viewer` → `docker compose build app-editor` →
`docker compose up -d app-editor` → HTTP checks against `/app/`.

Evidence that the new build is actually served:

| Asset | Value |
|---|---|
| App entry | `/app/assets/index-B4MnJUwr.js` |
| core_viewer glue chunk | `/app/assets/core_viewer-2sAWKK38.js` |
| WASM asset (served) | `/app/assets/core_viewer_bg-CC3Gv0y3.wasm` — **6,893,303 B** |
| WASM asset (previous) | 6,850,029 B |
| WASM local build | 6,893,303 B (exact match) |
| Container | `8af62d16-...-app-editor-1` recreated from the new image |
| Public response | `HTTP 200`, JS `Content-Type: application/javascript`, wasm `application/wasm` |

Caveats for reviewers:
- **H1 is a visual change**: on the migrated engine the 2D output is sRGB-encoded,
  so colours render brighter (mid-grey 0.5 → 187 instead of 128). Koncreta's
  palette was authored against the old (darker) output, so this deployment is
  exactly the "how does it look" check; palette recalibration may follow.
- The browser may cache the previous WASM; the asset filename is content-hashed
  (`core_viewer_bg-CC3Gv0y3.wasm`), so a normal reload should pick up the new
  one. A hard refresh is recommended.
- This VPS has no display/WebGPU browser, so the deployment is verified by HTTP
  and asset bytes only; runtime rendering was not visually confirmed server-side.
- Rollback of the deployment = rebuild from `master` with the same script.

