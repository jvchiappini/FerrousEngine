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
| Array de texturas | Hasta 8 texturas simultáneas por lote usando `BindingType::Texture` con `count: NonZeroU32`. |
| Pila de scissor | Intersección automática en `push_clip`; `current_scissor` se restaura en `pop_clip`. |
| Buffers dinámicos | Si el conteo de instancias supera la capacidad, se re-crea el buffer de GPU automáticamente. |
| MSAA | `sample_count` configurable en `GuiRenderer::new`. |
| Renderizado preservado | `render` usa `LoadOp::Load`; `render_clearing` usa `LoadOp::Clear`. |
