# Image

`Image` es un widget diseñado para mostrar una textura GPU dentro de un rectángulo de la UI. Soporta múltiples **modos de ajuste** (`fit`), recorte por **coordenadas UV**, **redondeo de bordes** y un **color de tinte**.

> **Import** — `ferrous_ui_core::{ImageWidget, ImageFit}`

Es fundamental para mostrar avatares, previsualizaciones de recursos, fondos decorativos y cualquier contenido rasterizado dentro de la interfaz.

---

## API de `ImageWidget<App>`

| Método | Descripción |
|--------|-------------|
| `ImageWidget::from_id(id)` | Crea un widget a partir de un ID de textura opaco (`u64`). |
| `.fit(ImageFit)` | Define cómo se escala la imagen dentro del widget. |
| `.uv(uv0, uv1)` | Recorta una subregión de la textura (atlas). |
| `.tint([r, g, b, a])` | Aplica un multiplicador de color a la imagen. |
| `.border_radius(px)` | Redondea las esquinas de la imagen. |
| `.intrinsic_size(w, h)` | Ayuda al sistema de layout proporcionando las dimensiones reales de la textura. |
| `.no_placeholder()` | Desactiva el gráfico de "cruz" cuando no hay textura cargada. |

## Modos de Ajuste (`ImageFit`)

| Variante | Comportamiento |
|----------|----------------|
| `Contain` (**Default**) | Escala uniformemente para que la imagen quepa entera sin recortarse. Puede dejar bordes vacíos. |
| `Cover` | Escala uniformemente para llenar el widget por completo, recortando lo sobrante. |
| `Stretch` | Estira la imagen para ocupar todo el espacio, ignorando la proporción original. |
| `None` | Muestra la imagen a su tamaño original, centrándola si es posible. |

---

## Ejemplo de uso

```rust
use ferrous_ui_core::{ImageWidget, ImageFit, StyleBuilder, StyleExt};

// Crear una previsualización de asset con bordes redondeados
let preview = ImageWidget::<MyApp>::from_id(app.asset_tex_id)
    .fit(ImageFit::Cover)
    .border_radius(12.0)
    .intrinsic_size(512.0, 512.0) // Ayuda al layout a mantener el aspect ratio
    .tint([1.0, 1.0, 1.0, 0.9]); // Ligeramente transparente

let image_id = ui_tree.add_node(Box::new(preview), Some(container_id));
ui_tree.set_node_style(image_id, StyleBuilder::new()
    .width_px(120.0)
    .height_px(120.0)
    .build());
```

---

## Coordenadas UV (Atlas)

Para usar una subregión de una textura (muy común en spritesheets o atlas de interfaz), configura las coordenadas UV:

```rust
// Mostrar solo el cuadrante superior derecho de una textura
let atlas_item = ImageWidget::<MyApp>::from_id(atlas_id)
    .uv([0.5, 0.0], [1.0, 0.5]);
```

---

## Integración con el Sistema de Assets

Si el feature `assets` está habilitado en `ferrous_ui_core`, el widget puede integrarse directamente con el gestor de recursos para garantizar que las texturas estén cargadas antes de intentar dibujarlas. De lo contrario, utiliza un `texture_id` numérico que el backend de renderizado debe saber resolver.

---

## Anatomía Visual

```
┌── ImageWidget (Contain) ───────┐
│                                 │
│      ┌───────────────────┐      │
│      │                   │      │
│      │     IMAGEN        │      │ ← Centrada
│      │     CENTRADA       │      │
│      │                   │      │
│      └───────────────────┘      │
│                                 │
└─────────────────────────────────┘
```

> [!NOTE]
> Si la textura tiene transparencia (canal Alpha), se renderizará correctamente mezclándose con el fondo del widget o los widgets de debajo.

> [!TIP]
> Combina `ImageWidget` con un `AspectRatio` de layout para asegurar que el contenedor siempre guarde la proporción correcta incluso si las dimensiones de píxeles cambian dinámicamente.
