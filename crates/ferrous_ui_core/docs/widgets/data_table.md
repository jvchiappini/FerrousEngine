# DataTable

`DataTable` es una tabla de datos completa con **headers fijos**, cuerpo con scroll virtualizado, columnas reordenables mediante **drag de resize handles**, **ordenación por columna** (clic en header), filtros, y selección de filas.

> **Import** — `ferrous_ui_core::{DataTable, TableColumn, SortDirection}`

---

## API de `TableColumn`

| Método | Descripción |
|--------|-------------|
| `TableColumn::new(title)` | Columna con ancho por defecto de 120 px. |
| `.width(px)` | Ancho inicial en píxeles. |
| `.min_width(px)` | Ancho mínimo al arrastrar el handle de resize. |
| `.sortable(bool)` | Permite ordenar la tabla haciendo clic en este header. |
| `.align_right()` | Alinea el contenido a la derecha (útil para números). |

## API de `DataTable<App>`

| Método | Descripción |
|--------|-------------|
| `DataTable::new()` | Crea una tabla vacía. |
| `.column(col)` | Añade una columna al esquema. |
| `.with_row_count(n)` | Total de filas en la fuente de datos. |
| `.row_height(px)` | Alto de cada fila (por defecto `28`). |
| `.header_height(px)` | Alto del header (por defecto `32`). |
| `.stripe_rows(bool)` | Activa/desactiva el striping alternado. |
| `.on_render_cell(f)` | Callback de pintado personalizado de celdas. |
| `.on_row_select(f)` | Callback al seleccionar una fila: `f(ctx, row_index)`. |
| `.on_sort(f)` | Callback al ordenar por columna: `f(ctx, col_index, dir)`. |

**Campos públicos de estado:**

| Campo | Tipo | Descripción |
|-------|------|-------------|
| `columns` | `Vec<TableColumn>` | Esquema mutable de columnas. |
| `row_count` | `usize` | Total de filas (modificable para filtrado). |
| `selected_rows` | `Vec<usize>` | Filas seleccionadas. |
| `sort_column` | `Option<usize>` | Columna activa de ordenación. |
| `sort_direction` | `SortDirection` | `Ascending` / `Descending`. |
| `scroll_offset_y` | `f32` | Scroll vertical actual en px. |
| `scroll_offset_x` | `f32` | Scroll horizontal actual en px. |

---

## Ejemplo de uso

```rust
use ferrous_ui_core::{DataTable, TableColumn, SortDirection, StyleBuilder, StyleExt};

struct MyApp {
    files: Vec<FileEntry>,  // la fuente de datos
}

// Crear la tabla
let table = DataTable::<MyApp>::new()
    .column(TableColumn::new("Nombre").min_width(150.0).sortable(true))
    .column(TableColumn::new("Tamaño").width(80.0).min_width(60.0).sortable(true).align_right())
    .column(TableColumn::new("Tipo").width(70.0))
    .column(TableColumn::new("Modificado").width(120.0).sortable(true))
    .with_row_count(app.files.len())
    .row_height(28.0)
    .stripe_rows(true)
    .on_render_cell(|ctx, row, col, rect, cmds| {
        let file = &ctx.app.files[row];
        let text = match col {
            0 => file.name.clone(),
            1 => format_size(file.size_bytes),
            2 => file.extension.clone(),
            3 => file.modified.to_string(),
            _ => String::new(),
        };
        cmds.push(RenderCommand::Text {
            rect: Rect::new(rect.x + 8.0, rect.y + 4.0, rect.width - 16.0, rect.height - 8.0),
            text,
            color: ctx.theme.on_surface.to_array(),
            font_size: ctx.theme.font_size_base,
        });
    })
    .on_row_select(|ctx, row| {
        if let Some(file) = ctx.app.files.get(row) {
            ctx.app.selected_file = Some(file.path.clone());
        }
    })
    .on_sort(|ctx, col, direction| {
        match (col, direction) {
            (0, SortDirection::Ascending) => ctx.app.files.sort_by(|a, b| a.name.cmp(&b.name)),
            (0, SortDirection::Descending) => ctx.app.files.sort_by(|a, b| b.name.cmp(&a.name)),
            (1, SortDirection::Ascending) => ctx.app.files.sort_by_key(|f| f.size_bytes),
            _ => {}
        }
        // El callback es responsable de marcar la tabla como dirty
        ctx.tree.mark_paint_dirty(ctx.node_id);
    });

let table_id = tree.add_node(Box::new(table), Some(panel_id));
tree.set_node_style(table_id, StyleBuilder::new().fill_width().fill_height().build());
```

---

## Comportamiento de eventos

### Header

| Interacción | Acción |
|-------------|--------|
| Clic en columna sortable | Alterna `Ascending` ↔ `Descending`; invoca `on_sort` |
| Clic en columna nuevamente | Invierte la dirección |
| Drag en resize handle (±4px del borde derecho) | Redimensiona la columna en tiempo real |

### Body

| Interacción | Acción |
|-------------|--------|
| Clic en fila | Selección + `on_row_select` |
| Rueda del ratón | Scroll vertical del body |
| Rueda horizontal / `delta_x` | Scroll horizontal (útil con trackpad) |

---

## Virtualización

La tabla solo pinta las filas de datos visibles:

```
row_count = 1 000 000   row_height = 28px   viewport body_h = 600px
  → visible rows = ceil(600 / 28) + 1 = 22    (de 1 000 000)
```

El header se pinta siempre fijo, independientemente del scroll.

---

## Anatomía visual

```
┌── DataTable (fill) ─────────────────────────────────────────────────────┐
│ ┌ Header (surface_elevated, 32px) ──────────────────────────────────┐  │
│ │  Nombre ↑  │  Tamaño  │  Tipo  │  Modificado  │         resize──▶│  │
│ └────────────┤──────────┼────────┼──────────────┤──────────────────┘  │
│ ┌ Body (clip, scroll) ──────────────────────────────────────────────┐  │
│ │  archivo_01.txt  │   4.2 KB │  TXT  │  2024-03-01  <- fila 0     │  │
│ │  proyecto.fui    │   1.8 MB │  FUI  │  2024-03-08  <- stripe     │  │
│ │  textura.png     │  48.2 KB │  PNG  │  2024-02-14  <- seleccion  │  │
│ │  ...             │   ...    │  ...  │  ...                        │  │
│ │                                                              ███  │  │← v.scrollbar
│ └───────────────────────────────────────────────────────────────────┘  │
│  ████                                                                   │←h.scrollbar
└─────────────────────────────────────────────────────────────────────────┘
```

- **Header**: `surface_elevated`, indicador de ordenación `↑`/`↓` en color `primary`.
- **Stripe**: filas pares en `surface`, impares en `surface_elevated.with_alpha(0.4)`.
- **Selección**: fondo `primary.with_alpha(0.25)`.
- **Línea de separación inferior del header**: `primary.with_alpha(0.4)`, 1 px.
- **Resize handles**: línea vertical de 1px, hitbox de ±4px.

> [!TIP]
> Para una tabla de solo lectura (sin ordenación), omite `.sortable(true)` en todas
> las columnas. Para una tabla compacta tipo inspector, usa `row_height(22.0)` y
> `header_height(24.0)`.

> [!IMPORTANT]
> El `on_sort` callback es responsable de reordenar la fuente de datos y de llamar
> a `ctx.tree.mark_paint_dirty(ctx.node_id)`. `DataTable` no reordena los datos
> internamente — solo gestiona la UI de ordenación.
