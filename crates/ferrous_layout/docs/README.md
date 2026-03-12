# ferrous_layout

Motor de cálculo de posiciones y dimensiones para la UI de Ferrous Engine.

Toma el `UiTree` de `ferrous_ui_core`, sincroniza su estructura con el motor
**Taffy** (implementación de Flexbox en Rust) y escribe el `Rect` resuelto
de vuelta en cada nodo del árbol. El resto del sistema (render, eventos)
sólo lee esos rects —nunca los calcula— por lo que este crate es el único
punto de verdad sobre las coordenadas físicas de cada widget.

---

## Flujo de ejecución

```
UiTree<App>  (Style de cada nodo)
      │
      ▼  LayoutEngine::compute_layout()
      │
      ├─1. sync_node()          → construye/actualiza el TaffyTree interno
      │                           (recursivo, DFS desde la raíz)
      │
      ├─2. taffy.compute_layout_with_measure()
      │       │  para cada nodo llama Widget::calculate_size()
      │       │  si Taffy necesita el tamaño intrínseco del widget
      │       └─► resuelve el árbol Flexbox
      │
      └─3. apply_layout()       → escribe Rect { x, y, width, height }
                                  en cada NodeId del UiTree,
                                  respetando scroll_offset() de los padres
```

---

## `LayoutEngine`

Único tipo público del crate.

### Constructor

```rust
let mut layout_engine = LayoutEngine::new();
```

Internamente mantiene:
- `taffy: TaffyTree<NodeId>` — árbol Taffy anotado con el `NodeId` de Ferrous
- `node_map: HashMap<NodeId, taffy::NodeId>` — mapeo bidireccional de IDs

### Método principal

```rust
layout_engine.compute_layout(&mut ui_tree, available_width, available_height);
```

| Parámetro | Descripción |
|---|---|
| `tree` | El `UiTree<App>` cuyo estilo y rect de cada nodo se va a procesar |
| `available_width` | Ancho disponible en píxeles (normalmente el ancho de la ventana) |
| `available_height` | Alto disponible en píxeles |

Después de la llamada, cada nodo del árbol tiene su `Rect` actualizado con
coordenadas absolutas respecto a la esquina superior izquierda de la ventana.

---

## Conversión de `Style` a Taffy

`LayoutEngine::convert_style()` traduce cada campo de `ferrous_ui_core::Style`
al equivalente de Taffy:

| `Style` Ferrous | Taffy equivalente |
|---|---|
| `DisplayMode::Block` | `Display::Block` |
| `DisplayMode::FlexRow` | `Display::Flex` + `FlexDirection::Row` |
| `DisplayMode::FlexColumn` | `Display::Flex` + `FlexDirection::Column` |
| `Position::Relative / Absolute` | `Position::Relative / Absolute` |
| `size.0 / size.1` | `size.width / height` con `Dimension` |
| `Units::Px(v)` | `Dimension::Length(v)` |
| `Units::Percentage(v)` | `Dimension::Percent(v / 100.0)` |
| `Units::Flex(v)` | `flex_grow = v` (tamaño → `Auto`) |
| `Units::Auto` | `Dimension::Auto` |
| `margin` / `padding` | `taffy::Rect<LengthPercentage(Auto)>` |
| `offsets` (inset) | `taffy::Rect<LengthPercentageAuto>` |
| `Alignment::Start/Center/End` | `align_items` + `justify_content` |
| `Alignment::Stretch` | `align_items = Stretch` |
| `Overflow::Visible/Hidden/Scroll` | `taffy::Overflow` en ejes X e Y |

---

## Medidas personalizadas (measure function)

Cuando Taffy necesita saber el tamaño intrínseco de un nodo (p. ej., un
`Label` cuyo ancho depende del texto), llama a la función de medida pasada a
`compute_layout_with_measure`. El motor delega en `Widget::calculate_size()`:

```rust
// Dentro del closure de medida:
let size = node.widget.calculate_size(&mut LayoutContext {
    available_space: vec2(available_w, available_h),
    known_dimensions: (known.width, known.height),
    node_id,
    theme,
});
// size.x / size.y se devuelven a Taffy
```

---

## Scroll

`apply_layout()` consulta `Widget::scroll_offset()` de cada nodo padre antes
de aplicar las coordenadas a sus hijos:

```
hijo.x = parent_x + taffy_layout.location.x - scroll_offset.x
hijo.y = parent_y + taffy_layout.location.y - scroll_offset.y
```

Esto permite que los widgets `ScrollView` desplacen su contenido sin
necesitar un re-cálculo completo del layout.

---

## Dependencias

| Crate | Uso |
|---|---|
| `taffy = "0.7"` | Motor Flexbox — `TaffyTree`, `compute_layout_with_measure` |
| `ferrous_ui_core` | `UiTree`, `NodeId`, `Rect`, `Style`, `Units`, `DisplayMode`, `Alignment`, `LayoutContext` |
| `glam` | `Vec2` para `LayoutContext::available_space` |


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
