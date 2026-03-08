# 🏗️ Ferrous UI Core — Referencia de Tipos

`ferrous_ui_core` es el cerebro del sistema de interfaz de usuario de FerrousEngine. Implementa un modelo de **Modo Retenido** (Retained Mode) diseñado para el alto rendimiento y el "Lag Cero".

---

## 🛡️ Estructuras Fundamentales

### `Rect`
Rectángulo de posición y dimensiones, usado en todo el sistema de UI.

```rust
let r = Rect::new(10.0, 20.0, 200.0, 50.0);
r.contains([15.0, 30.0]);      // → true
r.intersects(&other_rect);     // → bool
r.intersect(&clip_rect);       // → Rect (área en común)
```

### `RectOffset`
Define los cuatro lados de un espaciado (padding / margin). Constructor conveniente:
```rust
let padding = RectOffset::all(8.0); // top = right = bottom = left = 8
```

### `Style`
Conjunto de propiedades de layout para cada nodo: `margin`, `padding`, `size`, `display`, `position`, `alignment`, `offsets`.

### `UiTree`
El gestor principal del árbol de widgets.
- Utiliza un `SlotMap` para almacenar los `Node`s con acceso O(1).
- Proporciona estabilidad de IDs (`NodeId`). Incluso si los widgets cambian de posición en la memoria, su ID se mantiene constante.
- Gestiona la jerarquía (padre/hijos) y el sistema reactivo de forma integrada.

Métodos principales:

| Método | Descripción |
|--------|-------------|
| `add_node(widget, parent)` | Inserta un nuevo widget en el árbol. |
| `add_node_with_id(widget, parent, id)` | Inserta con un identificador de texto para búsquedas. |
| `build()` | Ejecuta la fase `build` de todos los widgets recursivamente. |
| `update(delta_time)` | Ejecuta la fase `update` y aplica cambios reactivos pendientes. |
| `collect_commands(&mut cmds, viewport)` | Recolecta `RenderCommand`s. Omite nodos fuera del `viewport` (culling). |
| `mark_layout_dirty(id)` | Invalida el layout del nodo y propaga hacia arriba. |
| `mark_paint_dirty(id)` | Invalida solo el aspecto visual del nodo. |
| `get_node_by_id(id_str)` | Búsqueda O(1) de un nodo por su identificador de texto. |
| `set_node_style(id, style)` | Establece el estilo y lanza `mark_layout_dirty`. |

### `Node`
La unidad de almacenamiento en el árbol. Contiene:
- `widget`: Boxed `dyn Widget`.
- `parent` / `children`: Enlaces jerárquicos.
- `style`: Preferencias de diseño (Padding, Margin, Alignment).
- `rect`: Geometría final calculada por el motor de layout.
- `cached_cmds`: Caché de comandos de dibujo.
- `dirty`: Flags de estado (`layout`, `paint`, `hierarchy`, `subtree_dirty`).

### Trait `Widget`
Cualquier componente UI de Ferrous debe implementar este trait:

| Método | Cuándo se llama | Para qué sirve |
|--------|-----------------|----------------|
| `build(&mut ctx)` | Una vez al insertar el nodo | Instanciar hijos |
| `update(&mut ctx)` | Cada frame | Animaciones, timers |
| `calculate_size(&ctx)` | Durante el layout | Devolver tamaño intrínseco deseado |
| `draw(&ctx, &mut cmds)` | Solo si `paint` es sucio | Generar `RenderCommand`s |
| `on_event(&mut ctx, event)` | Cuando hay input | Reaccionar a interacciones |

---

## 🎨 Sistema de Temas (`Theme` y `Color`)

> **Fase 5.5 del roadmap.** Elimina los `[f32; 4]` hardcodeados. Toda la app cambia de look desde un solo lugar.

### `Color`
Tipo RGBA normalizado con helpers para su creación y manipulación:

| Método / Constante | Descripción |
|--------------------|-------------|
| `Color::hex("#RRGGBB")` | Construye desde cadena hexadecimal. |
| `Color::from_rgba8(r, g, b, a)` | Construye desde enteros 0–255. |
| `.to_array()` | Convierte a `[f32; 4]` para `RenderCommand`. |
| `.lerp(other, t)` | Interpolación lineal entre dos colores. |
| `.with_alpha(a)` | Devuelve el color con la opacidad modificada. |
| `Color::BLACK` / `Color::WHITE` | Constantes de colores base. |
| `Color::FERROUS_ACCENT` | Violeta Ferrous `#6C63FF`. |

### `Theme`
Paleta semántica con roles de color predefinidos:

| Campo | Significado |
|-------|-------------|
| `primary` | Color de acento (botones, elementos activos). |
| `primary_variant` | Variante más oscura (hover, bordes). |
| `background` | Fondo de la aplicación. |
| `surface` | Fondo de paneles y tarjetas. |
| `surface_elevated` | Fondo de popups, tooltips. |
| `on_surface` | Color de texto principal. |
| `on_surface_muted` | Color de texto secundario / hint. |
| `error` / `success` / `warning` | Estados de feedback. |
| `border_radius` | Radio de borde global en píxeles. |
| `font_size_base` | Tamaño de fuente base en píxeles. |

Constructores predefinidos:
```rust
let dark  = Theme::dark();   // Paleta oscura (Catppuccin Mocha inspirada)
let light = Theme::light();  // Paleta clara
```

---

## 🔧 StyleBuilder — API Fluent (Fase 5.4)

> Reemplaza el verbose `Style { ... }` por una cadena de métodos legibles.

```rust
use ferrous_ui_core::StyleBuilder;

// Equivalente a: Style { display: FlexRow, padding: all(8), size: (100%, 48px), ... }
let style = StyleBuilder::new()
    .fill_width()       // size.0 = Percentage(100)
    .height_px(48.0)   // size.1 = Px(48)
    .padding_all(8.0)  // padding = all(8)
    .row()             // display = FlexRow
    .center_items()    // alignment = Center
    .build();
```

| Método | Descripción |
|--------|-------------|
| `.width_px(f32)` / `.height_px(f32)` | Tamaño fijo en píxeles. |
| `.width_pct(f32)` / `.height_pct(f32)` | Tamaño en porcentaje. |
| `.fill_width()` / `.fill_height()` / `.fill()` | Ocupa todo el espacio disponible. |
| `.flex(f32)` | Factor `flex-grow` de Flexbox. |
| `.padding_all(f32)` / `.padding_xy(x, y)` | Relleno uniforme o por eje. |
| `.margin_all(f32)` / `.margin_xy(x, y)` | Margen uniforme o por eje. |
| `.row()` / `.column()` / `.block()` | Modo de visualización. |
| `.center_items()` / `.start_items()` / `.end_items()` / `.stretch_items()` | Alineación. |
| `.absolute()` / `.relative()` | Posicionamiento. |
| `.top(f32)` / `.bottom(f32)` / `.left(f32)` / `.right(f32)` | Offsets de posicionamiento absoluto. |
| `.build()` | Finaliza y devuelve el `Style`. |

---

## 🔄 Sistema Reactivo (`Observable<T>`)

Permite vincular valores de la aplicación a los widgets de forma declarativa:

```rust
// Crear un observable
let volume: Arc<Observable<f32>> = Arc::new(Observable::new(0.5));

// Suscribir un nodo para que se repinte al cambiar
volume.subscribe(slider_node_id);

// Cambiar el valor — devuelve los nodos a marcar como sucios
let dirty = volume.set(0.8);
tree.reactivity.notify_change(dirty);

// El ReactivitySystem los aplica automáticamente en tree.update()
```

---

## 🚀 Optimización: Dirty Flags

El sistema utiliza una propagación de flags `subtree_dirty`. Si un nodo en la profundidad del árbol cambia, solo se marcan sus padres como sucios hacia arriba. Durante el recorrido de renderizado:

1. Si un nodo tiene `subtree_dirty = false` → se salta toda su descendencia **instantáneamente**.
2. Si el nodo está sucio (`paint`/`layout`) → se regeneran sus comandos.
3. Si no está sucio → se usan los comandos del **frame anterior** sin ningún cómputo adicional.

---

## 📦 Widgets Disponibles

| Widget | Callbacks / Features | Descripción |
|--------|---------------------|-------------|
| `Panel` | — | Contenedor visual con color de fondo y radio de esquina. |
| `Label` | `Observable<String>` binding | Texto con soporte reactivo automático. |
| `Button` | `on_click`, `on_hover`, `on_hover_end` | Botón con estados hover/press y callbacks fluent. |
| `Slider` | `on_change`, `Observable<f32>` binding | Control de arrastre con reactividad bidireccional. |
| `PlaceholderWidget` | — | Nodo vacío para uso estructural o provisional. |
