# ferrous_ui_render

Backend de renderizado GPU para la UI de Ferrous Engine.

Recibe `RenderCommand`s abstractos del árbol de `ferrous_ui_core`, los agrupa en
lotes de instancias (`GuiBatch`) y los ejecuta con dos pipelines WGPU:
uno para quads coloreados/texturizados y otro para glifos de texto.
El objetivo es minimizar las llamadas de dibujo (draw calls) agrupando el mayor
número posible de primitivas en un único pase de instancing.

---

## Flujo de datos

```
UiTree::collect_commands()
        │
        ▼
Vec<RenderCommand>
        │  ToBatches::to_batches()
        ▼
    GuiBatch  (quads + text_quads + segments)
        │
        ▼
GuiRenderer::render()
        │
        ├─► pipeline (gui.wgsl)       → draw_indexed instanced  (quads)
        └─► text_pipeline (text.wgsl) → draw_indexed instanced  (glifos)
```

---

## Tipos públicos

### `GuiQuad`

Estructura `#[repr(C)]` + `bytemuck::Pod` que se envía directamente al buffer
de instancias en GPU:

| Campo | Tipo | Descripción |
|---|---|---|
| `pos` | `[f32; 2]` | Posición en píxeles (esquina superior izquierda) |
| `size` | `[f32; 2]` | Ancho y alto en píxeles |
| `uv0 / uv1` | `[f32; 2]` | Rango de coordenadas UV (para imágenes) |
| `color` | `[f32; 4]` | Color RGBA normalizado |
| `radii` | `[f32; 4]` | Radios de esquinas (TL, TR, BR, BL) |
| `tex_index` | `u32` | Ranura de textura dentro del bind group de imágenes |
| `flags` | `u32` | Bits de comportamiento del shader (ej. `TEXTURED_BIT = 1 << 1`) |

### `TextQuad`

Equivalente a `GuiQuad` para glifos de fuente. Campos: `pos`, `size`, `uv0`,
`uv1`, `color`.

### `DrawSegment`

Define un rango de instancias a dibujar con el mismo scissor:

| Campo | Descripción |
|---|---|
| `quad_range` | Rango de índices en `GuiBatch::quads` |
| `text_range` | Rango de índices en `GuiBatch::text_quads` |
| `scissor` | `Option<Rect>` — activa `set_scissor_rect` en el render pass |

### `GuiBatch`

Colección mutable de primitivas organizada en segmentos por scissor.

| Método | Descripción |
|---|---|
| `new() / clear()` | Crea o vacía el batch |
| `rect(x,y,w,h,color)` | Quad sólido sin bordes redondeados |
| `rect_r(…, radius)` | Quad con radio uniforme en las cuatro esquinas |
| `rect_radii(…, radii)` | Quad con radio por esquina |
| `rect_textured(…, uv0, uv1, tex_index)` | Quad texturizado con UV explícitas |
| `image(…, Arc<Texture2d>, uv0, uv1, color)` | Atajo de alto nivel para imágenes *(feature `assets`)* |
| `push_clip(rect)` | Apila un scissor rect (intersecciona con el actual) |
| `pop_clip()` | Restaura el scissor anterior |
| `draw_text(font, text, pos, size, color)` | Rasteriza texto usando el atlas de la fuente *(feature `text`)* |
| `ensure_segment()` | Garantiza que existe un `DrawSegment` activo para el scissor actual |
| `reserve_texture_slot(texture)` | Registra una textura en el batch y devuelve su ranura (máx. 8) |
| `as_quad_bytes() / as_text_bytes()` | Vistas de bytes listas para `queue.write_buffer` |
| `extend(other)` | Fusiona otro batch en éste |

**Límite de texturas por batch:** `MAX_TEXTURE_SLOTS = 8`. Si se supera, el
programa entra en `panic!`.

### `ToBatches` trait

Convierte un `RenderCommand` en primitivas GPU añadiéndolas a un `GuiBatch`.

```rust
// Con feature "text"
command.to_batches(&mut batch, Some(&font));

// Sin feature "text"
command.to_batches(&mut batch);
```

Conversiones:

| `RenderCommand` | Resultado |
|---|---|
| `Quad { rect, color, radii, flags }` | Un `GuiQuad` en `batch.quads` |
| `Text { rect, text, color, font_size }` | N `TextQuad`s (un por glifo) vía `draw_text` *(feature `text`)* |
| `Image { rect, texture, uv0, uv1, color }` | Un `GuiQuad` texturizado *(feature `assets`)* |
| `PushClip { rect }` | Llama `batch.push_clip(rect)` |
| `PopClip` | Llama `batch.pop_clip()` |

---

### `GuiRenderer`

Motor GPU que gestiona los dos pipelines WGPU y los buffers de instancias.

#### Constructor

```rust
GuiRenderer::new(
    device: Arc<wgpu::Device>,
    format: wgpu::TextureFormat,
    max_instances: u32,
    width: u32,
    height: u32,
    sample_count: u32,
) -> Self
```

#### Método de renderizado

```rust
renderer.render(
    encoder: &mut wgpu::CommandEncoder,
    view: &wgpu::TextureView,
    resolve_target: Option<&wgpu::TextureView>,
    batch: &GuiBatch,
    load_op: wgpu::LoadOp<wgpu::Color>,
    queue: &wgpu::Queue,
)
```

Ejecuta un render pass con dos sub-pasadas por segmento:
1. **Quads** — pipeline de `gui.wgsl` con bind group de resolución y texturas.
2. **Texto** — pipeline de `text.wgsl` con bind group de atlas de fuente.

Redimensiona automáticamente los buffers de instancias si el batch supera
el tamaño máximo configurado.

#### Bind groups internos

| Bind group | Binding | Contenido |
|---|---|---|
| Uniform (group 0) | 0 | `[f32; 2]` resolución — visible en vertex shader |
| Image (group 1, quads) | 0 / 1 | Array de hasta 8 `TextureView` + `Sampler` |
| Font (group 1, texto) | 0 / 1 | Atlas de fuente `TextureView` + `Sampler` |

Un bind group "dummy" (textura 1×1 transparente) se crea en el inicio para
que el group 1 sea siempre válido aunque no haya imágenes en el frame.

---

## Shaders

| Shader | Archivo |
|---|---|
| Quads (color + imágenes + bordes redondeados) | `assets/shaders/gui.wgsl` |
| Texto (atlas SDF/bitmap) | `assets/shaders/text.wgsl` |

---

## Feature flags

| Feature | Efecto |
|---|---|
| `text` *(default)* | Activa `draw_text`, `ToBatches::to_batches(font)` y el pipeline de texto. Requiere `ferrous_assets` con feature `text` |
| `assets` | Activa `GuiBatch::image()`, `reserve_texture_slot()` y el mapeo de `RenderCommand::Image`. Requiere `ferrous_assets` con feature `gpu` |

---

## Dependencias

| Crate | Uso |
|---|---|
| `wgpu` | Pipelines, buffers, render pass |
| `bytemuck` | Cast de `GuiQuad`/`TextQuad` a `&[u8]` para `write_buffer` |
| `glam` | Tipos matemáticos de `ferrous_ui_core` |
| `ferrous_ui_core` | `RenderCommand`, `Rect` |
| `ferrous_assets` *(opcional)* | `Texture2d`, `Font`, atlas de glifos |


## Module overview

| Tipo | Descripción |
|------|-------------|
| `GuiRenderer` | Motor principal: gestiona pipelines WGPU, buffers de instancias y el estado global de la GPU. |
| `GuiBatch` | Acumula quads, texto y regiones de recorte en segmentos de dibujo ordenados. |
| `GuiQuad` | Estructura `repr(C)` de 80 bytes que describe un rectángulo en la GPU. |
| `TextQuad` | Estructura optimizada para glifos de texto (`pos`, `size`, `uv0`, `uv1`, `color`). |
| `DrawSegment` | Rango de instancias pertenecientes a un mismo scissor rect. |
| `ToBatches` | Trait que convierte un `RenderCommand` en primitivas de `GuiBatch`. |

---

## Estrategia de Renderizado: Batched Segments

El renderer agrupa todas las primitivas en **segmentos de dibujo** (`DrawSegment`). Cada segmento agrupa quads y texto que comparten la misma región de recorte (scissor). Dentro de cada segmento el orden es:

1. Dibujar todos los **Quads** del segmento (pipeline de quads).
2. Dibujar todo el **Texto** del segmento (pipeline de texto).

Esto minimiza los cambios de pipeline y las llamadas de dibujo al máximo.

---

## `GuiBatch` — API de dibujo

`GuiBatch` expone métodos convenientes para construir primitivas sin tocar `GuiQuad` directamente:

```rust
// Rectángulo sólido
batch.rect(x, y, w, h, color);

// Rectángulo con radio uniforme
batch.rect_r(x, y, w, h, color, 8.0);

// Rectángulo con radios por esquina [tl, tr, br, bl]
batch.rect_radii(x, y, w, h, color, [8.0, 8.0, 0.0, 0.0]);

// Imagen texturizada (requiere feature "assets")
batch.image(x, y, w, h, texture.clone(), [0.0, 0.0], [1.0, 1.0], color);

// Texto (requiere feature "text")
batch.draw_text(&font, "Hola, mundo!", [x, y], font_size, color);
```

### Pila de Recortes (Scissor Stack)

`GuiBatch` soporta recortes anidados. El área de dibujo se calcula automáticamente como la **intersección** de todos los recortes activos:

```rust
// Recortar al área del panel
batch.push_clip(panel_rect);

    // Los quads internos se recortan automáticamente
    batch.rect(child_x, child_y, child_w, child_h, color);

    // Recorte adicional (se intersecta con el anterior)
    batch.push_clip(inner_scroll_rect);
        batch.rect(content_x, content_y, content_w, content_h, color);
    batch.pop_clip();

batch.pop_clip();
```

### Gestión de Texturas

Hasta **8 texturas simultáneas** por lote sin cambiar el pipeline, gracias a arrays de texturas en el shader:

```rust
// Registrar textura y obtener su índice de ranura
let slot = batch.reserve_texture_slot(texture.clone()); // → u32 (0–7)
batch.rect_textured(x, y, w, h, color, uv0, uv1, slot);
```

---

## Shaders (WGSL)

Los shaders residen en `assets/shaders/`:

| Shader | Descripción |
|--------|-------------|
| `gui.wgsl` | Quads con soporte de bordes redondeados, texturas y flags bit a bit. |
| `text.wgsl` | Glifos rasterizados desde un `FontAtlas` con alpha blending. |

---

## Redimensionado Automático de Buffers

Si el número de primitivas en pantalla supera la capacidad actual del buffer de la GPU, `GuiRenderer` crea un buffer más grande de forma transparente:

```rust
// No hay límite fijo — el renderer crece según la demanda
renderer.render(&mut encoder, &view, resolve_target, &batch, &queue);
```

---

## Capacidades avanzadas

| Característica | Descripción |
|----------------|-------------|
| Bordes redondeados | Calculados en el fragment shader para cualquier combinación de radios por esquina. |
| Batching de texturas | 8 texturas simultáneas por lote sin cambio de pipeline. |
| Scissor Stack anidado | Intersección automática de regiones de recorte para scroll boxes y paneles overlapped. |
| Renderizado con limpieza | `render_clearing` limpia el framebuffer con un color base antes de dibujar. |
| MSAA | Soporte configurable de anti-aliasing mediante `sample_count`. |

---

## Further reading

- [Detalles técnicos del renderer](RENDER.md)
- [Núcleo de la UI — ferrous_ui_core](../../ferrous_ui_core/docs/README.md)
