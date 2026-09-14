# FerrousEngine 2D Audit — Shared Context (read this FIRST, in full)

You are one of 4 independent subagents auditing **FerrousEngine**. This file is the
common context every subagent receives. It is identical for A, B, C and D. Your
specific mandate is in your task prompt (and only yours — do not do another agent's job).

= Filesystem / repository rules (added by the orchestrator; OBEY STRICTLY) =
- There is ONE real repository and ONE working directory. All work happens inside it:
  `/opt/softwaremaker/workspaces/96a0b3c0-be13-4354-8674-72220ab22596/`.
  Do NOT use `/tmp`, do NOT create git worktrees, do NOT copy the repo. Changes must land in
  the real FerrousEngine checkout so they actually affect Ferrous.
- The orchestrator runs subagents **one at a time** in that same working directory, so before
  you start: `cd /opt/softwaremaker/workspaces/96a0b3c0-be13-4354-8674-72220ab22596`, make sure
  `git status` is clean, then `git checkout <YOUR BRANCH>` (your task prompt says which).
  Every feature branch is cut from `analysis/2d-audit-base` (which already contains Subagent D's
  headless harness). Stay on YOUR branch. Never checkout another agent's branch.
- Commit in the real repo. `brain/` IS gitignored, so to commit your report you MUST use
  `git add -f brain/2d_audit_subagent_<X>.md`. Only add the files you actually changed.
- This VPS has 4 CPUs and ~4 GB free RAM and ~28 GB free disk. Reuse the already-populated
  target dir: export
  `CARGO_TARGET_DIR=/opt/softwaremaker/workspaces/96a0b3c0-be13-4354-8674-72220ab22596/target`
  for every cargo command. If a command prints "Blocking waiting for file lock", that is
  expected — just let it wait.

= Headless GPU environment (verified by orchestrator) =
- Rust 1.94.0, cargo 1.94.0 available.
- Software Vulkan (lavapipe) is installed. ICD file: `/usr/share/vulkan/icd.d/lvp_icd.json`.
- `mesa-vulkan-drivers` 25.2.8 and `libgl1-mesa-dri` are installed.
- Recommended env for headless off-screen rendering:
  - `VK_ICD_FILENAMES=/usr/share/vulkan/icd.d/lvp_icd.json`
  - `VK_DRIVER_FILES=/usr/share/vulkan/icd.d/lvp_icd.json`
  - `WGPU_BACKEND=vulkan`
  - Request the adapter with `force_fallback_adapter: true` if the normal request fails.
- No X/Wayland display. Do NOT create a `Surface`/window. Render to a `wgpu::Texture`.

---

## 1. Contexto verificado

Estás auditando **FerrousEngine**, un motor Rust multi-crate (workspace, ~22 miembros, `resolver = "2"`), usado hoy por dos consumidores reales vía path-deps: **Koncreta** (`packages/core_viewer`, Rust→WASM) y **GUIMaker** (builder visual desktop). El motor apunta a 2D ahora y 3D a futuro (ver `FERROUS.md`, `ADR-001-unified-2d-3d-renderer.md`, `3D_BRIDGE.md`) — cualquier decisión de diseño en 2D debe evitar cerrarle la puerta a esa unificación futura, pero **no** se implementa 3D en esta auditoría.

**Backend gráfico:** wgpu 23.0.1 únicamente (`Cargo.toml:25`). No hay vulkano/gfx-hal/ash directo; ash/glow/metal solo aparecen como backends internos de wgpu-hal. Wrapper central: `crates/ferrous_gpu/src/context.rs` (`EngineContext`). Features: `webgpu`, `webgl`, `fragile-send-sync-non-atomic-wasm`.

**ECS:** propio (archetype/SoA), no bevy/hecs/specs. `ferrous_ecs` = storage + queries + scheduler genérico. `ferrous_core` = capa de escena sobre el ECS, y **todavía mantiene un `Vec<Option<Element>>` legacy espejado al ECS** (`scene/world/scene.rs:22-31`) — foco de deuda técnica. Scheduler por stages `PreUpdate → Update → PostUpdate → Render` (`system.rs:148-169`), **secuencial**; existe un `ParallelScheduler` pero está feature-gated y **no compila hoy**.

**Pipeline 2D actual (dos caminos que conviven):**
- **Pure2D** (`renderer_core.rs:1102-1221`): un solo pass, sin depth, z-sort en CPU, `DrawCommand2d` unificado para shapes+sprites.
- **Overlay Full3D** (`renderer_core.rs:1432-1478`): solo SDF shapes, con cámara ortográfica dedicada, separado del anterior.
- **Batchers:** `ShapeBatcher` (Vec plano, 1 draw call total) y `SpriteBatcher` (`BTreeMap<tex_id, Vec>`, 1 draw call por textura). Instancing vía vertex buffer (`step_mode: Instance`), quad generado desde `vertex_index`.
- **Sin atlas de texturas:** una textura = un bind group = un draw call por `texture_id`. `uv_rect` soporta spritesheets pero no hay packer.
- **Z-order:** `Transform2d.z_index → transform_c3[2] →` sort en CPU.
- **Dirty flags:** `ferrous_2d` no tiene ninguno — re-sube buffers completos cada frame y recrea buffers/bind groups cuando crecen. El único dirty tracking real está en `FrameBuilder::mark_dirty` (rama 3D) y `update_texture_data` de materiales.
- **AA:** SDF en `shape_2d.wgsl` (+ MSAA opcional en overlay, + FXAA/SMAA post en `antialiasing_pass.rs`); los sprites dependen solo de blending, sin AA propio.
**Threading:** single-threaded, todo en el hilo del event loop de winit. No hay render thread ni job system. `rayon` solo se usa para import de assets en background (`ferrous_assets/src/server.rs:371`). Async solo para APIs internas de wgpu (`pollster` nativo / `wasm-bindgen-futures` en web).

**Plataformas:** nativo + `wasm32-unknown-unknown`. Cero `cfg(target_os)`, solo `cfg(target_arch="wasm32")`. Sin mobile. Build web con `scripts/build_ferrous_web.sh` (wasm-pack).

**Problemas ya identificados y sin resolver** (no los re-descubras, verificalos y profundizá):
- **#9** — cap/join de lyon hardcodeados (referenciado en `brain/2d_renderer_issues.md`).
- **#10** — doble pipeline de Path2D no coordinado entre Pure2D y Overlay.
- **#12** — inconsistencia de color-space: swapchain no-sRGB vs. texturas sRGB (`graphics.rs:144-150` vs `renderer_core.rs:1744`).
- **Código muerto:** `SpritePass` y `RendererPasses` están exportados pero nunca instanciados — están confundiendo el análisis, hay que confirmarlo y proponer removerlos o documentarlos.
- **Cero tests** en `ferrous_2d`, `ferrous_gui`, `ferrous_ui_*`. Cero integration tests de render. Cero benchmarks (ni `benches/`, ni criterion/divan, ni tracy/puffin/flamegraph). Único número de referencia: `ferrous.toml:12` (`target_fps = 60`, aspiracional, no medido).
- `mkdocs.yml` está roto: apunta a `ferrous_app/*` y `ferrous_gui/widgets/*` que ya no existen.

**Cómo usan el motor los dos consumidores reales** (esto define qué NO se puede romper sin avisar):

*Koncreta / `core_viewer`* usa: `FerrousApp` (`setup`/`update`/`draw_ui`), `AppContext` (input, time, window, world, render), `enable_pure_2d`, `clear_2d`, cámara (`set_ortho_size`, `set_camera_eye`, `camera_mut().target`), `draw_2d_shape(ShapeInstance)` (rect / rect_outline / rect_rotated / circle / line / line_with_cap), `ferrous_assets::Texture2d::from_svg_bytes`, UI inmediata `DrawContext` + `gui_maker::Gui`, tipos de `ferrous_ui_core`. **No usa** físicas, gizmos del motor, meshes 3D/PBR ni animación/materiales — su escena BIM es un ECS propio (`src/scene/mod.rs`), no `ferrous_ecs`. Features críticas: fill de polígonos arbitrarios (ear-clipping → `ShapeInstance::rect`), líneas con cap flat para uniones CAD, cámara ortográfica por viewport, precisión de input. Hay un **mismatch de versión de `glam`** entre Koncreta (0.32.1) y el motor (0.29) — anotalo si un hallazgo lo toca, no es tarea de esta auditoría arreglarlo salvo que sea trivial.

*GUIMaker* depende de `ferrous_engine`, `ferrous_gui`, `ferrous_ui_core`, `ferrous_layout`, `ferrous_assets`, `ferrous_events`, `ferrous_reflection` (estos 3 últimos declarados pero sin uso real hoy). No usa hot-reload, gizmos, undo/redo ni modo editor/runtime (no existen en el motor). **Ya está desincronizado**: usa `AppMode::Desktop`/`Desktop2D`, que ya no existen (hoy es `AppMode::{Game3D, Flat2D}`), y su codegen plugin referencia `ferrous_app` (renombrado a `ferrous_engine`). Esto es deuda de compatibilidad **ya existente**, no algo que la auditoría vaya a causar — documentala si se toca, pero arreglar GUIMaker no es el alcance.

**Dato duro de compatibilidad:** Koncreta y GUIMaker son path-deps del mismo checkout del motor. Un cambio breaking en el motor rompe a los dos **inmediatamente**, sin versión intermedia de por medio.

---

## 2. Entorno de ejecución: VPS sin entorno gráfico

Los subagentes corren en una VPS **sin entorno gráfico** (sin servidor X/Wayland) y con hardware modesto. Esto condiciona cómo se valida cualquier cosa relacionada a rendering:

- **No asumir una ventana real de winit ni un swapchain.** Cualquier test o benchmark que necesite renderizar tiene que hacerlo **off-screen**: crear un `wgpu::Texture` como render target, renderizar ahí, y (si hace falta verificar píxeles) leer de vuelta con `copy_texture_to_buffer`. Nada de `Surface`/`SurfaceTexture`.
- **Verificar qué backend de wgpu está disponible antes de asumir nada.** Sin GPU dedicada, probablemente haga falta forzar un adaptador de software: Vulkan vía **lavapipe** (Mesa, `VK_ICD_FILENAMES` apuntando al ICD de lavapipe) o el backend GL vía **llvmpipe**. Usar `force_fallback_adapter: true` al pedir el adaptador si hace falta, y documentar en el informe qué paquetes del sistema hicieron falta instalar (ej. `mesa-vulkan-drivers`, `libgl1-mesa-dri`) para que quede reproducible.
- **Los números de rendimiento en esta máquina son solo relativos, no absolutos.** No afirmar "llegamos a 60 fps" ni nada parecido — con render por software y hardware modesto ese número no significa nada fuera de esta VPS. Medir y reportar en términos **hardware-independientes**: cantidad de draw calls, cantidad de allocations/frame, bytes subidos a GPU por frame, cantidad de bind groups creados. El tiempo de CPU/frame (ms) sirve como delta relativo **antes/después en la misma máquina**, no como número absoluto a citar.
- Si un hallazgo requiere sí o sí una GPU real o un entorno gráfico para confirmarse, decirlo explícitamente en el informe como "no verificable en este entorno" en vez de asumir o inventar el resultado.

---

## 3. Reglas de commits y trazabilidad

- **Commits atómicos y frecuentes.** Nunca un commit gigante — si algo sale mal, tiene que poder revertirse (`git revert`) sin perder todo el trabajo del subagente. Un commit = un cambio lógico coherente.
- **Conventional Commits, en inglés, minúscula**, siguiendo el estilo ya usado en el repo: `type(scope): resumen`, cuerpo con bullets. Tipos predominantes hoy: `feat:`, `refactor:`, `fix:`. Sumar `perf:`, `test:` y `docs:` donde corresponda — hoy casi no se usan y son los que más se van a necesitar en esta auditoría.
- **El cuerpo del commit siempre explica el "por qué", no solo el "qué".** Causa raíz del problema, qué se cambió, y — esto es obligatorio para cualquier cambio que toque la superficie de API del Apéndice A — **qué tiene que actualizarse en Koncreta y/o GUIMaker** como consecuencia.
- **Todo cambio breaking de API pública necesita, en el mismo commit:** (a) el motivo en el cuerpo del commit, (b) una nota de migración explícita (qué cambió, cómo migrar el código que lo llama). No existe `CHANGELOG.md` en el repo hoy — no inventarlo; la nota va en el cuerpo del commit y, si el cambio afecta a Koncreta, adicionalmente en su carpeta `book/`.
- **Cada subagente trabaja en su propia rama:** `analysis/2d-<área>` (ej. `analysis/2d-pipeline-gpu`). El diagnóstico puro de la Fase 1 (sin tocar código) puede ir directo como commit de docs a esa rama; no se mergea a `main` hasta que el orquestador consolide.
- Ningún subagente toca código de las otras 3 áreas. Si encuentra algo que es claramente de otra área, lo anota en su informe como "hallazgo cruzado para Subagente X" en vez de arreglarlo.

---

## 4. Formato obligatorio del informe

Archivo: `brain/2d_audit_subagent_<A|B|C|D>.md` (dentro de TU worktree/branch). Estructura fija:

```markdown
# Auditoría 2D — Subagente <X>: <área>

## Resumen ejecutivo
(3-5 líneas: qué tan sano está esto, los 2-3 problemas más graves)

## Hallazgos

### [SEVERIDAD] Título del hallazgo
- **Ubicación:** archivo:línea
- **Descripción:** qué pasa, con evidencia concreta (código, no intuición)
- **Causa raíz:** por qué pasa
- **Impacto:** medido (si el harness de D ya existe) o estimado, en términos hardware-independientes
- **Riesgo Koncreta/GUIMaker:** ¿toca la superficie de API del Apéndice A? ¿rompe algo?
- **Riesgo escalabilidad 3D:** ¿esta solución le cierra puertas a ADR-001?
- **Fix propuesto:** concreto, con esfuerzo estimado (S/M/L)
- **Prioridad sugerida:** 1-5

(repetir por cada hallazgo, ordenados de mayor a menor severidad)

## Hallazgos cruzados para otros subagentes
(lo que se vio pero no le toca arreglar a este subagente)
```

**Escala de severidad:** `CRÍTICO` (rompe Koncreta/GUIMaker hoy, o bloquea 3D futuro) · `ALTO` (impacto de performance medible o deuda que crece rápido) · `MEDIO` (DX/mantenibilidad, sin impacto funcional inmediato) · `BAJO` (cosmético, nice-to-have).

---

## Apéndice A — Superficie de API que NO se puede romper sin nota de migración

**Usada por Koncreta (`core_viewer`):**
`FerrousApp` (`setup`, `update`, `draw_ui`) · `AppContext` (input, time, window, world, render) · `enable_pure_2d` · `clear_2d` · cámara: `set_ortho_size`, `set_camera_eye`, `camera_mut().target` · `draw_2d_shape(ShapeInstance)` con variantes rect / rect_outline / rect_rotated / circle / line / line_with_cap · `ferrous_assets::Texture2d::from_svg_bytes` · `DrawContext` + `gui_maker::Gui` (UI inmediata) · tipos de `ferrous_ui_core`.

**Usada por GUIMaker:**
`ferrous_engine`, `ferrous_gui`, `ferrous_ui_core`, `ferrous_layout`, `ferrous_assets` (nota: ya está desincronizado con el `AppMode` actual — no arreglarlo salvo que se pida explícitamente).

Cualquier commit que toque algo de esta lista lleva, obligatoriamente, nota de migración en el cuerpo del commit (Sección 3).

---

## Apéndice B — Decisiones tomadas por default (revisables)

| Punto abierto | Decisión por default |
|---|---|
| Cobertura del análisis | Los 4 subagentes cubren todo su crate de área, no solo los issues #9/#10/#12 |
| Formato de entrega | Diagnóstico puro en Fase 1 (informe en `brain/`), implementación separada en Fase 3 |
| División de los 4 | A=GPU/pipeline, B=CPU/ECS/threading, C=Memoria/assets, D=API/DX/tests/harness |
| Ramas | Una rama por subagente (`analysis/2d-<área>`), merge a `main` después de consolidar |
| Migration notes | Cuerpo del commit + `book/` de Koncreta si aplica (no existe CHANGELOG.md) |
| Métrica de "perfecto" | Draw calls/frame, allocations/frame, bytes subidos a GPU, ms/frame como delta relativo — sin targets absolutos de FPS |
| Escalabilidad 3D | Restricción de diseño a respetar, no se implementa en esta auditoría |
| Baseline de benchmarks | La levanta el Subagente D en Fase 1, antes de que A/B/C reporten impacto medido |
