# FerrousEngine — 2D Audit Master Plan (Fase 2)

> Consolidation of the four Phase-1 reports:
> `brain/2d_audit_subagent_A.md` (GPU/pipeline),
> `brain/2d_audit_subagent_B.md` (CPU/ECS/threading),
> `brain/2d_audit_subagent_C.md` (memory/assets/textures),
> `brain/2d_audit_subagent_D.md` (API/DX/docs/tests).
>
> Baseline harness (Subagent D): `crates/ferrous_renderer/src/headless.rs`,
> `tests/pure2d_headless.rs`, `benches/pure2d_headless.rs`,
> `ferrous_2d::render::Frame2dCounters`. All metrics are hardware-independent
> (draw calls, bytes/frame, allocs/frame, bind groups) because the VPS uses
> lavapipe/llvmpipe (software). `cpu_ms` is a same-machine relative delta only.

## 0. Corrections to the original premises (verified by the subagents)

These change what the plan prioritises. Do not carry the original assumptions forward:

| Original premise | Verified reality | Source |
|---|---|---|
| `ParallelScheduler` "does not compile" | **It compiles** natively and for wasm32 (`cargo check -p ferrous_ecs --features parallel` OK). It is *sequential*, never enabled, and its docblock lies about `rayon::scope`. `cargo test -p ferrous_ecs` fails for an unrelated bug. | B |
| Buffers/bind groups "recreated when they grow" is a problem | GPU buffer reallocation is **amortized** (1 realloc across 12 dynamic frames; `max_instances` high-water-mark). The real per-frame cost is **transient scratch allocs** (56–92/frame) and the UI bind group. | C |
| `renderer_core.rs:1744` = sRGB texture creation | Actual site is `renderer_core.rs:1853` (`register_sprite_texture_inner`). Line refs drift; always verify. | A |
| `ADR-001-unified-2d-3d-renderer.md` / `3D_BRIDGE.md` are binding design docs | **Neither exists in the repo.** The "don't close the door to 3D" constraint lives only in the task prompt. | A, C, D |
| `mkdocs.yml` broken = stale paths | Stale paths fixed (15→0 nav warnings), but a **deeper structural mismatch** remains (16 cross-crate link warnings) from the docs copy-flatten build. | D |
| `draw_sprite_2d`/overlay path works like Pure2D | Overlay Full3D **ignores z-order, never renders sprites**, and hand-duplicates the ortho camera; `Flat2D` renders **no shapes/sprites at all**. | A, D |

## 1. Deduplicated / reconciled findings

- **#12 colour-space** — owner **A**, but C owns the *texture-format inventory* side (all `Texture2d`/sprite textures are hardcoded sRGB; `ferrous_assets::Texture2d` offers no linear option). One fix, two sub-tasks. A fix changes on-screen output → migration note required.
- **Dead code (`SpritePass`, `RendererPasses`, `ferrous_2d::systems`, `Transform2d`)** — A owns removal of the render-pass skeleton; B owns removal/decision for the dead ECS systems and the duplicated ECS→batcher bridge; D owns docs. **One coordinated item**, not three.
- **`draw_2d_path` ignores cap/join/fill** — A and D both found it. Single fix: one `Path2d → ShapeInstance` conversion (also kills the triplicated path logic). A owns implementation, D owns API/doc.
- **Sprite atlas (C) vs batching coalescing (A)** — **complementary, not competing**. Atlas collapses texture dimension; A's grouping "order-proof" batching makes it pay off even with interleaved z. Dependency: A3 before C3.
- **AA redundancy** — A found default FXAA runs over linear HDR on top of SDF AA (+opt-in MSAA); Pure2D has no post-AA. Interacts with #12 (AA should run post-encode/LDR). Sequence #12 → AA.
- **Dirty tracking** — B established it is **not worth doing for 2D today** (immediate-mode consumer, nothing persistent to dirty); the existing 3D `mark_dirty` is itself a no-op because `sync_world` re-marks dirty every frame. Keep as 3D-only fix; do not add 2D dirty flags now.

## 2. Prioritised master list

Ordering: severity → measured impact → risk to Koncreta/GUIMaker → effort.
`CRÍTICO-gated` = touches Apéndice A or changes consumer-visible behaviour ⇒ **needs human approval before implementation** (Sección 9.3).

### Tier 0 — CRÍTICO, gated (human approval required)

| ID | Item | Owner | Sev | Location | Why it matters | API risk | Effort |
|---|---|---|---|---|---|---|---|
| **C1** | 2D entrypoints missing from `RenderContext` + runner never clears 2D batchers ⇒ ghosting; `enable_pure_2d`/`clear_2d` only reachable via `renderer_mut()` | D | CRÍTICO | `render_context.rs:104-113`, `runner/frame.rs:240-336`, `renderer_core.rs:631,1905` | Silent geometry accumulation and an escape-hatch-only API for Koncreta's two main calls | **Apéndice A** (`enable_pure_2d`,`clear_2d`); fix is **additive delegation** + runner clear | S |
| **C2** | `Texture2d::from_svg_bytes` rasterises synchronously on the frame thread, no cache (measured 6–139 ms; 15.4 ms per identical call) | C | CRÍTICO | `ferrous_assets/src/texture.rs:190-226` | Koncreta's CAD-plan path; frame hitches on load/zoom | **Apéndice A**; keep signature/output identical, add cache/async wrapper | M |
| **C3** | Legacy `Vec<Option<Element>>` mirrored **partially** to ECS; `translate`/`set_rotation`/`set_scale`/`rotate_*` update only the legacy Vec the renderer never reads | B | CRÍTICO | `ferrous_core/.../scene.rs:302-401` | Stale transforms + double storage; blocks ECS unification | Not Apéndice A today, but `World` surface used by consumers | S (sync patch) / L (unify) |
| **C4** | `line_with_cap(..., cap_style: u8)` magic values; invalid values silently become Round; typed `CapStyle` exists | D | CRÍTICO | `ferrous_2d/src/render/types.rs:150-183`, `components/path.rs:22-31` | CAD joints unenforceable; typos compile | **Apéndice A** (`line_with_cap`); add typed `*_cap_style` method | S |

### Tier 1 — ALTO

| ID | Item | Owner | Location | Measured/est. impact | Depends on | Effort |
|---|---|---|---|---|---|---|
| **H1** | #12 colour-space policy: swapchain non-sRGB vs sRGB textures vs raw shape colours (0.5 → 128 native vs 187 sRGB) | A (+C inventory) | `graphics.rs:143-150`, `renderer_core.rs:1853`, `texture.rs:42` | Systematic ~59-level darkening on native; consumer palettes need re-validation | — | S/M |
| **H2** | #10 two uncoordinated 2D paths: overlay ignores z, drops all sprites, duplicates ortho camera | A | `renderer_core.rs:1432-1478,1795-1810` | Wrong draw order + invisible sprites in Full3D; ~200 duplicated lines | — | L |
| **H3** | Batching coalescing collapses to 1 draw call/primitive when z differs (measured 4096→4096, 2048 sprites→2048) | A | `renderer_core.rs:1140-1217`, `ferrous_2d/render/batch.rs` | O(primitives) draws + state changes once per-element z is used | — | M |
| **H4** | Sprite texture churn: monotonic IDs never reused; `free_sprite_texture` leaves orphan `SpriteBatcher` keys (measured 2000 stale keys) | C | `renderer_core.rs:1890-1901`, `batch.rs:16-27` | Unbounded CPU iteration + GPU leak on plan reloads | — | S |
| **H5** | No sprite atlas: 1 texture = 1 bind group = 1 draw call (measured 2048→2048, 64 bind groups); `uv_rect` already supports it | C | `batch.rs`, `sprite_2d.wgsl:11`, `renderer.rs:249-262` | Up to N→1 draws / T→1 bind groups | **H3** | L |
| **H6** | `Query` resolves its column with `binary_search` **per row** (~800k u128 compares/frame @100k entities) | B | `ferrous_ecs/src/query.rs:111-157`, `archetype.rs:265-267` | Hot loop of any large ECS scene | — | S/M |
| **H7** | `draw_2d_path` ignores cap/join/fill/close; `PathBuilder::close()` is a no-op; path logic triplicated | A (impl) / D (API) | `renderer_core.rs:1686-1733`, `path.rs:92-112` | CAD Flat joints render Round; API trap | **H2** (unify emission) | M |
| **H8** | Per-frame UI `image_bind_group` rebuild + batcher scratch realloc (measured 56–92 allocs/frame, 450–650 KB/frame sprites) | C | `gui_renderer.rs:375-420`, `ferrous_2d/render/renderer.rs:181-233` | Allocator pressure / staging copies every frame (Koncreta's immediate UI) | — | M |
| **H9** | `FrameBuilder::mark_dirty` is a no-op: `sync_world` re-marks `scene_dirty=true` every frame | B | `renderer_core.rs:995-1019`, `frame_builder.rs:616-664` | 3D "zero CPU work" fast path never taken | — | S |
| **H10** | Overlay + `Flat2D` + `draw_2d_path` + polygon case have no harness coverage; harness `DEFAULT_FORMAT` (sRGB) ≠ production (non-sRGB) | D | `headless.rs:57` | Pixel assertions don't reflect Koncreta; regressions invisible | H1/H2 | S/M |
| **H11** | No polygon/triangle primitive; `ShapeInstance::from_model` is private; shader is rounded-rect SDF only — Koncreta's ear-clipping case has no direct path | D | `shape_2d.wgsl:79-126`, `types.rs:84` | First-class CAD/BIM feature gap | — | S (expose) / M-L (primitive) |
| **H12** | 2D ECS path unreachable: `ferrous_2d::systems` never registered; spawning `Sprite`/`Shape2d` renders nothing (no warning) | B/D | `batching.rs:6,44`, `runner/types.rs:64-69` | Public components are a silent no-op; duplicated bridge in engine | — | M |
| **H13** | UI texture-array slots: native 16 vs web 8; validation uses 16 on both; web silently drops textures 9–16, native panics >16 | C/D | `ui_render/pipelines/layout.rs:51-100`, `gui_batch.rs:226-231` | Cross-platform correctness + native panic | — | S/M |
| **H14** | `draw_2d_path`/caps unify + `#9` remaining lyon cap/join hardcode in `circle_2d`/`rect_2d` | A | `shapes2d.rs:227-257` | Contractual; no measured impact | H7 | S |

### Tier 2 — MEDIO

| ID | Item | Owner | Location | Effort |
|---|---|---|---|---|
| M1 | `cargo test -p ferrous_ecs` doesn't compile (duplicate `mod tests`, missing derive import) — blocks all ECS tests | D (dev-experience) / B | `ferrous_ecs/src/component.rs:172,244` | S |
| M2 | `ferrous_assets --features gpu` fails without `text` (feature gate bug) | C | `font_importer.rs:63-71`, `Cargo.toml` | S |
| M3 | Only `from_svg_bytes` test doesn't compile; feature `svg` enabled nowhere → zero coverage | C | `texture.rs:249-270` | S |
| M4 | AA redundancy: default FXAA over linear HDR on top of SDF AA (Pure2D has no post-AA) | A | `antialiasing_pass.rs:48-52`, `renderer_core.rs:1496-1533` | S/M |
| M5 | Unstable `sort_unstable_by` for z-order; equal-z order only works by luck | A | `renderer_core.rs:1156-1158` | S |
| M6 | `Flat2D` renders no shapes/sprites; docs describe a full 2D mode | A/D | `renderer_core.rs:1256-1283` | S/M |
| M7 | README/getting-started examples don't compile against `DrawContext`/`UiSystem` | D | `README.md:37-101` | S |
| M8 | Dead code: `RendererPasses` (839 lines) + `SpritePass` + `ferrous_2d::systems`; `rayon` dead dep in `ferrous_renderer` | A/B/D | `renderer_passes.rs`, `passes/sprite_pass.rs` | S |
| M9 | `ParallelScheduler` is sequential, never enabled, docblock lies; fix or delete (delete recommended; native-only if implemented) | B | `ferrous_ecs/src/system.rs:287-540` | S |
| M10 | `PathCommand` 2D/3D name collision; glam 0.29 vs Koncreta 0.32.1 friction | D | `ferrous_engine/src/lib.rs:64` | S/M |
| M11 | `World::spawn` allocates ~5 Vec + linear column lookup per entity | B | `ferrous_ecs/src/world.rs:64-118` | S/M |
| M12 | `Query::State` never invalidated; `change_tick` ignored | B | `ferrous_ecs/src/query.rs:271-316` | S |
| M13 | `AssetServer::tick()` hot-reload imports synchronously on the render thread; no eviction | C | `server.rs:294-340` | S/M |
| M14 | `Texture2d` has no in-place update; `AssetServer` never frees slots | C | `texture.rs:8-94`, `server.rs:100-201` | S/M |
| M15 | `ferrous_svg::SvgMesh::from_str` is a placeholder (draws hardcoded geometry) | C | `ferrous_svg/src/lib.rs:33-86` | M/L |
| M16 | Error-policy inconsistency (`Result` vs `assert_eq!` vs slice panic); `register_sprite_texture` doesn't validate `data.len()` | D | `renderer_core.rs:1827-1859`, `texture.rs:24-31` | S |
| M17 | mkdocs structural: 16 residual cross-crate link warnings from copy-flatten | D | `scripts/build_docs.sh:16-37` | S/M |
| M18 | Pipeline `cache: None` everywhere; mode switches recompile pipelines | A | `ferrous_gpu/src/context.rs:12-19` | S/M |

### Tier 3 — BAJO

`_z` variants for `rounded_rect`/`rect_outline`/`rect_rotated`; `Vec4` vs `[f32;4]` inconsistency; `Shape2d` constructors; `AssetServer` stale wasm doc; instance full re-upload each frame (owner B/C, deferred — no benefit while immediate-mode); write the missing `ADR-001`/`3D_BRIDGE.md` (process); third-party consumer check before removing `RendererPasses`.

## 3. Implementation order (Fase 3)

Dependency-ordered waves. Each fix must report before/after delta using D's harness (draw calls, bytes/frame, allocs/frame).

1. **Safe non-breaking quick wins** (no Apéndice A, no consumer-visible change): H4 (churn/leak), M1 (ECS test compile), M2/M3 (feature gates + SVG test), M5 (stable sort), M8 (dead code), M18 (pipeline cache), H6 (Query column cache). Owner: respective.
2. **#12 colour-space** (H1) — decision + migration note; then update harness `DEFAULT_FORMAT` (H10) and C's texture inventory; then M4 (AA placement).
3. **Unify 2D emission** (H2) → then H3 (coalescing) → then H5 (atlas) → then H7/H14 (path unify). Owner A with C/D coordination.
4. **CRÍTICO gated** (C1–C4) once approved, each with migration note + README/`book` updates for Koncreta/GUIMaker.
5. **MEDIO/BAJO** as capacity allows.

## 4. Consumer migration summary (to hand to Koncreta / GUIMaker maintainers)

Only once Tier-0 items are approved and implemented:
- **C1**: canonical calls become `ctx.render.enable_pure_2d(...)` / `ctx.render.clear_2d()` (the old `renderer_mut()` path keeps working). Runner will auto-clear the 2D batchers each frame — remove any manual start-of-frame `clear_2d()` if it caused double clears.
- **C2**: `Texture2d::from_svg_bytes` signature and output unchanged; a cache is layered behind it (callers that relied on getting a fresh texture each call should switch to the cached handle).
- **C3**: if the legacy `Vec` is unified into the ECS, `World::get_mut`/`iter_mut` returning `&mut Element` may change; `clear`/`iter_with_handles`/`despawn` (what Koncreta uses) stay.
- **C4**: `line_with_cap(u8)` keeps working; prefer the new typed `line_with_cap_style(..., CapStyle)`.

## 5. Explicitly out of scope / not verifiable here

- Absolute FPS/vsync/latency (software rasteriser). Only same-machine deltas.
- Real GPU swapchain presentation and actual native colour output (H1 is code+readback verified, not a display-verified claim).
- The UI path (`GuiRenderer`/`GuiBatch`) bind-group cost and 16/8 slot mismatch — requires a window; reported from code.
- wasm32 runtime behaviour (8 slots, `fetch` asset loading) — not compiled/run on this VPS.
- Whether external (non-workspace) consumers reference `RendererPasses`/`SpritePass` — must be checked before removal.
- 3D implementation/unification itself — design constraint only, per Apéndice B.
