# ferrous_ui_render

`ferrous_ui_render` es el backend de dibujo optimizado para GPU. Su función es tomar los `RenderCommand` abstractos generados por el sistema de UI y traducirlos en operaciones eficientes de **WGPU**.

---

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
