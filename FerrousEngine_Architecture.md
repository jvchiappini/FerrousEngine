# FerrousEngine — Architecture Analysis & Modernization Roadmap

> **Status:** IN PROGRESS — Phase 1 ✅ complete, Phase 3 partially complete.
> **Date:** March 2026  
> **Analyst:** GitHub Copilot (Claude Sonnet 4.6)

---

## 🤖 Agent Session State — READ THIS FIRST

> This section is written for the AI agent resuming work after a context reset.
> Update it at the end of every work session before context is cleared.

### What has been implemented so far

#### ✅ Phase 1 — `ferrous_ecs` crate (100% complete, all tests pass)

**New crate:** `crates/ferrous_ecs/` — added to workspace `Cargo.toml` as first member.

Files created:
- `src/entity.rs` — `Entity { index: u32, generation: u32 }`, `EntityAllocator` (LIFO free stack), `EntityRecord`
- `src/component.rs` — `Component: Send+Sync+'static` blanket trait, `ComponentInfo` (type-erased metadata with drop/clone fn ptrs), `ComponentSet` (sorted+deduped `Vec<TypeId>`), `Bundle` trait + `impl_bundle!` macro for arities 1–12
- `src/archetype.rs` — `ComponentColumn` (type-erased SoA, `pub(crate) len`), `Archetype`, `ArchetypeStore`; key methods: `push_raw`, `swap_remove`, `swap_remove_no_drop`, `get<T>`, `get_mut<T>`, `clone_into`
- `src/world.rs` — `World` with `spawn`, `despawn`, `get`, `get_mut`, `insert`, `remove`, `query`, `query2`, `query3`; uses phase-based clone→push→swap_remove_no_drop to avoid dual borrow in `move_entity_between_archetypes`
- `src/query.rs` — `Query<'w,C>` (immutable), `QueryMut<'w,C>` (index-based to avoid borrow conflict)
- `src/resource.rs` — `ResourceMap` over `HashMap<TypeId, Box<dyn Any+Send+Sync>>`
- `src/system.rs` — `System` trait, `SystemScheduler`, `FnSystem`, `fn_system()` constructor
- `src/lib.rs` — module declarations + `pub mod prelude`

**Test results:** 21 unit tests + 5 doc-tests — ALL PASS (`cargo test -p ferrous_ecs`)

#### ✅ Phase 3 (partial) — Renderer struct decomposition

**New files in `crates/ferrous_renderer/src/`:**

- `camera_system.rs` — `CameraSystem { pub camera: Camera, pub orbit: OrbitState, pub gpu: GpuCamera }`
  - Methods: `new(device, layouts, width, height)`, `handle_input(&mut self, input, dt)`, `sync_gpu(&mut self, queue)`, `set_aspect(f32)`, `view_matrix()`, `proj_matrix()`, `view_proj()`, `eye()`, `target()`
  - Declared in `lib.rs` as `pub mod camera_system; pub use camera_system::CameraSystem;`
  
- `frame_builder.rs` — `FrameBuilder` with fields: `draw_commands_cache`, `instanced_commands_cache`, `instance_matrix_scratch`, `shadow_scene_cache`, `shadow_instanced_cache`, `shadow_matrix_scratch`, `prev_view_proj: Option<Mat4>`, `scene_dirty: bool`
  - Methods: `new()`, `mark_dirty()`
  - Declared in `lib.rs` as `pub mod frame_builder; pub use frame_builder::FrameBuilder;`

**`Renderer` struct changes in `lib.rs`:**
- Removed fields: `pub camera`, `pub orbit`, `gpu_camera`, `draw_commands_cache`, `instanced_commands_cache`, `instance_matrix_scratch`, `shadow_scene_cache`, `shadow_instanced_cache`, `shadow_matrix_scratch`, `prev_view_proj`, `scene_dirty`
- Added fields: `pub camera_system: CameraSystem`, `frame_builder: FrameBuilder`
- Added compat accessor methods on `Renderer`: `camera() -> &Camera`, `camera_mut() -> &mut Camera`, `orbit_mut() -> &mut OrbitState`
- ALL methods updated: `handle_input`, `resize`, `set_viewport`, `do_render`, `build_base_packet`, `reclaim_packet_cache`, `add_object`, etc.

**External callers updated:**
- `crates/ferrous_app/src/runner.rs` line ~256: `renderer.camera.eye/.target` → `renderer.camera().eye/.target`
- `crates/ferrous_editor/src/app.rs` lines ~327-341: `renderer.orbit.*` → `renderer.camera_system.orbit.*`, `renderer.camera.*` → `renderer.camera().*` / `renderer.camera_mut().*`

**Build status:** `cargo build` (full workspace) — ✅ `Finished` — zero errors, only pre-existing warnings.

---

### What to do next (in priority order)

#### 🔴 Next immediate task — Phase 3 continued

**3.1 — Extract `MaterialRegistry` module** *(medium effort)*

The `MaterialRegistry` struct is currently defined inside `lib.rs`. It should be moved to `crates/ferrous_renderer/src/material_registry.rs`:
- Search for `struct MaterialRegistry` in `lib.rs` — find the struct definition + all `impl MaterialRegistry` blocks
- Create `src/material_registry.rs`, move code there
- Add `pub mod material_registry; pub use material_registry::MaterialRegistry;` to `lib.rs`
- Fix all `use` imports in the new file (it will need `wgpu`, `Arc`, `PipelineLayouts`, etc.)
- Run `cargo build -p ferrous_renderer` to verify

**3.4 — Extract `sync_world` to `world_sync.rs`** *(medium effort)*

The method `Renderer::sync_world` is a large method in `lib.rs` that bridges `ferrous_core::World` → renderer internal objects. Extract it to `src/world_sync.rs` as a free function or impl block on a new `WorldSync` struct. Key dependencies it uses: `world_objects`, `model_buf`, `context.device/queue`, `material_registry`, `mesh_cache`.

**3.6 — `Renderer` as thin coordinator** *(high effort, do last in Phase 3)*

After 3.1 and 3.4 are done, `lib.rs` should be < 600 lines. Then:
- Move `gizmo_pipeline` + `execute_gizmo_pass` to a `GizmoSystem` struct
- `Renderer::do_render` delegates to `RenderGraph::execute` instead of inline pass calls

#### 🟡 Phase 2 — ECS integration into `ferrous_core`

Only start after Phase 3.1–3.4 are done. Key tasks:
- Add `ferrous_ecs` as dependency to `ferrous_core/Cargo.toml`
- Define `Transform`, `Visibility`, `Name`, `Tags` as `Component` in `ferrous_core`
- Bridge existing `ferrous_core::scene::World` to use `ferrous_ecs::World` internally
- Keep all existing public API (`spawn_cube`, `iter()`, etc.) working — zero breaking changes

#### 🟢 Phase 4+ (future)

Phases 4–11 not yet started. See roadmap below.

---

### Key file locations cheat sheet

| File | Purpose |
|------|---------|
| `crates/ferrous_ecs/src/` | New ECS crate — complete |
| `crates/ferrous_renderer/src/lib.rs` | ~1730 lines — main renderer, still needs decomposition |
| `crates/ferrous_renderer/src/camera_system.rs` | NEW — CameraSystem |
| `crates/ferrous_renderer/src/frame_builder.rs` | NEW — FrameBuilder (per-frame caches) |
| `crates/ferrous_renderer/src/passes/world_pass.rs` | PBR world rendering pass |
| `crates/ferrous_renderer/src/passes/prepass.rs` | Depth+normal prepass |
| `crates/ferrous_core/src/scene/world.rs` | Old entity container (~923 lines) — will be wrapped |
| `crates/ferrous_editor/src/app.rs` | Editor main — 907 lines, needs Phase 9 split |
| `crates/ferrous_app/src/runner.rs` | winit loop — 636 lines |
| `FerrousEngine_Architecture.md` | This file — update agent state section each session |

### How to verify nothing is broken

```
cargo build              # full workspace — must show Finished with zero errors
cargo test -p ferrous_ecs  # must show 21+5 tests passing
cargo run -p ferrous_editor  # editor must launch and render correctly
```

---

## Table of Contents

1. [Executive Summary](#1-executive-summary)
2. [Current Architecture Deep Dive](#2-current-architecture-deep-dive)
   - 2.1 [Dependency Graph](#21-dependency-graph)
   - 2.2 [ferrous_core](#22-ferrous_core)
   - 2.3 [ferrous_renderer](#23-ferrous_renderer)
   - 2.4 [ferrous_gui](#24-ferrous_gui)
   - 2.5 [ferrous_assets](#25-ferrous_assets)
   - 2.6 [ferrous_app](#26-ferrous_app)
   - 2.7 [ferrous_editor](#27-ferrous_editor)
3. [Strengths of the Current Design](#3-strengths-of-the-current-design)
4. [Critical Pain Points & Limitations](#4-critical-pain-points--limitations)
5. [Proposed Architecture](#5-proposed-architecture)
   - 5.1 [New Crate Map](#51-new-crate-map)
   - 5.2 [ECS Layer — ferrous_ecs](#52-ecs-layer--ferrous_ecs)
   - 5.3 [New Scene & World Design](#53-new-scene--world-design)
   - 5.4 [Render Graph Redesign](#54-render-graph-redesign)
   - 5.5 [Asset Pipeline Redesign](#55-asset-pipeline-redesign)
   - 5.6 [Configuration & Style System](#56-configuration--style-system)
   - 5.7 [Plugin System](#57-plugin-system)
   - 5.8 [Multithreading Strategy](#58-multithreading-strategy)
6. [Performance Targets & Optimizations](#6-performance-targets--optimizations)
7. [Implementation Roadmap (Step-by-Step Tasks)](#7-implementation-roadmap-step-by-step-tasks)
8. [API Preview (Post-Refactor)](#8-api-preview-post-refactor)
9. [Migration Guide (Current → New)](#9-migration-guide-current--new)

---

## 1. Executive Summary

FerrousEngine is an ambitious, ground-up Rust game engine built on **wgpu 23**, **winit 0.30** and **glam**. After a deep analysis of all six crates, the engine already demonstrates several impressive qualities:

- A clean `RenderPass` trait with a two-phase prepare/execute pattern.
- A working PBR pipeline with IBL, SSAO, shadows, bloom and tone mapping.
- A frustum-culling system based on Gribb-Hartmann plane extraction.
- A data-oriented instancing path for batching world objects.
- A custom GUI system (`ferrous_gui`) that avoids egui dependency entirely.
- Cross-platform design (native + wasm32).

However, the engine has reached a critical threshold where **further features will require architectural changes** to stay maintainable and performant. The three biggest structural blockers are:

1. **No ECS** — `World` is an ad-hoc entity container (Vec<Option<Element>>), coupling all components into one fat struct (`Element`). Adding new component types today requires modifying `ferrous_core`.
2. **Monolithic `Renderer` struct** — `lib.rs` in `ferrous_renderer` is 1,728 lines. It owns the scene sync, instancing, culling, material management and frame orchestration all in one place. Splitting functionality is impossible without a large refactor.
3. **No configuration/plugin layer** — Rendering style (PBR AAA vs. anime vs. low-poly) requires code changes. There is no hot-swappable material graph, render pipeline selector, or project-level configuration file.

The proposed architecture solves all three problems while remaining **backward-compatible** during the transition.

---

## 2. Current Architecture Deep Dive

### 2.1 Dependency Graph

```
ferrous_editor
    └── ferrous_app
            ├── ferrous_core       (no renderer deps — headless-safe)
            │       ├── glam
            │       └── winit (KeyCode/MouseButton re-exports only)
            ├── ferrous_renderer
            │       ├── ferrous_core
            │       ├── ferrous_gui    (GuiBatch, TextBatch)
            │       ├── ferrous_assets (gltf loader)
            │       └── wgpu / bytemuck / rayon / rand
            ├── ferrous_gui
            │       ├── ferrous_core  (InputState)
            │       └── winit
            └── ferrous_assets
                    ├── ferrous_core  (AlphaMode re-export)
                    └── gltf / image
```

**Key observations:**
- `ferrous_core` correctly has zero renderer dependencies. ✅
- `ferrous_renderer` depends on both `ferrous_gui` **and** `ferrous_assets`. This creates a tight coupling: changing the GUI API or asset format forces a renderer rebuild. ⚠️
- `ferrous_app` depends on `ferrous_renderer` directly, exposing `ctx.renderer` to game code. This means game authors can call low-level GPU APIs from application callbacks. ⚠️
- `rand` is in `ferrous_renderer` (used for SSAO kernel generation). It should live in a utility crate. ⚠️

---

### 2.2 ferrous_core

**Location:** `crates/ferrous_core/src/`

| Module | Lines (approx) | Role |
|--------|----------------|------|
| `scene/world.rs` | 923 | Entity container + EntityBuilder |
| `scene/camera.rs` | 112 | Camera + CameraUniform |
| `scene/material.rs` | 82 | MaterialDescriptor + MaterialHandle |
| `scene/gizmo.rs` | ~150 | Gizmo state and drawing types |
| `transform.rs` | 196 | TRS Transform |
| `input.rs` | 213 | InputState (keyboard + mouse) |
| `time.rs` | 132 | Time + TimeClock |
| `color.rs` | ~80 | RGBA Color |
| `context.rs` | 100 | EngineContext (wgpu device/queue) |
| `viewport.rs` | ~30 | Viewport rect |
| `metrics.rs` | ~40 | CPU/RAM helpers |
| `render_stats.rs` | 24 | RenderStats |

**Entity model:**

```rust
// Current: one fat struct, all components baked in
pub struct Element {
    pub id: u64,
    pub name: String,
    pub transform: Transform,
    pub material: MaterialComponent,  // always present, even for lights
    pub kind: ElementKind,            // enum over ALL possible geometry types
    pub visible: bool,
    pub tags: Vec<String>,
    pub render_handle: Option<usize>, // opaque renderer coupling
    pub point_light: Option<PointLightComponent>,
}
```

**Problems identified:**
- Every entity carries a `MaterialComponent` even if it's a light, empty, or logic-only entity.
- `ElementKind` is an exhaustive enum — adding a new primitive requires touching `ferrous_core`, `ferrous_renderer`'s `world_sync.rs`, `build_base_packet`, shadow code, and any match arm that enumerates kinds.
- `render_handle: Option<usize>` tightly couples the scene to the renderer's internal slot numbering.
- `World` uses a global `AtomicU64` ID counter with a `Vec<Option<Element>>` (slot array). IDs are globally unique across all `World` instances — this is actually correct but limits multi-world scenarios.
- `tags: Vec<String>` is a flexible but slow querying mechanism (O(n) for tag-based queries).

---

### 2.3 ferrous_renderer

**Location:** `crates/ferrous_renderer/src/`

**Render pass pipeline (frame order):**

```
PrePass (depth + normals, view-space)
  └─ SsaoPass  (ambient occlusion from depth + normals)
       └─ SsaoBlurPass  (bilateral blur of SSAO)
            └─ WorldPass  (PBR geometry + shadow map)
                 ├─ SkyboxPass  (procedural or HDRI skybox)
                 │    └─ PostProcessPass  (ACES tonemapping + bloom)
                 │         └─ UiPass  (2D GUI overlay)
                 └─ [extra_passes…]  (user-defined)
```

**Shader inventory:**

| Shader | Technique |
|--------|-----------|
| `pbr.wgsl` | Cook-Torrance BRDF, IBL (irradiance + prefilter + BRDF LUT), point lights, directional light + shadows |
| `prepass.wgsl` | Depth + view-space normals |
| `ssao.wgsl` | Hemisphere sampling SSAO |
| `ssao_blur.wgsl` | Bilateral blur |
| `post.wgsl` | ACES tone mapping + bloom combine |
| `bloom.wgsl` | Dual-filter kawase downsample/upsample |
| `shadow.wgsl` / `shadow_instanced.wgsl` | Depth-only shadow map |
| `skybox.wgsl` | Cube-map skybox |
| `equirect_to_cubemap.wgsl` | HDRI → cubemap conversion |
| `irradiance.wgsl` | Diffuse irradiance baking |
| `brdf.wgsl` | BRDF LUT precomputation |
| `prefilter.wgsl` | Specular prefilter mip baking |
| `instanced.wgsl` / `prepass_instanced.wgsl` | Storage-buffer instancing |
| `gizmo.wgsl` | Line gizmos |
| `gui.wgsl` | Quad-based GUI |
| `text.wgsl` | MSDF text |
| `base.wgsl` | Shared includes |

**Critical structural issues in `lib.rs` (1,728 lines):**
- `Renderer` is a god struct owning: camera, orbit state, all render passes, scene sync state, material registry, mesh cache, pipeline layouts, per-frame scratch buffers, shadow resources, light data, and statistics.
- Two parallel object systems: **legacy objects** (manually spawned via `add_object`, uses dynamic uniform offsets) and **world objects** (from `World::sync`, uses instancing). Both coexist but have inconsistent feature support (e.g., the legacy path doesn't support all PBR features equally).
- Frustum culling, instancing, and draw command building happen inside `build_base_packet()` which is one very large function.
- Only one sphere mesh is cached regardless of subdivision parameters — different quality spheres would need separate workarounds.

---

### 2.4 ferrous_gui

**Location:** `crates/ferrous_gui/src/`

| Module | Role |
|--------|------|
| `widget.rs` | `Widget` trait: collect/hit/mouse_input/keyboard_input |
| `layout.rs` | `Node`, `Style`, `Rect`, `Units`, `Alignment`, `DisplayMode` |
| `canvas.rs` | Widget container with focus tracking |
| `ui.rs` | High-level `Ui` wrapper around Canvas |
| `renderer.rs` | `GuiRenderer`, `GuiBatch`, `GuiQuad`, `TextBatch` |
| `button.rs`, `slider.rs`, `textinput.rs`, `color_picker.rs` | Concrete widgets |
| `builders.rs` | Declarative `Row`, `Column`, `Text`, `UiButton` |
| `viewport_widget.rs` | 3D viewport region widget |

**Current state:** The GUI is functional but uses `Rc<RefCell<T>>` for shared widget references, which prevents multi-threaded layout computation and creates complex ownership patterns in the editor. The layout system supports flex rows/columns but lacks CSS-grid, absolute positioning, and z-ordering beyond the basic 2D overlay.

**Critical issues:**
- `GuiRenderer` is constructed and managed inside `ferrous_renderer` — the GUI has no standalone render path.
- No theme/style system — colors and sizes are hardcoded per widget instantiation.
- No retained-mode vs immediate-mode abstraction. The current model is a hybrid that is hard to optimize.

---

### 2.5 ferrous_assets

**Location:** `crates/ferrous_assets/src/`

| Module | Role |
|--------|------|
| `gltf_loader.rs` | glTF/GLB import → `AssetModel` (CPU-side) |
| `texture.rs` | `Texture2d` — image loading (PNG/JPEG) |
| `font/` | MSDF font atlas baking: parser, binary_reader, msdf_gen, atlas, path, tables |

**Strengths:** The GLTF loader returns a purely CPU-side `AssetModel` with no GPU types, keeping `ferrous_assets` free of wgpu. ✅

**Critical issues:**
- No **asset handle system** — assets are loaded synchronously, returned by value, and the caller must track them manually.
- No **async loading** — large models block the main thread.
- No **asset registry** or **hot-reload** support.
- No **asset metadata** (LOD levels, compression settings, streaming priority).
- The font atlas is non-incremental — the full atlas is regenerated on each load.

---

### 2.6 ferrous_app

**Location:** `crates/ferrous_app/src/`

| Module | Role |
|--------|------|
| `traits.rs` | `FerrousApp` trait |
| `context.rs` | `AppContext<'a>` — per-frame context |
| `builder.rs` | `App`, `AppConfig` |
| `runner.rs` | winit event loop + GPU init (636 lines) |
| `graphics.rs` | `GraphicsState` (surface, swapchain) |
| `asset_bridge.rs` | `spawn_gltf` helper |

**Strengths:**
- Clean `FerrousApp` trait with sensible defaults.
- `AppConfig` exposes VSYNC, target FPS, MSAA, background color, font path — all without touching code.

**Critical issues:**
- `AppContext` exposes `pub renderer: &'a mut ferrous_renderer::Renderer` directly. Any game author can call GPU-level APIs from `update()`. This bypasses the abstraction and creates fragile dependencies.
- `runner.rs` is 636 lines and conflates: event routing, GPU surface management, frame pacing, world sync, GUI batch assembly, and draw call orchestration. It should be split into focused sub-systems.
- Frame pacing uses a manual deadline tracker (`next_frame_deadline`) that doesn't account for multi-monitor refresh or variable rate shading.
- No **game loop mode selector** (fixed timestep vs. variable timestep vs. semi-fixed).

---

### 2.7 ferrous_editor

**Location:** `crates/ferrous_editor/src/`

| Module | Role |
|--------|------|
| `app.rs` | `EditorApp` (907 lines) — editor main logic |
| `ui/global_light.rs` | Directional light panel |
| `ui/material_inspector.rs` | PBR material inspector |

**Critical issues:**
- `app.rs` is 907 lines with editor logic, benchmark code, UI layout, and 3D interaction all in one file.
- Editor state (selected entity, gizmo mode, inspector) mixes directly with benchmark state (`bench_cube_count`, `fps_history`).
- No **editor plugin system** — adding a new panel requires modifying `app.rs`.
- No **undo/redo** system.
- No **scene serialization** (save/load `.scene` files).

---

## 3. Strengths of the Current Design

The following existing patterns are **good and should be preserved** in the new architecture:

| Pattern | Where | Why It's Good |
|---------|-------|---------------|
| `RenderPass` trait (prepare/execute) | `ferrous_renderer/graph/pass_trait.rs` | Clean two-phase separation; custom passes already possible |
| `FramePacket` with typed extras map | `graph/frame_packet.rs` | Open for extension without coupling systems |
| `ferrous_core` has zero renderer deps | architecture | Enables headless testing and server logic |
| `MaterialDescriptor` in core | `ferrous_core/scene/material.rs` | CPU-side descriptors with GPU handles stay decoupled |
| Gribb-Hartmann frustum culling | `ferrous_renderer/scene/culling.rs` | Correct, fast O(6) AABB visibility test |
| Instancing via storage buffer | `pipeline/instancing.rs` + `scene/world_sync.rs` | Single draw call per mesh family |
| `AppConfig` builder | `ferrous_app/builder.rs` | Config without code changes |
| `FerrousApp` trait with defaults | `ferrous_app/traits.rs` | Minimal boilerplate for games |
| WASM32 conditional bounds | everywhere | Cross-platform without #[cfg] everywhere in user code |
| Rayon for CPU parallelism | `ferrous_renderer` (non-wasm) | Parallel draw command building |

---

## 4. Critical Pain Points & Limitations

### P1 — No ECS (Highest Priority)
**Impact:** Every new component type (physics body, audio emitter, script, animation state) requires modifying `Element` in `ferrous_core` and every system that iterates entities. This is the single biggest blocker to scalability.

### P2 — God-Object Renderer (High Priority)
**Impact:** `Renderer` in `lib.rs` (1,728 lines) is impossible to maintain long-term. It cannot be split across threads. Testing individual passes requires constructing the entire renderer.

### P3 — No Configuration Layer (High Priority)
**Impact:** Changing render quality (AAA PBR → stylized → low-poly) requires code changes. There is no project config file, no render quality preset, and no shader variant selection at runtime.

### P4 — Synchronous Asset Loading (Medium Priority)
**Impact:** Loading a large GLTF model blocks the main thread, causing visible freezes. No background loading means no loading screens or streaming.

### P5 — GUI Ownership Model (Medium Priority)
**Impact:** `Rc<RefCell<T>>` widgets prevent parallel layout and are ergonomically awkward. The GUI has no theming system.

### P6 — Editor Monolith (Medium Priority)
**Impact:** `app.rs` at 907 lines will become unmaintainable as the editor grows. No undo/redo or scene serialization limits practical use.

### P7 — Two Parallel Object Systems (Medium Priority)
**Impact:** "Legacy objects" and "world objects" have different feature sets and add complexity to every rendering code path. The legacy path should be removed.

### P8 — No Shader Variant / Permutation System (Low–Medium Priority)
**Impact:** Each quality tier (anime cel, toon outline, PBR, low-poly) currently requires a hand-written WGSL file with no shared infrastructure. A permutation system (defines/specialization constants) would eliminate duplication.

---

## 5. Proposed Architecture

### 5.1 New Crate Map

```
FerrousEngine (workspace)
├── ferrous_ecs          ← NEW: archetypes, queries, commands, scheduler
├── ferrous_core         KEEP (slim down; remove Entity/World; add component traits)
├── ferrous_math         ← EXTRACT FROM core: Transform, Color, AABB, BVH
├── ferrous_assets       EVOLVE: add async loader, handle system, hot-reload
├── ferrous_renderer     REFACTOR: split into focused sub-crates
│   ├── ferrous_rhi      ← NEW: thin wgpu abstraction (Buffer, Texture, Pipeline factories)
│   ├── ferrous_render_graph  ← EXTRACT: RenderGraph, passes, FramePacket
│   └── ferrous_renderer KEEP (coordinator only, drastically slimmed)
├── ferrous_gui          EVOLVE: add theming, layout caching, z-order
├── ferrous_physics      ← NEW (optional): AABB/OBB collision, rigid body
├── ferrous_audio        ← NEW (optional): spatial audio via cpal/rodio
├── ferrous_scripting    ← NEW (optional): Lua/Rhai scripting bridge
├── ferrous_app          EVOLVE: add plugin system, game loop modes
└── ferrous_editor       REFACTOR: split into editor panels + undo/redo
```

**Dependency rules (enforced by Cargo):**
```
ferrous_math  ←  ferrous_ecs  ←  ferrous_assets
                     ↓
              ferrous_rhi  ←  ferrous_render_graph  ←  ferrous_renderer
                                         ↓
                                  ferrous_gui
                                         ↓
                              ferrous_app  ←  ferrous_editor
```

No upward dependencies are allowed. Optional crates (`physics`, `audio`, `scripting`) communicate with the engine only through the ECS component/system interface.

---

### 5.2 ECS Layer — ferrous_ecs

The ECS will be **archetype-based** (same design as Bevy/Flecs) for maximum cache efficiency. A minimal, custom ECS is preferred over bringing in `hecs` or `bevy_ecs` to keep compile times low and give full control.

#### Core types

```rust
// ferrous_ecs/src/lib.rs

/// Stable entity identifier: 32-bit generation + 32-bit index.
/// Generation prevents use-after-free bugs with old handles.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct Entity {
    pub index: u32,
    pub generation: u32,
}

/// Marker trait for all components. Auto-implemented via derive macro.
pub trait Component: Send + Sync + 'static {}

/// The primary world container.
pub struct World {
    archetypes: ArchetypeStorage,
    entity_map: EntityAllocator,
    resources: ResourceMap,
}

impl World {
    /// Spawn an entity with the given bundle of components.
    pub fn spawn<B: Bundle>(&mut self, bundle: B) -> Entity { ... }
    
    /// Despawn an entity and all its components.
    pub fn despawn(&mut self, entity: Entity) { ... }
    
    /// Add a component to an existing entity (moves to new archetype).
    pub fn insert<C: Component>(&mut self, entity: Entity, component: C) { ... }
    
    /// Remove a component from an entity.
    pub fn remove<C: Component>(&mut self, entity: Entity) { ... }
    
    /// Query over all entities that have the specified components.
    pub fn query<Q: WorldQuery>(&self) -> QueryIter<Q> { ... }
    
    /// Insert a global resource.
    pub fn insert_resource<R: Resource>(&mut self, resource: R) { ... }
    
    /// Access a global resource immutably.
    pub fn resource<R: Resource>(&self) -> &R { ... }
}
```

#### Built-in components (replacing current Element fields)

```rust
// Components for core_types (ferrous_core re-exports these)
#[derive(Component, Clone, Copy)] pub struct Transform { ... }  // unchanged
#[derive(Component, Clone, Copy)] pub struct Visibility(pub bool);
#[derive(Component, Clone)]       pub struct Name(pub String);
#[derive(Component, Clone)]       pub struct Tags(pub Vec<String>);

// Render components (ferrous_renderer adds these)
#[derive(Component, Clone)]       pub struct MeshHandle(pub AssetHandle<Mesh>);
#[derive(Component, Clone, Copy)] pub struct MaterialHandle(pub u32);
#[derive(Component, Clone, Copy)] pub struct Aabb { pub center: Vec3, pub half_extents: Vec3 }

// Lighting components
#[derive(Component, Clone, Copy)] pub struct DirectionalLight { pub color: Vec3, pub intensity: f32 }
#[derive(Component, Clone, Copy)] pub struct PointLight { pub color: Vec3, pub intensity: f32, pub radius: f32 }

// Physics components (ferrous_physics adds these)
#[derive(Component, Clone)]       pub struct RigidBody { ... }
#[derive(Component, Clone)]       pub struct Collider { ... }
```

#### Archetype storage

```
Archetype table for (Transform, MeshHandle, MaterialHandle, Aabb):
┌─────────────┬────────────────┬──────────────────┬──────┐
│ Transform[] │ MeshHandle[]   │ MaterialHandle[] │ Aabb[]│
├─────────────┼────────────────┼──────────────────┼──────┤
│   tightly   │    packed,     │    contiguous    │  mem  │
│   packed    │   cache-hot    │    in memory     │ cols  │
└─────────────┴────────────────┴──────────────────┴──────┘
```

- Queries generate iterators over raw column slices → SIMD-friendly.
- Archetype moves on component add/remove are O(1) amortized with free lists.
- No boxing, no dynamic dispatch per entity.

#### System Scheduler

```rust
pub struct Scheduler {
    stages: Vec<Stage>,  // PreUpdate, Update, PostUpdate, Render, etc.
}

pub struct Stage {
    systems: Vec<BoxedSystem>,
    execution: ExecutionMode,  // Sequential | Parallel | Exclusive
}

// A system is just a function over queries + resources
pub trait System: Send + Sync + 'static {
    fn run(&mut self, world: &World);
}
```

Systems in a `Parallel` stage are analyzed for read/write conflicts and scheduled without data races using a borrow-check graph. This is how Bevy achieves multithreaded system dispatch.

---

### 5.3 New Scene & World Design

The new `World` replaces `ferrous_core::scene::World`. The editor scene view becomes a query:

```rust
// OLD (coupled struct)
for element in world.iter() {
    match element.kind { ElementKind::Cube { .. } => { ... } }
}

// NEW (ECS query — zero allocation, cache-friendly iteration)
for (entity, transform, mesh, material) in world.query::<(Entity, &Transform, &MeshHandle, &MaterialHandle)>() {
    renderer.submit_draw(entity, transform, mesh, material);
}
```

**Scene hierarchy** (parent–child relationships):

```rust
#[derive(Component)] pub struct Parent(pub Entity);
#[derive(Component)] pub struct Children(pub SmallVec<[Entity; 4]>);

// System: propagate global transforms from local transforms + parent chain
fn propagate_transforms(world: &World) {
    // Topological sort via Children graph → update GlobalTransform
}
```

**World serialization** (scene save/load):

```rust
// Scene file format: RON or custom binary
pub struct SceneDescriptor {
    pub entities: Vec<EntityDescriptor>,
}
pub struct EntityDescriptor {
    pub components: Vec<SerializedComponent>,
}
// Each Component can implement Serialize/Deserialize via serde
```

---

### 5.4 Render Graph Redesign

The current render graph already has the right abstraction (`RenderPass` trait + `FramePacket`). The proposal expands it into a true **dependency-tracked render graph**:

#### Render Graph Node

```rust
pub struct RenderGraphNode {
    pub name: &'static str,
    pub reads:  Vec<RenderResourceId>,  // input textures/buffers
    pub writes: Vec<RenderResourceId>,  // output textures/buffers
    pub pass:   Box<dyn RenderPass>,
}

pub struct RenderGraph {
    nodes: Vec<RenderGraphNode>,
    resources: RenderResourcePool,
}

impl RenderGraph {
    /// Topologically sort nodes and compile an execution order.
    pub fn compile(&mut self) -> ExecutionPlan { ... }
    
    /// Execute the compiled plan for one frame.
    pub fn execute(&mut self, device: &Device, queue: &Queue, packet: &FramePacket) { ... }
    
    /// Insert or replace a node at runtime (for dynamic quality presets).
    pub fn set_node(&mut self, node: RenderGraphNode) { ... }
    
    /// Remove a pass by name (e.g. disable SSAO for low-end devices).
    pub fn remove_node(&mut self, name: &str) { ... }
}
```

**Default render graph (high quality / AAA PBR):**

```
[ShadowPass]────────────────────────────────────────┐
[PrePass (depth+normals)] ──→ [SsaoPass] ──→ [SsaoBlurPass] ──→ [WorldPass (PBR)] ──→ [SkyboxPass] ──→ [PostProcessPass (ACES+Bloom)] ──→ [UiPass]
```

**Low-poly / stylized graph (swapped in via config):**

```
[ShadowPass] ──→ [WorldPass (FlatShaded)] ──→ [OutlinePass] ──→ [PostProcessPass (No Bloom)] ──→ [UiPass]
```

#### Render Quality Presets

```rust
pub enum RenderQuality {
    Ultra,      // Full PBR + SSAO + Bloom + MSAA 4x
    High,       // Full PBR + Bloom, no SSAO
    Medium,     // PBR no IBL + Bloom disabled
    Low,        // Simple diffuse + directional only
    Minimal,    // Depth only (for servers/headless)
}

pub enum RenderStyle {
    PbrRealistic,
    CelShaded { outline_width: f32, palette: ColorPalette },
    LowPoly    { flat_shading: bool },
    Anime      { outline_width: f32, gradient_steps: u32 },
    Custom     { graph: RenderGraph },
}
```

These are set via `AppConfig` or a runtime configuration file (`ferrous.toml`).

---

### 5.5 Asset Pipeline Redesign

#### Async Asset Handle System

```rust
// ferrous_assets/src/handle.rs

/// Type-erased asset handle (16 bytes, Copy).
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct AssetHandle<T> {
    id: u32,
    generation: u16,
    _phantom: PhantomData<T>,
}

pub enum AssetState<T> {
    Loading,
    Ready(Arc<T>),
    Failed(String),
}

pub struct AssetServer {
    registry: HashMap<PathBuf, ErasedHandle>,
    pool: Arc<ThreadPool>,  // rayon (native) or wasm_bindgen_futures (wasm)
}

impl AssetServer {
    /// Begin loading an asset. Returns immediately with a handle.
    pub fn load<T: Asset>(&mut self, path: impl AsRef<Path>) -> AssetHandle<T> { ... }
    
    /// Poll the state of a handle.
    pub fn get<T: Asset>(&self, handle: AssetHandle<T>) -> AssetState<T> { ... }
    
    /// Hot-reload: watch file for changes and trigger re-import.
    #[cfg(not(target_arch = "wasm32"))]
    pub fn watch(&mut self, handle: AssetHandle<impl Asset>) { ... }
}
```

#### Asset Pipeline Stages

```
File System ──→ Importer ──→ Processor ──→ Cache ──→ GPU Uploader
                  (gltf)      (compress,      (disk)    (wgpu)
                  (png)        generate
                  (ttf)        mips/LODs)
```

Each asset type implements:
```rust
pub trait Asset: Send + Sync + 'static {
    type ImportData: Send;
    fn import(path: &Path) -> Result<Self::ImportData>;
    fn process(data: Self::ImportData, settings: &ProcessSettings) -> Result<Self>;
}
```

---

### 5.6 Configuration & Style System

#### `ferrous.toml` (project-level config, no code changes needed)

```toml
[engine]
target_fps = 60
vsync = true
msaa = 4

[renderer]
quality = "ultra"    # ultra | high | medium | low
style = "pbr"        # pbr | cel | lowpoly | anime | custom

[renderer.pbr]
ssao = true
bloom = true
shadows = true
shadow_resolution = 2048
ibl = true

[renderer.cel]
outline_width = 1.5
palette = "assets/palettes/anime_palette.png"

[window]
title = "My Game"
width = 1920
height = 1080

[assets]
default_texture_compression = "bc7"  # bc1 | bc3 | bc7 | none
mip_generation = true
hot_reload = true   # watch for file changes (dev only)
```

#### Material Graph (visual shader nodes concept)

```rust
pub struct MaterialGraph {
    pub nodes: Vec<MaterialNode>,
    pub connections: Vec<Connection>,
}

pub enum MaterialNode {
    // Inputs
    Texture(AssetHandle<Texture>),
    Constant(Vec4),
    VertexColor,
    ScreenUv,
    Time,
    
    // Operations
    Multiply { a: NodeId, b: NodeId },
    Add      { a: NodeId, b: NodeId },
    Lerp     { a: NodeId, b: NodeId, t: NodeId },
    
    // Outputs
    PbrOutput {
        albedo:    NodeId,
        normal:    NodeId,
        metallic:  NodeId,
        roughness: NodeId,
        emissive:  NodeId,
        ao:        NodeId,
    },
    CelOutput {
        color:          NodeId,
        outline_color:  NodeId,
        ramp_texture:   NodeId,
    },
}
```

The material graph compiles to WGSL at shader compilation time.

---

### 5.7 Plugin System

Every optional engine system should be a **plugin**:

```rust
pub trait Plugin: 'static {
    fn name(&self) -> &'static str;
    
    /// Called once during App initialization.
    /// Register systems, resources, and render passes here.
    fn build(&self, app: &mut AppBuilder) { }
    
    /// Called when the app shuts down.
    fn cleanup(&self, app: &mut AppBuilder) { }
}

pub struct AppBuilder {
    world:     World,
    scheduler: Scheduler,
    renderer:  RenderGraph,
    assets:    AssetServer,
}

impl AppBuilder {
    pub fn add_plugin(mut self, plugin: impl Plugin) -> Self { ... }
    pub fn add_system<S: System>(mut self, stage: Stage, system: S) -> Self { ... }
    pub fn add_render_pass(mut self, node: RenderGraphNode) -> Self { ... }
}
```

**Example plugin composition:**

```rust
fn main() {
    App::new()
        .add_plugin(DefaultPlugins)          // core + renderer + input + time
        .add_plugin(PhysicsPlugin::default())
        .add_plugin(AudioPlugin::default())
        .add_plugin(LuaScriptingPlugin::new("scripts/"))
        .add_plugin(MyGamePlugin)
        .run();
}
```

**`DefaultPlugins` expands to:**
```rust
pub struct DefaultPlugins;
impl Plugin for DefaultPlugins {
    fn build(&self, app: &mut AppBuilder) {
        app.add_plugin(CorePlugin)
           .add_plugin(WindowPlugin::default())
           .add_plugin(InputPlugin)
           .add_plugin(TimePlugin)
           .add_plugin(AssetPlugin)
           .add_plugin(RendererPlugin::default())
           .add_plugin(GuiPlugin);
    }
}
```

---

### 5.8 Multithreading Strategy

#### Current state
- `rayon` is used for parallel culling in `build_base_packet` (non-wasm).
- All other work is single-threaded on the main thread.

#### Proposed threading model

```
Main Thread (winit event loop)
│
├─ Update Thread Pool (rayon)
│   ├─ System execution (parallel stages)
│   ├─ Transform propagation
│   └─ Visibility / frustum cull
│
├─ Render Thread
│   ├─ draw command building from ECS queries
│   ├─ GPU buffer uploads (write_buffer via wgpu queue)
│   └─ wgpu command encoding
│
└─ Asset IO Thread Pool
    ├─ file reads / decompression
    ├─ mesh processing (tangent generation, LOD baking)
    └─ texture upload preparation
```

**Key design rules:**
- `World` queries are read-only during the render thread phase. Mutations only happen during the update phase (using a command buffer / `Commands` pattern).
- `Arc<Device>` and `Arc<Queue>` are already `Send+Sync` via `ferrous_core::EngineContext`. ✅
- `FramePacket` is assembled on the render thread and passed to GPU encode.
- Double-buffering (2 frames in flight) allows the CPU to start building frame N+1 while the GPU executes frame N.

---

## 6. Performance Targets & Optimizations

### Render performance goals vs. Bevy

| Metric | Current (estimated) | Target | Technique |
|--------|--------------------|----|-----------|
| Draw calls/frame (1000 objects) | ~1000 (legacy) / ~10 (instanced) | < 20 | Full instancing + draw indirect |
| CPU frame time (1000 objects) | ~3 ms | < 0.5 ms | Parallel cull + indirect draw |
| Material bind changes/frame | O(N objects) | O(N unique materials) | Sort by material, bindless textures |
| Shadow pass | Full re-render | Cached + incremental | Mark dirty only on transform change |
| SSAO | Full-res every frame | Half-res + temporal | TAA-style temporal reprojection |

### Key optimizations to implement

1. **GPU-Driven Rendering (Draw Indirect)**
   ```wgsl
   // Indirect draw commands built on GPU via compute shader
   // CPU sets entity transforms; GPU builds DrawIndexedIndirect commands
   // Zero CPU readback → GPU-native culling
   ```

2. **Bindless Textures** (where supported)
   - Pack all material textures into a texture array or descriptor heap.
   - PBR shader indexes by `material_id` instead of binding per-material.
   - Reduces draw call count by eliminating bind group switches.

3. **Parallel Transform Propagation**
   ```rust
   // rayon parallel_iter over archetypes containing (Transform, Children, GlobalTransform)
   // Process roots first, then leaves — topological sort once at structure change
   ```

4. **View-Layer Occlusion Culling (HZB)**
   - Build a Hierarchical Z-Buffer mip chain from the depth prepass.
   - Compute shader tests AABBs against HZB — removes fully occluded objects.

5. **Level of Detail (LOD)**
   ```rust
   pub struct LodComponent {
       pub distances: [f32; 4],     // screen-space coverage thresholds
       pub meshes: [AssetHandle<Mesh>; 4],
   }
   // LOD selection system: replaces MeshHandle each frame based on camera distance
   ```

6. **Mesh Streaming / Virtual Geometry** (future)
   - For very large meshes: nanite-like cluster-based rendering.
   - Not in immediate roadmap but the asset pipeline should be designed to support it.

---

## 7. Implementation Roadmap (Step-by-Step Tasks)

> Each phase is independent and buildable. Phases do not break the existing `ferrous_editor` binary.

---

### Phase 1 — ECS Foundation (Week 1–2)

**Goal:** Standalone `ferrous_ecs` crate with archetype storage and basic queries.

- [x] **1.1** Create `crates/ferrous_ecs/` with `Cargo.toml`.
- [x] **1.2** Implement `Entity` struct (index + generation).
- [x] **1.3** Implement `EntityAllocator` (free list + generation bumping).
- [x] **1.4** Implement `ComponentStorage` trait (typed dense column `Vec<T>`).
- [x] **1.5** Implement `ArchetypeStorage` — map from `ComponentSet` → `Archetype`.
- [x] **1.6** Implement `World::spawn<B: Bundle>(bundle) -> Entity`.
- [x] **1.7** Implement `World::despawn(entity)` with archetype migration.
- [x] **1.8** Implement `World::insert<C: Component>(entity, component)`.
- [x] **1.9** Implement basic `Query<Q: WorldQuery>` with lifetime-safe iterator.
- [x] **1.10** Implement `World::insert_resource` / `World::resource`.
- [x] **1.11** Add unit tests: spawn/despawn/query round-trip, archetype migration. *(21 unit + 5 doc-tests — all pass)*
- [ ] **1.12** Add `#[derive(Component)]` proc macro (or simple blanket impl). *(blanket impl done; proc macro optional)*

---

### Phase 2 — ECS Integration into Core (Week 2–3)

**Goal:** Replace `ferrous_core::scene::World` with a bridge that wraps `ferrous_ecs::World`.

- [ ] **2.1** Define standard built-in components: `Transform`, `Visibility`, `Name`, `Tags`, `Aabb`.
- [ ] **2.2** Keep the existing `Handle(u64)` type but make it wrap `Entity`.
- [ ] **2.3** Implement `EntityBuilder` pattern on top of ECS spawn.
- [ ] **2.4** Keep all existing `World::spawn_cube`, `spawn_quad`, `spawn_sphere` convenience methods.
- [ ] **2.5** Add `World::query_mut` to allow systems to mutate components in-place.
- [ ] **2.6** Keep backward-compatible `World::iter()` → maps to `query::<(Entity, &Element_Compat)>()`.
- [ ] **2.7** Update `ferrous_core/Cargo.toml` to depend on `ferrous_ecs`.
- [ ] **2.8** Verify `ferrous_editor` still compiles and runs with zero API changes.

---

### Phase 3 — Renderer Decomposition (Week 3–5)

**Goal:** Split `lib.rs` (1,728 lines) into focused, testable modules.

- [ ] **3.1** Extract `RenderGraph` + `RenderGraphNode` to `ferrous_renderer/src/render_graph/`.
- [ ] **3.2** Extract `MaterialRegistry` to `ferrous_renderer/src/material_registry/`.
- [x] **3.3** Extract frame-cache state to `ferrous_renderer/src/frame_builder/`. *(FrameBuilder struct encapsulates all per-frame Vec caches)*
- [ ] **3.4** Extract scene sync (`sync_world`) to `ferrous_renderer/src/world_sync/`.
- [x] **3.5** Extract camera + orbit to `ferrous_renderer/src/camera_system/`. *(CameraSystem wraps Camera + OrbitState + GpuCamera)*
- [ ] **3.6** The new `Renderer` struct becomes a thin coordinator: owns `RenderGraph`, `MaterialRegistry`, `FrameBuilder`, camera state.
- [ ] **3.7** `RenderGraph::compile()` performs topological sort + validation.
- [ ] **3.8** Make all built-in passes (`PrePass`, `SsaoPass`, etc.) individually constructable and testable without the full `Renderer`.
- [ ] **3.9** Add `Renderer::new_minimal()` for headless/test scenarios.

---

### Phase 4 — Plugin System & AppBuilder (Week 5–6)

**Goal:** Replace the fixed `App::new(game).run()` pattern with a composable plugin model.

- [ ] **4.1** Define `Plugin` trait in `ferrous_app`.
- [ ] **4.2** Define `AppBuilder` with `add_plugin`, `add_system`, `add_render_pass`.
- [ ] **4.3** Create `CorePlugin`, `WindowPlugin`, `InputPlugin`, `TimePlugin`.
- [ ] **4.4** Create `RendererPlugin` — wraps renderer initialization.
- [ ] **4.5** Create `DefaultPlugins` convenience bundle.
- [ ] **4.6** Update `App::new` to return `AppBuilder`.
- [ ] **4.7** Keep `FerrousApp` trait working as a plugin wrapper (backward compat).
- [ ] **4.8** Move `runner.rs` logic into the `WindowPlugin` system.

---

### Phase 5 — Configuration System (Week 6–7)

**Goal:** Engine configurable from `ferrous.toml` without code changes.

- [ ] **5.1** Define `EngineConfig` struct covering: window, renderer quality, style, asset options.
- [ ] **5.2** Implement TOML parser for `EngineConfig` (using `toml` crate).
- [ ] **5.3** `AppBuilder::with_config_file("ferrous.toml")` loads and applies config.
- [ ] **5.4** Define `RenderQuality` enum (`Ultra`, `High`, `Medium`, `Low`, `Minimal`).
- [ ] **5.5** Define `RenderStyle` enum (`PbrRealistic`, `CelShaded`, `LowPoly`, `Anime`, `Custom`).
- [ ] **5.6** Implement `RendererPlugin::from_quality(quality, style)` — selects which render graph nodes to include.
- [ ] **5.7** Add quality preset descriptors: what passes are enabled, what resolution factors to use.
- [ ] **5.8** Implement `RenderQuality::toggle_pass(name, enabled)` at runtime.

---

### Phase 6 — Async Asset Pipeline (Week 7–8)

**Goal:** All asset loading is non-blocking; assets are tracked by handle.

- [ ] **6.1** Define `AssetHandle<T>` (typed, Copy, 8 bytes).
- [ ] **6.2** Define `Asset` trait: `import`, `process`, `type_name`.
- [ ] **6.3** Implement `AssetServer` with rayon thread pool (native) / spawn_local (wasm).
- [ ] **6.4** Implement `AssetServer::load<T: Asset>(&str) -> AssetHandle<T>`.
- [ ] **6.5** Implement `AssetServer::get<T>(&handle) -> AssetState<T>`.
- [ ] **6.6** Implement `GltfImporter: Asset` (wraps current `load_gltf`).
- [ ] **6.7** Implement `TextureImporter: Asset` (wraps current `Texture2d`).
- [ ] **6.8** Implement `FontImporter: Asset` (wraps font module).
- [ ] **6.9** Add `AssetServer` as an ECS resource: `world.resource::<AssetServer>()`.
- [ ] **6.10** Add file watcher for hot-reload (desktop only, `notify` crate).
- [ ] **6.11** Update `spawn_gltf` to use the new async API with a loading callback.

---

### Phase 7 — Render Style Variants (Week 8–10)

**Goal:** Multiple render styles (AAA PBR, cel, low-poly, anime) selectable without code changes.

- [ ] **7.1** Write `cel.wgsl` shader — toon ramp, flat color bands, no IBL.
- [ ] **7.2** Write `outline.wgsl` — inverted hull or jump-flood outline pass.
- [ ] **7.3** Write `flat.wgsl` — flat shaded (no interpolated normals), solid colors.
- [ ] **7.4** Write `anime.wgsl` — extended cel with gradient steps + specular highlight as solid disc.
- [ ] **7.5** Create `CelShadedPass` implementing `RenderPass`.
- [ ] **7.6** Create `OutlinePass` implementing `RenderPass`.
- [ ] **7.7** Create `FlatShadedPass` implementing `RenderPass`.
- [ ] **7.8** Implement `RenderStyle::apply(graph: &mut RenderGraph)` — swaps passes.
- [ ] **7.9** Add `MaterialDescriptor::style_override: Option<RenderStyle>` — per-material style override.
- [ ] **7.10** Test all styles with the ferrous_editor demo scene.

---

### Phase 8 — ECS Render System (Week 10–11)

**Goal:** Remove the legacy object system; the renderer is driven entirely by ECS queries.

- [ ] **8.1** Replace `legacy_objects: HashMap<u64, RenderObject>` with an ECS query over `(Transform, MeshHandle, MaterialHandle, Aabb)`.
- [ ] **8.2** Remove `world_objects: Vec<Option<RenderObject>>` — no more parallel sync array.
- [ ] **8.3** `FrameBuilder::build_packet(world)` iterates ECS directly.
- [ ] **8.4** Instancing: group by `MeshHandle` + `MaterialHandle` during packet build (sort key = mesh_id * 2^16 + material_id).
- [ ] **8.5** Remove all `render_handle: Option<usize>` fields from entities.
- [ ] **8.6** Shadow pass: query over `(Transform, MeshHandle, CastsShadow)`.
- [ ] **8.7** Light pass: query over `(Transform, PointLight)` and `(Transform, DirectionalLight)`.
- [ ] **8.8** Verify instancing still works for 10,000+ entities (benchmark).

---

### Phase 9 — Editor Refactor (Week 11–13)

**Goal:** Break `app.rs` (907 lines) into focused editor panels and systems.

- [ ] **9.1** Create `ferrous_editor/src/panels/hierarchy.rs` — entity tree view.
- [ ] **9.2** Create `ferrous_editor/src/panels/inspector.rs` — component property editor.
- [ ] **9.3** Create `ferrous_editor/src/panels/material_inspector.rs` — move from current `ui/`.
- [ ] **9.4** Create `ferrous_editor/src/panels/global_light.rs` — move from current `ui/`.
- [ ] **9.5** Create `ferrous_editor/src/panels/viewport.rs` — 3D viewport + gizmo handling.
- [ ] **9.6** Create `ferrous_editor/src/panels/performance.rs` — FPS, draw calls, GPU stats.
- [ ] **9.7** Create `ferrous_editor/src/systems/selection.rs` — selection state as ECS resource.
- [ ] **9.8** Create `ferrous_editor/src/systems/gizmos.rs` — gizmo interaction as ECS system.
- [ ] **9.9** Implement `CommandHistory` (undo/redo) with `EditorCommand` trait.
- [ ] **9.10** Implement `SceneSerializer` — save/load `.ferrous_scene` files (RON format).
- [ ] **9.11** `app.rs` becomes a thin orchestrator calling each panel.
- [ ] **9.12** Add `EditorPlugin` — registers all editor systems.

---

### Phase 10 — GPU Driven Rendering (Week 13–15)

**Goal:** Draw indirect + GPU culling for > 100k objects.

- [ ] **10.1** Implement `DrawIndirectBuffer` — stores `DrawIndexedIndirect` commands.
- [ ] **10.2** Write `cull.wgsl` compute shader — frustum cull per-instance, writes visible indices.
- [ ] **10.3** Implement `CullPass: RenderPass` — dispatches cull compute, fills indirect buffer.
- [ ] **10.4** Update `WorldPass` to use `draw_indexed_indirect` from the filled buffer.
- [ ] **10.5** Benchmark: compare CPU culling vs GPU culling at 10k, 50k, 100k entities.
- [ ] **10.6** Implement `OcclusionCullPass` (HZB) — optional, toggleable via config.
- [ ] **10.7** Implement `LodSystem` — selects mesh LOD per entity based on screen size.

---

### Phase 11 — GUI Theming & Optimization (Week 15–16)

**Goal:** Theme system + layout caching.

- [ ] **11.1** Define `Theme` struct: palette, font sizes, corner radii, spacing constants.
- [ ] **11.2** Implement `Theme::apply(widget: &mut dyn Widget)` visitor.
- [ ] **11.3** Add `Theme` as a resource: `world.resource::<Theme>()`.
- [ ] **11.4** Implement layout caching — only re-solve layout on dirty nodes.
- [ ] **11.5** Replace `Rc<RefCell<Widget>>` with an `Arena<Widget>` with typed handles.
- [ ] **11.6** Add z-order / layering (multiple draw layers instead of one flat pass).
- [ ] **11.7** Implement `GuiAnimation` — interpolated property transitions.

---

## 8. API Preview (Post-Refactor)

### Game code (minimal boilerplate, ECS native)

```rust
use ferrous_app::prelude::*;

fn main() {
    App::new()
        .with_config_file("ferrous.toml")   // all settings from file
        .add_plugin(DefaultPlugins)
        .add_plugin(MyGamePlugin)
        .run();
}

struct MyGamePlugin;
impl Plugin for MyGamePlugin {
    fn build(&self, app: &mut AppBuilder) {
        app.add_system(Stage::Setup, setup_scene)
           .add_system(Stage::Update, player_movement)
           .add_system(Stage::Update, camera_follow);
    }
}

fn setup_scene(world: &mut World, assets: &mut AssetServer) {
    // Load asynchronously — returns immediately
    let model = assets.load::<GltfModel>("assets/models/player.glb");
    
    world.spawn((
        Name::new("Player"),
        Transform::from_position(Vec3::ZERO),
        MeshHandle(model),
        MaterialHandle::pbr(Color::RED, 0.0, 0.5),
        PlayerComponent { speed: 5.0 },
    ));
}

fn player_movement(
    mut query: Query<(&mut Transform, &PlayerComponent)>,
    input: Res<InputState>,
    time: Res<Time>,
) {
    for (mut transform, player) in query.iter_mut() {
        if input.is_key_down(KeyCode::KeyW) {
            transform.position.z -= player.speed * time.delta;
        }
    }
}
```

### Render style selection (zero code — just `ferrous.toml`)

```toml
# To switch from AAA PBR to cel-shaded anime style:
[renderer]
style = "anime"

[renderer.anime]
outline_width = 2.0
gradient_steps = 4
```

### Adding a custom render pass (plugin)

```rust
pub struct PixelizePlugin { pub pixel_size: u32 }
impl Plugin for PixelizePlugin {
    fn build(&self, app: &mut AppBuilder) {
        app.add_render_pass(RenderGraphNode {
            name: "PixelizePass",
            reads:  vec![ResourceId::HDR_COLOR],
            writes: vec![ResourceId::HDR_COLOR],
            pass:   Box::new(PixelizePass::new(self.pixel_size)),
        });
    }
}
```

---

## 9. Migration Guide (Current → New)

### Existing game code

| Old pattern | New pattern | Breaking? |
|-------------|-------------|-----------|
| `ctx.world.spawn_cube("x", pos)` | unchanged | ✅ No |
| `ctx.world.spawn("x").with_kind(...).build()` | unchanged | ✅ No |
| `ctx.renderer.create_material(&desc)` | `ctx.materials.create(&desc)` | ⚠️ Minor |
| `ctx.renderer.update_light(...)` | `ctx.world.resource_mut::<GlobalLight>().update(...)` | ⚠️ Minor |
| `impl FerrousApp for MyGame` | still works (wrapped as a plugin) | ✅ No |
| `App::new(game).with_title("x").run()` | unchanged | ✅ No |
| `AppConfig::default()` with builder | unchanged | ✅ No |
| Direct `ctx.renderer` access | deprecated but still available during transition | ⚠️ Soft deprecation |

### Timeline estimate

| Phase | Duration | Risk |
|-------|----------|------|
| Phase 1-2 (ECS) | 2–3 weeks | Medium — new data layout |
| Phase 3 (Renderer split) | 2 weeks | Low — pure refactor |
| Phase 4-5 (Plugin + Config) | 2 weeks | Low |
| Phase 6 (Asset pipeline) | 2 weeks | Medium — async complexity |
| Phase 7 (Style variants) | 2 weeks | Low — additive |
| Phase 8 (ECS render) | 2 weeks | Medium |
| Phase 9 (Editor) | 2 weeks | Low |
| Phase 10 (GPU-driven) | 2 weeks | High — GPU compute |
| Phase 11 (GUI) | 1 week | Low |
| **Total** | **~18–20 weeks** | |

> **Recommended start:** Phase 3 (Renderer split) is the lowest-risk, highest-ROI first step. It requires zero API changes and immediately makes the codebase more navigable. Phase 1–2 (ECS) should run in parallel on a feature branch.

---

*This document reflects the state of FerrousEngine as of March 2026. All proposals are subject to review and modification before implementation begins.*
