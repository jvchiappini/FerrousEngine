# VirtualList

`VirtualList` renderiza **exclusivamente los items visibles** en el viewport actual, reciclando la lógica de pintado a medida que el usuario hace scroll. Admite listas de **100 000+ filas** sin degradación de FPS ni memoria.

> **Import** — `ferrous_ui_core::VirtualList`

---

## ¿Por qué virtualización?

Una lista ordinaria que crea un nodo de árbol por cada fila de 100 000 items consumiría ~25 MB de memoria y forzaría un cálculo de layout O(N) en cada frame. `VirtualList` mantiene una memoria O(viewport):

```
total_items = 100 000  →  pintados en cada frame ≈ viewport_h / row_height
                                                  ≈ 600 / 32 = ~19 filas
```

---

## API

| Método | Descripción |
|--------|-------------|
| `VirtualList::new(count, height)` | Crea una lista con `count` items de `height` px de alto. |
| `.on_render_item(f)` | Callback de pintado: `f(ctx, index, rect, cmds)`. |
| `.on_select(f)` | Callback al hacer clic en un item: `f(ctx, index)`. |
| `.on_double_click(f)` | Callback al hacer doble clic. |

**Campos públicos:**

| Campo | Tipo | Descripción |
|-------|------|-------------|
| `item_count` | `usize` | Total de ítems (modificable en runtime para filtrado). |
| `item_height` | `f32` | Alto de cada fila en px. |
| `selected` | `Vec<usize>` | Índices seleccionados. |
| `scroll_offset` | `f32` | Desplazamiento de scroll actual en px. |

**Métodos de instancia:**

| Método | Descripción |
|--------|-------------|
| `.set_item_count(n)` | Cambia el número de ítems (p.ej. tras un filtro). |
| `.scroll_to(idx, viewport_h)` | Desplaza el scroll para hacer visible el item `idx`. |

---

## Ejemplo de uso

```rust
use ferrous_ui_core::{VirtualList, StyleBuilder, StyleExt};

// Fuente de datos en la App
struct MyApp {
    log_lines: Vec<String>,  // puede tener 100 000+ entradas
}

// Crear la lista
let log_list = VirtualList::<MyApp>::new(app.log_lines.len(), 28.0)
    .on_render_item(|ctx, idx, rect, cmds| {
        // Colorear líneas de error en rojo
        let line = &ctx.app.log_lines[idx];
        let color = if line.starts_with("[ERROR]") {
            ctx.theme.error.to_array()
        } else {
            ctx.theme.on_surface.to_array()
        };
        cmds.push(RenderCommand::Text {
            rect: Rect::new(rect.x + 8.0, rect.y + 4.0, rect.width - 16.0, 20.0),
            text: line.clone(),
            color,
            font_size: ctx.theme.font_size_base,
        });
    })
    .on_select(|ctx, idx| {
        ctx.app.selected_log_line = Some(idx);
    });

let list_id = tree.add_node(Box::new(log_list), Some(panel_id));
tree.set_node_style(list_id, StyleBuilder::new().fill_width().fill_height().build());
```

```rust
// Filtrar la lista (actualizar item_count)
fn apply_filter(tree: &mut UiTree<MyApp>, list_id: NodeId, count: usize) {
    if let Some(node) = tree.get_node_mut(list_id) {
        if let Some(list) = node.widget.downcast_mut::<VirtualList<MyApp>>() {
            list.set_item_count(count);
        }
        node.dirty.paint = true;
    }
    tree.mark_paint_dirty(list_id);
}
```

---

## Comportamiento

| Interacción | Resultado |
|-------------|-----------|
| Scroll con rueda del ratón | `scroll_offset` cambia; solo los nuevos items visibles se pintan |
| Clic en fila | Selección + `on_select` |
| Doble clic | `on_double_click` |

---

## Rendimiento

| Items | Memoria (nodos DOM) | Items pintados/frame |
|-------|---------------------|----------------------|
| 1 000 | ~1 (el widget raíz) | ~N visibles ≈ 20-30 |
| 100 000 | ~1 | ~20-30 |
| 1 000 000 | ~1 | ~20-30 |

> [!TIP]
> Usa `scroll_to()` después de un filtrado o búsqueda para llevar el primer
> resultado al centro del viewport automáticamente.

> [!IMPORTANT]
> El callback `on_render_item` no tiene acceso directo al `EventContext` durante
> la fase `draw`. Para reacciones a eventos, usa los callbacks `on_select` /
> `on_double_click` que sí reciben el contexto completo.

---

## Anatomía visual

```
┌── VirtualList (fill, scroll) ─────────────────────────────────────┐  ▲
│  item 100  ▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓               │  │ scroll
├──────────────────────────────────────────────────────────────────────┤  │
│  item 101  seleccionado (primary 0.25)                              │  │
├──────────────────────────────────────────────────────────────────────┤  │
│  item 102  hovered (on_surface_muted 0.06)                          │  │
├──────────────────────────────────────────────────────────────────────┤  ▼
│  ...                                                                │
├──────────────────────────────────────────────────────────────────────┤
│  item 120                                                   ███     │← scrollbar
└─────────────────────────────────────────────────────────────────────┘
     solo ~20 filas pintadas de 100 000 totales
```
