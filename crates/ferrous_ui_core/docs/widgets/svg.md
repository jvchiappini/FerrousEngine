# SVG

`SVG` es el widget para el renderizado de gráficos vectoriales escalables. Proporciona dos modos de trabajo: **rasterización diferida** a textura o mediante **primitivas vectoriales nativas** de la UI.

> **Import** — `ferrous_ui_core::{SvgWidget, SvgPrimitive, Icons}`

Se usa preferentemente para iconos (nítidos en cualquier resolución), logotipos y formas gráficas que requieran escalado sin pérdida de calidad.

---

## Modos de Funcionamiento

### 1. Rasterización a Textura (`from_source` / `from_texture`)
El SVG se dibuja una sola vez a una textura GPU (con el tamaño deseado) y luego se muestra como una imagen normal. Esto es ideal para SVGs muy complejos con degradados, filtros o muchos caminos.

### 2. Primitivas Vectoriales (`from_primitives`)
El widget dibuja formas geométricas básicas (`Rect`, `Circle`, `Line`) directamente en el buffer de comandos de la UI. Es extremadamente rápido para iconos de línea simples y permite cambiar colores instantáneamente sin re-rasterizar.

---

## API de `SvgWidget<App>`

| Método | Descripción |
|--------|-------------|
| `SvgWidget::from_source(svg_text)` | Crea de una cadena de texto SVG (modo rasterización diferida). |
| `SvgWidget::from_primitives(vec)` | Crea una lista de primitivas `SvgPrimitive`. |
| `SvgWidget::from_texture(id)` | Usa una textura previamente rasterizada por el backend. |
| `.viewbox(x, y, w, h)` | Define la región de coordenadas lógica (por defecto `0,0,24,24`). |
| `.color([r, g, b, a])` | Color de relleno para primitivas o tinte para la textura. |
| `.fit(ImageFit)` | Ajuste dentro del widget (Contain, Cover, etc.). |
| `.size(w, h)` | Tamaño intrínseco inicial. |

---

## Librería de Iconos Integrada

La clase `Icons` proporciona un conjunto de iconos básicos de línea para facilitar el diseño rápido:

```rust
use ferrous_ui_core::{Icons, StyleBuilder};

let close_btn = Icons::close::<MyApp>()
    .color([1.0, 0.2, 0.2, 1.0])
    .size(24.0, 24.0);

let search_icon = Icons::search::<MyApp>()
    .color([1.0, 1.0, 1.0, 1.0]);
```

Iconos disponibles: `close`, `plus`, `search`, `settings`, `menu`, `arrow_right`, `check`, `warning`, `info`.

---

## Ejemplo de uso — Icono Vectorial

```rust
use ferrous_ui_core::{SvgWidget, SvgPrimitive, Icons};

// Crear un icono de "Añadir" personalizado con primitivas
let add_icon = SvgWidget::<MyApp>::from_primitives(vec![
    SvgPrimitive::HLine { x: 4.0, y: 12.0, length: 16.0, stroke_width: 2.0 },
    SvgPrimitive::VLine { x: 12.0, y: 4.0, length: 16.0, stroke_width: 2.0 },
])
.color([1.0, 1.0, 1.0, 1.0])
.viewbox(0.0, 0.0, 24.0, 24.0);

let icon_id = tree.add_node(Box::new(add_icon), Some(header_id));
```

---

## Primitivas Disponibles

| Primitiva | Argumentos |
|-----------|------------|
| `Rect` | `x, y, width, height, radius, fill, stroke_width` |
| `Circle` | `cx, cy, r, fill, stroke_width` |
| `Line` | `x1, y1, x2, y2, stroke_width` |
| `HLine` | `x, y, length, stroke_width` |
| `VLine` | `x, y, length, stroke_width` |

> [!CAUTION]
> Las líneas diagonales (`Line`) se aproximan al eje más cercano en el backend si este no soporta rotación arbitraria de primitivas.

---

## Anatomía Visual

```
┌── SvgWidget (Viewbox 24x24) ──┐
│                                │
│      (0,0)  ─────────  (24,0)  │ ← Espacio de coordenadas
│        │               │       │   lógico del Viewbox
│        │     FORMA     │       │
│        │   VECTORIAL   │       │
│        │               │       │
│      (0,24) ───────── (24,24)  │
│                                │
└────────────────────────────────┘
```

> [!TIP]
> Usa `from_source()` para cargar archivos `.svg` externos. El backend de Ferrous Renderer los procesará automáticamente para generar las texturas necesarias en tiempo de ejecución.
