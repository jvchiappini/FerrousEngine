# ColorPicker

`ColorPicker` es un widget de selección de color HSV de primera clase. Soporta tres formas de presentación intercambiables y se integra con el sistema reactivo mediante `Observable<[f32; 4]>` y el callback `on_change`.

> **Import** — `ferrous_ui_core::ColorPicker`

---

## Campos y Configuración

```rust
pub struct ColorPicker<App> {
    pub colour: [f32; 4],        // Color actual en formato RGBA (0.0–1.0)
    pub pressed: bool,           // true mientras el usuario interactúa
    pub shape: PickerShape,      // Forma del área de selección
    pub pick_pos: Option<[f32; 2]>, // Posición normalizada del selector
    pub binding: Option<Arc<Observable<[f32; 4]>>>,
}
```

- `colour`: Color RGBA actual. Canales en el rango `[0.0, 1.0]`.
- `shape`: Controla qué región del espacio HSV se expone (ver sección siguiente).
- `pick_pos`: Posición del indicador visual en coordenadas normalizadas `[0,1]×[0,1]`.
- `binding`: Vinculación reactiva opcional — actualiza el árbol automáticamente cuando el color cambia.

---

## `PickerShape`

```rust
pub enum PickerShape {
    Circle,    // Rueda circular HSV: ángulo → hue, radio → saturación
    Rect,      // Rectángulo HSV: eje-X → hue, eje-Y → (1-saturación)
    Triangle,  // Triángulo HSV: hypotenusa → hue, altura → (1-saturación)
}
```

| Shape | Uso recomendado |
|-------|----------------|
| `Circle` | Picker clásico de rueda de color. Intuitivo para artistas. |
| `Rect` | Selector plano compacto. Ideal para inspectores de propiedades. |
| `Triangle` | Presentación alternativa cuando el espacio es limitado. |

---

## Builder API

| Método | Descripción |
|--------|-------------|
| `new()` | Crea un picker blanco con forma `Circle`. |
| `with_colour([r,g,b,a])` | Establece el color inicial. |
| `with_shape(PickerShape)` | Cambia la forma del área de selección. |
| `on_change(closure)` | Callback `Fn(&mut EventContext<App>, [f32;4])` invocado en cada cambio. |
| `with_binding(obs, id)` | Vincula el color a un `Observable<[f32;4]>`. |

---

## Ejemplo de Uso

```rust
use ferrous_ui_core::{ColorPicker, PickerShape};
use std::sync::Arc;

// ColorPicker básico con callback
let picker = ColorPicker::new()
    .with_shape(PickerShape::Rect)
    .with_colour([0.3, 0.7, 1.0, 1.0])
    .on_change(|ctx, rgba| {
        // rgba es [f32; 4] con componentes 0.0–1.0
        ctx.app.selected_color = rgba;
        println!("Color: #{:02X}{:02X}{:02X}",
            (rgba[0] * 255.0) as u8,
            (rgba[1] * 255.0) as u8,
            (rgba[2] * 255.0) as u8,
        );
    });

tree.add_node(Box::new(picker), Some(panel_id));
```

```rust
// ColorPicker con Binding Reactivo
let color_obs = Arc::new(Observable::new([1.0f32, 0.5, 0.2, 1.0]));

let picker = ColorPicker::<MyApp>::new()
    .with_shape(PickerShape::Circle)
    .with_binding(color_obs.clone(), picker_node_id);

// color_obs.get() siempre refleja el color actual
// Cualquier suscriptor al Observable se actualiza automáticamente
```

---

## Anatomía Visual

El widget emite dos `RenderCommand::Quad` en `draw`:

1. **Área de selección** — Un quad con `flags` especiales que el shader GPU interpreta como un gradiente HSV (rueda, rectángulo o triángulo según `shape`).
2. **Indicador** — Un pequeño círculo blanco de 8×8 px en la posición del color actualmente seleccionado.

```
┌──────────────────────┐
│  ╭─── Hue ───╮       │
│  │           │  Sat  │  ← Gradiente HSV (flags=1,2,3)
│  ╰───────────╯       │
│          ●           │  ← Indicador (blanco, 8px, sin flags)
└──────────────────────┘
```

---

## Interacción

- **`MouseDown`**: Calcula las coordenadas normalizadas `(nx, ny)` dentro del rect. Dependiendo de `shape`, convierte a HSV y actualiza `colour` y `pick_pos`.
- **`MouseMove`** (mientras `pressed`): Interpola continuamente el color al arrastrar.
- **`MouseUp`**: Libera el estado de presión.

El hit-test varía según la forma:
- `Circle`: prueba de radio dentro de la elipse inscrita.
- `Rect`: AABB estándar.
- `Triangle`: prueba `u + v ≤ 1.0`.

> [!TIP]
> Para inspectores de material donde el espacio es reducido, usa `PickerShape::Rect`
> con un tamaño de `44×44` px — exactamente lo que usa el `MaterialInspector` del editor.

> [!NOTE]
> El canal Alpha (`colour[3]`) se preserva a través de las operaciones de selección. Para añadir un slider de alpha, combina un `Slider` vinculado con `on_change` que modifique `colour[3]` y llame a `tree.mark_paint_dirty(picker_id)`.

---

## Mapeado HSV → Posición (Round-trip)

El módulo incluye funciones internas `rgb_to_hs` y `color_to_point` que permiten reconstruir la posición del indicador a partir de un color RGBA arbitrario, garantizando consistencia entre `with_colour(rgba)` y la posición del selector visual.

```
pick(nx, ny) → colour  (hsv_to_rgba)
colour → point          (rgb_to_hs + color_to_point)
```

Ambas funciones pasan la suite de tests de round-trip incluida en el módulo.
