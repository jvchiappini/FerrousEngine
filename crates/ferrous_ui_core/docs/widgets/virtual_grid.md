# VirtualGrid

`VirtualGrid` es el equivalente en **2D** de `VirtualList`. Renderiza exclusivamente las celdas visibles en el viewport, ideal para **galerías de assets**, selectores de texturas, spritesheet pickers y cualquier cuadrícula de gran tamaño.

> **Import** — `ferrous_ui_core::VirtualGrid`

---

## API

| Método | Descripción |
|--------|-------------|
| `VirtualGrid::new(count, cell_w, cell_h)` | Crea el grid con `count` items de dimensiones `cell_w × cell_h`. |
| `.columns(n)` | Número fijo de columnas. `0` = auto (llena el ancho disponible). |
| `.gap(px)` | Separación entre celdas en píxeles (por defecto `4`). |
| `.padding(px)` | Padding exterior del grid (por defecto `8`). |
| `.on_render_cell(f)` | Callback: `f(ctx, col, row, rect, cmds)` pinta una celda. |
| `.on_select(f)` | Callback al seleccionar una celda: `f(ctx, flat_index)`. |

**Campos públicos:**

| Campo | Tipo | Descripción |
|-------|------|-------------|
| `item_count` | `usize` | Total de items. |
| `cell_width` | `f32` | Ancho de celda en px. |
| `cell_height` | `f32` | Alto de celda en px. |
| `columns` | `usize` | Columnas fijas (`0` = auto). |
| `selected` | `Vec<usize>` | Índice(s) seleccionado(s). |
| `scroll_offset` | `f32` | Desplazamiento vertical en px. |

---

## Ejemplo de uso

```rust
use ferrous_ui_core::{VirtualGrid, StyleBuilder, StyleExt};

// Galería de 2048 texturas con miniaturas 96×96
let gallery = VirtualGrid::<MyApp>::new(app.textures.len(), 96.0, 96.0)
    .columns(0)      // auto: llena el ancho del panel
    .gap(8.0)
    .padding(12.0)
    .on_render_cell(|ctx, col, row, rect, cmds| {
        let idx = row * /* cols calculados dinámicamente */ + col;
        if idx >= ctx.app.textures.len() { return; }

        let tex = &ctx.app.textures[idx];
        // Fondo placeholder
        cmds.push(RenderCommand::Quad {
            rect,
            color: ctx.theme.surface_elevated.to_array(),
            radii: [4.0; 4],
            flags: 0,
        });
        // Nombre truncado
        cmds.push(RenderCommand::Text {
            rect: Rect::new(rect.x + 4.0, rect.y + rect.height - 20.0, rect.width - 8.0, 16.0),
            text: tex.name.clone(),
            color: ctx.theme.on_surface_muted.to_array(),
            font_size: ctx.theme.font_size_small,
        });
    })
    .on_select(|ctx, idx| {
        ctx.app.selected_texture = Some(idx);
    });

let grid_id = tree.add_node(Box::new(gallery), Some(panel_id));
tree.set_node_style(grid_id, StyleBuilder::new().fill_width().fill_height().build());
```

```rust
// Spritesheet picker: grid fijo 4 columnas, celdas 64×64
let sprite_picker = VirtualGrid::<MyApp>::new(spritesheet.frame_count, 64.0, 64.0)
    .columns(4)
    .gap(2.0)
    .on_select(|ctx, frame_idx| {
        ctx.app.animation.current_frame = frame_idx;
    });
```

---

## Modo columnas automático vs fijo

| Modo | Configuración | Uso típico |
|------|--------------|------------|
| **Automático** | `.columns(0)` (por defecto) | Galerías que se adaptan al ancho del panel |
| **Fijo** | `.columns(n)` | Spritesheet pickers, cuadrículas de definición exacta |

Con columnas automáticas, la fórmula es:
```
cols = floor((width - padding*2 + gap) / (cell_width + gap))
```

---

## Rendimiento

Similar a `VirtualList`, solo se pintan las celdas de las filas visibles:

```
item_count = 50 000  →  pintadas/frame ≈ visible_rows × columns
viewport_h = 600px, cell_h = 128px, cols = 6
   → visible_rows ≈ 6   → celdas pintadas = 6 × 6 = 36  de 50 000
```

---

## Anatomía visual

```
┌── VirtualGrid (gap=8, padding=12, columns=auto=4) ───────────────┐
│                                                                   │
│  ┌─────────┐  ┌─────────┐  ┌─────────┐  ┌─────────┐            │
│  │  [cel]  │  │  [cel]  │  │▓▓ sel ▓▓│  │  [cel]  │            │
│  │  idx 0  │  │  idx 1  │  │  idx 2  │  │  idx 3  │            │
│  └─────────┘  └─────────┘  └─────────┘  └─────────┘            │
│  ┌─────────┐  ┌─────────┐  ┌─────────┐  ┌─────────┐            │
│  │  [cel]  │  │  [cel]  │  │  [cel]  │  │  [cel]  │            │
│  │  idx 4  │  │  idx 5  │  │  idx 6  │  │  idx 7  │            │
│  └─────────┘  └─────────┘  └─────────┘  └─────────┘            │
│  ···  solo filas visibles  ···                          █         │← scrollbar
└───────────────────────────────────────────────────────────────────┘
```

- `idx 2` muestra la celda seleccionada con fondo `primary.with_alpha(0.35)`.
- El borde superior de la celda seleccionada es una línea de 2px en `primary`.
- Las celdas en hover muestran `surface_elevated` como fondo.

> [!TIP]
> Para implementar un selector de iconos de fuente, usa celdas pequeñas (32×32)
> con columnas automáticas y pinta el carácter unicode centrado en cada celda.
