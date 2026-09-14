# Auditoría 2D — Subagente C: Memoria, assets y texturas

> Rama: `analysis/2d-memory-assets`. Fase 1 (diagnóstico). Todos los números son
> **hardware-independientes** (draw calls, allocs/frame, bytes/frame, bind groups)
> o tiempos de CPU medidos en esta VPS con **lavapipe/llvmpipe (software)**; los
> ms solo valen como delta relativo, nunca como FPS absoluto (Shared Context §2).
> El harness de Subagente D (`ferrous_renderer::headless`) y el bench divan
> (`crates/ferrous_renderer/benches/pure2d_headless.rs`) se usaron como base.
> NO se commiteó instrumentación: las sondas temporales se crearon, midieron y
> borraron antes de este commit (ver "Metodología" para reproducirlas).

## Resumen ejecutivo

El subsistema de memoria/assets 2D está **funcional pero sin ninguna capa de
caché, pool o atlas**: `Texture2d::from_svg_bytes` —la ruta que usa Koncreta para
planos CAD— rasteriza **sincrónicamente en el hilo de frame y sin caché** (medido:
6–139 ms para un SVG de ~600 elementos, y ~15 ms por cada repetición idéntica), lo
que es el riesgo más grave y es un CRÍTICO condicional a cómo lo invoque Koncreta.
Los batchers 2D **re-suben todo cada frame y alocan buffers de scratch nuevos**
(medido: ~56 allocs/frame en shapes y ~75 allocs + ~0,5 MB/frame en ~1000 sprites),
pero **la recreación de buffers GPU está amortizada** (solo 1 realloc en 12 frames
dinámicos: el high-water-mark funciona), así que el "pooling" urgente no es de
buffers GPU sino de las allocs transitorias y del `StagingBelt`. No hay atlas de
sprites, pero el engine ya tiene dos patrones reutilizables: el **packer de rejilla
de `FontAtlas`/`IconAtlas`** y el **texture array de 16 slots de la UI**; la UI
además recrea su `image_bind_group` cada frame con texturas y tiene un **mismatch
16/8 de slots entre nativo y web** que descarta texturas 9–16 en WASM en silencio.

## Metodología / evidencia cruda

- `cargo test -p ferrous_renderer --test pure2d_headless` → 3/3 OK (llvmpipe).
- `cargo bench -p ferrous_renderer --bench pure2d_headless` (líneas `[baseline]`,
  release): shapes `16→16`, `256→256`, `4096→4096` draw calls;
  sprites `(64,1)→64`, `(512,8)→512`, `(2048,64)→2048` draw calls, con
  `bind_groups_total = 1/8/64` respectivamente. `buffer_reallocations` = 1 solo en
  escenas que cruzan la capacidad inicial (1024, `renderer_core.rs:428`).
- Sonda temporal **per-frame allocs** (`probe_dynamic.rs`, `#[global_allocator]`
  contador + `Pure2dHarness`, release, borrada tras medir), con un patrón de 12
  frames de conteo fluctuante (900…1300):
  - shapes: `buffer_reallocations` total = **1** (en el cruce 1024→2048);
    allocs/frame ≈ **56–79**, 24–43 KB/frame; draw_calls = 1 (z uniforme).
  - sprites (8 texturas, z distinto): `buffer_reallocations` total = **1**;
    allocs/frame ≈ **75–92**, **450–650 KB/frame**; draw_calls = N (700–1300).
- Sonda temporal **churn de texturas** (borrada tras medir): 3 ciclos de
  registrar+liberar 100 texturas → `next_sprite_texture_id` = 100→200→300 (nunca
  reutiliza); tras usar+liberar 2000 texturas, `sprite_bind_groups=0` pero
  `sprite_batcher_2d.batches.len()=2000` (claves huérfanas que se iteran cada frame).
- Sonda temporal **costo SVG** (`ferrous_assets/tests/svg_probe.rs`, feature
  `svg,gpu,text`, release, borrada tras medir) con un SVG sintético de 300 rects +
  300 lines (~51 KB), `Texture2d::from_svg_bytes`:
  - `64px → 5.9 ms`, `256px → 6.9 ms`, `512px → 11.8 ms`,
    `1024px → 62.3 ms`, `2048px → 138.7 ms`.
  - 20 llamadas **idénticas** a 256px = **15.4 ms/call** (sin caché: escala lineal).
- `cargo check -p ferrous_assets --features gpu` → **falla** (sin `text`).

---

## Hallazgos

### [CRÍTICO] `Texture2d::from_svg_bytes` rasteriza sin caché, sincrónicamente en el hilo de frame
- **Ubicación:** `crates/ferrous_assets/src/texture.rs:190-226` (parseo `resvg`
  → `Tree::from_usvg` → `tiny_skia` render → `from_rgba8` → `queue.write_texture`).
  Call sites de Koncreta: vía `ferrous_ui_core::widgets::{image,svg}`
  (`widgets/image.rs:82,125`, `widgets/svg.rs:99-161`) y `GuiBatch::image`
  (`crates/ferrous_ui_render/src/gui_batch.rs:331-345`).
- **Descripción:** cada llamada hace el trabajo completo (parseo usvg, construcción
  del render tree, rasterización tiny-skia y subida GPU) y **devuelve una
  `wgpu::Texture` nueva**. No hay ninguna caché por (contenido, ancho, alto) ni
  reutilización de textura. **Medido (release, lavapipe):** un SVG de ~600
  elementos tarda 5.9 ms @64px, 11.8 ms @512px, **62.3 ms @1024px** y
  **138.7 ms @2048px**; 20 llamadas idénticas @256px = 15.4 ms/cada una. Un plano
  CAD real (miles de entidades) y a resolución de viewport escala muy por encima de
  eso, y todo ocurre en el hilo del event loop (Shared Context §1: single-threaded).
- **Causa raíz:** `from_svg_bytes` es una función "pura de conveniencia" que mezcla
  rasterización (CPU) + upload (GPU) y no tiene estado; no existe un tipo de asset
  SVG en CPU equivalente a `ImageData`, así que no puede pasar por el
  `AssetServer`/rayon (ver hallazgo de assets abajo) ni cachearse.
- **Impacto:** hitch de frame proporcional al tamaño y complejidad del SVG, en el
  hilo que también hace el input y el render. Si Koncreta lo llama al abrir/cambiar
  de plano o al hacer zoom (re-rasterizar a nueva resolución), el stall es de
  cientos de ms a segundos. Si además lo llama por frame con una `Arc<Texture2d>`
  nueva, rompe la deduplicación de slots de la UI (ver hallazgo ALTO de slots).
- **Riesgo Koncreta/GUIMaker:** `Texture2d::from_svg_bytes` **está en el Apéndice A**
  → cualquier cambio que altere lo que devuelve (tamaño, píxeles, formato) es
  CRÍTICO. Por eso el fix debe ser **aditivo**: mantener firma y salida idénticas y
  añadir caché por fuera.
- **Riesgo escalabilidad 3D:** ninguno; una caché de assets por hash de contenido
  es deuda que el pipeline 3D necesita igual.
- **Fix propuesto (aditivo, no rompe Apéndice A):** (1) introducir `SvgData: Asset`
  (bytes SVG) en `ferrous_assets`, de modo que parseo y **rasterización** puedan
  correr en background vía `AssetServer`/rayon devolviendo un buffer RGBA8 CPU;
  (2) cachear por `(hash(svg), width, height)` → `Arc<Texture2d>` en un
  `SvgRasterCache` propiedad del renderer/app; (3) para actualizar pixeles sin
  crear textura nueva, añadir `Texture2d::update_rgba8(&self, queue, data)` usando
  el patrón ya existente de `TextureRegistry::update_texture_data`
  (`resources/texture_registry.rs:365-419`). Mantener `from_svg_bytes` como wrapper
  que consulta la caché y cae al path actual. **Esfuerzo: M**. Prioridad 1.
- **Prioridad sugerida:** 1

### [ALTO] Churn de texturas de sprite: IDs monotónicos y claves huérfanas en el `SpriteBatcher`
- **Ubicación:** `crates/ferrous_renderer/src/renderer_core.rs:1890-1901`
  (`next_sprite_texture_id += 1`, `free_sprite_texture` sólo hace
  `sprite_bind_groups.remove`), `crates/ferrous_2d/src/render/batch.rs:16-27`
  (`clear` recorre `values_mut` y no elimina claves), iteración de claves en
  `renderer_core.rs:1147` y en `ferrous_2d/src/render/renderer.rs:174-208`.
- **Descripción:** registrar una textura crea una textura + bind group y asigna un
  id **monotónico** que nunca se reutiliza; `free_sprite_texture` borra el bind
  group pero **no** la entrada del `BTreeMap` del `SpriteBatcher`. Medido: 3 ciclos
  de registrar+liberar 100 texturas → `next_sprite_texture_id` = 100→200→300
  (`sprite_bind_groups` vuelve a 0, los ids no); tras usar+liberar 2000 texturas,
  `batches.len() = 2000` con bind groups liberados, y el frame de un solo sprite
  sigue iterando las 2000 claves vacías en la construcción del `draw_list`
  (`renderer_core.rs:1147`) y en `prepare`.
- **Causa raíz:** no hay pool de texturas ni reciclado de id; el batcher está
  diseñado como "acumular por frame" y nunca olvida texturas. No hay `Drop`/RAII
  que devuelva el bind group al liberar una `Texture2d`.
- **Impacto:** memoria GPU: si el consumidor no llama `free_sprite_texture`, cada
  recarga de un SVG deja textura+bind group vivos para siempre. CPU: aunque los
  bind groups se liberen, el costo de iteración por frame crece O(nº de texturas
  históricas) para siempre (medido: 2000 claves en un frame de 1 sprite).
- **Riesgo Koncreta/GUIMaker:** no toca el Apéndice A (`draw_sprite_2d`/`Sprite`
  no están listados); sí afecta a Koncreta si recarga planos repetidamente.
- **Riesgo escalabilidad 3D:** bajo, pero un pool de texturas con id reciclado es
  requisito de cualquier renderer de escenas grandes.
- **Fix propuesto:** (a) `free_sprite_texture` debe borrar también la clave del
  `SpriteBatcher` (`batches.remove(&id)`); (b) pool de ids libres
  (`Vec<u32>` de reciclaje) reutilizado por `register_sprite_texture_inner`;
  (c) opcional RAII: un `SpriteTextureHandle` con `Drop` que devuelva el slot.
  **Esfuerzo: S**. Prioridad 2.
- **Prioridad sugerida:** 2

### [ALTO] Sin atlas/packer de sprites: 1 textura = 1 bind group = 1 draw call por textura
- **Ubicación:** `crates/ferrous_2d/src/render/batch.rs:6-34` (`SpriteBatcher` =
  `BTreeMap<u32, Vec>`), `renderer_core.rs:1147-1214` (1 draw por run de
  `texture_id`), `crates/ferrous_2d/src/render/renderer.rs:249-262` (bind group por
  lote), pipeline con una sola textura en `group(1)`:
  `assets/shaders/sprite_2d.wgsl:11-12` y `ferrous_2d/src/render/pipeline.rs:32-52`.
- **Descripción:** no existe packer de sprites. `SpriteInstance.uv_rect`
  (`ferrous_2d/src/render/types.rs:12`, shader `sprite_2d.wgsl:62`) **ya soporta
  sub-rect de atlas**, pero nadie lo rellena: `register_sprite_texture` da un id por
  textura y cada textura necesita su propio bind group. **Medido:**
  `pure2d_sprites (2048,64)` → **2048 draw calls, 64 bind groups**;
  `(64,1)` → 64 draw calls. Con z distinto, el z-sort fragmenta además cada
  textura en runs de tamaño 1 (coincide con el hallazgo ALTO de Subagente A).
- **Causa raíz:** el diseño original asumió "una textura por sprite"; el soporte de
  `uv_rect` quedó como spritesheet manual para el usuario, sin packer automático.
- **Impacto / ahorro estimado:** un atlas de una página colapsa la dimensión
  textura a 1, con lo que **N sprites → 1 draw call y T bind groups → 1** (caso
  uniform-z, típico UI) y, **si se combina con el fix de coalescencia de A**
  (agrupar por pipeline+textura conservando un índice de orden en vez de exigir z
  contiguo, `brain/2d_audit_subagent_A.md`), incluso con z intercalado el sprite
  batch queda en **1 draw call**. Medido sobre el baseline: hasta 2048→1 draw calls
  en el peor caso sprite del harness.
- **Riesgo Koncreta/GUIMaker:** no toca el Apéndice A. Koncreta hoy usa shapes
  (no sprites) para los planos; el atlas beneficia sobre todo a la UI/GUIMaker y a
  escenas con muchos iconos/sprites.
- **Riesgo escalabilidad 3D:** un atlas 2D único no cierra puertas; el camino
  general es el descriptor-array bindless (ya hay stub `bindless` en
  `ferrous_renderer`), que convive con atlas.
- **Fix propuesto / coordinación con A:** reutilizar **dos patrones ya existentes**
  en vez de inventar uno nuevo:
  1. **Packing:** `FontAtlas`/`IconAtlas` (`crates/ferrous_font/src/atlas.rs:39-101`,
     `icons.rs:76-108`) ya implementan un packer de rejilla con mapa
     `nombre → [uv:4]`. Extraer un `TextureAtlas`/`Packer` genérico (rejilla para
     tamaños homogéneos, shelf/skyline para heterogéneos) a `ferrous_2d` o
     `ferrous_assets`.
  2. **Binding:** la UI ya usa un **texture array** de hasta 16 slots con índice por
     quad (`MAX_TEXTURE_SLOTS`, `gui_batch.rs:214-235`, `layout.rs:51-72`,
     `gui_renderer.rs:375-420`). Alternativa al atlas clásico: array de N texturas +
     `tex_index`, 1 bind group por cada N texturas (evita bleeding de atlas, soporta
     mips/wrapping). Limitación Web: 8 slots (ver hallazgo ALTO de slots).
  Propuesta compatible con A: `register_sprite_texture` devuelve `(atlas_page,
  uv_rect)`; el engine rellena `SpriteInstance.uv_rect` internamente; A agrupa por
  `atlas_page`. **Esfuerzo: L**. Prioridad 3.
- **Prioridad sugerida:** 3

### [ALTO] Recreación de recursos durante el frame: bind group de imágenes UI cada frame + allocs de scratch de los batchers
- **Ubicación:** GUI: `crates/ferrous_ui_render/src/gui_renderer.rs:375-420`
  (`if !batch.textures.is_empty() { create_bind_group(...) }` en `prepare`, que se
  llama cada frame desde `UiPass::prepare`,
  `crates/ferrous_renderer/src/passes/ui_pass.rs:67-71`). Batchers:
  `ferrous_2d/src/render/renderer.rs:181-191` y `:214-224` (recrean buffer al
  crecer, sin pool), `renderer.rs:196` (`Vec::with_capacity` por frame),
  `renderer_core.rs:1140-1142` (`draw_list: Vec::with_capacity` por frame).
- **Descripción:** (1) `GuiRenderer::prepare` **recrea el `image_bind_group` cada
  frame** en que el batch tenga texturas, sin comparar contra el set anterior: es
  una creación de bind group + 2 `Vec` de views/samplers por frame (en Koncreta,
  `draw_ui` con `Image`/`Svg` lo dispara). (2) Los batchers 2D no tienen pooling:
  recrean el `wgpu::Buffer` cuando el frame supera el high-water-mark y **alocan
  scratch nuevo cada frame** (`flat: Vec<SpriteInstance>`, `draw_list`).
  **Medido:** las recreaciones GPU están **amortizadas** (total 1 realloc en 12
  frames dinámicos, porque `max_instances` solo crece:
  `renderer.rs:181-182`/`:214-215`), pero las allocs transitorias son de **56–79
  allocs/frame** (shapes) y **75–92 allocs + 450–650 KB/frame** (≈1000 sprites),
  además del staging interno de `queue.write_buffer`.
- **Causa raíz:** no hay scratch reutilizable en `Renderer2d`/`renderer_core`, ni
  `wgpu::util::StagingBelt`; el bind group de UI no está cacheado por conjunto de
  texturas. El diseño "sin dirty flags" de `ferrous_2d` (Shared Context §1) no
  distingue frame estático de dinámico.
- **Impacto:** presión sobre el allocador y copias CPU→staging proporcionales al
  contenido cada frame; en UI inmediata (exactamente el caso de Koncreta) el
  bind group se reconstruye aunque las texturas no cambien. No se midió el costo
  del bind group UI (fuera del harness Pure2D, ver "no verificable").
- **Riesgo Koncreta/GUIMaker:** no toca el Apéndice A; mejora directa de Koncreta
  (UI inmediata).
- **Riesgo escalabilidad 3D:** el patrón de scratch/pool reutilizable es base para
  escenas grandes; no cierra puertas.
- **Fix propuesto:** (a) cachear el `image_bind_group` de UI con una clave
  `(Vec<*const TextureView>, n)` y reconstruirlo solo si cambia, en vez de en cada
  `prepare`; (b) mover el `flat` de `prepare` y el `draw_list` a buffers scratch
  reutilizables en `Renderer2d`/`Renderer` (se limpian con `.clear()`); (c) usar
  `wgpu::util::StagingBelt` para los `write_buffer` por frame; (d) `buffer pooling`
  real solo hace falta si se instancian varios `Renderer2d`/viewports (hoy hay 2:
  `renderer_2d` y `renderer_2d_pure`, `renderer_core.rs:425-428`) — un pool de
  buffers por capacidad evita reconstruirlos. **Esfuerzo: M**. Prioridad 4.
- **Prioridad sugerida:** 4

### [ALTO] Texture array de la UI: native 16 slots vs web 8; `reserve_texture_slot` valida 16 (panic) y en Web descarta texturas 9–16 en silencio
- **Ubicación:** `crates/ferrous_ui_render/src/lib.rs:19-20` (`MAX_TEXTURE_SLOTS=16`),
  `pipelines/layout.rs:51-100` (nativo `count=16`; wasm loop `0..8`),
  `gui_batch.rs:226-231` (panic si `len >= 16`), `gui_renderer.rs:384-419`
  (wasm bind group solo con 8).
- **Descripción:** el layout nativo declara 16 pares textura/sampler; el de web
  declara 8. `reserve_texture_slot` valida contra 16 **en ambas plataformas**. En
  web, `GuiBatch` con 9–16 texturas distintas entra sin error, pero
  `create_bind_group` solo usa las primeras 8 → los quads con `tex_index>=8`
  muestrean la textura equivocada (o la última) sin warning. En nativo, 17
  texturas distintas en un batch → **panic**. Además, `reserve_texture_slot`
  deduplica por `Arc::ptr_eq`, así que **si el consumidor re-crea la
  `Arc<Texture2d>` cada frame (p. ej. re-rasterizando el SVG sin caché, hallazgo
  CRÍTICO), los slots se llenan y se llega al panic**.
- **Causa raíz:** el límite del layout se parametrizó por plataforma pero la
  validación quedó con la constante global; el contrato (8 vs 16) no está unificado.
- **Impacto:** correctness multiplataforma (Web, que es un target real de Ferrous)
  y estabilidad (panic en nativo con UI de muchas imágenes).
- **Riesgo Koncreta/GUIMaker:** GUIMaker desktop podría panicar con >16 imágines por
  batch; Koncreta WASM podría renderizar texturas incorrectas. No toca el Apéndice A.
- **Riesgo escalabilidad 3D:** el texture array es la base del bindless; su límite
  debe ser explícito, no accidental.
- **Fix propuesto:** usar un límite por plataforma
  (`const UI_TEXTURE_SLOTS: u32 = if cfg!(wasm32) {8} else {16}`) tanto en el layout
  como en `reserve_texture_slot`; al excederlo, **partir el batch en segmentos** en
  vez de panicar (o documentar y devolver error). **Esfuerzo: S/M**. Prioridad 5.
- **Prioridad sugerida:** 5

### [MEDIO] `ferrous_assets` no compila con `gpu` sin `text` (bloquea testear el path SVG)
- **Ubicación:** `crates/ferrous_assets/src/font_importer.rs:63-71`
  (`into_font` gateado por `#[cfg(feature = "gpu")]` pero usa `ferrous_font`, que
  sólo activa la feature `text`), `Cargo.toml` (`text = ["dep:ferrous_font","gpu"]`).
- **Descripción:** **medido:** `cargo check -p ferrous_assets --features gpu` falla
  con `E0433: unresolved module ferrous_font`. Lo mismo con
  `--features svg,gpu`. El único test de `from_svg_bytes`
  (`texture.rs:242-270`) vive bajo `#[cfg(all(test, feature="svg"))]` y requiere
  `gpu`; para compilarlo hay que añadir `text`.
- **Causa raíz:** el gate de `into_font` debería ser `#[cfg(feature = "text")]` (o
  incluir `ferrous_font` en `gpu`), no `gpu`.
- **Impacto:** feature-composition rota: cualquier build que active `gpu` sin `text`
  (p. ej. `ferrous_renderer --no-default-features --features assets`, o un
  consumidor que use `svg` + `gpu` para `from_svg_bytes` sin texto) no compila.
  En el workspace hoy no se nota porque `ferrous_gui` activa `gpu,text` por
  defecto; es una trampa latente.
- **Riesgo Koncreta/GUIMaker:** Koncreta probablemente activa `svg` y `gpu`; si no
  activa `text`, **su build no compila**. Nota de migración: usar
  `ferrous_assets = { features = ["gpu","svg","text"] }` hasta que se arregle.
  No toca el Apéndice A.
- **Riesgo escalabilidad 3D:** bajo.
- **Fix propuesto:** cambiar el gate de `into_font` a `feature = "text"` y mover
  `pub mod font_importer`/`pub use FontData` también bajo `text`, o declarar
  `gpu = ["text"]`. **Esfuerzo: S**. Prioridad 6.
- **Prioridad sugerida:** 6

### [MEDIO] El único test de `from_svg_bytes` no compila y la feature `svg` no la activa nadie → cero cobertura real de la ruta de Koncreta
- **Ubicación:** `crates/ferrous_assets/src/texture.rs:249-270`
  (`wgpu::Instance::new(wgpu::Backends::all())`).
- **Descripción:** la API de wgpu 23 es
  `wgpu::Instance::new(wgpu::InstanceDescriptor::default())` (ver
  `headless.rs:232`); el test usa la firma antigua y **no compila**. Además, grep de
  `ferrous_assets/svg` en todos los `Cargo.toml` del workspace → **ninguno** activa
  la feature, así que `from_svg_bytes` no se compila ni se testea en CI local.
- **Causa raíz:** código feature-gated sin job de CI que active la combinación de
  features; el test quedó obsoleto al subir wgpu.
- **Impacto:** la función crítica para Koncreta no tiene ni un test; el harness
  headless de D tampoco la cubre. Cualquier regresión pasa desapercibida.
- **Riesgo Koncreta/GUIMaker:** ninguno directo, pero es la deuda que permite que el
  CRÍTICO de performance persista sin detección.
- **Riesgo escalabilidad 3D:** bajo.
- **Fix propuesto:** arreglar la firma del test, añadir
  `cargo test -p ferrous_assets --features svg,gpu,text` al CI, y usar el
  `Pure2dHarness`/`EngineContext` headless de D para un test de render de SVG
  (compartir dispositivo). **Esfuerzo: S**. Prioridad 7.
- **Prioridad sugerida:** 7

### [MEDIO] `ferrous_svg::SvgMesh::from_str` no parsea SVG (placeholder que dibuja geometría hardcodeada)
- **Ubicación:** `crates/ferrous_svg/src/lib.rs:33-86` (sólo mira
  `contains("<rect")`/`contains("<circle")` y emite vértices fijos), usado por
  `ferrous_ui_core/src/widgets/svg.rs:189-190` (`SvgMesh::from_str`).
- **Descripción:** el "tessellator SVG" es un stub explícito (`// Simple heuristic`,
  `// Default placeholder triangle`) que ignora el contenido real: cualquier SVG
  sin `<rect`/`<circle` produce un triángulo, y un `<rect` real produce un cuadrado
  fijo, sin transformaciones, colores, paths ni viewBox.
- **Causa raíz:** implementación placeholder de Fase 6 nunca completada, pero ya
  cableada a la widget `Svg` de la UI.
- **Impacto:** los SVG vectoriales de la UI renderizan basura. Es la alternativa
  "vectorial" a `from_svg_bytes` y hoy no es usable.
- **Riesgo Koncreta/GUIMaker:** Koncreta usa `from_svg_bytes` (raster), no
  `SvgMesh`; GUIMaker/UI podrían usarlo y ver geometría incorrecta.
- **Riesgo escalabilidad 3D:** bajo.
- **Fix propuesto:** integrar `usvg` (ya disponible vía la feature `svg` de
  `ferrous_assets`) o eliminar/marcar `SvgMesh` como no funcional para que nadie lo
  use. **Esfuerzo: M/L** (o S para deprecarlo). Prioridad 8.
- **Prioridad sugerida:** 8

### [MEDIO] `Texture2d` no permite actualizar datos (patrón aplicable: `TextureRegistry::update_texture_data`); `AssetServer` sin eviction
- **Ubicación:** `crates/ferrous_assets/src/texture.rs:8-94` (no hay
  `update_*`), `crates/ferrous_renderer/src/resources/texture_registry.rs:365-419`
  (patrón de hot-reload), `crates/ferrous_assets/src/server.rs:100-119,169-201`
  (`slots: Vec` y `path_index: HashMap` sólo crecen), `asset_types/src/handle.rs`
  (generación `u16`).
- **Descripción:** `TextureRegistry::update_texture_data` (usado por materiales) es
  el único dirty-tracking real de texturas fuera de 3D: **reutiliza la misma
  `wgpu::Texture`** y sólo hace `write_texture` (+ regenera mips en CPU), sin
  recrear bind groups. `Texture2d`, en cambio, sólo sabe construir texturas nuevas.
  Además, `AssetServer` nunca libera slots ni `path_index` (no hay `unload`),
  `get()` clona el `Arc` por poll y las generaciones son `u16` con `wrapping_add`.
- **Causa raíz:** `Texture2d` es el helper "legacy" previo al registry; no se le
  añadió actualización in-place. `AssetServer` se diseñó sin ciclo de vida de
  descarga.
- **Impacto:** sin `update`, cualquier refresco obliga a crear textura + bind group
  nuevos (alimenta el hallazgo ALTO de churn). Sin eviction, cargar assets
  repetidamente (hot-reload, cambio de plano) crece memoria indefinidamente.
- **Riesgo Koncreta/GUIMaker:** no toca el Apéndice A si `update` es aditivo.
- **Riesgo escalabilidad 3D:** un registry con eviction/streaming es requisito de
  escenas 3D grandes.
- **Fix propuesto:** añadir `Texture2d::update_rgba8(&self, queue, width, height,
  data)` copiando la semántica de `update_texture_data`; añadir
  `AssetServer::unload(handle)`/LRU o, como mínimo, documentar que no hay
  eviction. **Esfuerzo: S/M**. Prioridad 9.
- **Prioridad sugerida:** 9

### [MEDIO] `AssetServer::tick()` (hot-reload) importa sincrónicamente en el hilo de render
- **Ubicación:** `crates/ferrous_assets/src/server.rs:294-340` (llama `reimport(&path)`
  inline dentro de `tick`, típicamente una vez por frame), vs
  `server.rs:367-381` (carga inicial sí usa `rayon::spawn`).
- **Descripción:** la primera carga es background (rayon, verificado en
  `server.rs:371`); en **wasm** es `spawn_local` + `fetch` no bloqueante
  (`server.rs:383-419`). Pero el **hot-reload nativo** ejecuta `T::import` en el
  hilo que llama `tick()`, que es el de render. Un asset pesado (GLTF/textura) en
  disco se importa en medio del frame.
- **Causa raíz:** `tick` no delega el reimport a rayon como `spawn_import`.
- **Impacto:** hitch en cada guardado de archivo en modo editor/hot-reload. No
  afecta a producción de Koncreta si no usa `watch`.
- **Riesgo Koncreta/GUIMaker:** ninguno hoy (no usan hot-reload por lo documentado).
- **Riesgo escalabilidad 3D:** el hot-reload sincrónico no escala.
- **Fix propuesto:** encolar el reimport en el mismo pool rayon y publicar por el
  inbox con la nueva generación, reutilizando `spawn_import`/`drain_inbox`.
  **Esfuerzo: S**. Prioridad 10.
- **Prioridad sugerida:** 10

### [BAJO] El doc de `AssetServer` sobre wasm está desactualizado (dice "sincrónico" pero usa `fetch` async)
- **Ubicación:** `server.rs:21-24` y `:166-168` ("runs synchronously on the calling
  thread"/"runs synchronously right now") vs `server.rs:383-419`
  (`wasm_bindgen_futures::spawn_local` + `window.fetch_with_str`).
- **Descripción:** el comentario describe una implementación anterior; el código
  actual es asíncrono vía `fetch`. Confunde al evaluar si el import bloquea el frame.
- **Causa raíz:** doc no actualizado al portar a `fetch`.
- **Impacto:** ninguno funcional; riesgo de conclusiones erróneas en auditorías.
- **Riesgo Koncreta/GUIMaker/3D:** ninguno.
- **Fix propuesto:** corregir el doc. **Esfuerzo: S**. Prioridad 11.
- **Prioridad sugerida:** 11

---

## Hallazgos cruzados para otros subagentes

- **Subagente A (GPU/pipeline) — #12 sRGB, componente de texturas:** sí hay una
  parte que me corresponde: **todas** las texturas de sprite y las `Texture2d`
  RGBA8 son sRGB. `Texture2d::from_rgba8` fuerza `Rgba8UnormSrgb`
  (`ferrous_assets/src/texture.rs:42`), `register_sprite_texture_inner` también
  (`renderer_core.rs:1853`), y `Texture2d::from_svg_bytes` pasa por `from_rgba8`.
  El único lugar que distingue color vs dato es el `TextureRegistry` de materiales
  (`register_rgba8` vs `register_rgba8_linear`,
  `resources/texture_registry.rs:84-107`). Es decir, hoy no existe forma de cargar
  un sprite/normal-map lineal por `ferrous_assets::Texture2d`. El fix de #12 debe
  fijar el inventario "qué textura es sRGB vs lineal" además del swapchain; lo dejo
  como hallazgo cruzado, **no lo arreglo** (dueño: A). Impacto: si A invierte la
  preferencia del swapchain a sRGB, las texturas ya son correctas, pero cualquier
  dato no-color que se haya cargado por `Texture2d` quedará mal.
- **Subagente A — baseline/coalescencia:** el ahorro del atlas depende de tu fix de
  coalescencia por pipeline+textura. Sin él, el atlas sólo da 1 draw call con z
  uniforme/contiguo (UI); con él, da 1 draw call incluso con z intercalado. Los
  números de sprite que cito (2048→2048, 64 bind groups) salen de tu bench.
- **Subagente B (CPU/ECS/threading):** `prepare_sprites_system`/`SpritePass` están
  muertos (coincide con tu hallazgo); el `SpriteBatcher` sin eviction que reporto
  es el mismo objeto que esos sistemas usarían, así que si se revive la ruta ECS
  hereda el leak de claves. `extract_subpaths` y los `Vec<Vec<Vec2>>` por frame son
  del área B.
- **Subagente D (API/DX/harness/tests):** (1) `headless.rs:57` documenta el
  `DEFAULT_FORMAT` como sRGB "para que coincida con Koncreta", pero la producción
  nativa es no-sRGB (coincide con A); (2) el harness **no cubre** el path de UI
  (`GuiRenderer`/`GuiBatch`), donde detecté la recreación del `image_bind_group`
  por frame y el mismatch 16/8 de slots — no verificable en este entorno con el
  harness actual; (3) el único test de `from_svg_bytes` no compila y la feature
  `svg` no se activa en el workspace: falta un job de CI con
  `--features svg,gpu,text`.
- **Proceso (ADR-001):** coincido con A en que `ADR-001-unified-2d-3d-renderer.md`
  no existe en disco; cualquier decisión de atlas/bindless debería quedar
  registrada ahí.

## Respuestas directas a las preguntas del mandato

1. **¿Estrategia de atlas y ahorro de draw calls?** Sí, diseñada sin implementar
   (ver hallazgo ALTO de atlas). Ahorro medido sobre el baseline: hasta
   **N→1 draw calls** y **T→1 bind groups** para sprites (2048→1 en el peor caso
   del harness). Coordinado con A: su coalescencia por textura es el multiplicador
   que hace que el atlas rinda incluso con z intercalado; propongo reutilizar el
   packer de `FontAtlas`/`IconAtlas` y/o el texture-array de la UI en vez de crear
   un tercer mecanismo. **No hay dos soluciones incompatibles con A.**
2. **¿Buffer pooling en vez de recrear en cada resize?** La premisa es parcial: la
   recreación de buffers GPU está **amortizada** (solo al cruzar potencias de 2;
   medido 1 realloc en 12 frames dinámicos) porque `max_instances` es un
   high-water-mark. Lo que sí conviene: (a) scratch reutilizable para el `flat` de
   `prepare` y el `draw_list` (allocs/frame medidos 56–92), (b) `StagingBelt` en
   vez de `write_buffer` por frame, (c) cachear el `image_bind_group` de la UI que
   hoy se recrea cada frame, (d) un pool de buffers por capacidad sólo si hay
   múltiples `Renderer2d`/viewports.
3. **¿`update_texture_data` de materiales es patrón aplicable a `Texture2d`?** Sí:
   reutiliza el mismo `wgpu::Texture` y sólo hace `write_texture` (+mips CPU) sin
   recrear bind groups (`texture_registry.rs:365-419`). `Texture2d` debería exponer
   un `update_rgba8` aditivo con esa semántica (ver hallazgo MEDIO).
4. **¿Tengo responsabilidad en el mismatch sRGB (#12)?** Parcial: la elección de
   formato sRGB/lineal de las texturas 2D (`Texture2d::from_rgba8`,
   `register_sprite_texture_inner`) es mía. Lo reporto como hallazgo cruzado para A
   y **no lo arreglo**; el owner es A.

## No verificable en este entorno

- El costo por-frame de la recreación del `image_bind_group` de la UI y el
  mismatch de slots 16/8: requieren el path de UI (`GuiRenderer`), que el harness
  headless Pure2D de D no ejercita y que necesita una app/ventana para medirse.
  Reportado desde código, no medido.
- Comportamiento real de `from_svg_bytes` **dentro de Koncreta** (frecuencia de
  llamada, resolución, si cachea): Koncreta no está en este repo. Los ms medidos son
  de un SVG sintético en lavapipe; valen como orden de magnitud relativo.
- Web/wasm: el mismatch de 8 slots y el `fetch` de `AssetServer` no se pudieron
  ejecutar (target `wasm32` no compilado en esta VPS).
