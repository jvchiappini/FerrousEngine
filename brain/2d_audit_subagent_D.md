# Auditoría 2D — Subagente D: API pública / DX / docs / mensajes de error

> Rama: `analysis/2d-api-dx-harness`. Fase 1 (diagnóstico + fixes seguros no
> breaking). El entorno es lavapipe/llvmpipe (software, Shared Context §2): no
> hay FPS absolutos; los impactos de esta área son de correctitud/DX y no se
> miden en GPU. Cuando un hallazgo toca la superficie del Apéndice A se marca
> **CRÍTICO** y se indica que NO se cambia su firma en Fase 1.
>
> **Entrega previa ya commiteada en esta rama (no se repite aquí):**
> el harness headless (`crates/ferrous_renderer/src/headless.rs`), los tests de
> integración (`crates/ferrous_renderer/tests/pure2d_headless.rs`), el benchmark
> divan (`crates/ferrous_renderer/benches/pure2d_headless.rs`) y los contadores
> `ferrous_2d::render::Frame2dCounters`. Verificados de nuevo hoy:
> `cargo test -p ferrous_renderer --test pure2d_headless` → 3/3 OK (llvmpipe).

## Resumen ejecutivo

La API 2D es **funcional para Koncreta por la puerta de atrás**: los dos
entrypoints que realmente usa (`enable_pure_2d`, `clear_2d`) **no están en la
fachada pública `RenderContext`**, así que hay que atravesar `renderer_mut()`
(el escape hatch que la propia doc dice que no hace falta). Encima, el runner
del motor **nunca limpia los batchers** entre frames: el modo inmediato 2D es
append-only y el usuario debe acordarse de llamar a `clear_2d()` o su geometría
se acumula ("ghosting"). El segundo problema grave es que varias promesas de la
API no se cumplen en silencio: `PathBuilder`/`Path2d` exponen cap/join/fill/close
pero `draw_2d_path` los ignora, `line_with_cap` usa un `u8` mágico en vez del
enum `CapStyle` existente, y no hay primitivo de polígono/triángulo (el shader
solo implementa un SDF de rounded-rect), justo lo que el caso CAD de Koncreta
(ear-clipping → `ShapeInstance::rect`) necesita. En cuanto a docs, la cobertura
rustdoc de `ferrous_2d` era **cero (104 warnings `missing_docs`)** y
`mkdocs.yml` apuntaba a 15 páginas inexistentes; ambos se han corregido en
commits atómicos separados. Ningún cambio de esta fase tocó firmas públicas.

## Hallazgos

### [CRÍTICO] Los entrypoints 2D del Apéndice A no están en `RenderContext`, y el runner no limpia los batchers (ghosting)
- **Ubicación:** `crates/ferrous_engine/src/render_context.rs:104-113`
  (solo existen `enable_flat_2d`/`enable_full_3d`; no hay `enable_pure_2d` ni
  `clear_2d`), `crates/ferrous_renderer/src/renderer_core.rs:631`
  (`enable_pure_2d`) y `:1905` (`clear_2d`),
  `crates/ferrous_engine/src/runner/frame.rs:240-336` (el frame no llama nunca
  a `clear_2d`), `crates/ferrous_renderer/src/headless.rs:289-292` (el harness
  SÍ lo hace y lo documenta como "mirrors AppContext::render.clear_2d()").
- **Descripción:** `RenderContext` (el "user-facing renderer facade" que
  `AppContext.render` entrega) se auto-documenta como "all renderer control
  that was previously done through `ctx.renderer.<method>()` is now available
  as `ctx.render.<method>()`" pero **omite exactamente los métodos 2D que
  Koncreta usa**. `enable_pure_2d` y `clear_2d` quedan solo en
  `Renderer` (internals), accesibles vía `RenderContext::renderer_mut()`
  (`render_context.rs:331`). Además, en modo 2D el runner construye el
  `AppContext`, llama `draw_3d`/`draw_ui` y renderiza, pero **jamás vacía
  `shape_batcher`/`sprite_batcher_2d`**: `draw_2d_shape` es un `push` puro
  (`renderer_core.rs:1666-1668`) y `clear_2d` solo lo llama quien lo invoca.
  Si la app no lo llama al inicio de cada frame, los shapes se acumulan.
- **Causa raíz:** la fachada se diseñó alrededor de 3D y nunca se extendió a la
  ruta 2D; el batcher 2D se modeló como estado persistente con limpieza a cargo
  del llamador, pero ni el runner ni la fachada materializan ese contrato.
- **Impacto:** no medible en GPU (correctitud/DX). El coste de no limpiar es
  crecimiento lineal del buffer de instancias y draws duplicados cada frame
  (el propio `Renderer2d` re-sube todo cada frame, ver hallazgo ALTO de B/C).
- **Riesgo Koncreta/GUIMaker:** **toca el Apéndice A** (`enable_pure_2d`,
  `clear_2d`). NO se cambia ninguna firma en Fase 1. El fix es **aditivo**:
  añadir métodos de delegación en `RenderContext` (misma firma que los de
  `Renderer`), lo que no rompe a Koncreta (seguirá funcionando
  `renderer_mut()`). Documentar en la nota de migración que el reemplazo
  canónico de `renderer_mut().clear_2d()` es `ctx.render.clear_2d()`.
- **Riesgo escalabilidad 3D:** ninguno.
- **Fix propuesto:** (a) delegar en `RenderContext`: `enable_pure_2d`,
  `clear_2d`, `draw_sprite_2d`, `register_sprite_texture`; (b) hacer que el
  runner limpie los batchers 2D al inicio de cada frame (o exponer un flag de
  contrato claro); (c) actualizar la tabla de `RenderContext`. **Esfuerzo: S**.
- **Prioridad sugerida:** 1

### [CRÍTICO] `line_with_cap(..., cap_style: u8)` usa valores mágicos en vez del enum `CapStyle` que ya existe
- **Ubicación:** `crates/ferrous_2d/src/render/types.rs:150-183`
  (`line`/`line_with_cap`), enum `CapStyle {Flat, Round, Square}` en
  `crates/ferrous_2d/src/components/path.rs:22-31`, `Path2d::with_cap`
  (`path.rs:157-160`).
- **Descripción:** `ShapeInstance::line_with_cap` —constructor del Apéndice A,
  usado por Koncreta para uniones CAD— recibe `cap_style: u8` con `0=Flat,
  1=Round, 2=Square`, y cualquier otro valor cae **en silencio** a Round
  (`_ => (length + width, width * 0.5)`, `types.rs:171`). El enum tipado
  `CapStyle` existe en el mismo crate y se usa en `Path2d`, pero no aquí: hay
  dos representaciones de lo mismo y una no valida. `line()` fija Round sin
  poder expresar el enum.
- **Causa raíz:** el constructor se añadió de forma independiente del enum y no
  se unificó; el `u8` sobrevivió por compatibilidad.
- **Impacto:** DX / type-safety. Un `3` o `255` no es un error de compilación ni
  runtime; se interpreta como Round. No medible en perf.
- **Riesgo Koncreta/GUIMaker:** **toca el Apéndice A** (`line_with_cap`). No se
  cambia la firma en Fase 1. Fix aditivo: añadir
  `line_with_cap_style(from, to, width, color, CapStyle)` (nombre nuevo) y
  documentar el `u8` actual como legacy en la misma nota.
- **Riesgo escalabilidad 3D:** ninguno.
- **Fix propuesto:** documentar los valores aceptados (hecho en el commit de
  rustdoc) + añadir el constructor tipado como método nuevo. **Esfuerzo: S**.
- **Prioridad sugerida:** 2

### [ALTO] `PathBuilder`/`Path2d`/`draw_2d_path` prometen cap/join/fill/close y los ignoran en silencio
- **Ubicación:** `crates/ferrous_renderer/src/renderer_core.rs:1686-1733`
  (`draw_2d_path`: `geom_length = length + stroke_width` y `radius =
  stroke_width*0.5` fijos, ignora `path.cap_style`, `path.join_style`,
  `path.is_filled`, `path.fill_color`, `path.is_closed`),
  `crates/ferrous_2d/src/components/path.rs:92-97` (`PathBuilder::close` es un
  no-op) y `:99-112` (`build()` siempre `is_closed: false`).
- **Descripción:** el builder permite encadenar `.with_cap(CapStyle::Flat)` /
  `.with_join(...)` / `.with_fill(...)` / `.close()`, y `Path2d` guarda esos
  campos, pero el único consumidor inmediato (`draw_2d_path`) dibuja **siempre
  cápsulas redondas** y nunca rellena ni cierra. Es una API trampa: el código
  compila, la intención es clara y el resultado es otro. Además
  `PathBuilder::build()` fija `is_closed:false`, así que construir un polígono
  cerrado con el builder es imposible (hay que mutar `Path2d` después).
- **Causa raíz:** la lógica de paths está triplicada (esta, `prepare_paths_system`
  y `sync_path_to_batcher` del engine) y los commits de caps/joins tocaron la
  copia ECS que nadie ejecuta (coincide con el hallazgo #9/#10 de Subagente A).
- **Impacto:** salida visual incorrecta silenciosa (uniones CAD con Round en vez
  de Flat); no medible en GPU. `draw_2d_path` **no** está en el Apéndice A, así
  que arreglarlo es seguro.
- **Riesgo Koncreta/GUIMaker:** hoy Koncreta usa
  `draw_2d_shape(ShapeInstance::line_with_cap)` (sí respeta el cap), así que no
  está roto, pero cualquiera que adopte `draw_2d_path` cae en la trampa.
- **Riesgo escalabilidad 3D:** agrava la divergencia de las tres copias de
  paths frente a ADR-001.
- **Fix propuesto:** una única función `Path2d → ShapeInstance` que honre
  cap/join/fill/close, compartida por los tres caminos (es el fix L que propone
  A); como paso intermedio barato, documentar el comportamiento real y hacer que
  `PathBuilder::close()`/`build()` produzcan `is_closed=true`. **Esfuerzo: S**
  (doc + close) / **M** (unificación).
- **Prioridad sugerida:** 3

### [ALTO] No hay primitivo de polígono/triángulo y `from_model` es privado: el caso CAD de Koncreta no tiene soporte directo
- **Ubicación:** `assets/shaders/shape_2d.wgsl:79-126` (un único SDF de
  rounded-rect; `circle` es `corner_radius = radius`),
  `crates/ferrous_2d/src/render/types.rs:50-207` (constructores),
  `:84` (`from_model` es `fn`, privado).
- **Descripción:** el shader de shapes solo representa una familia de
  rectángulos redondeados. No existe `ShapeInstance::polygon`, `triangle` ni
  `polyline`, y `from_model` (la única forma de construir un `ShapeInstance`
  desde una `Mat4`) es privado. El caso descrito de Koncreta —rellenar un
  polígono arbitrario vía ear-clipping hacia `ShapeInstance::rect`— no tiene una
  vía directa: un triángulo no es un rounded-rect, y el consumidor no puede
  construir la matriz a mano sin replicar el encoding (6 arrays `[f32;4]`) ni
  sin acceso al helper privado. Para rellenos arbitrarios solo caben
  aproximaciones (N rects/líneas) con coste O(primitivas).
- **Causa raíz:** el pipeline 2D se diseñó sobre un SDF mínimo (rect/círculo)
  para UI técnica, sin primitivos de polígono; el ear-clipping quedó como
  workaround del consumidor.
- **Impacto:** feature gap funcional para CAD/BIM (fill de polígonos
  arbitrarios), que es requisito de primer orden para Koncreta. No medible con
  el harness actual (no cubre polígonos).
- **Riesgo Koncreta/GUIMaker:** **toca el Apéndice A** (`draw_2d_shape(ShapeInstance)`),
  pero añadir un constructor/método es **aditivo y no breaking**. No cambiar la
  firma de `draw_2d_shape`.
- **Riesgo escalabilidad 3D:** un primitivo poligonal (SDF o por índices) es
  reutilizable por el renderer unificado; no cierra puertas.
- **Fix propuesto:** (a) hacer público `ShapeInstance::from_model` (o
  `new(Mat4, color, params)`) con doc del encoding, como escape hatch estable;
  (b) mediano plazo: primitivo de polígono en el shader (SDF de polígono o
  abanico de triángulos instanciados). **Esfuerzo: S** (a) / **M-L** (b).
- **Prioridad sugerida:** 4

### [ALTO] Los ejemplos de la documentación no compilan contra la API actual
- **Ubicación:** `README.md:37-101` (receta "minimum viable recipe"),
  `crates/ferrous_engine/src/traits.rs:6-10` (el `DrawContext` real) y
  `:121-144` (`FerrousApp`), `crates/ferrous_gui/src/lib.rs:24-34`.
- **Descripción:** el `README` (que es la home del sitio de docs) muestra
  `fn draw_ui(&mut self, gui, text, font, ctx)` y tipos `Ui`, `TextBatch`,
  `TextInput`, `Container`; la API real es
  `fn draw_ui(&mut self, dc: &mut DrawContext)` con
  `DrawContext { gui: &mut GuiBatch, font: &Font, ctx: &mut AppContext }`, y
  `configure_ui` recibe `&mut ferrous_gui::UiSystem<Self>`. `ferrous_gui` ya no
  exporta `TextBatch`/`Ui` (fachada sobre `ferrous_ui_render`+`ferrous_ui_core`).
  Copiar-pegar la receta no compila.
- **Causa raíz:** el split de crates de UI (`ferrous_ui_core` + `ferrous_ui_render`)
  y el refactor a `DrawContext` no se propagaron a las docs.
- **Impacto:** DX de primer contacto; la home es lo primero que ve un usuario o
  un agente IA. No medible en GPU.
- **Riesgo Koncreta/GUIMaker:** ninguno (solo docs).
- **Riesgo escalabilidad 3D:** ninguno.
- **Fix propuesto:** reescribir el ejemplo contra `DrawContext`/`UiSystem` y
  compilarlo (p. ej. como doc-test `rust,no_run` o test `#[cfg(doctest)]`).
  Requiere verificar tipos de widgets; no verificable en este entorno sin
  ventana, por lo que se reporta antes que reescribirlo a ciegas. **Esfuerzo: S**.
- **Prioridad sugerida:** 5

### [MEDIO] `mkdocs.yml` roto (15 refs inexistentes) y el sitio sin documentación 2D; el resto de warnings viene de un problema estructural del build de docs
- **Ubicación:** `mkdocs.yml` (toda la sección `nav`), `scripts/build_docs.sh:16-37`
  (copia-flatten `crates/*/docs/* → docs/<crate>/`),
  ejemplos: `crates/ferrous_assets/docs/README.md` enlaza
  `../../ferrous_ui_core/docs/README.md`, `crates/ferrous_gui/api/core.md`
  enlaza `../../ferrous_app/docs/...`.
- **Descripción:** el nav apuntaba a `ferrous_app/*` (crate renombrado a
  `ferrous_engine`) y a `ferrous_gui/widgets/*` (los widgets viven en
  `ferrous_ui_core/widgets/*`), y `ferrous_gui/layout.md` (el layout vive en
  `ferrous_layout`). MkDocs 1.6.1 no falla (exit 0) pero **deja las páginas
  404**: 15 warnings "not found in the nav". Además `ferrous_2d` no tenía
  página. **Arreglado y verificado** en el commit de docs: nav reescrito a los
  paths reales, sección `ferrous_2d` añadida y `crates/ferrous_2d/docs/README.md`
  creado; el build ahora da **0 warnings de nav y 0 de páginas huérfanas**.
  Quedan **16 warnings** (de 30) que no son del nav: los docs fuente enlazan
  entre crates con `../../<crate>/docs/...` (correcto en el árbol fuente) y el
  copy-flatten de `build_docs.sh` rompe esa profundidad. Otros son nombres
  muertos (`ferrous_app`).
- **Causa raíz (estructural):** MkDocs tiene un solo `docs_dir` generado por
  copia aplanada, pero los docs de cada crate se escriben asumiendo la
  profundidad del árbol fuente. No hay una convención de enlaces ni un paso que
  los reescriba.
- **Impacto:** docs públicas con páginas y links muertos; no medible en GPU.
- **Riesgo Koncreta/GUIMaker/3D:** ninguno.
- **Fix propuesto (para el orquestador):** elegir una estrategia, p. ej.
  (a) generar el sitio in-place respetando la estructura `crates/`, (b) usar
  `mkdocs-monorepo-plugin`, o (c) añadir una reescritura de enlaces
  (`../../<crate>/docs/` → `../<crate>/`) en `build_docs.sh`. No se aplicó
  porque reescribir los `../../` a mano rompería los links en el árbol fuente.
  **Esfuerzo: S/M**.
- **Prioridad sugerida:** 6

### [MEDIO] Los componentes ECS 2D no están registrados en ningún scheduler: spawnear un `Sprite`/`Shape2d` no renderiza
- **Ubicación:** `crates/ferrous_2d/src/systems/batching.rs:6` y `:44`
  (`prepare_*_system` sin call sites), único consumidor `SpritePass`
  (`crates/ferrous_renderer/src/passes/sprite_pass.rs`) nunca instanciado;
  `crates/ferrous_engine/src/runner/types.rs:64-69` (el `StagedScheduler` solo
  registra sistemas 3D).
- **Descripción:** `Transform2d`, `Sprite`, `Shape2d`, `Path2d` son API pública
  exportada (`ferrous_2d::*`) y se pueden añadir a un `World`, pero ninguna
  ruta los convierte en draws. El consumidor no recibe warning ni error:
  simplemente no pasa nada. La ruta viva es el modo inmediato.
- **Causa raíz:** la integración ECS 2D quedó inalcanzable y el bridge real se
  reimplementó en `ferrous_engine/src/renderer.rs` (coincide con B).
- **Impacto:** footgun de DX/deuda; no medible en GPU.
- **Riesgo Koncreta/GUIMaker:** ninguno hoy (Koncreta usa inmediato). Si algún
  consumidor adopta la API de componentes, falla silenciosamente.
- **Riesgo escalabilidad 3D:** un grafo de escena unificado (ADR-001) necesitaría
  esta ruta resuelta, no duplicada.
- **Fix propuesto:** decidir (registrar los sistemas y borrar el bridge
  duplicado) o **deprecar/documentar** `ferrous_2d::systems` + los componentes
  como no soportados. Con el rustdoc ya se documenta que no está registrado;
  falta una decisión de producto. **Esfuerzo: M**.
- **Prioridad sugerida:** 7

### [MEDIO] `register_sprite_texture` no valida `data.len()`, y el manejo de errores es inconsistente (`Result` vs `panic`)
- **Ubicación:** `crates/ferrous_renderer/src/renderer_core.rs:1827-1859`
  (`register_sprite_texture` → `create_texture_with_data`),
  wgpu-23.0.1 `src/util/device.rs:174` (`&data[binary_offset..end_offset]`),
  `crates/ferrous_assets/src/texture.rs:24-31` (`from_rgba8` con `assert_eq!`),
  `:85-94` y `:190-226` (`from_file`/`from_svg_bytes` devuelven `Result`).
- **Descripción:** si `data` es más corto que `width*height*4`,
  `create_texture_with_data` indexa fuera de rango y panica con un mensaje
  genérico de slice ("range end index … out of range"), sin decir qué textura
  ni qué dimensiones. `Texture2d::from_rgba8` al menos usa `assert_eq!`, pero
  también panica; en cambio `from_file`/`from_svg_bytes` devuelven
  `anyhow::Result`. La misma clase de operación tiene tres políticas distintas.
- **Causa raíz:** helpers escritos en momentos distintos sin una política común
  de validación de tamaño de textura.
- **Impacto:** crash de app con diagnóstico pobre; no medible en GPU.
- **Riesgo Koncreta/GUIMaker:** `register_sprite_texture` no está en el
  Apéndice A; `Texture2d::from_svg_bytes` sí, por lo que **no** se cambia su
  firma. Un `try_register_sprite_texture(...) -> Result<u32, _>` aditivo sería
  no breaking.
- **Riesgo escalabilidad 3D:** ninguno.
- **Fix propuesto:** validar el tamaño y paniquear con mensaje explícito (textura,
  esperado vs recibido) y/o añadir variante `try_*` con `Result`. Alinear
  `from_rgba8` con `Result` es breaking → Fase 3 con nota de migración.
  **Esfuerzo: S**.
- **Prioridad sugerida:** 8

### [MEDIO] `Flat2D` no dibuja shapes/sprites y su documentación pública es incorrecta/duplicada
- **Ubicación:** comportamiento en `crates/ferrous_renderer/src/renderer_core.rs:1256-1283`
  (el fast path `Flat2D` ejecuta UI + extra passes y retorna; no toca
  `renderer_2d*`), docs en `crates/ferrous_engine/src/builder.rs:292-305`
  (`AppMode::Flat2D` listado dos veces) y
  `crates/ferrous_engine/src/traits.rs:89-104` (tabla con columna `Flat2D`
  duplicada).
- **Descripción:** un usuario que elija `AppMode::Flat2D` y llame
  `draw_2d_shape` ve su geometría **descartada en silencio** (hallazgo M-04 de
  A); la doc de `with_mode` describe `Flat2D` a la vez como "2-D game" y "GUI
  tool", y la tabla de `FerrousApp` repite `Flat2D` en una columna que
  presumiblemente quería ser otro modo. No existe `AppMode::Pure2D`, que es el
  modo que Koncreta activa por `enable_pure_2d`.
- **Causa raíz:** se documentó un modo 2D completo que nunca se implementó como
  `AppMode`; el nombre `Flat2D` sugiere más de lo que hace.
- **Impacto:** DX; pérdida silenciosa de geometría. No medible en GPU.
- **Riesgo Koncreta/GUIMaker:** GUIMaker usa `Flat2D` (según Shared Context);
  si dibuja 2D técnico, es un bug latente. No toca el Apéndice A.
- **Riesgo escalabilidad 3D:** ninguno.
- **Fix propuesto:** corregir las tablas de docs y documentar `Flat2D` como
  "UI-only"; opción de producto: emitir el mismo draw-list 2D en `Flat2D` o
  añadir un `AppMode::Pure2D`. **Esfuerzo: S/M** (coordinar con A).
- **Prioridad sugerida:** 9

### [MEDIO] Cobertura rustdoc de `ferrous_2d` = 0 (104 warnings) y drift en la doc de la fachada
- **Ubicación:** todo `crates/ferrous_2d/src/**` (104 warnings `missing_docs`);
  `crates/ferrous_engine/src/render_context.rs:10-32` (tabla de métodos sin los
  2D); `crates/ferrous_engine/src/builder.rs:294-298`.
- **Descripción:** el crate 2D no tenía ni un `//!` de módulo ni docs en sus
  tipos públicos. **Arreglado** en el commit de rustdoc: `RUSTDOCFLAGS="-W
  missing_docs" cargo doc -p ferrous_2d --no-deps` pasa de 104 a **0 warnings**
  y 0 links rotos. Queda el drift de la fachada (`RenderContext`/`AppMode`),
  que es de `ferrous_engine`.
- **Causa raíz:** se priorizó el pipeline sobre la documentación; no hay
  workflow de `cargo doc` en CI (solo `mkdocs`).
- **Impacto:** un crate clave sin documentación es DX hostil para humanos y
  agentes IA. No medible en GPU.
- **Riesgo:** ninguno (docs).
- **Fix propuesto:** mantener con `#![warn(missing_docs)]` (opcional) y añadir
  un job de `cargo doc --no-deps -D warnings` al CI; corregir la tabla de
  `RenderContext`. **Esfuerzo: S**.
- **Prioridad sugerida:** 10

### [BAJO] Inconsistencias de nombres/tipos y falta de variantes `_z` en los constructores de shape
- **Ubicación:** `crates/ferrous_2d/src/render/types.rs:129-207`,
  `crates/ferrous_2d/src/components/shape.rs:32-59`,
  `crates/ferrous_engine/src/render_context.rs:280` vs
  `crates/ferrous_renderer/src/renderer_core.rs:1686`.
- **Descripción:** (1) `Shape2d::rect/circle` toman `Vec4` pero
  `ShapeInstance::rect/circle` toman `[f32;4]`; (2) `circle`/`rect` tienen
  variante `_z` pero `rounded_rect`, `rect_outline` y `rect_rotated` **no**,
  así que esos tres no permiten fijar profundidad explícita (el
  painter's-order/z-sort no se puede controlar); (3)
  `RenderContext::draw_2d_path(path: Path2d)` toma por valor mientras
  `Renderer::draw_2d_path(&Path2d)` toma por referencia; (4) `Shape2d` no tiene
  constructor `rounded_rect`/outline aunque el pipeline sí.
- **Impacto:** DX/mantenibilidad; no funcional.
- **Riesgo Apéndice A:** `draw_2d_shape(ShapeInstance)`; añadir `_z` y tipos
  nuevos es aditivo; unificar `Vec4` vs `[f32;4]` sería breaking (Fase 3).
- **Riesgo escalabilidad 3D:** ninguno.
- **Fix propuesto:** añadir `rounded_rect_z`/`rect_outline_z`/`rect_rotated_z`
  (aditivo) y documentar la convención de color. **Esfuerzo: S**.
- **Prioridad sugerida:** 11

### [BAJO] Colisión de nombre `PathCommand` y mismatch de versión `glam` con Koncreta
- **Ubicación:** `crates/ferrous_engine/src/lib.rs:64`
  (`pub use ferrous_core::scene::world::types::{PathData, PathCommand}`),
  `crates/ferrous_core/src/scene/world/types.rs:240-254`,
  `crates/ferrous_2d/src/components/path.rs:33-38`;
  `Cargo.toml:26` (glam 0.29), constructores de `ShapeInstance`/cámara toman
  `glam::Vec2`/`Vec3`.
- **Descripción:** (1) `ferrous_engine` re-exporta un `PathCommand` 3D
  (`MoveTo/LineTo/CubicTo/Close`) con el mismo nombre que el `PathCommand` 2D
  de `ferrous_2d` (`MoveTo/LineTo`); un `use ferrous_engine::PathCommand` no
  sirve para `draw_2d_path` y el error de tipos es confuso. (2) El motor usa
  glam 0.29 y Koncreta 0.32.1; los constructores del Apéndice A toman
  `glam::Vec2/Mat4`, así que Koncreta debe usar el re-export
  `ferrous_engine::glam` o convertir. Es fricción directa sobre
  `draw_2d_shape`/cámara.
- **Impacto:** DX de integración; no funcional.
- **Riesgo Koncreta/GUIMaker:** el mismatch de glam toca el Apéndice A. No se
  arregla en esta auditoría (fuera de alcance); se anota con nota si se alineara
  la versión.
- **Riesgo escalabilidad 3D:** alinear glam es sano para la unificación.
- **Fix propuesto:** renombrar el `PathCommand` 2D (breaking, Fase 3 con nota)
  o exportar alias `PathCommand2d`; alinear glam en un PR coordinado.
  **Esfuerzo: S** (alias) / **M** (alinear glam).
- **Prioridad sugerida:** 12

---

## Hallazgos cruzados para otros subagentes

- **Subagente A (GPU/pipeline):**
  - El harness de D (`headless.rs:57`, `DEFAULT_FORMAT = Rgba8UnormSrgb`)
    declara "mirrors the sRGB swapchain format used by native consumers
    (Koncreta)", pero la producción nativa elige **no-sRGB**
    (`crates/ferrous_engine/src/graphics.rs:148`). Medido por A: 0.5 → 128
    (nativo) vs 187 (harness). Desde DX, los ejemplos/rustdoc de `ShapeInstance`
    dicen "straight RGB" sin especificar espacio de color; hay que fijar la
    semántica (sRGB display-referred vs lineal) y documentarla en los
    constructores cuando se cierre #12.
  - `draw_2d_path` ignorando caps/joins/fill es el mismo hallazgo #9/#10 visto
    desde la API; el fix de unificación de A (una sola conversión
    `Path2d → ShapeInstance`) cierra también el hallazgo ALTO de este informe.
- **Subagente B (CPU/ECS):**
  - Confirma que los sistemas `ferrous_2d::systems::*` están muertos. Desde DX,
    eso significa que los componentes 2D públicos son una API sin efecto; si B
    decide borrarlos, hay que actualizar la doc del crate (ya documentada como
    "no registrada") y la sección de `ferrous_2d/README.md`.
  - `cargo test -p ferrous_ecs` no compila por un bug en `component.rs`; no toca
    2D, pero impide que el harness de D pueda cubrir la ruta ECS→batcher.
- **Subagente C (memoria/assets):**
  - El panic de `GuiBatch::reserve_texture_slot`
    (`crates/ferrous_ui_render/src/gui_batch.rs:226-231`) es un caso de
    mensaje/errores que cae en el área de D; pertenece a UI, no a 2D. Recomiendo
    devolver un `Result` o partir el batch en vez de `panic!`, y unificar el
    límite 16/8 nativo/web (hallazgo ALTO de C).
  - `Texture2d::from_rgba8` panica (`assert_eq!`) mientras `from_file`/
    `from_svg_bytes` devuelven `Result`; mismo problema de política de errores
    que el hallazgo MEDIO de `register_sprite_texture`.
- **Orquestador / proceso:**
  - La afirmación del Shared Context "mkdocs.yml está roto: apunta a
    `ferrous_app/*` y `ferrous_gui/widgets/*`" se confirma, pero el problema es
    más profundo que paths obsoletos: hay **dos convenciones de enlaces** entre
    los docs de los crates y el copy-flatten de `build_docs.sh` deja 16 links
    rotos que ni arreglando el nav desaparecen. Requiere una decisión de
    estructura (ver hallazgo MEDIO).
  - Sigue sin existir `ADR-001-*.md` ni `3D_BRIDGE.md` en el repo (lo reportaron
    A y C); la restricción de no cerrar la puerta a 3D solo vive en el prompt.

## Fixes aplicados en esta fase (commits atómicos)

1. `docs(mkdocs)`: `mkdocs.yml` reescrito a los paths reales + sección
   `ferrous_2d` + `crates/ferrous_2d/docs/README.md`. Verificado con MkDocs
   1.6.1 + Material: 0 warnings de nav (eran 15) y 0 páginas huérfanas.
2. `docs(readme)`: links obsoletos del homepage (`README.md`) y de
   `ferrous_engine/docs/getting-started.md` corregidos; warnings de docs
   bajaron de 30 a 17.
3. `docs(2d)`: rustdoc completo de la superficie pública de `ferrous_2d`
   (104 → 0 warnings `missing_docs`, 0 links roto). `cargo check -p ferrous_2d`
   OK.

Ninguno de los tres cambia firmas, campos ni comportamiento.

## No verificable en este entorno

- Que el ejemplo del README compile: requeriría una app/window y verificar
  tipos de widgets; no hay display (Shared Context §2). Se reporta, no se
  reescribe a ciegas.
- El render visual de `draw_2d_path` (caps/joins/fill) y del relleno de
  polígonos: requiere leer píxeles de una escena específica; el harness Pure2D
  actual no los cubre.
- Los `AppMode::Flat2D`/UI-only y el path de UI (bind group por frame, mismatch
  16/8) requieren `GuiRenderer`/ventana; no verificable con el harness Pure2D.
- Los números absolutos de FPS/vsync: rasterizador software (llvmpipe).

## Fase 3 — implementación aprobada (rama `fix/2d-api-dx`)

Alcance aprobado: **C1**, **C4**, **H10** y cobertura de tests **M5**. H11/M6/M7/
M10/M16/M17 quedaron fuera de esta ola. Ningún cambio toca ECS (B) ni los
internals de assets/cache SVG (C).

### C1 — entrypoints 2D en `RenderContext` + clear por frame
- **Commit:** `01f091e` `feat(engine): expose 2D entrypoints on RenderContext and clear 2D per frame`.
- **Qué:** delegaciones aditivas en `RenderContext` (`enable_pure_2d`, `clear_2d`,
  `draw_sprite_2d`, `register_sprite_texture`, mismas firmas que `Renderer`) +
  tabla de API actualizada. El runner limpia ambos batchers 2D al inicio de cada
  frame (`renderer.clear_2d()` antes de `update`/`draw_3d`/`draw_ui`).
- **Por qué:** ghosting por acumulación de shapes y API 2D principal solo
  alcanzable por `renderer_mut()`.
- **Contrato elegido (el más seguro):** el runner es dueño del ciclo de vida y
  limpia una vez por frame al inicio; `clear_2d()` sigue siendo público e
  idempotente, por lo que un clear manual de la app es redundante pero inocuo
  (no hay doble-clear con efectos). `sprite_batcher_2d` ya se limpiaba al final
  del pass Pure2D; el clear de inicio fija el mismo contrato para shapes y para
  el overlay 2D del modo Full3D.
- **Nota de migración (Koncreta/GUIMaker):** llamada canónica
  `ctx.render.enable_pure_2d(color)` / `ctx.render.clear_2d()`; `renderer_mut()`
  sigue funcionando. Quitar el `clear_2d()` manual de inicio de frame si lo
  hubiera. La geometría empujada en `setup()` ya no se renderiza (inmediato es
  por frame): dibujar desde `update()`/`draw_ui()`.

### C4 — `CapStyle` tipado para `line_with_cap`
- **Commit:** `2747e1f` `feat(2d): add typed CapStyle line constructor`.
- **Qué:** `ShapeInstance::line_with_cap_style(from, to, width, color, CapStyle)`
  aditivo que delega en el `u8` para geometría idéntica. Doc del `u8` aclarada
  (0=Flat, 1=Round, 2=Square; fuera de rango → Round).
- **Por qué:** el `u8` mágico no valida y el enum `CapStyle` ya existía; un typo
  compilaba y cambiaba la punta CAD en silencio.
- **Nota de migración:** `line_with_cap(u8)` no cambia; preferir el método
  tipado en código nuevo.

### H10 — exactitud del harness
- **Commit:** `a302627` `fix(renderer): count live sprite bind groups in headless harness`.
- **Qué:** `bind_groups_total` pasa de `next_sprite_texture_id` (high-water mark
  tras el reciclado de ids de C) a `sprite_bind_groups.len()` (bind groups vivos).
- **DEFAULT_FORMAT:** verificado — `Rgba8UnormSrgb` sí coincide con producción
  tras la inversión de A (`crates/ferrous_engine/src/graphics.rs:152-158` ahora
  prefiere `*Srgb`). Sin cambio de código; comentario actualizado.

### M5 — cobertura de tests (headless)
- **Commit:** `7b0b675` `test(renderer): cover z-order determinism, per-frame clear and live bind groups`.
- `z_order_is_deterministic_for_equal_and_interleaved_z`: orden total determinista
  (z primario; empate de z por orden de comando) con coalescing para z iguales y
  separación para z distintos; 3 iteraciones idénticas + lectura de píxel.
- `per_frame_clear_prevents_shape_accumulation`: fija el contrato que ahora aplica
  el runner, a través del harness (el runner real necesita ventana).
- `bind_groups_total_tracks_live_groups_after_free`: regresión de H10.
- **Evidencia:** `cargo test -p ferrous_renderer --test pure2d_headless --
  --test-threads=1` → **6/6 OK** (llvmpipe). `cargo test -p ferrous_2d --doc` →
  1/1 OK (doctest nuevo). `cargo check --workspace` y `cargo build --workspace`
  OK.

### No verificable / caveats
- El `render_frame` real del runner (limpieza al inicio) **no es testeable
  headless**: requiere `ActiveEventLoop` + `Surface` de winit. Se cubre el
  contrato compartido vía `Pure2dHarness::begin_frame()`.
- La ruta Full3D overlay también se beneficia del clear, pero su render visual no
  está cubierto por el harness Pure2D (M6 fuera de alcance).
- Números absolutos de FPS/vsync siguen sin ser significativos (software).

