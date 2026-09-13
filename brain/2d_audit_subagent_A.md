# Auditoría 2D — Subagente A: GPU / Pipeline de renderizado 2D

> Rama: `analysis/2d-gpu-pipeline`. Fase 1 (diagnóstico). Toda la evidencia de
> rendimiento es **hardware-independiente** (draw calls, bytes subidos,
> instancias, reallocs) porque el entorno es lavapipe/llvmpipe (software, ver
> Shared Context §2). Los `cpu_ms` del harness solo valen como delta relativo.

## Resumen ejecutivo

El pipeline 2D está **funcional pero con deuda estructural seria**: conviven dos
caminos (Pure2D y Overlay Full3D) que **no comparten la lógica de orden/batching**
y divergen en z-order, sprites y cámara; la coalescencia de draw calls colapsa a
O(primitivas) cuando cada primitiva usa un `z` distinto (medido: 4096 shapes →
4096 draw calls); y el **espacio de color es inconsistente** (swapchain nativo
prefiere no-sRGB mientras sprites/UI son sRGB y los shapes van crudos → medido:
un gris 0.5 sale como 128 en el target de producción y 187 en el harness). No se
reprodujo ningún fallo que rompa a Koncreta hoy (ningún `CRÍTICO`); el riesgo
mayor es de escalabilidad y de color.

## Metodología / evidencia cruda

- `cargo test -p ferrous_renderer --test pure2d_headless` → 3/3 OK (llvmpipe).
- `cargo bench -p ferrous_renderer --bench pure2d_headless` (líneas `[baseline]`):
  - `pure2d_shapes n=16 → 16` draw calls; `n=256 → 256`; `n=4096 → 4096`
    (4096 instancias, 393 216 bytes/frame, 1 realloc).
  - `pure2d_sprites (n=64,t=1) → 64`; `(512,8) → 512`; `(2048,64) → 2048`.
- Sonda temporal (eliminada tras medir) de z-order con 5000 shapes `z=0`
  superpuestos: `draw_calls=1`, píxel final idéntico al último dibujado
  (el painter's order se preserva hoy). Ver hallazgo M-06.
- Sonda temporal de color (eliminada tras medir): el shader escribe 0.5 y el
  readback da **128** con `Rgba8Unorm` y **187** con `Rgba8UnormSrgb`.

---

## Hallazgos

### [ALTO] #12 — Espacio de color inconsistente: swapchain no-sRGB vs texturas sRGB vs colores crudos
- **Ubicación:** `crates/ferrous_engine/src/graphics.rs:143-150` (selección de
  formato: se prefiere `!f.is_srgb()`), `crates/ferrous_renderer/src/renderer_core.rs:1853`
  (`register_sprite_texture_inner` crea `Rgba8UnormSrgb`), `graphics.rs:180-188`
  usa ese formato para `Renderer::new`; `crates/ferrous_renderer/src/render_target/hdr.rs:24`
  (`HdrTexture::FORMAT = Rgba16Float`).
  **Nota:** la referencia del prompt a `renderer_core.rs:1744` está desplazada; la
  creación sRGB real está en `:1853` (verificado).
- **Descripción:** el swapchain se configura preferentemente en formato lineal
  (`Bgra8Unorm`/`Rgba8Unorm`), por lo que wgpu **no aplica encoding sRGB** al
  presentar. Las texturas de sprite y los atlas de UI son `*Srgb` (se decodifican
  a lineal al samplear). Los colores de shapes vienen crudos desde
  `ShapeInstance.color` y se tratan como lineales. Todo el 2D termina siendo
  lineal-in → lineal-out, sin encode de salida. Medición: `0.5` del shader →
  `[128,128,128]` en el target no-sRGB (producción nativa) vs `[187,187,187]` en
  sRGB. Es decir, el render nativo sale ~59 niveles más oscuro para un gris medio.
- **Causa raíz:** nunca se decidió una política de espacio de color única. El
  comentario de `graphics.rs:148` ("Prefer linear format for engine UI
  compatibility") sugiere que la UI se implementó contra un target lineal y nadie
  cerró el lazo de conversión de salida.
- **Impacto:** sistemático en todos los píxeles 2D renderizados en nativo; no
  afecta al número de draw calls. Afecta a Koncreta (que ve colores más oscuros y
  compensa a ojo) y a GUIMaker.
- **Riesgo Koncreta/GUIMaker:** **no toca el Apéndice A** (no cambia la firma de
  `draw_2d_shape`/`ShapeInstance`/cámara), pero **sí rompe visualmente** si se
  corrige: cualquier constante de color calibrada contra el pipeline actual debe
  re-auditarse. **Nota de migración obligatoria** en el commit del fix (qué cambia,
  cómo recalibrar): Koncreta y GUIMaker deben revalidar sus paletas.
- **Riesgo escalabilidad 3D:** ninguno; una política explícita de sRGB es
  requisito para un renderer PBR/3D correcto (iluminación lineal + encode final),
  así que corrige deuda que ADR-001 necesitaría igual.
- **Fix propuesto:** invertir la preferencia en `graphics.rs` para elegir un
  formato `*Srgb` cuando exista, y documentar la semántica de color de
  `ShapeInstance.color`/`SpriteInstance.color` como sRGB display-referred
  (decodificando a lineal en el VS/FS o al construir la instancia). Alternativa
  conservadora: mantener swapchain lineal y añadir un pase de encode sRGB, pero
  eso re-introduce el costo que se quiso evitar. **Esfuerzo: S/M** (decisión +
  una línea en `graphics.rs` + conversión de instancias). Prioridad 1.
- **Prioridad sugerida:** 1

### [ALTO] #10 — Dos pipelines 2D no coordinadas: Overlay ignora z-order, descarta sprites y duplica la math de cámara
- **Ubicación:** Pure2D `renderer_core.rs:1102-1221`; Overlay Full3D
  `renderer_core.rs:1432-1478`; cámara overlay `renderer_core.rs:1795-1810`;
  cámara Pure2D `crates/ferrous_renderer/src/camera_system.rs:117-119` +
  `crates/ferrous_core/src/scene/camera.rs:125-170`.
- **Descripción:** Pure2D construye una `draw_list` de `DrawCommand2d`, la ordena
  por `transform_c3[2]` y emite runs (shapes **y** sprites). El Overlay hace
  `renderer_2d.prepare_shapes(...)` + `renderer_2d.render_shapes(...)`
  (`renderer_core.rs:1453,1477`), que:
  1. dibuja **todos** los shapes en 1 draw call en **orden de inserción**, sin
     ordenar por z (ignora `transform_c3[2]`),
  2. **nunca renderiza sprites** (`sprite_batcher_2d` no se usa en Full3D; ver
     `grep` de `sprite_batcher_2d` → solo `:1141,1147,1221,1682,1907`),
  3. usa una cámara ortográfica **duplicada a mano** en vez de `camera_system`.
  Verifiqué que ambas ortho coinciden **hoy** (`OPENGL_TO_WGPU * ortho * view`,
  mismo eye/target/up), pero es una copia que se desincroniza en silencio: el
  propio TODO de `:1439-1440` admite que el overlay no soporta pan/zoom 2D.
- **Causa raíz:** el commit de "unified z-order" se aplicó solo al fast path
  Pure2D; el overlay quedó con el `render_shapes` monolítico anterior. La lógica
  de orden vive dentro del closure del render pass en vez de ser una función
  compartida.
- **Impacto:** en Full3D, cualquier `draw_2d_shape` con distintos `z` se dibuja en
  orden ECS, no en orden de profundidad; cualquier `draw_sprite_2d` desaparece
  sin warning. ~200 líneas de lógica de dibujo duplicada/divergente.
- **Riesgo Koncreta/GUIMaker:** Koncreta usa Pure2D, no el overlay → **sin
  impacto directo**. GUIMaker usa `Flat2D` (ver M-04), tampoco overlay.
- **Riesgo escalabilidad 3D:** **alto**: la coexistencia de dos caminos 2D es
  justamente lo que ADR-001 debe resolver; dejarlos divergir encarece la
  unificación y ya produce comportamiento distinto según modo.
- **Fix propuesto:** extraer la construcción de `draw_list` (shapes+sprites,
  z-sort estable, coalescing, emisión) a un único helper
  `emit_2d_draw_list(renderer, rpass, shape_batcher, sprite_batcher, bind_groups)`
  y llamarlo desde ambos caminos; el overlay pasa a respetar z y a dibujar
  sprites. Mantener los **destinos** separados (swapchain directo vs HDR) — esa
  parte sí es necesaria. **Esfuerzo: L** (refactor acotado, sin tocar
  `ShapeInstance`). Prioridad 2.
- **Prioridad sugerida:** 2

### [ALTO] Coalescencia de batching: 1 draw call por primitiva cuando los `z` difieren
- **Ubicación:** `renderer_core.rs:1140-1217` (construcción/sort/emisión),
  `crates/ferrous_2d/src/render/batch.rs` (`ShapeBatcher` es 1 draw call total en
  teoría; `SpriteBatcher` es 1 draw call por textura), `renderer.rs` (ferrous_2d):
  `render_shapes` `:265-276`, `render_shapes_range` `:281-293`,
  `render_sprites_range` `:298-314`.
- **Descripción:** el único criterio de coalescencia es `z` **igual** + índice
  contiguo (`renderer_core.rs:1176-1183` y `:1195-1206`). Cuando cada primitiva
  lleva un `z` distinto, el sort las separa y cada una se emite como un run de
  tamaño 1. **Medido con el harness:** `pure2d_shapes` 16→16, 256→256, 4096→4096
  draw calls; `pure2d_sprites` 64→64, 512→512, 2048→2048. Además, cada run
  re-setea pipeline + bind group + vertex buffer (`render_shapes_range`/
  `render_sprites_range` lo hacen por llamada), y como el sort por z destruye la
  agrupación por textura del `BTreeMap`, una escena con muchos sprites y z
  intercalado paga 1 bind group + 1 draw por sprite.
- **Causa raíz:** se prioriza corrección de orden sobre batching (comentario
  `renderer_core.rs:1160`), pero sin una alternativa GPU-side (no hay depth en
  Pure2D, así que el orden es puramente CPU). El z-sort reordena globalmente y
  fragmenta los batches.
- **Impacto:** O(primitivas) draw calls y O(primitivas) cambios de estado por
  frame en cuanto se use `z` por elemento. Un frame BIM de miles de elementos con
  z individual degrada linealmente. **El caso `z=0` (constructores sin `_z`, los
  del Apéndice A) SÍ coalesce**: la sonda de 5000 shapes `z=0` dio 1 draw call.
  Por eso el impacto es condicional al uso de z, no constante.
- **Riesgo Koncreta/GUIMaker:** Koncreta hoy tiende a `z=0` (constructores del
  Apéndice A), así que probablemente no sufre hoy; sufre en cuanto adopte
  layering por elemento. No toca el Apéndice A.
- **Riesgo escalabilidad 3D:** bajo (un depth buffer real en 3D resuelve el orden
  sin CPU sort). De hecho el fix debería alinearse con ADR-001.
- **Fix propuesto:** (a) corto plazo: separar "orden" de "coalescencia" — agrupar
  por pipeline+textura y usar un índice de orden por grupo, en vez de exigir z
  contiguo; (b) mediano plazo: en 3D usar depth-test real y prescindir del sort
  CPU. **Esfuerzo: M** para (a). Prioridad 3.
- **Prioridad sugerida:** 3

### [ALTO/MEDIO] #9/#10 — `draw_2d_path` ignora cap/join y duplica la lógica de `prepare_paths_system`
- **Ubicación:** `renderer_core.rs:1686-1733` (`draw_2d_path`) vs
  `crates/ferrous_2d/src/systems/batching.rs:99-197` (`prepare_paths_system`);
  `crates/ferrous_engine/src/renderer.rs:524-608` (`sync_path_to_batcher`, tercera
  implementación).
- **Descripción:** `draw_2d_path` emite cada segmento como un quad con
  `geom_length = length + stroke_width` y `radius = stroke_width*0.5` **fijos**
  (`renderer_core.rs:1707,1723`): siempre cap redondo, sin joins, ignorando
  `path.cap_style`/`path.join_style`. `prepare_paths_system` sí honra caps/joins
  (incluido Flat para uniones CAD) pero **no se llama desde ningún lado**
  (verificado por grep: su único consumidor, `SpritePass`, está muerto). El motor
  (`ferrous_engine`) usa una **tercera** ruta (`sync_path_to_batcher`) que
  clasifica círculos/rects y cae a `ShapeInstance::line` (cap redondo por
  defecto).
- **Causa raíz:** la lógica de Path2D se reimplementó en cada capa en vez de
  centralizarse; los commits de caps/joins tocaron la copia ECS que nadie
  consume.
- **Impacto:** un consumidor que use `draw_2d_path` y pida Flat (lo que Koncreta
  necesita para uniones CAD) obtiene Round. Hoy Koncreta usa
  `draw_2d_shape(ShapeInstance::line_with_cap)` (Apéndice A), que sí respeta el
  cap, por lo que **no está roto hoy**, pero la API `draw_2d_path` es una trampa.
- **Riesgo Koncreta/GUIMaker:** no toca el Apéndice A. `draw_2d_path` no está en
  el Apéndice A, así que cambiarlo es seguro.
- **Riesgo escalabilidad 3D:** el overlay ya usa otra ruta de paths (lyon en
  `frame_builder.rs`), agravando la divergencia.
- **Fix propuesto:** una sola función de conversión `Path2d → ShapeInstance`
  (caps/joins incluidos) usada por `draw_2d_path`, el sistema ECS y el engine;
  borrar `prepare_paths_system` o convertirlo en esa única fuente. **Esfuerzo: M**.
  Prioridad 4.
- **Prioridad sugerida:** 4

### [MEDIO] AA redundante/solapada: SDF + MSAA opcional + FXAA por defecto en espacio lineal
- **Ubicación:** `crates/ferrous_renderer/src/passes/antialiasing_pass.rs:48-52`
  (`default() == Fxaa(..)`), `renderer_core.rs:1496-1533` (`run_aa` incondicional
  en Full3D), `assets/shaders/shape_2d.wgsl:88-119` (SDF AA), `assets/shaders/antialiasing.wgsl`
  (sin `pow`/gamma), `renderer_core.rs:426-428` (`renderer_2d_pure` con
  `sample_count=1`). MSAA por defecto es **1** (`builder.rs:109`), o sea apagado.
- **Descripción:** en Full3D se aplica en cascada: (1) AA analítico en el SDF del
  shape, (2) MSAA del render target si `with_msaa(4)`, y (3) **FXAA post por
  defecto** (`AntialiasingMode::default() = Fxaa`) sobre el HDR `Rgba16Float`
  **lineal** antes del tone-mapping. FXAA 3.11 está calibrado para input en
  gamma-espacio; aplicarlo sobre lineal cambia el contraste de detección de
  bordes. Pure2D, en cambio, **no tiene post-AA** (retorna en `:1253`). La
  calidad AA difiere entre modos.
- **Causa raíz:** el pase AA se integró a nivel de escena completa (3D+2D+UI) y
  no se desactivó por defecto cuando ya hay AA geométrico/analítico.
- **Impacto:** suavizado redundante (pérdida de nitidez en líneas técnicas, que
  es justo lo contrario a lo que pide CAD) y detección de bordes FXAA degradada
  en lineal. Coste GPU extra de 1-3 pases por frame.
- **Riesgo Koncreta/GUIMaker:** Pure2D/Flat2D no ejecutan este pase → sin impacto
  directo en Koncreta.
- **Riesgo escalabilidad 3D:** bajo; conviene resolverlo con el tonemap correcto
  (AA post sobre LDR).
- **Fix propuesto:** no activar FXAA por defecto cuando el SDF ya hace AA y el
  target no es MSAA; si se quiere post-AA, ejecutarlo **después** del encode
  sRGB/tone-map (LDR), no sobre HDR lineal. Alinear la política de AA entre
  Pure2D y Overlay. **Esfuerzo: S/M**. Prioridad 5.
- **Prioridad sugerida:** 5

### [MEDIO] M-06 — El orden entre primitivas con `z` igual depende de un sort inestable
- **Ubicación:** `renderer_core.rs:1156-1158` (`sort_unstable_by`) y
  `:1176-1183`/`:1195-1206` (merge que exige índice contiguo).
- **Descripción:** `sort_unstable_by` (pdqsort) no garantiza estabilidad. El
  painter's order entre `z` iguales y la propia coalescencia (que exige índices
  contiguos crecientes) dependen de que el sort preserve el orden de inserción.
  **Probado con 5000 shapes `z=0` superpuestos: `draw_calls=1` y el píxel final
  coincide con el último dibujado → hoy se preserva.** Pero no hay garantía
  contractual; el propio test `draw_calls_match_batched_scene` lo admite
  ("relies on `sort_unstable` preserving equal-key order for small slices").
- **Causa raíz:** se eligió un sort inestable para un caso que requiere orden
  determinista.
- **Impacto:** rendimiento/determinismo latentes: si una versión de `std` (o un
  cambio a `sort_unstable_by_key`) reordena claves iguales, el conteo de draw
  calls puede inflarse y el solapamiento de shapes cambiar. No reproducible con
  los 5000 elementos actuales.
- **Riesgo Koncreta/GUIMaker:** Koncreta usa mayormente `z=0` y depende del orden
  de dibujo → riesgo latente de artefactos de solapamiento si el sort cambia.
- **Riesgo escalabilidad 3D:** se elimina al usar depth real.
- **Fix propuesto:** usar `sort_by` estable con tie-break explícito por índice del
  comando (`(z, seq)`), de forma que el orden sea determinista y la coalescencia
  no dependa de la impl. **Esfuerzo: S**. Prioridad 6.
- **Prioridad sugerida:** 6

### [MEDIO] M-04 — `Flat2D` no renderiza shapes ni sprites (solo UI + extra passes)
- **Ubicación:** `renderer_core.rs:1256-1283`.
- **Descripción:** el fast path `Flat2D` ejecuta `ui_pass` + `extra_passes` y
  retorna; nunca llama a `renderer_2d*` ni toca `shape_batcher`/`sprite_batcher`.
  Un consumidor que haga `enable_flat_2d(...)` y luego `draw_2d_shape(...)` ve su
  geometría **descartada en silencio**.
- **Causa raíz:** `Flat2D` se concibió como "UI-only sobre clear color", pero el
  nombre y la API (`enable_flat_2d`) sugieren un modo 2D completo.
- **Impacto:** funcional para quien use `Flat2D` + shapes. Sin efecto en draw
  calls.
- **Riesgo Koncreta/GUIMaker:** GUIMaker usa `Flat2D` (según Shared Context) →
  riesgo real si dibuja 2D técnico. Koncreta usa Pure2D → sin impacto.
- **Riesgo escalabilidad 3D:** bajo.
- **Fix propuesto:** en `Flat2D`, emitir también el mismo `draw_list` 2D que
  Pure2D (sin depth, target swapchain), o renombrar/documentar el modo como
  "UI-only". **Esfuerzo: S/M**. Prioridad 7.
- **Prioridad sugerida:** 7

### [MEDIO] Oportunidades de cache de pipelines perdidas (`cache: None` en todo el motor)
- **Ubicación:** `crates/ferrous_gpu/src/context.rs:12-19` (`EngineContext` no
  expone `PipelineCache`), `pipeline.rs:109` y `:203`, `antialiasing_pass.rs:429`,
  y ~20 sitios más (`grep -n "cache:"` → todos `None`). Rebuild en
  `crates/ferrous_ui_render/src/gui_renderer.rs:283-295`.
- **Descripción:** ningún `create_render_pipeline` pasa un `cache`, y nunca se
  llama `device.create_pipeline_cache`. Además `GuiRenderer::update_output_config`
  **recompila 4 pipelines** cada vez que cambia `format`/`sample_count`
  (log observado en tests: "Rebuilding pipelines for format=..."), y
  `set_mode` alterna Pure2D/Full3D cambiando formato (`swapchain` ↔
  `Rgba16Float`+MSAA), por lo que un switch de modo recompila shaders.
- **Causa raíz:** el proyecto nació con `wgpu 23` sin cablear `PipelineCache`;
  el formato dependiente del modo se traduce en recompilación.
- **Impacto:** stalls de compilación de shader en cada cambio de modo/resize en
  nativo y, sobre todo, en WASM (donde la compilación es costosa). No aparece en
  los contadores de draw calls; es CPU/frame (no medible de forma fiable en
  lavapipe).
- **Riesgo Koncreta/GUIMaker:** no toca API pública.
- **Riesgo escalabilidad 3D:** bajo.
- **Fix propuesto:** añadir un `wgpu::PipelineCache` compartido en
  `EngineContext` y pasarlo en cada `create_render_pipeline` (`cache:
  Some(&cache)`). Evitar recompilar al alternar modos dejando ambos pipelines
  vivos (ya viven `renderer_2d` y `renderer_2d_pure`; el problema es el GUI).
  **Esfuerzo: S/M**. Prioridad 8.
- **Prioridad sugerida:** 8

### [MEDIO] Código muerto: `RendererPasses` (839 líneas) y `SpritePass`, más los sistemas ECS de `ferrous_2d`
- **Ubicación:** `crates/ferrous_renderer/src/renderer_passes.rs:45-103` (struct),
  `:119+` (`do_render`), re-export en `lib.rs:52`; `passes/sprite_pass.rs:10-97`,
  export en `passes/mod.rs:38`; sistemas `ferrous_2d/src/systems/batching.rs`
  (`prepare_shapes_system`, `prepare_paths_system`, `prepare_sprites_system`).
- **Descripción / evidencia:** `grep -rn "RendererPasses\|SpritePass"` en
  `crates/` solo encuentra la definición y el `pub use` de `lib.rs:52` — **nunca
  se instancian** (`RendererPasses` es un duplicado no cableado de
  `render_internal`). `prepare_shapes_system`/`prepare_paths_system` no tienen
  ningún call site; `prepare_sprites_system` solo lo llama `SpritePass` (muerto).
  Curiosamente `sprite_pass.rs` ya implementa el binding de texturas que el issue
  #11 daba por pendiente, pero como nadie lo usa, ese fix no tiene efecto.
- **Causa raíz:** refactors a medias: se extrajo la coordinación de passes a
  `renderer_passes.rs` y luego se reimplementó en `renderer_core.rs` sin borrar la
  copia; el motor reimplementó la conversión ECS→batcher en
  `ferrous_engine/renderer.rs`.
- **Impacto:** ~1000 líneas muertas que confunden el análisis, bit-rot
  garantizado (la copia de la pipeline ya divergió) y superficie pública de más
  (`pub use RendererPasses`).
- **Riesgo Koncreta/GUIMaker:** `RendererPasses` está re-exportado; si algún
  consumidor externo (no verificado en este repo) lo referenciara, removerlo
  sería breaking. En este workspace nadie lo usa.
- **Riesgo escalabilidad 3D:** la copia divergente es un riesgo directo para
  ADR-001.
- **Fix propuesto:** **remover** (no es medio-refactor: ya hay dos
  implementaciones vivas y la copia no está cableada). Confirmar antes que
  ningún repo externo consuma `RendererPasses`; en caso de duda, deprecar con
  `#[deprecated]` un release y luego borrar. Eliminar también los sistemas ECS
  muertos de `ferrous_2d` o documentarlos como API pública intencional.
  **Esfuerzo: S** (borrado + build verde). Prioridad 9.
- **Prioridad sugerida:** 9

### [BAJO] #9 — Lyon: cap/join hardcodeados restantes en `circle_2d` y `rect_2d`
- **Ubicación:** `crates/ferrous_renderer/src/geometry/primitives/shapes2d.rs:227-228`
  y `:256-257` (`with_line_cap(Round)`/`with_line_join(Round)` fijos);
  `into_line_cap`/`into_line_join` en `:62-77`.
- **Descripción:** `path_to_mesh` (`:189-194`) y `line_2d` (`:284-288`) ya
  reciben cap/join; el hardcodeo del issue #9 sobrevive solo en los strokes de
  `circle_2d`/`rect_2d`, que fuerzan Round/Round e ignoran
  `element.line_cap_style`/`line_join_style`. Si un consumidor pide Flat en el
  borde de un círculo/rect, se ignora.
- **Causa raíz:** se parametrizaron las funciones del issue pero no los dos
  helpers de primitivas.
- **Impacto:** cosmético/contractual; sin efecto medible en draw calls.
- **Riesgo Koncreta/GUIMaker:** Koncreta renderiza círculos/rects vía SDF
  (`ShapeInstance`), no vía lyon → sin impacto.
- **Riesgo escalabilidad 3D:** bajo.
- **Fix propuesto:** añadir `cap: LineCap, join: LineJoin` (o un struct
  `StrokeStyle`) a `circle_2d`/`rect_2d` y pasarlos desde
  `frame_builder.rs:438`. **Esfuerzo: S**. Prioridad 10.
- **Prioridad sugerida:** 10

### [BAJO] Subida de instancias completa cada frame (sin dirty tracking) + alloc por frame
- **Ubicación:** `crates/ferrous_2d/src/render/renderer.rs:174-233`
  (`prepare`/`prepare_shapes` re-suben todo; `prepare` crea `Vec` en `:196`),
  `renderer_core.rs:1111-1114` (siempre re-sube en Pure2D).
- **Descripción:** no hay dirty flags en `ferrous_2d`: cada frame se sube el
  buffer de instancias completo (`bytes_uploaded = 96 B × shapes + 112 B ×
  sprites`; medido 393 216 B para 4096 shapes) y `prepare` aloca un `Vec` plano
  nuevo cada frame. Un frame estático paga el mismo coste que uno dinámico.
- **Causa raíz:** ausencia de tracking de cambios (reconocido en Shared Context).
- **Impacto:** ancho de banda de subida de GPU proporcional al nº de instancias
  por frame; allocs/frame medibles con `divan::AllocProfiler`. Este hallazgo es
  del área de **Subagente C** (memoria/assets); se reporta como cruzado para no
  duplicar trabajo.
- **Riesgo Koncreta/GUIMaker:** ninguno funcional.
- **Riesgo escalabilidad 3D:** la ausencia de dirty tracking no escala a escenas
  3D grandes.
- **Fix propuesto:** ver Subagente C. **Esfuerzo: M**. Prioridad 11.
- **Prioridad sugerida:** 11

### [BAJO] El artefacto de arquitectura referenciado (`ADR-001-unified-2d-3d-renderer.md`) no existe en el repo
- **Ubicación:** búsqueda global (`find . -name 'ADR-001*'`) → sin resultados.
  El único doc raíz es `FERROUS.md` (59 líneas, nivel marketing) y
  `brain/2d_renderer_issues.md`.
- **Descripción:** el Shared Context y el mandato asumen una ADR-001 con la
  restricción de "no cerrarle la puerta a la unificación 2D/3D", pero **no hay
  ningún `ADR-001-*.md` ni `3D_BRIDGE.md` en disco**. La restricción de diseño
  solo existe como texto en la conversación, no como documento versionado.
- **Causa raíz:** documentación de arquitectura no commiteada (o vive en otro
  repo/workspace).
- **Impacto:** la decisión de unificar Pure2D/Overlay (hallazgo ALTO #2) no tiene
  un documento de referencia vinculante; cualquier agente futuro puede
  contradecirla. No medible en GPU.
- **Riesgo Koncreta/GUIMaker:** ninguno directo.
- **Riesgo escalabilidad 3D:** **alto a nivel proceso**: sin ADR, la unificación
  queda a criterio de cada PR.
- **Fix propuesto:** escribir/commitear la ADR-001 con: (a) política de espacio
  de color única, (b) un solo pipeline de comandos 2D reutilizable por el
  overlay 3D, (c) depth real reemplazando el z-sort CPU cuando haya escena 3D.
  **Esfuerzo: S**. Prioridad 12.
- **Prioridad sugerida:** 12

---

## Hallazgos cruzados para otros subagentes

- **Subagente D (API/DX/tests/harness):** `crates/ferrous_renderer/src/headless.rs:57`
  documenta `DEFAULT_FORMAT = Rgba8UnormSrgb` como "Mirrors the sRGB swapchain
  format used by native consumers (Koncreta)", pero **la producción nativa elige
  no-sRGB** (`graphics.rs:148`). Medido: 0.5 → 128 (nativo) vs 187 (harness). El
  harness corre en un espacio de salida distinto al de producción; las aserciones
  de píxel no representan lo que ve Koncreta. El harness además **no cubre** el
  overlay Full3D ni Flat2D ni caps/joins de paths.
- **Subagente C (memoria/assets):** `ferrous_2d::prepare` aloca un `Vec` por frame
  (`renderer.rs:196`) y ambas rutas re-suben el buffer completo cada frame sin
  dirty tracking (`renderer.rs:174-233`). `SpriteBatcher` (`batch.rs:9-34`) nunca
  libera texturas: `free_sprite_texture` solo borra el `HashMap`, y las `Vec`
  vacías del `BTreeMap` persisten entre frames.
- **Subagente B (CPU/ECS/threading):** `ferrous_2d::systems::{prepare_shapes_system,
  prepare_paths_system}` están **muertos** (sin call sites) y
  `prepare_sprites_system` solo lo usa `SpritePass` (muerto); el motor
  reimplementa ECS→batcher en `ferrous_engine/src/renderer.rs:524-608`. La lógica
  de paths está triplicada (engine, `draw_2d_path`, `prepare_paths_system`).
  `extract_subpaths` (`batching.rs:201`) aloca `Vec<Vec<Vec2>>` por path por
  frame.
- **Subagente D / proceso:** la `ADR-001-unified-2d-3d-renderer.md` que el plan
  cita como restricción vinculante **no existe en el repo** (ver hallazgo BAJO).

---

## Respuestas directas a las preguntas del mandato

1. **¿Por qué Pure2D y Overlay son dos caminos y es necesario o deuda?** La
   separación de **destinos** es necesaria (swapchain directo sin depth/HDR vs
   composite sobre el HDR 3D con depth compartido, `renderer_core.rs:1102` vs
   `:1432`). La separación de **lógica de orden/batching/cámara** es deuda: el
   overlay no ordena por z, no dibuja sprites y duplica la ortho
   (`:1795-1810`). Recomendación: mantener destinos, **unificar la emisión**.
2. **Draw calls por frame (medido):** z distinto → 1 por primitiva (4096→4096,
   2048 sprites→2048); z igual y contiguo → 1 por pipeline (5000 shapes→1,
   3 shapes+2 sprites→2 en el test). Overlay → 1 draw call para todos los shapes
   (contado desde el código; el harness no lo cubre).
3. **`SpritePass`/`RendererPasses`:** son **código muerto** (definición + export,
   cero instanciaciones). Recomendación: **remover** (no es medio-refactor; ya
   hay dos implementaciones vivas). Verificar consumidores externos antes.
4. **¿Unificar Pure2D/Overlay?** Sí en la emisión de comandos; no en el destino.
   La restricción ADR-001 citada no tiene artefacto en disco (ver BAJO).
5. **¿AA redundante/conflictiva?** Redundante/solapada: SDF AA (siempre) + MSAA
   (opt-in, default off) + FXAA (default on) sobre HDR **lineal**; Pure2D no
   aplica post-AA. FXAA sobre lineal es una elección de calidad cuestionable.
