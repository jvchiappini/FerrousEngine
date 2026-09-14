# Auditoría 2D — Subagente B: CPU / ECS / Escena / Threading

> Rama: `analysis/2d-cpu-ecs`. Fase 1 (diagnóstico). El entorno es lavapipe/llvmpipe
> (software, ver Shared Context §2): los `cpu_ms` solo valen como delta relativo en
> la misma máquina; las métricas que uso son **hardware-independientes** (bytes
> subidos/frame, nº de instancias, allocs, comparaciones de sort, nº de copias de
> `Element`). Las cifras de bytes salen de los contadores ya instrumentados por
> Subagente D y del benchmark `benches/pure2d_headless.rs` (medidos, no estimados).

## Resumen ejecutivo

El ECS en sí está sano (archetype/SoA real, `Query` sin allocs por iteración,
generación de entidades correcta), pero **la capa de escena no usa el ECS como
fuente de verdad**: `ferrous_core::scene::World` mantiene un `Vec<Option<Element>>`
legacy y un espejo ECS **parcial**, con un bug latente de datos obsoletos
(mutadores como `translate`/`set_rotation`/`set_scale` no escriben al ECS que el
renderer sí lee). El único dirty tracking real, `FrameBuilder::mark_dirty`, **no
ahorra trabajo** porque `sync_world` vuelve a marcar `scene_dirty=true` cada frame.
Y `ferrous_2d` no tiene dirty flags: re-sube el buffer completo y ordena
`shapes+sprites` con O(n log n) cada frame, lo que empieza a doler a partir de
~50k instancias. No hay render thread ni job system (todo en el hilo de winit).
Ningún hallazgo rompe a Koncreta hoy, pero el espejo doble bloquea la unificación
2D/3D de ADR-001.

## Metodología / evidencia cruda

- `cargo test -p ferrous_renderer --test pure2d_headless` → 3/3 OK (llvmpipe).
- `cargo bench -p ferrous_renderer --bench pure2d_headless` (líneas `[baseline]`):
  - shapes: `n=4096 → instances=4096, bytes_uploaded=393 216, buffer_reallocations=1`.
  - sprites: `n=2048, t=64 → instances=2048, bytes_uploaded=229 376, buffer_reallocations=1`.
  - derivado: `ShapeInstance = 393 216/4096 = 96 B`; `SpriteInstance = 229 376/2048 = 112 B`.
  - allocs/frame casi constante `≈56–57` (dominado por wgpu); el motor añade solo
    1 `Vec` de `draw_list` + 1 `Vec` `flat` por frame, pero de tamaño O(n).
- Diagnóstico del scheduler en paralelo (comandos ejecutados, ver hallazgo MEDIO del `ParallelScheduler`):
  - `cargo check -p ferrous_ecs --features parallel` → **OK** (nativo).
  - `cargo build -p ferrous_ecs --features parallel --target wasm32-unknown-unknown` → **OK**.
  - `cargo test -p ferrous_ecs [--features parallel] --no-run` → **FALLA** con E0428/E0277
    en `component.rs`, **con y sin** el feature (mismos 3 errores).
- Verificación de call sites: `SpritePass::new`/`RendererPasses::new` → 0 usos;
  `prepare_*_system` → 1 solo call site (`SpritePass`, muerto); `Transform2d` → 0
  usos fuera de `ferrous_2d`; `sync_ecs_to_shape_batcher` → solo 2 call sites internos.

---

## Hallazgos

### [CRÍTICO] El `Vec<Option<Element>>` legacy está espejado **parcialmente** al ECS: fuente de verdad doble y mutaciones que el renderer nunca ve
- **Ubicación:** `crates/ferrous_core/src/scene/world/scene.rs:22-31` (struct),
  `:302-314` (`set_position`, el único que sincroniza), `:333-401`
  (`translate`/`set_rotation`/`rotate_*`/`set_scale*`/`set_cube_half_extents`),
  `crates/ferrous_core/src/scene/world/query.rs:18-65` (material/visible/color/tags)
  y `:98-120` (`get_mut`/`iter_mut` devuelven `&mut Element` legacy),
  `crates/ferrous_core/src/scene/world/builder.rs:99-115` (doble almacenamiento al
  hacer `spawn`).
- **Descripción:** `set_position` escribe en 3 sitios (Vec legacy + ECS `Transform`
  + ECS `Element`, `scene.rs:305-311`), pero **`translate` (`:334`), `set_rotation`
  (`:343`), `rotate_around*` (`:350-367`), `set_scale*` (`:377-388`) y
  `set_cube_half_extents` (`:392-400`) mutan solo el Vec legacy**. El renderer lee
  el ECS: `FrameBuilder::build_world_commands` consulta
  `Query<(&Element, &Transform, &MaterialComponent, …)>` (`frame_builder.rs:186-194`).
  Es decir, tras `world.translate(h, v)` o `world.set_scale(h, s)` el renderer
  sigue viendo el transform viejo. Además `get_mut`/`iter_mut` exponen `&mut Element`
  legacy, que nunca se refleja. Al hacer `spawn` cada entidad se guarda 2 veces
  (`self.element.clone()` en el bundle ECS + `self.element` en el Vec, builder.rs:99/113)
  y `MaterialComponent` queda almacenado 3 veces (columna tipada + dentro del
  `Element` ECS + dentro del `Element` legacy).
- **Causa raíz:** se introdujo el ECS como "capa de consulta" sin retirar el Vec
  legacy, y no se decidió cuál es autoritativo. Solo se actualizó el espejo en los
  mutadores que se fueron tocando, dejando el resto desincronizados.
- **Impacto:** correctitud, no tiempo. `Element` es pesado (String `name`,
  `Vec<String>` tags, `ElementKind` con `String`/`Vec<u8>`): duplicarlo por entidad
  duplica el heap y los caminos de alloc/free. `sync_world` recorre el Vec legacy 3
  veces por frame y aloca un `HashSet` de ids (`renderer_passes.rs:654/690/703`) —
  aunque `sync_world` no se llama en Pure2D (`runner/frame.rs:241`).
- **Riesgo Koncreta/GUIMaker:** Koncreta usa del `World` del motor **solo**
  `clear()` (`lib.rs:300`), `iter_with_handles()` y `despawn()` (`lib.rs:330-336`);
  **no spawnea ni muta transforms en el World del motor** (su escena BIM es su propio
  `crate::scene`, y dibuja 2D por modo inmediato). Por eso el bug no es visible hoy,
  pero `iter_with_handles`/`despawn`/`clear` son la superficie que NO se puede
  romper sin nota de migración. `get`/`get_mut`/`iter`/`iter_mut`/`Handle` son API
  pública potencialmente usada por GUIMaker.
- **Riesgo escalabilidad 3D:** **alto**: ADR-001 asume ECS unificado; mantener dos
  estructuras sincronizadas a mano garantiza divergencia a medida que crezca la
  superficie de componentes.
- **Fix propuesto:** declarar el ECS como autoritativo y convertir el Vec legacy en
  un `Handle → Entity` (o eliminar `ecs_mapping` guardando el índice legacy en el
  propio `Element`), portando cada mutador a `world.ecs.get_mut::<…>()`. Mientras no
  se haga, al menos sincronizar `translate/rotate/scale` al ECS (S) y documentar que
  `get_mut` desincroniza. La eliminación completa es **L** (hay que reimplementar la
  iteración por handle sobre el ECS) y **no** es segura en Fase 1. **Esfuerzo: S**
  (parche de sincronización) / **L** (unificación). **Prioridad 1.**
- **Prioridad sugerida:** 1

### [ALTO] `FrameBuilder::mark_dirty` no evita el trabajo real: `sync_world` re-marca dirty cada frame y reconstruye todo
- **Ubicación:** `crates/ferrous_renderer/src/frame_builder.rs:127-131` (`mark_dirty`)
  y `:616-664` (fast path `!scene_dirty`), `crates/ferrous_renderer/src/renderer_core.rs:995-1019`
  (`sync_world` llama `build_world_commands` **incondicionalmente** y luego pone
  `scene_dirty = true`), `crates/ferrous_engine/src/runner/frame.rs:241-243`
  (`sync_world` se llama cada frame en Game3D/Full3D).
- **Descripción:** el fast path de `build()` exige `!self.scene_dirty`
  (`frame_builder.rs:622`), pero `sync_world` setea `scene_dirty=true` al final de
  **cada** llamada (`renderer_core.rs:1019`). Con el bucle estándar del motor esa
  rama nunca se toma: el trabajo caro (`Query` ECS, `Vec` de `entities_to_render`,
  `sort_by_key`, dos `HashMap` de grupos, `sorted_group_keys`, `instance_buf.write_slice`)
  corre cada frame aunque nada haya cambiado (`frame_builder.rs:180-205, 524-561`).
  `mark_dirty` solo ahorraría el `mem::swap` de dos `Vec` y `compute_stats`.
- **Causa raíz:** `sync_world` re-deriva todo el estado y marca dirty por
  precaución, en vez de comparar un tick de mundo/entidades o confiar en
  `mark_dirty` de los setters.
- **Impacto:** CPU O(n log n) + allocs por frame en 3D, sin importar la métrica de
  dirty. No medido en el harness (el harness cubre Pure2D, donde `sync_world` no se
  ejecuta); evidencia estructural de código.
- **Riesgo Koncreta/GUIMaker:** ninguno directo: Koncreta está en Pure2D y no llama
  `sync_world`. Si se "arregla" cambiando cuándo `sync_world` marca dirty, **puede
  cambiar el timing de `setup`/`update`/`draw_ui`** solo si se toca el orden de
  stages — ver nota CRÍTICA de proceso abajo.
- **Riesgo escalabilidad 3D:** el fast path prometido ("zero CPU work") es lo que
  necesita una escena 3D grande; hoy es letra muerta.
- **Fix propuesto:** que `sync_world` marque dirty **solo** cuando detecte cambios
  (p. ej. `world.ecs.change_tick` distinto, o comparar `RenderStats`/contadores de
  spawn/despawn), o mover la detección a `mark_dirty` de los setters como pretende
  el diseño. Para 2D: **no** portar el patrón todavía (ver hallazgo ALTO de re-subida): con el consumo
  actual en modo inmediato no hay estado persistente que ensuciar, así que sería
  infraestructura muerta. **Esfuerzo: S** (3D) / **inútil hoy** (2D).
- **Prioridad sugerida:** 2

### [ALTO] Re-subida completa cada frame en `ferrous_2d` (sin dirty tracking) y sort O(n log n) de shapes+sprites: cuantificación
- **Ubicación:** `crates/ferrous_2d/src/render/renderer.rs:174-233`
  (`prepare`/`prepare_shapes` re-suben todo; `prepare` aloca un `Vec` plano en `:196`),
  `crates/ferrous_renderer/src/renderer_core.rs:1140-1158` (construcción y
  `sort_unstable_by` de `draw_list` cada frame con todos los shapes+sprites),
  `crates/ferrous_2d/src/components/transform.rs:6-12` (sin flag de cambio).
- **Descripción:** cada frame se crea `draw_list: Vec<(f32, DrawCommand2d)>` con
  capacidad `shapes+sprites`, se rellena y se ordena por z (O(n log n)); luego se
  copian y suben **todos** los instancias (`queue.write_buffer` de `batcher.instances`
  y de un `Vec` plano recién alocado). No hay ninguna bandera de cambio en
  `Transform2d`/`Sprite`/`Shape2d`. Datos medidos: 96 B por shape, 112 B por sprite.
- **Causa raíz:** el pipeline 2D se diseñó para modo inmediato: la app decide qué
  dibujar, así que el renderer no tiene estado persistente que reutilizar.
- **Impacto (estimación con razonamiento):**
  - Subida: `n × ~100 B/frame`. A 60 fps → 1k ≈ 5.6 MB/s; 10k ≈ 58 MB/s;
    50k ≈ 288 MB/s; 100k ≈ 576 MB/s. Cada `write_buffer` copia además a un staging.
  - Sort: `n log₂ n` comparaciones con `partial_cmp` f32 y tuples de 20 B. 4096 →
    ~49k comparaciones; 100k → ~1.7M comparaciones/frame. Estimación gruesa en un
    core: ~0.2–0.5 ms a 10k, ~3–8 ms a 100k, dentro de un presupuesto de 16.6 ms.
  - **Umbral:** el coste fijo por frame (2 `write_buffer`, setup de bind groups,
    ~56 allocs wgpu) domina hasta ~10k instancias; de ~10k a ~20k la subida y el
    sort son menores; a partir de **~50k** empieza a doler y a **≥100k** se come el
    frame. Los `draw_calls` explotan antes si cada primitiva tiene z distinto (ver
    hallazgo ALTO de Subagente A: 4096 z distintos → 4096 draw calls), lo cual es la
    primera pared real de escalado.
- **Riesgo Koncreta/GUIMaker:** ninguno funcional. Es la ruta que Koncreta usa
  (Pure2D inmediato), así que un cambio aquí es sensible pero no de API.
- **Riesgo escalabilidad 3D:** la ausencia de dirty tracking no escala; ADR-001
  necesita al menos revisión de componentes.
- **Fix propuesto (DX):** no introducir dirty flags ad-hoc en el batcher. El único
  esquema que vale la pena es a nivel ECS: **revisión por componente** (un `u32`
  dentro de `Transform2d`/`Sprite` que se incrementa al mutar, o cambio de
  `World::get_mut`), una caché `última revisión subida → rango del buffer`, y subir
  solo los rangos cuyo máximo cambió. Es **M**, toca `World`+`Query`+batcher, pero es
  reutilizable por 3D (a diferencia del bool de `FrameBuilder`, que es global y hoy
  no ahorra nada). Mientras el consumo siga siendo inmediato, el retorno es ~0: no
  hacerlo en Fase 1/2. Un fix barato e inmediato sí es reutilizar un scratch `Vec`
  en `prepare()` (S). **Esfuerzo: S/M**. **Prioridad 3.**
- **Prioridad sugerida:** 3

### [ALTO] El iterador de `Query` hace un `binary_search` sobre la firma **por fila**
- **Ubicación:** `crates/ferrous_ecs/src/query.rs:111-120` (`&T::fetch`),
  `:145-157` (`&mut T::fetch`), `:305-316` (`Query::iter`, `flat_map` sobre archetipos),
  `crates/ferrous_ecs/src/archetype.rs:265-267` (`column_index` → `binary_search` sobre
  `Vec<TypeId>`, comparaciones de `u128`).
- **Descripción:** `Q::fetch` se llama por cada fila y cada elemento vuelve a
  resolver su columna con `arch.column::<T>()`, que hace `binary_search` en
  `signature.0`. Una query de 2 componentes hace 2 búsquedas por entidad por frame;
  no se cachea el puntero de columna entre filas de un mismo archetipo.
- **Causa raíz:** el `State` de la query solo guarda índices de archetipo, no
  índices/ponteros de columna; `fetch` es genérico sobre el tipo y "redescubre" la
  columna.
- **Impacto:** O(filas·k·log c) en vez de O(filas·k). A 100k entidades × 2 comps ×
  ~4 TypeIds ≈ ~800k comparaciones de `u128`/frame solo para localizar columnas,
  antes de hacer cualquier trabajo útil. No medido aislado (el harness de 2D no usa
  queries ECS), evidencia de código.
- **Riesgo Koncreta/GUIMaker:** ninguno: `ferrous_ecs::Query` no está en el
  Apéndice A y Koncreta no lo usa. Sí lo usa el `FrameBuilder` 3D y el
  `sync_ecs_to_shape_batcher` del motor.
- **Riesgo escalabilidad 3D:** alto: es exactamente el hot loop de cualquier sistema
  ECS grande.
- **Fix propuesto:** resolver punteros de columna **una vez por archetipo** al
  construir el iterador (o guardar `column_index` por `TypeId` en el `State`) y que
  `fetch` reciba el puntero ya resuelto. Cambio interno, sin tocar la API de
  `Query`. **Esfuerzo: S/M**. **Prioridad 4.**
- **Prioridad sugerida:** 4

### [MEDIO] `sync_ecs_to_shape_batcher` clona el `Element` completo de cada entidad visible, cada frame
- **Ubicación:** `crates/ferrous_engine/src/renderer.rs:467-481` (y `:483-521`,
  llamado desde `:630` y `:671`).
- **Descripción:** para evitar el conflicto de borrows, recorre `self.node_map`,
  y por cada entidad visible hace `el.clone()` de todo el `Element` dentro de un
  `Vec<(Entity, Element, Vec3)>` (deep clone de `String`/`Vec`/`ElementKind`), y
  luego lo recorre de nuevo para emitir draws. Es una copia O(n) pesada + 1 alloc
  por frame.
- **Causa raíz:** borrow-checker: `&self.world` y `&mut self.gpu` simultáneos.
  Solución rápida aceptable, pero cara a escala.
- **Impacto:** tiempo O(n) con clones de heap; no medido (ruta no cubierta por el
  harness). El `Renderer` de `ferrous_engine` (distinto de `ferrous_renderer::Renderer`)
  es API pública exportada (`ferrous_engine/src/lib.rs:39`), pero no encontré
  consumidor del runner que la use; el runner usa `ferrous_renderer::Renderer`
  (`graphics.rs:2`).
- **Riesgo Koncreta/GUIMaker:** no verificable si algún consumidor usa
  `ferrous_engine::Renderer::sync_ecs_to_shape_batcher`; ninguno en el árbol
  inspeccionado.
- **Riesgo escalabilidad 3D:** patrón que no escala, pero es de la ruta 2D.
- **Fix propuesto:** separar el recorrido de recolección del de emisión con índices
  (`Vec<Entity>` solo, y volver a leer `Element`/`Transform` en el segundo pase), o
  usar `split` explícito de borrows en lugar de clonar. **Esfuerzo: S**. **Prioridad 7.**
- **Prioridad sugerida:** 7

### [MEDIO] `ParallelScheduler`: compila, **no es paralelo**, nunca se habilita, y su documentación describe un `rayon::scope` que no existe
- **Ubicación:** `crates/ferrous_ecs/src/system.rs:287-540` (módulo `parallel`),
  `:518-528` (`run_all`), `:299-315`/`:450`/`:503-516` (docs). Feature:
  `crates/ferrous_ecs/Cargo.toml:7-14,20`.
- **Diagnóstico (tarea #3):** la afirmación "no compila" es **inexacta para la
  librería**:
  - `cargo check -p ferrous_ecs --features parallel` → OK.
  - `cargo build -p ferrous_ecs --features parallel --target wasm32-unknown-unknown` → OK.
  - Lo que **sí** falla es `cargo test -p ferrous_ecs` (con o sin el feature), por un
    bug ajeno en `component.rs` (ver hallazgo BAJO de tests del cruce a D). Atribuir ese fallo al `ParallelScheduler`
    sería un falso positivo.
  - El código **no llama a `rayon` en ningún sitio** (grep: solo comentarios). `run_all`
    corre secuencialmente (`:524-526`); el docblock dice "dispatched in parallel via
    rayon::scope" (`:503`) y describe un contrato unsafe con punteros crudos que no
    existe. `rayon` es `optional` y **ningún miembro del workspace habilita la feature**.
- **Causa raíz:** implementación incompleta detenida (comentada como "for now" en
  `:520-523`) que quedó publicitada como terminada; falta el `run_parallel` con
  `&World`/`&ResourceMap` compartidos que permitiría el scope real. El diseño de
  batching (`conflicts_with`) sí es correcto y testeable.
- **Impacto:** funcional nulo hoy. El riesgo es que un agente futuro asuma paralelismo
  inexistente. `rayon` no entra al binario normal (feature off), así que no hay coste
  de build.
- **Riesgo Koncreta/GUIMaker:** ninguno (nadie habilita el feature).
- **Riesgo escalabilidad 3D:** el diseño es necesario para escenas grandes, pero
  **prematuro hoy**.
- **Sobre wasm:** `ferrous_assets/src/server.rs:368-381` y
  `ferrous_font/src/atlas.rs:6-8` ya demuestran el patrón correcto: uso de rayon
  gateado con `#[cfg(not(target_arch = "wasm32"))]`. Si se
  implementara el scope real, **debería llevar el mismo gate**; en wasm32 no hay
  paralelismo real sin `SharedArrayBuffer`+COOP/COEP y aun así el pool de rayon es
  un fallback mono-hilo. Un `ParallelScheduler` real es **native-only**.
- **Fix propuesto:** decidir explícitamente: (a) borrar el módulo mientras no haya
  consumidor multicore (más honesto), o (b) implementar `System::run_parallel` +
  `rayon::scope` gateado por `#[cfg(not(target_arch = "wasm32"))]` y corregir los
  docblocks. **Esfuerzo: S** (borrar/documentar) / **M** (implementar). **Prioridad 6.**
- **Prioridad sugerida:** 6

### [MEDIO] La integración ECS de `ferrous_2d` está inalcanzable; no hay jerarquía 2D
- **Ubicación:** `crates/ferrous_2d/src/systems/batching.rs:6,44,99`
  (`prepare_*_system` son funciones libres, no `impl System`), único consumidor
  `crates/ferrous_renderer/src/passes/sprite_pass.rs:41`, y `SpritePass` sin
  instanciaciones (`grep SpritePass::new` → 0). `Transform2d`
  (`crates/ferrous_2d/src/components/transform.rs`) → 0 usos fuera de `ferrous_2d`.
- **Descripción:** el "ECS-driven 2D" que el mandato pide analizar es **código
  muerto**: no hay sistemas registrados en el `StagedScheduler` para 2D
  (`runner/types.rs:64-69` solo registra Time/Velocity/Animation/Behavior/Transform
  3D), y el único puente ECS→batcher vivo es el del motor
  (`ferrous_engine/src/renderer.rs:467`), que no usa `ferrous_2d::systems`. Tampoco
  existe jerarquía 2D: `Transform2d` no tiene `Parent`/`Children`; el `TransformSystem`
  3D (`ferrous_core/src/scene/systems/hierarchy.rs:41-79`) propaga **un solo nivel**
  (no hace topo-sort), lo que además es incorrecto para jerarquías 3D multinivel.
- **Causa raíz:** dos implementaciones del mismo puente (engine vs `ferrous_2d`) y
  nunca se completó la de `ferrous_2d` ni se registró.
- **Impacto:** deuda/DX (mantenimiento): hay que mantener una capa que no
  se ejecuta. Sin impacto en tiempo para el consumidor actual.
- **Riesgo Koncreta/GUIMaker:** ninguno; Koncreta usa el modo inmediato.
- **Riesgo escalabilidad 3D:** la jerarquía 2D ausente y la propagación 3D
  mononivel son bloqueantes para un grafo de escena unificado (ADR-001).
- **Fix propuesto:** o registrar los sistemas 2D en el scheduler y borrar el puente
  duplicado del engine, o **borrar `ferrous_2d::systems` + `SpritePass`/`RendererPasses`**
  y dejar un único camino. Corregir `TransformSystem` para recorrer la jerarquía en
  orden topológico. **Esfuerzo: M**. **Prioridad 5.**
- **Prioridad sugerida:** 5

### [MEDIO] `World::spawn` aloca ~5 `Vec` por entidad y resuelve columnas con búsqueda lineal
- **Ubicación:** `crates/ferrous_ecs/src/world.rs:64-118`.
- **Descripción:** por cada `spawn` se aloca `B::type_ids()` (2 veces: `:66` y `:95`),
  `B::component_infos()` (`:70`), `ComponentSet(type_ids.clone())` (`:74`) y
  `ptrs: Vec<*mut u8>` (`:96`); además cada componente resuelve su columna con
  `arch.columns.iter_mut().find(...)` (`:99`, lineal).
- **Causa raíz:** el bundle re-deriva metadata en cada spawn en vez de cachearla por
  tipo.
- **Impacto:** ~5 allocs y varios recorridos por entidad; castiga el spawn en lote
  (carga de escena). No medido (el harness no spawnea ECS).
- **Riesgo Koncreta/GUIMaker:** ninguno (no usa `ferrous_ecs`).
- **Riesgo escalabilidad 3D:** relevante al instanciar/NPCs en masa.
- **Fix propuesto:** cachear `ComponentInfo`/firma por `TypeId` del bundle en un
  `OnceLock`/tipo estático y escribir por índice en vez de `find`. **Esfuerzo: S/M**.
  **Prioridad 8.**
- **Prioridad sugerida:** 8

### [MEDIO] El `State` de `Query` nunca se invalida y `change_tick` se ignora
- **Ubicación:** `crates/ferrous_ecs/src/query.rs:271-286` (`Query::new`),
  `:305-316` (`iter`), `crates/ferrous_ecs/src/world.rs:21-23` (`change_tick`,
  incrementado en cada mutación estructural).
- **Descripción:** la doc dice "rebuild if the world structure changes", pero
  `Query` cachea los índices de archetipo y **nunca los refresca**. Si después de
  crear la query se spawnea una entidad en un archetipo nuevo, la query no la ve.
  `change_tick` existe justamente para esto pero no se consulta.
- **Causa raíz:** se añadió `change_tick` pero no se cableó a la invalidación de
  queries.
- **Impacto:** footgun de correctitud para quien mantenga una `Query` entre frames
  (p. ej. el `FrameBuilder`, que la reconstruye cada vez, así que no se dispara hoy).
- **Riesgo Koncreta/GUIMaker:** ninguno.
- **Riesgo escalabilidad 3D:** medio; un sistema que cachee queries se quedará ciego.
- **Fix propuesto:** guardar `change_tick` en `State` y reconstruir (o `debug_assert`
  / panic) cuando difiera. **Esfuerzo: S**. **Prioridad 9.**
- **Prioridad sugerida:** 9

### [BAJO] ¿Merece un hilo de render separado del de winit? No hoy (y sería native-only)
- **Ubicación:** `crates/ferrous_engine/src/runner/frame.rs:25-356` (todo el frame en
  un solo hilo: `run_all` ECS `:193`, `app.update` `:217`, `draw_ui` `:297`,
  `queue.submit` `:331`).
- **Descripción:** no existe render thread ni job system; `rayon` solo se usa en
  import de assets (`ferrous_assets/src/server.rs:371`) y en generación de glyphs
  (`ferrous_font/src/atlas.rs`), ambos fuera del frame. wgpu ya paraleliza el trabajo
  de GPU y `queue.submit` es asíncrono; el único punto bloqueante es el readback
  `poll(Maintain::Wait)` (headless/export).
- **Causa raíz:** diseño single-thread intencional para simplicidad (y obligatorio en
  wasm).
- **Impacto:** el coste CPU real es el sort/construcción de comandos 2D; un hilo
  dedicado solo ayudaría si ese trabajo fuera paralelizable y grande (>50k instancias).
- **Riesgo Koncreta/GUIMaker:** nulo si no se hace.
- **Riesgo escalabilidad 3D:** un job system native-only sería el siguiente paso
  razonable, no un render thread completo.
- **Decisión:** **no hacerlo** en esta auditoría. Si se hiciera, marcar explícitamente
  **"native only, not applicable to web build"**: wasm32 no tiene threads reales sin
  `SharedArrayBuffer` + cabeceras COOP/COEP, y aun así el backend web de wgpu no se
  beneficia de un render thread. Prioridad mínima. **Prioridad 10.**
- **Prioridad sugerida:** 10

---

## Hallazgos cruzados para otros subagentes

- **Subagente A (GPU/pipeline):** `SpritePass` (`crates/ferrous_renderer/src/passes/sprite_pass.rs`)
  y `RendererPasses` (`crates/ferrous_renderer/src/renderer_passes.rs:45`) están
  **exportados pero nunca instanciados** (grep de `::new`/struct literal → 0). Son
  superficie de *render pass* → es tu área confirmar/eliminar. Consecuencia de mi
  área: `SpritePass` es el **único** consumidor de `ferrous_2d::systems::prepare_sprites_system`,
  así que la capa ECS de `ferrous_2d` queda inalcanzable por elevación (ver hallazgo MEDIO de la integración ECS de `ferrous_2d`).
- **Subagente D (API/DX/tests/harness):** `cargo test -p ferrous_ecs` **no compila**
  (con y sin `--features parallel`) por dos `#[cfg(test)] mod tests` en
  `crates/ferrous_ecs/src/component.rs:172` y `:244` (E0428) y por usar
  `#[derive(Component)]` (`:177`) sin importar el derive macro (E0277). Bloquea
  cualquier test de ECS. Además, el harness de 2D no cubre la ruta ECS→batcher
  (`sync_ecs_to_shape_batcher`) ni el `FrameBuilder` 3D, que es donde vive el
  dirty tracking.
- **Subagente C (memoria/assets):** `crates/ferrous_renderer/Cargo.toml:27-28` declara
  `rayon = "1.11.0"` bajo `[target.'cfg(not(target_arch = "wasm32"))'.dependencies]`
  pero **no se usa en ningún `.rs` de `src/`/`tests/`/`benches/`** (grep: el único
  match es un comentario en `benches/pure2d_headless.rs:21`) — dependencia muerta
  (native-only) que engorda el build. `ferrous_2d::prepare` aloca un `Vec` plano por
  frame (`renderer.rs:196`) — solapado con tu análisis de allocations.
- **Proceso/orquestador:** el mandato asume que `ParallelScheduler` "no compila";
  **verificado que sí compila** como librería en nativo y wasm32 (ver hallazgo MEDIO del `ParallelScheduler`). La
  premisa del shared context debería corregirse para no heredar un diagnóstico falso.

---

## Respuestas directas a las tareas del mandato

1. **Coste de re-subir todo cada frame:** 96 B/shape + 112 B/sprite. A 60 fps:
   10k ≈ 58 MB/s, 50k ≈ 288 MB/s, 100k ≈ 576 MB/s; sumado al sort O(n log n)
   (ver hallazgo ALTO de re-subida), el umbral donde empieza a doler es **~50k instancias/frame** y a
   **~100k** se come el presupuesto. Antes de eso, la pared real son los draw calls
   cuando cada primitiva usa un z distinto (Subagente A).
2. **Esquema de dirty tracking inspirado en `FrameBuilder::mark_dirty`:** el patrón
   (`bool` global + `prev_view_proj`) es trivial de portar (**S**) pero **hoy no
   aporta**: (a) en 3D no ahorra porque `sync_world` re-marca dirty cada frame; (b)
   en 2D el consumidor es modo inmediato y rediseña cada frame, así que no hay estado
   que ensuciar. Si se revive la ruta ECS 2D, conviene **revisión por componente +
   caché de revisiones subidas** (M), reutilizable por 3D, en vez de un bool ad-hoc.
3. **`ParallelScheduler`:** **no es un bug de compilación de la librería** — compila
   en nativo y wasm32. Es un **problema de diseño/estado**: nunca se habilita, su
   `run_all` es secuencial y su doc describe un `rayon::scope` inexistente. El patrón
   correcto para wasm ya existe en `ferrous_assets/src/server.rs:368` (gate
   `#[cfg(not(target_arch = "wasm32"))]`). Arreglarlo hoy es **prematuro**; borrar o
   documentar honestamente es lo razonable.
4. **`SpritePass`/`RendererPasses`:** son de **Subagente A** (superficie de pass).
   El efecto colateral (capa ECS de `ferrous_2d` muerta) es de mi área; listado en
   ambos.
5. **Hilo de render separado:** **no vale la pena** hoy; si algún día se hace, es
   **native-only** (wasm32 sin threads reales sin `SharedArrayBuffer`+COOP/COEP).
   Un job system para paralelizar el sort sería el paso previo razonable.

## No verificable en este entorno

- FPS absolutos / vsync / latencias reales de GPU: el rasterizador es software
  (llvmpipe). Solo valen deltas en la misma máquina.
- Impacto visual del bug de espejo (hallazgo CRÍTICO del espejo legacy): requiere un consumidor que use
  `translate`/`rotate`/`set_scale` sobre el `World` del motor; Koncreta no lo hace,
  GUIMaker no se auditó aquí.
- Rendimiento real del `ParallelScheduler` aun implementado: la VPS tiene 4 vCPUs y
  no hay consumidor multicore; no habría señal representativa.
- Si `ferrous_engine::Renderer::sync_ecs_to_shape_batcher` está en uso por algún
  consumidor externo: no hay referencias en el árbol inspeccionado.

---

# Fase 3 — Implementación (Subagente B: CPU / ECS / scheduler)

> Rama: `fix/2d-cpu-ecs` (parte de `analysis/2d-audit-base` + Phase-3 de Subagente A).
> Alcance aprobado: **H6**, **M1**, **M8-B**. NO tocado: C3, `ParallelScheduler` (M9),
> orden de stages. `CARGO_TARGET_DIR` apuntado al `target/` real del repo.

## Commits atómicos

| Hash | Tipo | Qué |
|---|---|---|
| `476463d` | `fix(ecs)` | M1: `cargo test -p ferrous_ecs` compila y pasa |
| `58dacbf` | `perf(ecs)` | H6: columnas de `Query` resueltas una vez por arquetipo |
| `83d3731` | `refactor(2d)` | M8-B: deprecación de los sistemas ECS muertos |
| `176fc1e` | `chore(renderer)` | M8-B: elimina dependencia `rayon` muerta en `ferrous_renderer` |
| *(este informe)* | `docs(2d-audit)` | Fase 3 en `brain/` |

## H6 — resolución de columnas por arquetipo (`Query`)

- **Qué:** `WorldQuery` ahora tiene estado por arquetipo (`type Fetch<'w>` +
  `init_fetch(arch)`); `fetch` consume ese estado en vez de recibir `&Archetype`.
  `Query::iter` construye el estado **una vez por arquetipo** antes de recorrer filas.
  Antes, `Q::fetch(arch, row)` re-ejecutaba `Archetype::column::<T>()` →
  `binary_search` sobre la firma de `TypeId` (comparaciones `u128`) por cada fila.
- **Por qué:** era el hot loop de cualquier escena ECS grande: O(filas·k·log c)
  comparaciones solo para localizar columnas, no para el trabajo útil.
- **API pública de `Query` intacta:** `new`/`iter`/`len`/`is_empty`/`reads`/`writes`
  y el contrato `State = Vec<usize>` sin cambios (por eso `system_param`/`QueryParam`
  no cambian). **Sí cambia el trait `WorldQuery`** (firma de `fetch` + `Fetch`/
  `init_fetch`); nota de migración en el cuerpo del commit para implementadores
  out-of-tree (0 en el árbol).
- **Evidencia (delta operacional, medido con instrumentación `#[cfg(test)]`
  thread-local sobre `Archetype::column_index`):**
  - Query `(&Pos, &Vel)` sobre 64 entidades en 1 arquetipo (2 componentes):
    **128 lookups antes → 2 ahora** (`<= 8` con margen). Test
    `query::tests::iter_resolves_columns_once_per_archetype`.
  - Query con 3 arquetipos y `Option<&Vel>`: resultado idéntico y lookups
    proporcionales a arquetipos, no a filas. Test
    `query::tests::iter_matches_len_across_archetypes`.
  - **No hay número de wall-clock:** el harness de D no ejercita queries ECS, así
    que el delta es de número de operaciones (comparaciones/`binary_search`), no de
    ms. No se inventa un `cpu_ms`.

## M1 — el test de ECS no compilaba

- **Qué:** se fusionaron los dos `#[cfg(test)] mod tests` de `component.rs` (E0428);
  se importó el derive `ferrous_ecs_macros::Component` (gateado por feature
  `derive`); se añadió `extern crate self as ferrous_ecs` para que el path que emite
  el derive (`ferrous_ecs::component::Component`) resuelva dentro del propio crate
  (E0433, antes enmascarado por E0277). Al correr por primera vez, el test
  `event::tests::reader_reads_previous_frame` fallaba por inferencia (`Events::new()`
  → `i32` vs consulta `Events<u32>`): se fijó el tipo.
- **Por qué:** destrabar toda la cobertura de tests del ECS (M1 era el blocker).
- **Evidencia:** `cargo test -p ferrous_ecs` → **35 passed / 0 failed**;
  `cargo test -p ferrous_ecs --features parallel` → **35 passed / 0 failed**.
  El build de tests no introduce warnings `non_snake_case` (el macro de tuplas
  lleva `#[allow(non_snake_case)]` local en `fetch`).

## M8-B — sistemas ECS 2D muertos + `rayon`

- **Confirmación (grep):** `prepare_{sprites,shapes,paths}_system` **0 call sites**
  (solo referencias en comentarios/doc); `SpritePass` ya fue eliminado por A.
- **Decisión:** los tres son `pub` re-exports de `ferrous_2d`, así que se
  **deprecan** con `#[deprecated]` + doc + nota de migración (no se borran; podrían
  romper consumidores externos). Migración: `Renderer::draw_sprite_2d` /
  `draw_2d_shape` / `draw_2d_path`.
- **Componentes NO borrados:** `Transform2d`/`Sprite`/`Camera2d` se usan en
  `ferrous_web/src/dispatcher.rs` y `Path2d`/`Shape2d` en `ferrous_renderer`; no
  están huérfanos.
- **`rayon` en `ferrous_renderer`:** confirmado sin uso en `src/`/`tests/`/`benches/`
  (único match: un comentario en el bench). Eliminado. Nota cruzada para C.
- **Evidencia:** `cargo check -p ferrous_2d` sin warnings nuevos; la deprecación de
  re-exports vía `pub use ...::*` no emite warnings.

## Verificación obligatoria ejecutada

| Comando | Resultado |
|---|---|
| `cargo check -p ferrous_ecs -p ferrous_2d` | OK, 0 warnings nuevos |
| `cargo check --workspace` | OK (solo warnings preexistentes de `ferrous_web`) |
| `cargo test -p ferrous_ecs` | 35 passed / 0 failed |
| `cargo test -p ferrous_ecs --features parallel` | 35 passed / 0 failed |
| `cargo test -p ferrous_renderer --test pure2d_headless -- --test-threads=1` | 3 passed / 0 failed (llvmpipe) |

## Notas de migración / cross-agent

- **`WorldQuery` (H6):** cambio de trait interno del ECS; no es Apéndice A y no lo
  usa Koncreta/GUIMaker, pero cualquier implementador externo de `WorldQuery` debe
  adaptarse. Normal `Query` no cambia.
- **M8-B (2D):** los `prepare_*` siguen existiendo (deprecados); Koncreta/GUIMaker
  no los usan. Sin cambio de comportamiento.
- **C:** `rayon` de `ferrous_renderer` confirmado muerto y eliminado.
- **Fuera de alcance, sin tocar:** C3 (espejo legacy), `ParallelScheduler` M9,
  orden de stages, cualquier área de A/C/D.

## No verificable en este entorno (Fase 3)

- No hay benchmark de wall-clock del ECS (el harness de D no hace queries); el
  beneficio de H6 se mide en número de `binary_search`/comparaciones, no en ms.
- Build/run wasm32 no ejecutado (sin toolchain target en este pase); el cambio de
  `WorldQuery` es puro Rust seguro/`unsafe` ya existente, sin `cfg` de plataforma.
