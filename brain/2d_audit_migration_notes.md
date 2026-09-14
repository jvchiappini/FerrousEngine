# FerrousEngine 2D Audit — Consumer Migration Notes (Koncreta / GUIMaker)

> Produced at the end of Phase 3. Every item below is already implemented and merged
> into `main`. Koncreta and GUIMaker are path-deps of this checkout, so these changes
> take effect immediately on their next build. Each entry states what changed, what
> breaks (if anything), and what to update in each consumer.

## Required updates

### 1. `RenderContext` 2D facade + automatic per-frame 2D clear (C1)
- **Changed:** `crates/ferrous_engine/src/render_context.rs` now exposes additive
  delegations `enable_pure_2d`, `clear_2d`, `draw_sprite_2d`, `register_sprite_texture`
  (same signatures as `Renderer`). The engine runner now clears the 2D batchers at the
  **start of every frame**.
- **Breaks:** nothing (additive). But behaviour changed for immediate-mode 2D:
  - Canonical calls become `ctx.render.enable_pure_2d(color)` / `ctx.render.clear_2d()`.
    The old `ctx.render.renderer_mut().clear_2d()` path keeps working.
  - Any geometry pushed once in `setup()` and expected to persist is **no longer
    rendered**; draw 2D geometry from `update()`/`draw_ui()`.
  - Remove any manual start-of-frame `clear_2d()` (harmless/redundant now).
- **Koncreta action:** switch to `ctx.render.*`; move 2D draws into the per-frame hook;
  drop manual clears.
- **GUIMaker action:** same if it uses the 2D shape path; note GUIMaker is already
  desynced on `AppMode` independently of this audit.

### 2. Colour-space policy is now sRGB output (#12 / H1)
- **Changed:** `graphics.rs` now prefers an `*Srgb` surface format; `ShapeInstance.color` /
  `SpriteInstance.color` are documented/handled as **linear**; the manual `pow(1/2.2)`
  in `post.wgsl` was removed (would double-encode).
- **Breaks:** on-screen colours change — native 2D renders **brighter** (mid-grey 0.5:
  128 → 187). This is a visual/palette change, not an API break.
- **Koncreta action:** re-validate CAD/UI palettes. To preserve the old look, convert
  display-referred colour constants through the sRGB EOTF (0.5 → ~0.214), or author
  colours in linear.
- **GUIMaker action:** same palette re-validation for desktop UI.

### 3. `Texture2d::from_svg_bytes` now caches (C2)
- **Changed:** signature and returned pixels are **unchanged**; a content-keyed cache
  `(hash,width,height)` sources repeated calls. New additive `Texture2d::from_svg_bytes_cached`
  returns a shared `Arc<Texture2d>`; new `Texture2d::update_rgba8` updates in place.
- **Breaks:** nothing.
- **Koncreta action:** keep calling `from_svg_bytes` (now ~9.5× faster on repeats). If you
  re-create an `Arc<Texture2d>` every frame to feed the UI, switch to
  `from_svg_bytes_cached` so the UI's `Arc::ptr_eq` slot dedup stays stable. Do not call
  `update_rgba8` on a shared cached handle.
- **GUIMaker action:** none required.
- **Note:** the global SVG cache assumes a single `wgpu::Device`; multi-device apps must
  build their own `SvgCache`.

### 4. `line_with_cap` typed variant (C4)
- **Changed:** additive `ShapeInstance::line_with_cap_style(from, to, width, color, CapStyle)`.
  `line_with_cap(..., cap_style: u8)` still works unchanged (0=Flat, 1=Round, 2=Square,
  else → Round).
- **Breaks:** nothing.
- **Koncreta action:** prefer the typed method for CAD joints to get compile-time safety.

### 5. Sprite texture ids are now recycled (H4)
- **Changed:** `free_sprite_texture` drops the `SpriteBatcher` key and returns the id to a
  free-list; `register_sprite_texture` can now return a previously used id.
- **Breaks:** only if external code assumed `register_sprite_texture` ids are monotonically
  increasing and never reused.
- **Koncreta/GUIMaker action:** do not treat texture ids as permanent/unique across frees;
  release textures you no longer use.

## Changes with no consumer action

- **M5** deterministic z-sort (total order) — internal, no API change.
- **M8-A** `SpritePass` + `PassManager` removed; `RendererPasses` deprecated (was dead,
  no workspace consumers). If an external crate used `RendererPasses`, migrate off it;
  it will be hard-deleted in a later wave once external usage is ruled out.
- **M8-B** `ferrous_2d::systems::{prepare_shapes_system, prepare_paths_system,
  prepare_sprites_system}` deprecated (were unreachable); use the immediate
  `Renderer::draw_*` API.
- **M18** `EngineContext.pipeline_cache` + `Renderer2d::new_with_cache` added (additive);
  existing `new()` keeps working.
- **H6** ECS `Query` now resolves columns once per archetype; the public `Query` API is
  unchanged, but the internal `WorldQuery` trait changed — only matters if you implemented
  it manually (unlikely; it is engine-internal).
- **M1/M2/M3** ECS test suite compiles; `ferrous_assets --features gpu` compiles without
  `text`; the `from_svg_bytes` test compiles under `--features svg,gpu,text`.

## Not addressed (not approved / out of scope this wave)
H2/H3/H5/H7 (2D path unification, batching coalescing, sprite atlas, path cap/join unify),
H8/H11/H13, M4, M6, M7, M9, M10–M17, C3 (legacy scene mirror). See
`brain/2d_audit_master_plan.md` for the full prioritised backlog.
