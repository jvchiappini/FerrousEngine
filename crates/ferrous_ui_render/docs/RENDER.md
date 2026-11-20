# 🎨 Ferrous UI Render — Referencia Técnica

`ferrous_ui_render` es el backend de renderizado GPU para la UI de FerrousEngine basado en **WGPU**.

---

## 🛡️ Núcleo Gráfico

El renderizador está diseñado para minimizar drásticamente las llamadas de dibujo (Draw Calls) mediante el agrupamiento masivo de primitivas en segmentos de renderizado.

### `GuiBatch` y `DrawSegment`

Un `GuiBatch` contiene una lista ordenada de `DrawSegment`s. Cada segmento agrupa quads y texto que comparten la misma región de recorte (scissor):

```
GuiBatch
├── DrawSegment[0]  (scissor: None)
│   ├── quads[0..12]  → fondo del panel principal
│   └── text[0..5]    → etiquetas del panel principal
├── DrawSegment[1]  (scissor: Rect { scroll box area })
│   ├── quads[12..80] → contenido del scroll box
│   └── text[5..20]   → texto dentro del scroll box
└── DrawSegment[2]  (scissor: None)
    └── quads[80..85] → overlay / tooltip
```

### Pila de Recortes Anidados (Scissor Stack)

A diferencia de los sistemas que solo soportan un scissor a la vez, `GuiBatch` mantiene una **pila** interna. Al hacer `push_clip`, el nuevo rect se intersecta con el rect actual, garantizando que el contenido siempre quede dentro de todos sus contenedores:

```
push_clip(window_area)          → scissor = window_area
  push_clip(panel_area)         → scissor = window_area ∩ panel_area
    push_clip(scroll_viewport)  → scissor = panel_area ∩ scroll_viewport
    pop_clip()                  → scissor = panel_area
  pop_clip()                    → scissor = window_area
pop_clip()                      → scissor = None (sin recorte)
```

### `GuiQuad` — Layout en Memoria (80 bytes)

```
Offset  Size  Campo
──────  ────  ─────────────────────────────────────────
0       8     pos:       [f32; 2]  — posición (x, y)
8       8     size:      [f32; 2]  — dimensiones (w, h)
16      8     uv0:       [f32; 2]  — UV esquina superior-izquierda
24      8     uv1:       [f32; 2]  — UV esquina inferior-derecha
32      16    color:     [f32; 4]  — RGBA
48      16    radii:     [f32; 4]  — radios de esquina [tl, tr, br, bl]
64      4     tex_index: u32       — índice en el array de texturas
68      4     flags:     u32       — bits de configuración del shader
```

El flag `TEXTURED_BIT (1 << 1)` activa el muestreo de textura en el fragment shader.

`GuiBatch::line` y `GuiBatch::polyline` usan `LINE_BIT (1 << 9)`: el quad es
la caja del segmento expandida por media anchura más un píxel, `uv0`/`uv1`
llevan los extremos en coordenadas locales al quad y `radii.x` lleva la media
anchura. `gui.wgsl` calcula la distancia exacta al segmento por fragmento, así
que las diagonales quedan anti-aliased y los dos extremos son redondos.

### Modelo de estado por segmento (P5)

El bucle de `GuiRenderer::render_with_timestamps` mantiene un estado de bind
local al render pass y solo emite una llamada de estado cuando el slot cambia:

| Estado | Cuándo se enlaza |
|--------|------------------|
| `set_scissor_rect` | Una vez por segmento (único estado legítimo por segmento). |
| Grupo 0 (uniforme) | Una vez por pass; es idéntico para los pipelines de quad, texto e iconos. |
| Grupo 1 | Al cambiar de atlas: imagen (quads), fuente (texto) o iconos. |
| Pipeline | Al cambiar entre `transparent`, `text` y `svg`. Texto e iconos comparten `text`. |
| VB slot 0 | Quad compartido por quads, texto e iconos; el SVG usa su propio VB. |
| VB slot 1 | Buffer de instancias: quads, texto o iconos. |
| Índice | Buffer de quads en `Uint16`; el SVG usa su buffer en `Uint32`. |

Consecuencias:

- El vertex buffer del quad y el índice `Uint16` se enlazan en el primer rango
  que los necesita y no se repiten mientras ningún SVG los sustituya. Si un
  segmento SVG los reemplaza, el siguiente rango de quad/texto/icono los vuelve
  a enlazar mediante la caché; nunca asume que siguen puestos.
- Texto reaprovecha el VB/índice del quad; iconos reaprovechan además el
  pipeline de texto y solo reenlazan su buffer de instancias y su grupo 1.
- El SVG enlaza su propio VB e índice `Uint32` una vez por segmento con SVG.
- Los segmentos posteriores no dependen del último grupo 1 enlazado: cada
  rango enlaza el suyo solo si la caché no lo tiene ya.

El render pass de ID (`read_pixel_id`) es independiente y no usa esta caché.

### Contadores de trabajo GPU (P5)

`UiFrameCounters` (devuelto por `GuiRenderer::frame_counters`) cuenta
**llamadas realmente emitidas**, no rangos ni cambios de identidad:

- `passes`: render passes abiertos por el renderer de UI (0 o 1).
- `segments`: segmentos visitados (uno por cambio de scissor).
- `scissor_changes`: llamadas a `set_scissor_rect`.
- `draw_calls`: llamadas a `draw_indexed`.
- `pipeline_binds`: llamadas a `set_pipeline`.
- `bind_group_binds`: llamadas a `set_bind_group`.
- `buffer_binds`: llamadas a `set_vertex_buffer` + `set_index_buffer`.

Antes de P5 cada rango de primitiva reenlazaba pipeline, grupo 0, grupo 1 y
todos los buffers, así que `pipeline_binds` era igual a `draw_calls`. Con la
caché, un frame con el mismo contenido reporta menos binds pero exactamente
los mismos `draw_calls` y `segments`, requisito de identidad de píxeles.

---

## 🚀 Trait `ToBatches`

El trait `ToBatches` es el puente entre `ferrous_ui_core` y este backend:

```rust
impl ToBatches for RenderCommand {
    fn to_batches(&self, batch: &mut GuiBatch, font: Option<&Font>) {
        match self {
            RenderCommand::Quad { rect, color, radii, flags } => { /* → GuiQuad */ }
            RenderCommand::Text { rect, text, color, font_size } => { /* → TextQuad × n */ }
            RenderCommand::Image { rect, texture, uv0, uv1, color } => { /* → GuiQuad texturizado */ }
            RenderCommand::PushClip { rect } => { batch.push_clip(*rect); }
            RenderCommand::PopClip => { batch.pop_clip(); }
        }
    }
}
```

---

## 💎 Características Avanzadas

| Característica | Implementación |
|----------------|----------------|
| Bordes redondeados | Calculados en el fragment shader (`gui.wgsl`) mediante SDF para radios arbitrarios por esquina. |
| Array de texturas | `MAX_TEXTURE_SLOTS = 16` texturas simultáneas por lote en nativo (8 en wasm) usando `BindingType::Texture` con `count: NonZeroU32`. `GuiBatch::reserve_texture_slot` hace `panic!` al superar el límite; los slots libres se rellenan con la última textura al construir el bind group. |
| Pila de scissor | Intersección automática en `push_clip`; `current_scissor` se restaura en `pop_clip`. |
| Buffers dinámicos | Si el conteo de instancias supera la capacidad, se re-crea el buffer de GPU automáticamente. |
| MSAA | `sample_count` configurable en `GuiRenderer::new`. |
| Renderizado preservado | `render` usa `LoadOp::Load`; `render_clearing` usa `LoadOp::Clear`. |
