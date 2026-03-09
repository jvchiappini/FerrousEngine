# AspectRatio

`AspectRatio` es un contenedor que obliga a su widget hijo a mantener una proporción fija (`width / height`), independientemente del tamaño del contenedor padre.

> **Import** — `ferrous_ui_core::AspectRatio`

---

## API Constructor

| Método | Descripción |
|--------|-------------|
| `AspectRatio::new(ratio)` | Crea el contenedor con la proporción `width/height` dada. |
| `.with_child(widget)` | Establece el widget hijo. |
| `.no_center()` | Por defecto el hijo se centra; esto lo ancla a la esquina superior izquierda. |

Proporciones comunes:

| Proporción | Valor |
|-----------|-------|
| Cuadrado | `1.0` |
| 16:9 (pantalla ancha) | `16.0 / 9.0` ≈ `1.778` |
| 4:3 (clásico) | `4.0 / 3.0` ≈ `1.333` |
| 21:9 (ultrawide) | `21.0 / 9.0` ≈ `2.333` |
| Retrato 9:16 | `9.0 / 16.0` ≈ `0.5625` |

---

## Ejemplo de uso

```rust
use ferrous_ui_core::{AspectRatio, StyleBuilder, StyleExt};

// Viewport de juego 16:9 dentro de un panel
let game_view = AspectRatio::<MyApp>::new(16.0 / 9.0)
    .with_child(Box::new(game_viewport_widget));

let id = tree.add_node(Box::new(game_view), Some(editor_panel_id));
tree.set_node_style(id, StyleBuilder::new().fill_width().fill_height().build());
```

```rust
// Miniatura cuadrada con imagen
let thumb = AspectRatio::<MyApp>::new(1.0)
    .with_child(Box::new(image_widget))
    .no_center(); // alineado a la esquina en lugar de centrado
```

---

## Comportamiento de layout

El widget calcula el **rectángulo inscrito** más grande con la proporción dada dentro del espacio disponible:

```
disponible: 800 × 400   ratio: 16/9
  → target_h_from_w = 800/1.778 = 450   (no cabe, 450 > 400)
  → usar altura: w = 400 × 1.778 = 711
  → rect: 711 × 400  centrado: offset_x = (800-711)/2 = 44.5
```

Las franjas que quedan fuera del área útil se rellenan con **negro** (efecto letterbox/pillarbox), apropiado para viewports de juego o miniaturas de imagen.

---

## Anatomía visual

```
┌──────────────────────────────────────────┐  ← contenedor padre 800×400
│▓▓▓│                                 │▓▓▓│  ← pillarbox negro (44.5px c/u)
│▓▓▓│   área child 711×400  (16:9)    │▓▓▓│
│▓▓▓│                                 │▓▓▓│
└──────────────────────────────────────────┘
```

> [!TIP]
> `AspectRatio` es ideal para incrustar el viewport de Ferrous3D dentro del editor.
> Combínalo con `DockLayout` para un layout tipo Unity/Godot.
