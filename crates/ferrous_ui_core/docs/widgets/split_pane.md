# SplitPane

`SplitPane` divide una región rectangular en dos paneles separados por un **divisor interactivo** que el usuario puede arrastrar para cambiar la proporción entre ellos. Soporta orientación horizontal y vertical.

> **Import** — `ferrous_ui_core::SplitPane`

---

## API Constructor

| Método | Descripción |
|--------|-------------|
| `SplitPane::new(orientation)` | Crea un split con ratio inicial 50/50. |
| `.with_first(widget)` | Widget del panel izquierdo/superior. |
| `.with_second(widget)` | Widget del panel derecho/inferior. |
| `.with_ratio(f32)` | Proporción inicial del primer panel (0.0–1.0). |
| `.divider_size(f32)` | Ancho/alto del divisor en píxeles (por defecto `6.0`). |
| `.ratio_range(min, max)` | Límites admisibles para el ratio al arrastrar. |

### `SplitOrientation`

```rust
pub enum SplitOrientation {
    Horizontal,  // divisor vertical: primer panel izquierda, segundo derecha
    Vertical,    // divisor horizontal: primer panel arriba, segundo abajo
}
```

---

## Ejemplo de Uso

```rust
use ferrous_ui_core::{SplitPane, SplitOrientation, Label, StyleBuilder, StyleExt};

// Panel Izquierdo + Derecho (35% / 65%)
let split = SplitPane::<MyApp>::new(SplitOrientation::Horizontal)
    .with_first(Box::new(scene_hierarchy_widget))
    .with_second(Box::new(viewport_widget))
    .with_ratio(0.30)
    .divider_size(5.0)
    .ratio_range(0.15, 0.60);

let id = tree.add_node(Box::new(split), Some(root_id));
tree.set_node_style(id, StyleBuilder::new().fill().build());
```

```rust
// Vertical: editor arriba, consola abajo
let split_v = SplitPane::<MyApp>::new(SplitOrientation::Vertical)
    .with_first(Box::new(code_editor))
    .with_second(Box::new(console_panel))
    .with_ratio(0.70);
```

---

## Comportamiento del Divisor

El divisor es un nodo `Divider` interno con su propio `NodeId`. Dibuja:
- Un fondo semitransparente que se intensifica (`theme.primary` al 60%) al pasar el cursor.
- Tres puntos de agarre centrados para indicar la interactividad.

Al arrastrar:
1. `MouseDown` sobre el divisor activa `is_dragging` y guarda la posición inicial.
2. `MouseMove` calcula el delta desde el inicio del arrastre y ajusta `ratio` dentro de `ratio_range`.
3. Los estilos de los dos paneles se actualizan inmediatamente mediante `set_node_style`.
4. `MouseUp` desactiva el arrastre.

---

## Arquitectura Interna

```
SplitPane (root — FlexRow o FlexColumn)
├── <primer widget>   (width/height: ratio%)
├── Divider           (width/height: divider_size px)
└── <segundo widget>  (width/height: (1-ratio)%)
```

> [!NOTE]
> `SplitPane` usa `Overflow::Hidden` en ambos paneles para recortar el contenido
> que desborde tras cambiar el ratio.

> [!TIP]
> Para aplicaciones tipo IDE, anida `SplitPane`s:
> ```rust
> let main_split = SplitPane::new(Horizontal)
>     .with_first(Box::new(sidebar))
>     .with_second(Box::new(
>         SplitPane::new(Vertical)
>             .with_first(Box::new(editor))
>             .with_second(Box::new(console))
>     ));
> ```
