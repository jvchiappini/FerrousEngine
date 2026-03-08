# ferrous_layout

`ferrous_layout` es el cerebro geométrico detrás de la UI de FerrousEngine. Transforma las reglas de diseño abstractas (`Style`) en coordenadas físicas exactas (`Rect`) para cada elemento de la pantalla.

---

## Motor Subyacente: Taffy

Este crate utiliza **Taffy** como motor de resolución de restricciones. Taffy es una implementación de alto rendimiento en Rust para los algoritmos de **Flexbox** y **CSS Grid**.

Gracias a Taffy, `ferrous_layout` puede manejar:
- Anchos y altos porcentuales, fijos o flexibles (`Flex`).
- Alineación dinámica (`Alignment::Center`, `Stretch`, etc.).
- Modos de visualización complejos (`FlexRow`, `FlexColumn`).
- Tamaños intrínsecos de widgets mediante función de medida (`Widget::calculate_size`).
- Posicionamiento absoluto mediante `Position::Absolute` con `offsets`.

---

## Flujo de Trabajo

```
UiTree (Ferrous)           TaffyTree (interno)
─────────────               ──────────────────
NodeId A  ────sync_node───▶  taffy::NodeId A
  NodeId B                     taffy::NodeId B
  NodeId C                     taffy::NodeId C

     ▼ compute_layout(w, h)

TaffyTree calcula posiciones y dimensiones usando Flexbox

     ▼ apply_layout (recursivo)

NodeId A → rect { x, y, w, h }  (coordenadas absolutas en pantalla)
  NodeId B → rect { x, y, w, h }
  NodeId C → rect { x, y, w, h }
```

1. **Sincronización:** Se recorre el `UiTree` y se crea un espejo de la jerarquía en `TaffyTree`. Los nodos ya existentes se actualizan en lugar de recrearse.
2. **Cálculo:** Se ejecuta el algoritmo de layout sobre el grafo de Taffy. Los widgets pueden influir en el resultado a través de `Widget::calculate_size` (función de medida).
3. **Aplicación:** Los resultados (x, y, width, height) se escriben de vuelta en el campo `rect` de cada `Node` del `UiTree` en coordenadas **absolutas de pantalla**.

---

## Unidades Soportadas (`Units`)

| Unidad | Taffy equivalente | Comportamiento |
|--------|-------------------|----------------|
| `Px(f32)` | `Dimension::Length` | Valor absoluto en píxeles. |
| `Percentage(f32)` | `Dimension::Percent` | Relativo al contenedor padre (0.0 a 100.0). |
| `Flex(f32)` | `flex_grow` | Reparte el espacio sobrante proporcionalmente. |
| `Auto` | `Dimension::Auto` | Taffy infiere el tamaño por contenido o contenedor. |

---

## Ejemplo de Uso

```rust
use ferrous_layout::LayoutEngine;
use ferrous_ui_core::{UiTree, Style, Units, DisplayMode, Alignment};

let mut layout = LayoutEngine::new();
let mut tree = UiTree::<()>::new();

// Configurar el estilo de un nodo
tree.set_node_style(root_id, Style {
    display: DisplayMode::FlexColumn,
    alignment: Alignment::Center,
    size: (Units::Percentage(100.0), Units::Percentage(100.0)),
    padding: RectOffset::all(8.0),
    ..Default::default()
});

// Calcular el layout para una ventana de 1920×1080
layout.compute_layout(&mut tree, 1920.0, 1080.0);

// Los Rects de todos los nodos están actualizados
let rect = tree.get_node_rect(some_node_id).unwrap();
println!("El botón está en ({}, {}) con {}×{}", rect.x, rect.y, rect.width, rect.height);
```

---

## Modos de Visualización

| `DisplayMode` | Comportamiento |
|---------------|----------------|
| `Block` | Los hijos se apilan verticalmente; flujo estándar de bloque. |
| `FlexRow` | Los hijos se disponen horizontalmente con lógica Flexbox. |
| `FlexColumn` | Los hijos se disponen verticalmente con lógica Flexbox. |

---

## Posicionamiento Absoluto

Un nodo con `Position::Absolute` se posiciona relativo a su ancestro más cercano con `Position::Relative`, ignorando a sus hermanos. Los desplazamientos se controlan con `Style::offsets`:

```rust
Style {
    position: Position::Absolute,
    offsets: RectOffset { top: 10.0, right: 10.0, ..Default::default() },
    size: (Units::Px(120.0), Units::Px(36.0)),
    ..Default::default()
}
```

---

## Optimizaciones

El cálculo de layout solo ocurre cuando un nodo o uno de sus hijos es marcado con la bandera `layout_dirty`. Si la jerarquía no ha cambiado y los tamaños son estables, el motor de layout no consume ciclos de CPU. Taffy también implementa caching interno de sus propios resultados para nodos no modificados.

---

## Further reading

- [Detalles técnicos del motor de layout](LAYOUT.md)
- [Estilos y unidades — ferrous_ui_core](../../ferrous_ui_core/docs/CORE.md)
