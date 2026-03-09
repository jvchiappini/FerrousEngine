# Label

`Label` es el widget fundamental para mostrar texto en la interfaz. Puede representar tanto texto estático como texto dinámico vinculado a un estado reactivo.

> **Import** — `ferrous_ui_core::Label`

## Campos y Estructura

```rust
pub struct Label {
    pub text: String,
    pub color: Option<Color>,
    pub font_size: Option<f32>,
    pub binding: Option<Arc<Observable<String>>>,
}
```

- `text`: El contenido literal del texto (usado como fallback si no hay binding).
- `color`: Color opcional. Si es `None`, usa `theme.on_surface`.
- `font_size`: Tamaño opcional. Si es `None`, usa `theme.font_size_base`.
- `binding`: Referencia a un `Observable<String>` para actualizaciones automáticas.

## Construcción

```rust
use ferrous_ui_core::{Label, Color};

// Texto estático simple
let l1 = Label::new("Hola Mundo");

// Con personalización de estilo
let l2 = Label::new("Aviso")
    .with_color(Color::RED)
    .with_size(18.0);
```

## Data Binding (Reactividad)

`Label` soporta vinculación bidireccional (lectura). Cuando el valor del `Observable` cambia, el label se marca automáticamente como "paint dirty" y se repinta en el siguiente frame.

```rust
// 1. Crear el observable
let fps_text = Arc::new(Observable::new("60 FPS".into()));

// 2. Crear el label y vincularlo
// Es necesario pasar el node_id para que el observable sepa a quién notificar
let l3 = Label::new("")
    .with_binding(fps_text.clone(), node_id);

// 3. Actualizar el valor desde cualquier parte
fps_text.set("120 FPS".into()); // El Label se actualiza solo
```

## Builder API

| Método | Descripción |
|--------|-------------|
| `new(text)` | Crea una instancia con texto inicial. |
| `with_color(color)` | Establece un color fijo. |
| `with_size(size)` | Establece un tamaño de fuente fijo. |
| `with_binding(obs, id)`| Vincula el texto a un `Observable`. |

## Detalles Técnicos

- **Layout**: Su tamaño se calcula automáticamente en `calculate_size` basándose en el número de caracteres y el tamaño de la fuente.
- **Eficiencia**: Si el texto no cambia, el `draw` no se vuelve a llamar y se reutiliza el `RenderCommand::Text` cacheados.
